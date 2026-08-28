# JoveWorks Hub API v1

This document specifies the HTTP contract implemented by the current Hub
service. The API is JSON over HTTP and is rooted at `/api/v1`. A deployment
should be served behind HTTPS; the token headers below are shared secrets, not
an identity or authorization system.

## Discovery and health

`GET /.well-known/joveworks` is unauthenticated and returns `200 OK`:

```json
{"protocolVersion":1,"api":"/api/v1"}
```

`GET /healthz` is unauthenticated and returns `204 No Content` with an empty
body. It is intended for a container/process health probe, not as a readiness
or catalogue check.

## Authentication headers

Write requests require the exact configured administrator token in:

```text
X-JoveWorks-Admin-Token: <JOVEWORKS_ADMIN_TOKEN>
```

The protected writes are `POST /api/v1/courses/{slug}`,
`PUT /api/v1/courses/{slug}/catalogues`, `POST /api/v1/catalogues/{id}/{version}`,
the admin-console catalogue upload, and `POST /api/v1/publications`.
`GET`/`DELETE /api/v1/admin/catalogues` also require this header. A missing, malformed, or incorrect header is
`401 Unauthorized`; the token is never returned in a response.

Retrieving a catalogue whose stored content has `restricted: true` additionally
requires the exact configured course token:

```text
X-JoveWorks-Course-Token: <JOVEWORKS_COURSE_TOKEN>
```

If no course token is configured, restricted catalogue retrieval is refused.
The response is `401 Unauthorized`. A course response containing restricted
catalogues also requires that course token (or the admin token). Public
catalogues, publications, discovery, and health do not require either token.

## Common response and error rules

Successful JSON responses use `Content-Type: application/json`. Errors raised
by a Hub handler use the same content type and have exactly this shape (with a
human-readable message):

```json
{"error":"the requested resource was not found"}
```

The status meanings are:

| Status | Meaning |
| --- | --- |
| `200 OK` | Successful retrieval, or a successful catalogue upload response. |
| `201 Created` | A publication was created. |
| `204 No Content` | Health response or successful course upsert. |
| `400 Bad Request` | Invalid JSON, missing/invalid required fields, or a failed catalogue/publication validation. |
| `401 Unauthorized` | Required admin or course token is absent or wrong. |
| `404 Not Found` | The requested course, catalogue revision, or publication does not exist. |
| `409 Conflict` | A catalogue already exists at the same `(id, version)`. Catalogue revisions cannot be overwritten. |
| `500 Internal Server Error` | Storage failure. The body is `{"error":"storage failed"}`. |

Unknown routes and malformed path values may be handled by the framework's
default response. In particular, JSON extractor failures (for example,
malformed JSON or a missing JSON body) may use the framework rejection format;
they are not part of the v1 JSON error contract. Errors from the validation and
storage paths listed above use the JSON shape shown here.

## Courses

### `POST /api/v1/courses/{slug}`

Creates or updates the course at `slug`; despite the HTTP method, this is an
upsert. The request requires the admin header and has this shape:

```json
{"title":"Machine design 2026","theme":{"accent":"blue"}}
```

`title` is required and must be 1–200 characters. `theme` is optional and may
be any JSON value. Success is `204 No Content`. Updating a course changes its
title/theme but does not change or delete its publications.

### `GET /api/v1/courses`

Returns the courses available from this Hub without requiring a course slug.
It is unauthenticated and ordered case-insensitively by title, then by slug:

```json
{
  "protocolVersion":1,
  "courses":[
    {"slug":"machine-design-2026","title":"Machine design 2026","theme":{"accent":"blue"}}
  ]
}
```

`courses` is an empty array when no courses have been created. The index
contains only course-selection metadata; retrieve an individual course to get
its publications.

### `GET /api/v1/courses/{slug}`

Returns `200 OK`:

```json
{
  "protocolVersion":1,
  "slug":"machine-design-2026",
  "title":"Machine design 2026",
  "theme":{"accent":"blue"},
  "publications":[
    {"id":"Ab12Cd34Ef56","title":"Week 3","mode":"viewer","publishedAt":"2026-08-27 09:00:00"}
  ],
  "catalogues":[{"id":"public-example","version":1,"hash":"<64 lowercase hex characters>"}],
  "catalogueContents":[{"id":"public-example","version":1,"hash":"<64 lowercase hex characters>","content":{"schemaVersion":1,"id":"public-example","restricted":false,"formulas":[]}}]
}
```

`publications` is ordered newest first by the stored publication timestamp.
`catalogues` is the ID/version-sorted set of immutable revisions explicitly
pinned to the course. `catalogueContents` includes each full document, so a
client can load the complete course catalogue set in the same response.

### `GET /api/v1/courses/{slug}/catalogues`

Returns the same course-level refs and full catalogue documents without course
metadata or publication summaries:

```json
{
  "protocolVersion":1,
  "courseSlug":"machine-design-2026",
  "catalogues":[{"id":"public-example","version":1,"hash":"<64 lowercase hex characters>"}],
  "catalogueContents":[{"id":"public-example","version":1,"hash":"<64 lowercase hex characters>","content":{"schemaVersion":1,"id":"public-example","restricted":false,"formulas":[]}}]
}
```

It returns `404` for an unknown course and empty arrays for a course with no
catalogues. Restricted course bundles require the course token.

### `PUT /api/v1/courses/{slug}/catalogues`

Requires the admin header. Replaces the course's pinned catalogue set with the
provided immutable references. Every reference must already exist and match its
stored hash. An empty set is allowed only when no published NodeBook in the
course still pins a revision; this preserves the ability to open all published
course material.

## Catalogues

### `POST /api/v1/catalogues/{id}/{version}`

Requires the admin header. The positive integer `version` and URL `id` are
immutable coordinates. The request wraps the catalogue document:

