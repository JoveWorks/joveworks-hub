# Feature review

Hub is a content-distribution backend, not yet a complete course platform or
personal-project cloud.

| Area | Present in backend | Missing for real use |
| --- | --- | --- |
| Course catalogue | Courses; course discovery; immutable, hash-pinned catalogue versions; public/restricted catalogue reads | Full catalogue/schema validation; per-course access control |
| Instructor publishing | Admin-protected course/catalogue/publication API; shell helpers to create a course and publish a NodeBook | Author/note metadata, dry run, multi-catalogue publishing helper, richer validation |
| Course material | Immutable publications with short IDs and course association | Editor viewer integration; `/p/{id}` is still a redirect, not a reader page |
| Student work | Anonymous private workspaces, protected by an edit token; update/delete; course catalogue pins | User identity, workspace library/listing, cross-device recovery, revision history/conflict handling |
| Sharing | Owner can create one public read-only share link per workspace | Revoke/rotate shares, expiry, access policy, snapshot sharing |
| Deployment | Docker Compose, SQLite migrations, health endpoint, rate guard, Nginx deployment path | Backup/restore automation, readiness, metrics, audit trail, retention policy |
| Security | Admin token; optional global restricted-catalogue token; workspace edit tokens | OIDC/LTI/LMS login, users/roles/course enrolment, restricted CORS, per-user rate limiting |

## Current model

- **Publications** are immutable course material.
- **Workspaces** are mutable anonymous personal documents.
- There are no users yet, so workspace privacy means that the browser retaining
  the workspace ID and edit token can access it. It is not an account-backed
  private library.

## Course integration

The immediate course path is:

1. Integrate the editor with `GET /api/v1/courses`, publication retrieval, and
   pinned-catalogue retrieval.
2. Build the read-only publication viewer and make `/p/{id}` open it.
3. Add **Open a copy in editor**, creating a student workspace that retains
   the publication's catalogue pins.
4. For an initial pilot, use public catalogues, or explicitly treat the current
   shared course token as a temporary access gate.
5. Before broadly serving restricted material, choose institutional OIDC or
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
those catalogues. Implement this before treating course publications as fully
reproducible.

## Recommended implementation order

1. Editor publication viewer and fork into a workspace.
2. Stronger NodeBook, catalogue, and formula-reference validation.
3. Identity decision and course-enrolment integration.
4. Account-backed personal-project library and revisions.
5. Submissions and instructor review.
