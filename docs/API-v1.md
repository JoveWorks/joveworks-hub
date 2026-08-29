# JoveWorks Hub API v1

This document specifies the HTTP contract implemented by the current Hub
service. The API is JSON over HTTP and is rooted at `/api/v1`. A deployment
should be served behind HTTPS; the token headers below are shared secrets, not
an identity or authorization system.

This document is the normative contract, not the Rust implementation.
**Anyone may implement their own Hub server in any language or storage
engine.** The JoveWorks editor and any other client speak only this HTTP
contract — they do not know or care whether a Hub is the reference
implementation in this repository. An administrator who wants to run
infrastructure independent of the maintainer of this repository (for example,
to remove any dependency on someone else's hosted service, or to satisfy a
local security/compliance review) can implement everything on this page and
interoperate fully. See [Building an independent Hub](BUILD-YOUR-OWN-HUB.md)
for an implementer's checklist, the exact canonical-JSON/hash algorithm, and a
conformance script.

## Discovery and health

`GET /.well-known/joveworks` is unauthenticated and returns `200 OK`:

```json
{"protocolVersion":1,"api":"/api/v1"}
```

`GET /healthz` is unauthenticated and returns `204 No Content` with an empty
body. It is intended for a container/process health probe, not as a readiness
or catalogue check.

`protocolVersion` is how a client decides whether it can talk to a given Hub
at all: it is a single integer, currently `1`, that increments only on a
breaking change to this contract. A client should refuse to operate (rather
than guess) against a `protocolVersion` it does not recognize. Every JSON
response documented below that includes `protocolVersion` reports the same
value; there is no independent per-resource versioning.

## Authentication headers

Write requests require the exact configured administrator token in:

```text
X-JoveWorks-Admin-Token: <JOVEWORKS_ADMIN_TOKEN>
```