```json
{"content":{"schemaVersion":1,"id":"public-example","restricted":false,"formulas":[]}}
```

The content must be a JSON object whose `id` matches the URL, has a
`schemaVersion`, and has boolean `restricted`. On success, `200 OK` returns:

```json
{"id":"public-example","version":1,"hash":"<64 lowercase hex characters>"}
```

`hash` is the SHA-256 digest of the server's compact JSON serialization of
`content`; publications must send this exact hash when pinning the revision.
An upload at an existing `(id, version)` always returns `409` even if its
content is identical.

### `GET /api/v1/catalogues/{id}/{version}`

Returns the stored catalogue `content` itself (not the upload wrapper) with
`200 OK`. A restricted entry first performs the course-token check. A missing
entry returns `404`.

### `GET /api/v1/admin/catalogues`

Requires the admin header and returns every stored immutable revision, ordered
by id and version. It powers the course catalogue checklist in `/admin`.

### `DELETE /api/v1/admin/catalogues/{id}/{version}`

Requires the admin header. Deletes an unused immutable revision. Hub rejects
the request with `409 Conflict` when any course, publication, or workspace
still references it.

## Publications

### `POST /api/v1/publications`

Requires the admin header. The request is:

```json
{
  "title":"Week 3 — belt drive",
  "mode":"viewer",
  "workspaceId":"Ab12Cd34Ef56",
  "courses":["machine-design-2026"]
}
```

`mode` is optional and defaults to `viewer`; the only values are `viewer` and
`editor`. Hub copies the workspace's exact document, catalogue pins, and
compiled report in one transaction. The stored report must be complete and
every course slug must already exist. Validation failures return `400`.

Success is `201 Created` with a newly generated random 12-character `id` and
relative resource link:

```json
{"id":"Ab12Cd34Ef56","href":"/api/v1/publications/Ab12Cd34Ef56"}
```

### `GET /api/v1/publications/{id}`

Returns `200 OK` with the immutable publication snapshot:

```json
{
  "protocolVersion":1,
  "id":"Ab12Cd34Ef56",
  "title":"Week 3 — belt drive",
  "mode":"viewer",
  "document":{"schemaVersion":1,"id":"belt-week-3"},
  "catalogues":[{"id":"public-example","version":1,"hash":"<catalogue hash>"}],
  "publishedAt":"2026-08-27 09:00:00"
}
```

Publication records are immutable and are not deleted or updated by v1.

### `GET /api/v1/publications/{id}/notebook`

Returns only the immutable, presentation-ready compiled report. It contains no
graph, expressions, catalogue content, edges, or canvas positions.

### `GET /p/{id}`

Returns `307 Temporary Redirect` to the frontend's matching `/p/{id}` route.
When Hub and JoveWorks use different origins, the redirect adds the Hub origin
as the `hub` query parameter.

## Student workspaces

Workspaces are mutable, private student graphs. Creating a workspace returns
an edit token once. The editor stores that token in
the creating browser's local storage so ordinary reloads can continue saving;
it never places it in a link or sends it while loading.

### `POST /api/v1/workspaces`

No administrator token is required. The request is:

```json
{"title":"Belt study","document":{"schemaVersion":1,"id":"belt-study"},"compiledNotebook":{"schemaVersion":1,"title":"Belt study","sections":[],"marks":[],"axisReadouts":[]}}
```

When a student starts from course material, the editor also sends its course
slug and immutable catalogue pins. Hub verifies both before storing them, then
returns them on every load so another browser can fetch the exact revisions.

`title` is required (1–200 characters); `document` must be a JSON object with
`schemaVersion` and `id`. The compiled report may contain unavailable outputs,
but is limited to 1 MiB uncompressed. Success is `201 Created`:

```json
{"id":"Ab12Cd34Ef56","editToken":"<32-character secret>"}
```

Keep `editToken` private. Hub stores only its SHA-256 digest.

### `GET /api/v1/workspaces/{id}`

Requires the workspace-token header. It returns the saved `id`, `title`,
`document`, and `updatedAt` only to the browser that owns that workspace.
Course material is shared through immutable publications, not workspaces.

### `PUT /api/v1/workspaces/{id}`

Uses the same atomic `title`/`document`/`compiledNotebook` request shape as
create and requires:

```text
X-JoveWorks-Workspace-Token: <editToken>
```

It replaces the workspace and returns its new snapshot with `200 OK`. An
unknown workspace is `404`; a missing or incorrect edit token is `401`.

### `DELETE /api/v1/workspaces/{id}`

Requires the same workspace-token header and returns `204 No Content`. It is
intended for the owning browser's workspace library; a shared reader cannot
delete someone else's work.

## Immutability, ETags, and caching

Catalogue revisions and publications are immutable. Clients should therefore
retain catalogue hashes and publication IDs as content-addressed pins and
must not assume that a later request at the same URL can replace their
content. Courses are the exception: their manifest is mutable through the
course upsert endpoint.

For every successful catalogue GET, Hub sends:

```text
ETag: "<catalogue hash>"
Cache-Control: private, max-age=0
```

The ETag is the quoted SHA-256 catalogue hash. `max-age=0` asks clients to
revalidate before reuse and `private` prevents shared/proxy caches from storing
catalogue responses. This is especially important for restricted catalogues.
Catalogue requests do not currently evaluate `If-None-Match`; clients use the
ETag for integrity comparison.

Publication source and compiled-report responses use immutable one-year cache
headers and ETags, including `304 Not Modified`. Shared workspace reports use
ETags with `Cache-Control: no-cache`, so each reuse revalidates current state.
JSON responses are gzip-compressed when the client accepts gzip. Course
manifests remain mutable and must not receive long-lived cache policy.
