# Feature review

Hub is a content-distribution backend, not yet a complete cloud platform or
personal-project cloud.

| Area | Present in backend | Missing for real use |
| --- | --- | --- |
| Cloud catalogue | Clouds; cloud discovery; immutable, hash-pinned catalogue versions; public/restricted catalogue reads | Full catalogue/schema validation; per-cloud access control |
| Instructor publishing | Admin-protected cloud/catalogue/publication API; shell helpers to create a cloud and publish a NodeBook | Author/note metadata, dry run, multi-catalogue publishing helper, richer validation |
| Cloud material | Immutable publications with short IDs and cloud association; editor viewer integration; `/p/{id}` redirects into the editor with the publication opened | The publication response does not expose which clouds a publication belongs to, so a copy saved from a public `/p/{id}` link cannot be bound to its cloud or retain its catalogue pins |
| Student work | Anonymous private workspaces, protected by an edit token; update/delete; cloud catalogue pins | User identity, workspace library/listing, cross-device recovery, revision history/conflict handling |
| Sharing | Owner can create one public read-only share link per workspace | Revoke/rotate shares, expiry, access policy, snapshot sharing |
| Deployment | Docker Compose, SQLite migrations, health endpoint, rate guard, Nginx deployment path | Backup/restore automation, readiness, metrics, audit trail, retention policy |
| Security | Admin token; optional global restricted-catalogue token; workspace edit tokens | OIDC/LTI/LMS login, users/roles/cloud enrolment, restricted CORS, per-user rate limiting |

## Current model

- **Publications** are immutable cloud material.
- **Workspaces** are mutable anonymous personal documents.
- There are no users yet, so workspace privacy means that the browser retaining
  the workspace ID and edit token can access it. It is not an account-backed
  private library.

## Cloud integration

The editor now integrates with `GET /api/v1/clouds`, cloud catalogue
retrieval, publication retrieval, and pinned-catalogue retrieval, and `/p/{id}`
redirects into the editor with the publication opened, both when browsed from
inside the app and via a public link. Saving a copy from a publication opened
inside the app creates a student workspace that retains the publication's
catalogue pins. A copy saved from a public `/p/{id}` link does not, and the
reason is a missing piece of the API rather than an oversight in the editor.

A public link carries only a Hub address and a publication id. The editor
therefore synthesises a placeholder cloud for that path and deliberately
skips the binding (`packages/editor/src/App.tsx:651-652`), because the Hub
rejects a workspace whose cloud slug does not exist — `validate_workspace_binding`
returns "cloud '…' does not exist" (`src/main.rs:1172`). Binding the
placeholder would not preserve the pins; it would make saving fail outright.
Nor can the pins travel alone: the same function refuses catalogue pins that
arrive without a cloud slug (`src/main.rs:1164`).

The blocker is that `get_publication` (`src/main.rs:1005`) does not return the
publication's cloud association at all, so the editor has no way to learn the
real slug from a public link. What remains:

1. Expose a publication's clouds in the publication response, then bind the
   public `/p/{id}` path to the real cloud slug so a saved copy keeps its
   catalogue pins. The Hub change must come first.
2. For an initial pilot, use public catalogues, or explicitly treat the current
   shared cloud token as a temporary access gate.
3. Before broadly serving restricted material, choose institutional OIDC or
   LMS/LTI and implement users, memberships, and roles.

## Personal projects

The workspace API is useful as a lightweight browser-owned project store and
supports read-only sharing. It does not cover projects that must survive device
changes, lost local storage, graduation, or collaboration.

That requires:

- authenticated users;
- a **My projects** library;
- documents with immutable revisions and conflict/version tokens;
- fork-from-publication; and
- optional submissions that pin a selected revision.

## Important correctness gap

Publication creation currently verifies basic document fields and the hashes of
its pinned catalogues. It does **not** inspect graph formula references against
those catalogues. Implement this before treating cloud publications as fully
reproducible.

## Recommended implementation order

The editor's publication viewer and the `/p/{id}` open path are done; what
remains of that first step is the cloud binding a public link cannot yet
carry.

1. Expose a publication's clouds in the publication response, then bind the
   public `/p/{id}` path to the real cloud slug so a saved copy keeps its
   catalogue pins.
2. Stronger NodeBook, catalogue, and formula-reference validation — the
   correctness gap above is the substantial part of this.
3. Identity decision and cloud-enrolment integration.
4. Account-backed personal-project library and revisions.
5. Submissions and instructor review.