The protected writes are `POST /api/v1/clouds/{slug}`,
`PUT /api/v1/clouds/{slug}/catalogues`, `POST /api/v1/catalogues/{id}/{version}`,
`POST /api/v1/admin/catalogues/{version}` (the admin-console YAML/JSON
catalogue upload), `DELETE /api/v1/admin/catalogues/{id}/{version}`,
`GET /api/v1/admin/catalogues`, and `POST /api/v1/publications`. A missing,
malformed, or incorrect header is `401 Unauthorized`; the token is never
returned in a response, and comparison is an exact string match against the
single configured `JOVEWORKS_ADMIN_TOKEN` value (there are no per-admin
accounts or scopes — see [Building an independent
Hub](BUILD-YOUR-OWN-HUB.md#security-model) for what that does and does not
protect against).

Retrieving a catalogue whose stored content has `restricted: true` additionally
requires the exact configured cloud token:

```text
X-JoveWorks-Cloud-Token: <JOVEWORKS_CLOUD_TOKEN>
```

If no cloud token is configured, restricted catalogue retrieval is refused.
The response is `401 Unauthorized`. A cloud response containing restricted
catalogues also requires that cloud token (or the admin token). Public
catalogues, publications, discovery, and health do not require either token.

Student workspace endpoints use a third, per-resource header instead of the
admin or cloud token:

```text
X-JoveWorks-Workspace-Token: <editToken returned by POST /api/v1/workspaces>
```

This token is scoped to exactly one workspace (see [Student
workspaces](#student-workspaces)) and is never accepted in place of the admin
or cloud token, or vice versa.

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
| `204 No Content` | Health response or successful cloud upsert. |
| `400 Bad Request` | Invalid JSON, missing/invalid required fields, or a failed catalogue/publication validation. |
| `401 Unauthorized` | Required admin or cloud token is absent or wrong. |
| `404 Not Found` | The requested cloud, catalogue revision, or publication does not exist. |
| `409 Conflict` | A catalogue already exists at the same `(id, version)`, or an immutable revision is still referenced by a cloud, publication, or workspace and cannot be deleted. |
| `413 Payload Too Large` | The request body exceeds 1 MiB (`MAX_REQUEST_BYTES`). Plain-text body, framework-generated — not the `{"error":...}` shape. |
| `429 Too Many Requests` | The deployment-wide rate limit was exceeded (see [Rate limiting](#rate-limiting-body-size-and-cors)). Body is `{"error":"Hub is busy; try again shortly"}`. |
| `500 Internal Server Error` | Storage failure. The body is `{"error":"storage failed"}`. |

Unknown routes and malformed path values may be handled by the framework's
default response. In particular, JSON extractor failures (for example,
malformed JSON, a missing JSON body, or a JSON body that doesn't match the
expected shape) return `400 Bad Request` with a **plain-text** body
(`Content-Type: text/plain; charset=utf-8`), not the JSON error shape — a
conformant client must not assume every error response is JSON. Likewise
`413` is plain text. Errors from the validation and storage paths listed above
(cloud/catalogue/publication/workspace validation, auth, not-found, conflict)
always use the `{"error":"..."}` JSON shape shown here.

## Rate limiting, body size, and CORS

These are deployment-tunable behaviors of the reference server, not strict
protocol requirements, but a client should not assume they are absent:

- **Body size.** Requests over 1 MiB are rejected with `413` before any
  handler or auth check runs.
- **Rate limit.** The reference server allows 600 requests per minute
  *service-wide* (not per client/IP) in a fixed 60-second sliding window,
  returning `429` with `{"error":"Hub is busy; try again shortly"}` once
  exceeded. There is no `Retry-After` header.
- **CORS.** The reference server allows any origin, any header, and
  `GET`/`POST`/`PUT`/`DELETE` on every route, so a browser-based client (such
  as the standalone JoveWorks editor) can call any Hub cross-origin. A
  same-origin deployment (Hub and the editor served from one origin) does not
  need this; an independent implementation may tighten CORS to its actual
  deployed editor origin without breaking the protocol, but should keep
  responses to unauthenticated `GET` routes (discovery, health, public
  catalogues, publications) broadly fetchable, since those are the routes a
  browser-hosted client depends on before it has any credentials.

## Clouds

### `POST /api/v1/clouds/{slug}`

Creates or updates the cloud at `slug`; despite the HTTP method, this is an
upsert. The request requires the admin header and has this shape:

```json
{"title":"Machine design 2026","theme":{"accent":"blue"}}
```

`title` is required and must be 1–200 characters. `theme` is optional and may
be any JSON value. Success is `204 No Content`. Updating a cloud changes its
title/theme but does not change or delete its publications.

### `GET /api/v1/clouds`

Returns the clouds available from this Hub without requiring a cloud slug.
It is unauthenticated and ordered case-insensitively by title, then by slug:

```json
{
  "protocolVersion":1,
  "clouds":[
    {"slug":"machine-design-2026","title":"Machine design 2026","theme":{"accent":"blue"}}
  ]
}
```

`clouds` is an empty array when no clouds have been created. The index
contains only cloud-selection metadata; retrieve an individual cloud to get
its publications.

### `GET /api/v1/clouds/{slug}`

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
pinned to the cloud. `catalogueContents` includes each full document, so a
client can load the complete cloud catalogue set in the same response.

### `GET /api/v1/clouds/{slug}/catalogues`

Returns the same cloud-level refs and full catalogue documents without cloud
metadata or publication summaries:

```json
{
  "protocolVersion":1,
  "cloudSlug":"machine-design-2026",
  "catalogues":[{"id":"public-example","version":1,"hash":"<64 lowercase hex characters>"}],
  "catalogueContents":[{"id":"public-example","version":1,"hash":"<64 lowercase hex characters>","content":{"schemaVersion":1,"id":"public-example","restricted":false,"formulas":[]}}]
}
```

It returns `404` for an unknown cloud and empty arrays for a cloud with no
catalogues. Restricted cloud bundles require the cloud token.

### `PUT /api/v1/clouds/{slug}/catalogues`

Requires the admin header. Replaces the cloud's pinned catalogue set with the
provided immutable references. Every reference must already exist and match its
stored hash. An empty set is allowed only when no published NodeBook in the
cloud still pins a revision; this preserves the ability to open all published
cloud material.

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

`hash` is the SHA-256 digest of the server's **canonical** JSON serialization
of `content`; publications must send this exact hash when pinning the
revision. An upload at an existing `(id, version)` always returns `409` even
if its content is identical.

Canonicalization is: compact form (no insignificant whitespace), object keys
sorted ascending by byte value of their UTF-8 encoding (not insertion order,
not schema order), and array element order preserved exactly as submitted.
This makes the hash a function of `content` alone, independent of how the
uploading client ordered its JSON keys. For example, this content —

```json
{"restricted":false,"schemaVersion":1,"id":"public-example","formulas":[]}
```

— canonicalizes to `{"formulas":[],"id":"public-example","restricted":false,"schemaVersion":1}`
and hashes to `26037960f0c7c83c233269070cea9f199255913f188a7601c590f17a53a33aaa`
regardless of the key order in the request body. An independent
implementation that needs to reproduce identical hashes for identical content
(for example, to migrate data between Hub implementations, or for a client to
verify a hash without trusting the server's round trip) must sort keys and
use this exact separator/whitespace convention; see [Building an independent
Hub](BUILD-YOUR-OWN-HUB.md#canonical-json-and-hashing) for the full
byte-level rules (including number and string formatting).

### `POST /api/v1/admin/catalogues/{version}`

Requires the admin header. An admin-console convenience alternative to
`POST /api/v1/catalogues/{id}/{version}` for uploading a file directly: the
request body is the raw catalogue document as **JSON or YAML** text
(`Content-Type` is not inspected), and the catalogue `id` is read from the
document's own `id` field rather than the URL — only `version` is a path
parameter. This avoids requiring a browser to parse YAML merely to construct
the immutable resource URL before uploading. Otherwise it applies the exact
same validation (`schemaVersion`, `restricted`) and returns the same
`{"id","version","hash"}` shape as the JSON endpoint above, including `409`
on a duplicate `(id, version)`. Malformed JSON/YAML is `400`.

### `GET /api/v1/catalogues/{id}/{version}`

Returns the stored catalogue `content` itself (not the upload wrapper) with
`200 OK`. A restricted entry first performs the cloud-token check. A missing
entry returns `404`.

### `GET /api/v1/admin/catalogues`

Requires the admin header and returns every stored immutable revision, ordered
by id and version. It powers the cloud catalogue checklist in `/admin`.

### `DELETE /api/v1/admin/catalogues/{id}/{version}`

Requires the admin header. Deletes an unused immutable revision. Hub rejects
the request with `409 Conflict` when any cloud, publication, or workspace
still references it.

## Publications

### `POST /api/v1/publications`

Requires the admin header. The request is:

```json
{
  "title":"Week 3 — belt drive",
  "mode":"viewer",
  "workspaceId":"Ab12Cd34Ef56",
  "clouds":["machine-design-2026"]
}
```

`mode` is optional and defaults to `viewer`; the only values are `viewer` and
`editor`. Hub copies the workspace's exact document, catalogue pins, and
compiled report in one transaction. The stored report must be complete and
every cloud slug must already exist. Validation failures return `400`.

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

When a student starts from cloud material, the editor also sends its cloud
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
Cloud material is shared through immutable publications, not workspaces.

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

### `POST /api/v1/workspaces/{id}/shares`

Requires the workspace-token header for `{id}`. Creates a **read-only**,
unauthenticated share link for the current content of that workspace and
requires `JOVEWORKS_PUBLIC_URL` to be configured (`400` otherwise, since a
relative `href` alone would not tell another browser which Hub to query).
Calling it again for the same workspace is idempotent: it returns the
existing share rather than creating a second one. Success is `201 Created` on
first creation or `200 OK` when an existing share is returned:

```json
{"id":"hZDUAVO001e9","href":"/s/hZDUAVO001e9","url":"https://hub.example.edu/s/hZDUAVO001e9"}
```

A share id is a distinct, separately-allocated identifier from the workspace
id — it does not grant edit access, only read access to a live view of the
owner's current save (not a frozen snapshot: it always reflects the
workspace's latest `PUT`).

### `GET /api/v1/shares/{id}`

Unauthenticated. Returns `200 OK` with the same
[`WorkspaceDocument`](#get-apiv1workspacesid) shape as the authenticated
workspace read (`id`, `title`, `document`, `cloudSlug`, `catalogues`,
`updatedAt`) for the workspace behind that share id, or `404` if the share
does not exist. Note the returned `id` is the underlying **workspace** id, not
the share id, and it does not include (or require) the edit token — a reader
following this link can never mutate or delete the workspace.

### `GET /s/{id}`

Returns `307 Temporary Redirect` to
`{JOVEWORKS_EDITOR_URL}?hub={JOVEWORKS_PUBLIC_URL}&share={id}` (each Hub-owned
value percent-encoded), or `400` if either URL is not configured. Like
`/p/{id}`, it exists so a share link is short and stable even before an
editor's own routing is wired up; the editor is expected to read the `hub`
and `share` query parameters and call `GET /api/v1/shares/{id}` itself.

## Immutability, ETags, and caching

Catalogue revisions and publications are immutable. Clients should therefore
retain catalogue hashes and publication IDs as content-addressed pins and
must not assume that a later request at the same URL can replace their
content. Clouds are the exception: their manifest is mutable through the
cloud upsert endpoint.

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
JSON responses are gzip-compressed when the client accepts gzip. Cloud
manifests remain mutable and must not receive long-lived cache policy.
