# Building an independent Hub

JoveWorks Hub is a protocol, not a service you have to trust. This document
is for an administrator — an instructor, a department, an institution — who
wants to run course material and student workspaces on infrastructure they
control end to end, instead of depending on someone else's hosted Hub for
security, uptime, or data custody. It explains what you need to implement,
what you're free to change, and how to check your own implementation against
the reference behavior.

Nothing here requires Rust, SQLite, or any code from this repository. The
normative contract is [API-v1.md](API-v1.md); this page is the practical
walkthrough for building something that speaks it.

## Why this is possible

The JoveWorks editor and any other client only ever talk to a Hub over the
HTTP contract in [API-v1.md](API-v1.md). It discovers a Hub via
`GET /.well-known/joveworks`, checks `protocolVersion`, and from then on
issues plain HTTP requests. It has no special knowledge of this codebase —
no shared library, no bundled client SDK, no hidden handshake. Anything that
answers those requests correctly *is* a Hub, as far as every client is
concerned.

That means the choice to depend on one specific hosted Hub is exactly
that — a choice, made for convenience, not a technical requirement. If you'd
rather not have your course material, your students' workspace tokens, or
your restricted-catalogue access depend on someone else's server, database,
and operational discipline, you can run your own and point your editor at
it.

## What you must match exactly

For the JoveWorks editor (or any client written against API-v1) to work
against your Hub without modification, these must be byte-for-byte
compatible with [API-v1.md](API-v1.md):

- Every route path, method, and status code in that document.
- The `{"error":"..."}` JSON error shape for the errors it lists as using
  that shape (not for framework-level rejections like malformed JSON or an
  oversized body — those may be plain text, per the spec).
- The three auth headers (`X-JoveWorks-Admin-Token`,
  `X-JoveWorks-Course-Token`, `X-JoveWorks-Workspace-Token`) and exactly
  which routes require which.
- The request/response JSON field names and types (the spec uses
  `camelCase` throughout).
