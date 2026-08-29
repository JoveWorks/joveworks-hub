# JoveWorks Hub

JoveWorks Hub is a small, self-hosted backend for distributing immutable
catalogue versions and NodeBook publications. Clouds explicitly pin their
catalogue revisions; opening a cloud includes the full pinned catalogues. A
publication has a random short identifier and retains its own historical pins.
The graph itself never embeds formula bodies.

This is an MVP distribution API, not yet student cloud storage or live
collaboration. Writes require an administrator token. Restricted catalogues
also require a separate cloud token; Hub refuses to serve them when that token
has not been configured.

## Run locally

Rust 1.94 or newer is required.

```sh
export JOVEWORKS_ADMIN_TOKEN='a-local-admin-secret'
export JOVEWORKS_CLOUD_TOKEN='a-local-cloud-secret'
cargo run
```

The service listens on `http://127.0.0.1:8080` and creates
`joveworks-hub.sqlite` in the repository. Check that it is alive:

```sh
curl -i http://127.0.0.1:8080/healthz
curl http://127.0.0.1:8080/.well-known/joveworks
```

### Expose it on a local network

For one-command network binding, copy `.env.example` to `.env`, set the admin
token (and the cloud token when serving restricted catalogues), then run:

```sh
./scripts/run-network.sh
```

It starts the release build at `0.0.0.0:8080`. Set `JOVEWORKS_BIND` in `.env`
to choose another address or port. Allow the selected port through the host
firewall only when network access is intended. For student use, put an HTTPS
reverse proxy in front of Hub: the JoveWorks editor accepts plain HTTP only for
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

### Production HTTPS and backups

Set the public URL and editor URL in `.env`, configure your HTTPS reverse proxy
to send traffic to the loopback host port in `.env` (8083 on this server), then run:

```sh
docker compose -f compose.production.yaml up --build -d
```

The production composition publishes Hub only on the host loopback interface,
so a host Nginx reverse proxy can reach it while it remains inaccessible from
the Internet. Set `JOVEWORKS_HOST_BIND` only when your reverse proxy is on a
different host, and firewall port 8080 to that proxy. Hub limits compiled
reports to 1 MiB uncompressed, rejects whole requests above 3 MiB, and applies
a basic 600-request/minute service-wide guard. Back up
the SQLite database before upgrades. For a local database, run
`./scripts/backup-db.sh`; Docker-volume backups should use the same SQLite
`.backup` operation from a maintenance container.

For the full DNS, router/firewall, TLS, verification, and upgrade sequence,
see [the homelab deployment guide](docs/HOMELAB-DEPLOYMENT.md).

## Admin console

Open `https://your-hub.example/admin` to create clouds, upload immutable
catalogue revisions from JSON or YAML files, and publish NodeBooks without using
the shell. Enter the
administrator token for the current browser session; Hub never stores it and
the console does not persist it in browser storage. Catalogue files and
NodeBooks are read locally by the browser, then sent only to the same Hub.

Use the catalogue-library checklist to attach immutable revisions to a cloud.
The cloud’s complete pinned set is then used when publishing NodeBooks and is
included in the cloud response. A revision needed by an already-published
NodeBook cannot be removed from that cloud; publish a replacement NodeBook
before retiring material.

## API v1

Hub's HTTP contract is fully specified in [docs/API-v1.md](docs/API-v1.md).
It is a versioned, deployment-independent protocol: the JoveWorks editor and
any other client speak only this contract, so **anyone can implement their
own Hub-compatible backend**, in any language or storage engine, without
depending on this repository's server at all. If you'd rather run your own
infrastructure than trust someone else's hosted Hub for security or data
custody, see [docs/BUILD-YOUR-OWN-HUB.md](docs/BUILD-YOUR-OWN-HUB.md) for the
implementer's guide, the exact catalogue-hash algorithm, and a conformance
script (`scripts/conformance-check.sh`) that checks any Hub deployment —
this one or your own — against the documented behavior.

All write requests use this header:

```text
X-JoveWorks-Admin-Token: <JOVEWORKS_ADMIN_TOKEN>
```

Restricted catalogue downloads additionally require:

```text
X-JoveWorks-Cloud-Token: <JOVEWORKS_CLOUD_TOKEN>
```

Student workspace reads/writes/deletes require a third, per-workspace token
returned once by `POST /api/v1/workspaces` (see below).

| Endpoint | Purpose |
| --- | --- |
| `GET /healthz` | Container health probe (`204 No Content`). |
| `GET /.well-known/joveworks` | Hub discovery and protocol version. |
| `GET /api/v1/clouds` | Discover available clouds. |
| `POST /api/v1/clouds/{slug}` | Create or update a cloud. |
| `GET /api/v1/clouds/{slug}` | Cloud manifest and its publications. |
| `GET`/`PUT /api/v1/clouds/{slug}/catalogues` | Get full cloud catalogue bundle / replace its pinned revision set. |
| `GET /api/v1/admin/catalogues` | List catalogue revisions for the admin console. |
| `POST /api/v1/admin/catalogues/{version}` | Upload a JSON/YAML catalogue file, id read from the document. |
| `DELETE /api/v1/admin/catalogues/{id}/{version}` | Delete an unused catalogue revision. |
| `POST /api/v1/catalogues/{id}/{version}` | Store an immutable catalogue version. |
| `GET /api/v1/catalogues/{id}/{version}` | Retrieve that exact version. |
| `POST /api/v1/publications` | Promote a stored workspace to an immutable NodeBook snapshot. |
| `GET /api/v1/publications/{id}` | Retrieve a published NodeBook. |
| `POST /api/v1/workspaces` | Create a private student workspace; no admin token needed. |
| `GET`/`PUT`/`DELETE /api/v1/workspaces/{id}` | Load, save, or delete a workspace (requires its edit token). |
| `POST /api/v1/workspaces/{id}/shares` | Create a read-only share link for a workspace. |
| `GET /api/v1/shares/{id}` / `GET /s/{id}` | Read a shared workspace / its short redirect link. |
| `GET /api/v1/publications/{id}/notebook` | Retrieve its presentation-only compiled report. |
| `GET /api/v1/shares/{id}/notebook` | Retrieve the current compiled report for a workspace share. |
| `GET /p/{id}` / `GET /s/{id}` | Redirect to the matching JoveWorks viewer route. |

The full request/response shapes, validation rules, error contract, and
caching semantics for every route above are in
[docs/API-v1.md](docs/API-v1.md), not repeated here.

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
  "workspaceId": "<stored Hub workspace id>",
  "clouds": ["machine-design-2026"]
}
```

Each `POST /api/v1/publications` copies that workspace's exact graph, catalogue
pins, and complete compiled report into a new immutable random 12-character
publication ID. Incomplete student workspaces may be shared but cannot be
promoted. Share `https://your-hub.example/p/{id}`.

## Publish cloud material

Create the cloud and upload its catalogue revision first:

```sh
./scripts/create-cloud.sh machine-design-2026 "Machine design 2026"
```

After saving the finished workspace from JoveWorks, promote it with:

```sh
./scripts/publish-workspace.sh WORKSPACE_ID "Week 3 — belt drive" viewer machine-design-2026
```

Publishing never overwrites a catalogue revision. Every publication retains
the graph, report, and exact catalogue hashes captured by that workspace save.

## Cloud-material links

Set `JOVEWORKS_PUBLIC_URL` to Hub's public HTTPS origin and
`JOVEWORKS_EDITOR_URL` to the editor's public HTTPS origin. Hub then turns
`https://hub.example.edu/p/<publication-id>` into an editor link that opens
the immutable published NodeBook automatically. A workspace itself stays
private, browser-owned storage; a student can additionally opt in to a
read-only `https://hub.example.edu/s/<share-id>` link via
`POST /api/v1/workspaces/{id}/shares`, which reflects that one workspace's
current save and cannot be used to edit or delete it.

## Deliberate limits

- No accounts, submissions, or live collaboration yet.
- No server-side formula evaluation.
- No claim of DRM. A student who can evaluate a restricted catalogue receives
  it in their browser. The cloud token is an MVP access gate, to be replaced
  by proper cloud/identity integration before broad deployment.
