# JoveWorks Hub

JoveWorks Hub is a small, self-hosted backend for distributing immutable
catalogue versions and NodeBook publications. A publication has a random short
identifier; it stores a graph document and pins the catalogue versions needed
to open it. The graph itself never embeds formula bodies.

This is an MVP distribution API, not yet student cloud storage or live
collaboration. Writes require an administrator token. Restricted catalogues
also require a separate course token; Hub refuses to serve them when that token
has not been configured.

## Run locally

Rust 1.94 or newer is required.

```sh
export JOVEWORKS_ADMIN_TOKEN='a-local-admin-secret'
export JOVEWORKS_COURSE_TOKEN='a-local-course-secret'
cargo run
```

The service listens on `http://127.0.0.1:8080` and creates
`joveworks-hub.sqlite` in the repository. Check that it is alive:

```sh
curl -i http://127.0.0.1:8080/healthz
curl http://127.0.0.1:8080/.well-known/joveworks
```

### Expose it from WSL

For one-command network binding, copy `.env.example` to `.env`, set the admin
token (and the course token when serving restricted catalogues), then run:

```sh
./scripts/run-network.sh
```

It starts the release build at `0.0.0.0:8080`. Set `JOVEWORKS_BIND` in `.env`
to choose another address or port. Windows can normally reach the service at
`http://localhost:8080`; use WSL mirrored networking plus the relevant Windows
firewall rule to accept LAN traffic. For student use, put an HTTPS reverse
proxy in front of Hub: the JoveWorks editor accepts plain HTTP only for
`localhost`.

## Run with Docker Compose

Copy `.env.example` to `.env`, replace both secrets with generated random
values, then run:

```sh
docker compose up --build
```

The named Docker volume `joveworks-data` holds the SQLite database. Back it up
before upgrading the container. Put the public deployment behind HTTPS; an
opaque publication ID is an identifier, never permission to retrieve a
restricted catalogue.

## API v1

All write requests use this header:

```text
X-JoveWorks-Admin-Token: <JOVEWORKS_ADMIN_TOKEN>
```

Restricted catalogue downloads additionally require:

```text
X-JoveWorks-Course-Token: <JOVEWORKS_COURSE_TOKEN>
```

| Endpoint | Purpose |
| --- | --- |
| `GET /healthz` | Container health probe (`204 No Content`). |
| `GET /.well-known/joveworks` | Hub discovery and protocol version. |
| `POST /api/v1/courses/{slug}` | Create or update a course. |
| `GET /api/v1/courses/{slug}` | Course manifest and its publications. |
| `POST /api/v1/catalogues/{id}/{version}` | Store an immutable catalogue version. |
| `GET /api/v1/catalogues/{id}/{version}` | Retrieve that exact version. |
| `POST /api/v1/publications` | Publish an immutable NodeBook snapshot. |
| `GET /api/v1/publications/{id}` | Retrieve a published NodeBook. |
| `GET /p/{id}` | Short human-facing link; currently redirects to its API resource. |

A catalogue upload body wraps the existing catalogue JSON:

```json
{ "content": { "schemaVersion": 1, "id": "public-example", "name": "Example", "restricted": false, "formulas": [] } }
```

The response gives the SHA-256 hash that a publication must pin. The server
does not permit overwriting a catalogue at the same `(id, version)`.

Publication request shape:

```json
{
  "title": "Week 3 — belt drive",
  "mode": "viewer",
  "document": { "schemaVersion": 1, "id": "belt-week-3" },
  "catalogues": [
    { "id": "public-example", "version": 1, "hash": "<hash returned by catalogue upload>" }
  ],
  "courses": ["machine-design-2026"]
}
```

Each `POST /api/v1/publications` creates a new immutable random 12-character
publication ID. Share `https://your-hub.example/p/{id}`. Until the editor is
wired to Hub, that short route redirects to the immutable JSON resource; the
link and API contract will not change when it starts serving the NodeBook
viewer.

## Deliberate limits

- No accounts, submissions, personal storage, or live collaboration yet.
- No server-side formula evaluation.
- No claim of DRM. A student who can evaluate a restricted catalogue receives
  it in their browser. The course token is an MVP access gate, to be replaced
  by proper course/identity integration before broad deployment.