- The catalogue hash algorithm — see [Canonical JSON and
  hashing](#canonical-json-and-hashing) below. Get this wrong and every
  publication that pins a catalogue hash will fail validation.
- Immutability: a catalogue `(id, version)` and a publication `id`, once
  created, must never change content. Clients are entitled to cache and pin
  against that assumption.

## What you're free to change

Everything not listed above is an implementation detail:

- **Language and framework.** The reference server is Rust/axum; yours can
  be anything that can serve HTTP and parse/emit JSON.
- **Storage.** SQLite, Postgres, a document store, files on disk — the spec
  describes wire behavior, not a schema. (The reference schema is in
  `migrations/*.sql` if you want a starting point, not because you're bound
  to it.)
- **The `/admin` console.** `GET /admin` in the reference server is a static
  HTML page that happens to be convenient; it is not part of the API
  contract, isn't linked from any client, and you can replace it with your
  own tooling, a CLI, or nothing at all.
- **Rate limiting numbers, CORS policy, and body-size limits**, within the
  bounds noted in [API-v1.md's rate-limiting
  section](API-v1.md#rate-limiting-body-size-and-cors) — these protect your
  deployment, they aren't something a client depends on having a specific
  value.
- **Everything about *how* you generate and store secrets**, as long as the
  header contract and comparison semantics (exact match) are preserved. See
  below for concrete recommendations.

## Suggested build order

Each phase is independently testable with `curl` before you write the next
one, and roughly matches how much of the contract you need before something
useful works end to end.

1. **Discovery and health.** `GET /.well-known/joveworks` and `GET
   /healthz`. Nothing else depends on these existing, but every client
   checks discovery first.
2. **Courses.** `POST /api/v1/courses/{slug}`, `GET /api/v1/courses`,
   `GET /api/v1/courses/{slug}`. Get the admin-token gate working here — it's
   reused by everything else.
3. **Catalogues**, including the [canonical hash](#canonical-json-and-hashing).
   This is the part most worth writing a unit test for before moving on,
   because a hash bug won't surface as an obvious error — it'll surface as
   publications silently failing catalogue-pin validation.
4. **Course catalogue pins** (`PUT /api/v1/courses/{slug}/catalogues`) and
   restricted-catalogue access via the course token.
5. **Publications.** These depend on catalogues existing and validating, so
   build them after step 3.
6. **Student workspaces and shares.** Independent of courses/catalogues
   except for the optional course-material binding fields; can be built any
   time after step 2.

At each phase, run [`scripts/conformance-check.sh`](../scripts/conformance-check.sh)
(see [Conformance checking](#conformance-checking) below) — it's organized in
the same order and will tell you concretely what's missing rather than
leaving you to infer it from the prose spec.

## Canonical JSON and hashing

`POST /api/v1/catalogues/{id}/{version}` returns a `hash`, and every
publication or course pin that references that catalogue must supply the
exact same value. The hash is SHA-256 over a **canonical** JSON encoding of
the catalogue's `content`, defined as:

1. Compact form: no whitespace between tokens (no spaces after `:` or `,`,
   no newlines).
2. Object keys sorted ascending by the byte value of their UTF-8 encoding
   (for ASCII keys, this is plain alphabetical order). This is the one rule
   that differs from "however your JSON library happens to serialize an
   object" — most libraries preserve insertion order by default, which is
   *not* what the reference server does (it uses Rust's `BTreeMap`, which
   sorts keys).
3. Array element order preserved exactly as given — arrays are never
   reordered.
4. Strings escaped per standard JSON string escaping (`"`, `\`, and control
   characters below `0x20`); everything else, including non-ASCII
   characters, emitted as literal UTF-8 rather than `\uXXXX` escapes.
5. Numbers formatted as the shortest decimal representation that round-trips
   exactly: integers as bare digits with no leading zeros or `+`/decimal
   point, floating-point values using a minimal round-trippable decimal form
   (e.g. `1.0`, not `1`, for a float that is exactly one). Catalogue schemas
   in practice avoid floats for exactly this reason — prefer integers or
   strings for anything that must hash reproducibly.

Concretely, given this upload body (deliberately out of alphabetical order,
to demonstrate that upload order doesn't matter):

```json
{"content":{"restricted":false,"schemaVersion":1,"id":"public-example","formulas":[]}}
```

the canonical form is:

```json
{"formulas":[],"id":"public-example","restricted":false,"schemaVersion":1}
```

and the hash is:

```
26037960f0c7c83c233269070cea9f199255913f188a7601c590f17a53a33aaa
```

You can reproduce this in one line with Python (`json.dumps(...,
sort_keys=True, separators=(',', ':'))` implements the same rule for
non-float content) or the equivalent in your language — most standard JSON
libraries offer a "sort keys" option; pair it with a compact/no-whitespace
mode. [`scripts/conformance-check.sh`](../scripts/conformance-check.sh)
computes this independently and checks it against your server's response,
which is the fastest way to know whether your canonicalization is right.

Reproducing the *exact* reference hash for identical content only matters if
you need cross-implementation agreement — for example, migrating catalogues
from one Hub implementation to another while preserving existing publication
pins, or a client that wants to verify a hash without trusting the server's
round trip. If you're running a standalone Hub that only ever talks to
clients that trust whatever hash *it* returns, you technically only need
your own hashing to be internally consistent — but matching the canonical
form exactly costs nothing and keeps the door open for both cases, so do it
anyway.

## Security model

Hub has three secrets, each with a narrow, specific job. None of them is an
identity system — there are no accounts, sessions, or per-user permissions
anywhere in the protocol.

- **Admin token** (`X-JoveWorks-Admin-Token`) gates every write except
  creating a workspace: courses, catalogues, publications, and the admin
  catalogue-management routes. It is a single deployment-wide secret. Anyone
  who has it can publish, republish, or repoint course material for the
  whole deployment. Treat it like a root credential: generate it with real
  entropy (`openssl rand -hex 32`, as `.env.example` suggests), keep it out
  of version control and client-side storage, and rotate it if it's ever
  exposed. There is no scoping below "has full write access" — if you need
  per-course or per-instructor write boundaries, that's a real gap in the
  current protocol, not a configuration option; you'd need to extend the
  contract (and update any client you control accordingly) to add it.
- **Course token** (`X-JoveWorks-Course-Token`) gates reading a catalogue
  whose content has `restricted: true`. It is explicitly documented in this
  project as an MVP access gate, not real access control: anyone who has the
  token — a student who received it through the normal course channel, or
  anyone they forward it to — can read every restricted catalogue on the
  deployment. If a restricted catalogue's confidentiality matters more than
  "keeps it out of casual/accidental public reach," don't rely on this token
  alone; put real identity-based access control (institutional SSO, LMS
  integration) in front of it, or don't mark the content restricted in a
  Hub that lacks that.
- **Workspace edit token** is different in kind from the other two: it's a
  *bearer capability* scoped to one workspace, generated per-workspace, and
  never chosen by an admin. Whoever creates a workspace gets it back exactly
  once in the `POST /api/v1/workspaces` response; the server stores only its
  SHA-256 digest (mirroring password-hash practice, even though this is a
  random high-entropy token rather than a user-chosen password), so it
  cannot be reconstructed from your database even by someone with full
  database access. Generate it with a real CSPRNG and enough length that
  guessing is infeasible (the reference implementation uses 32 random
  alphanumeric characters, i.e. roughly 190 bits of entropy) — don't be
  tempted to shorten it for convenience.

A hardening note for anyone implementing this from scratch: the reference
server compares all three tokens with ordinary string equality, not a
constant-time comparison. For high-entropy secrets delivered over HTTPS this
is a low-risk simplification (a timing side channel over the network is a
hard attack to mount, and none of these tokens double as short user-chosen
passwords), but if your threat model includes a lower-latency attacker (for
example, tokens also checked by something colocated on the same host or
LAN), use a constant-time comparison for all three header checks. It costs
nothing and removes the question entirely.

Two things the protocol deliberately does **not** give you, so don't assume
your own implementation has them just because it's compatible: server-side
formula evaluation (a restricted catalogue's content is fully readable by
anyone who can pass the course-token check — there's no redaction or
evaluation-without-disclosure), and any DRM claim on published NodeBooks or
catalogues — "restricted" is a distribution gate, not an enforcement
mechanism against a student who has already legitimately received the
content.

## Conformance checking

[`scripts/conformance-check.sh`](../scripts/conformance-check.sh) is a
black-box test suite you can run against *any* Hub — this reference server
or your own from-scratch implementation — to check it against the documented
contract:

```sh
./scripts/conformance-check.sh https://your-hub.example your-admin-token your-course-token
```

It creates its own randomly-suffixed course, catalogue, publication, and
workspace, exercises the auth rules, verifies the canonical-hash computation
against an independently computed reference value, and cleans up after
itself — it's safe to run against a real deployment with existing data. Run
it after each phase in [Suggested build order](#suggested-build-order); a
failing line names the exact endpoint and expected-vs-actual status, which
is usually enough to find the gap without re-reading the whole spec.

The script only covers what's mechanically checkable from the outside
(status codes, header enforcement, hash correctness, response shapes). It
cannot check things like whether your storage is actually durable, whether
your rate limiter holds up under real load, or whether your secrets are
generated with enough entropy — those are on you.

## Configuration surface worth exposing

Not part of the wire contract, but every one of these showed up as a real
operational need in this project, and you'll likely want equivalents:

| Reference env var | Purpose |
| --- | --- |
| `JOVEWORKS_ADMIN_TOKEN` | The admin secret. Required; a Hub should refuse to start without one rather than run with unprotected writes. |
| `JOVEWORKS_COURSE_TOKEN` | The restricted-catalogue secret. Optional — when unset, restricted catalogues should be refused entirely rather than silently served. |
| `JOVEWORKS_DATABASE_URL` / equivalent | Where persistent state lives. |
| `JOVEWORKS_BIND` | Listen address, so loopback-only and network-exposed deployments are one setting apart. |
| `JOVEWORKS_PUBLIC_URL`, `JOVEWORKS_EDITOR_URL` | Needed only for the short-link redirect routes (`/p/{id}`, `/s/{id}`) and workspace-share creation; everything else works without them. |

## Once your Hub exists

Nothing about switching Hubs requires this repository's cooperation:
generate your own admin/course tokens, point the JoveWorks editor's Hub
address at your deployment's `JOVEWORKS_PUBLIC_URL`, and publish course
material exactly as described in the main [README](../README.md). Your
users' trust in their course material and workspace data now rests on your
operational security, not a third party's — which, if that's what motivated
building your own Hub in the first place, is the point.
