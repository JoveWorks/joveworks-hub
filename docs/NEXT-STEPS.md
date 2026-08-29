# From Hub MVP to cloud sharing

This plan turns the present prototype into a service students can use to open
cloud material through a short link. Its order matters: publishable,
versioned cloud content comes before accounts and student cloud storage.

## Current baseline

Hub stores immutable catalogue versions and published NodeBook snapshots in
SQLite. It has a small JSON API, random publication IDs, an administrator
token for writes, and a separate shared token for restricted catalogue reads.
The editor can connect to a named cloud, download a publication and the exact
catalogue revisions it pins, and open it as a local document.

The limits are intentional: `/p/{id}` currently redirects to JSON; cloud
selection requires knowing a slug; there is no author publishing flow, account
system, server-side student work, or HTTPS deployment path yet.

## Phase 1 — Stabilise the publication contract

Goal: an instructor can rely on a published revision remaining reproducible.

1. Move Hub's `CREATE TABLE IF NOT EXISTS` setup into numbered SQL migrations,
   applied on startup and tested against a fresh database and an upgrade.
2. Specify API v1 as a versioned, checked-in contract: discovery, cloud
   index, publication, catalogue, error shape, ETag semantics, and cache
   policy.
3. Strengthen publication validation. A publish request must parse its graph
   and catalogues with the same schema rules as the editor, verify every graph
   formula reference against its pinned catalogue, and reject missing or
   mismatched references.
4. Add end-to-end tests covering cloud creation, catalogue upload, publication
   creation, short-link lookup, a restricted-catalogue refusal, and a
   successful authenticated open.

Done when: a test can publish a NodeBook, retrieve it from its public ID, and
prove that each formula reference resolves only against the declared catalogue
revision.

## Phase 2 — Instructor publishing workflow

Goal: publishing does not require hand-writing HTTP requests.

1. Build an instructor-only command-line client (`hubctl` or Hub subcommands)
   that uploads a validated catalogue revision, creates/updates a cloud, and
   publishes a NodeBook from a `.jove.json` file.
2. Print the immutable short publication URL on success and support a dry run
   that shows the catalogue versions and hashes which will be pinned.
3. Keep real R&M catalogue files in their private catalogue repository. The
   public Hub repository contains only generic code, fixtures with invented
   formulae, and documentation.
4. Record simple publication metadata: author, cloud, timestamp, title, and
   an optional explanatory note. Do not add mutable editing of a published
   revision; correcting material creates a new publication.

Done when: an instructor can publish a cloud NodeBook with one command and
paste the returned link into the LMS.

## Phase 3 — Link-based cloud viewer

Goal: a student can click a short link and read the intended NodeBook on any
device without first configuring the editor.

1. Add `GET /api/v1/clouds` so an editor that knows only a Hub address can
   show available clouds; remove the cloud-slug requirement from the normal
   student connection flow.
2. Teach the editor's read-only NodeBook viewer to fetch a Hub publication,
   download its pinned catalogues, and render the generic document rather than
   only bundled examples.
3. Make `/p/{id}` a real reader route. Preferred deployment: serve the static
   JoveWorks build and Hub API from one HTTPS origin, so the short route loads
   the reader directly. A temporary alternative can redirect to the static app
   with the Hub origin and publication ID as parameters; the document itself
   never enters the URL.
4. For a viewer-mode publication, offer **Open a copy in editor**. It makes a
   local editable copy and never changes the instructor's published snapshot.
5. Keep mobile read-only. The desktop editor remains the only surface for
   wiring and direct graph edits.

Done when: opening `https://hub.example/p/abc123` renders a mobile-friendly,
read-only NodeBook and its interactive exposed sliders.

## Phase 4 — Real cloud access control

Goal: restricted cloud content is available to enrolled students, not merely
to anyone who receives a shared secret.

1. Decide the identity source before implementation: institutional OIDC, LMS
   launch/LTI, or a short-term managed cloud-login service. Do not build a
   bespoke password-account system unless that decision explicitly calls for
   it.
2. Introduce users, cloud memberships, and roles (`instructor`, `student`).
   Replace the browser-held cloud token with short-lived authenticated access.
3. Retain the current shared token only as an explicit transition mode for
   local/private testing, with a documented expiry/rotation plan.
4. Restrict CORS to the deployed JoveWorks origin; retain HTTPS everywhere;
   add rate limiting and audit entries for publishing and restricted-content
   reads.

Done when: an unenrolled browser cannot retrieve a restricted catalogue, while
an enrolled student can open a cloud link without manually copying a secret.

## Phase 5 — Student storage and submissions

Goal: students can keep and hand in their own work without altering cloud
material.

1. Add private document records owned by a user, with immutable revisions and
   an optimistic version token for save-conflict detection.
2. **Fork** copies a published NodeBook into private student storage; cloud
   publications remain immutable and never become shared edit buffers.
3. Add submissions as a reference to one chosen private revision, timestamped
   and locked for the instructor's review policy. A submission is not a live
   collaboration channel.
4. Add a student library and an instructor submission list. Export/download
   remains available as a portable fallback.

Done when: a student can fork a lab, save revisions across devices, and submit
one reproducible revision to the cloud.

## Phase 6 — Deployment and operations

Goal: Hub is maintainable as a real service rather than a laptop process.

1. Put Hub behind Caddy or another HTTPS reverse proxy on a stable hostname;
   run the Rust process/container only on the internal network.
2. Back up SQLite before every upgrade and restore-test those backups. Stay on
   SQLite while one small Hub instance is sufficient; plan a PostgreSQL move
   only when concurrent writes, hosting topology, or operational limits demand
   it.
3. Add structured logs, health/readiness checks, metrics, data-retention rules,
   and an upgrade procedure for migrations.
4. Document DNS, reverse-proxy, firewall, environment-secret, and backup setup
   in an operator guide. The network launcher remains a development/demo
   convenience, not the classroom production deployment.

Done when: a fresh server can be deployed, restored from backup, upgraded, and
observed without depending on one developer's local machine.

## Decisions to make before Phase 4

- Which institution/LMS identity system is available?
- Is the first real cloud deployed on a university-controlled hostname, a
  personal server, or another hosting provider?
- What is the required submission workflow: export-only, deadline-based Hub
  submissions, or LMS upload of a Hub revision link?
- Is any material public, or should every cloud and publication require
  enrolment from the start?

## Explicit non-goals for this sequence

- Real-time collaborative graph editing.
- Server-side formula evaluation or a CAS.
- Embedding catalogue formula bodies in NodeBooks, publication URLs, or the
  public JoveWorks repository.
