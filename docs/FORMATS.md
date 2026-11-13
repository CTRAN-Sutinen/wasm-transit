# Formats — what the store persists

The at-rest contract of the sync store (the storage model is
`docs/architecture/storage.md`). Every document is an open
format, readable without the app (D-115): plain JSON, or an external
open format (GTFS, GTFS-rt, GeoJSON). Everything here is readable
without any wasm: the blobs are plain bytes, the envelopes and log
records plain JSON, and the hash is sha256. The JSON Schemas generated from
the serde types pin the *document payloads*: `just caps` publishes
them in the registry as `registry/schemas/<kind>.v<N>.schema.json`,
each with a worked example (`registry/examples/<kind>.v<N>.json`),
indexed by `registry/documents.json` — each entry's bus `kind`, its
`store_kind` (the envelope's `kind`, when the store persists it), its
`schema_version`, and the files' hashes. A reader maps a blob's
envelope (`kind`, `schema_version`) to its schema there. This file
pins the *storage envelope* and the invariants the schema cannot say.
The manifest is the Go host's own document and has no generated
schema; its shape is below.

The store lives in object storage — S3 and S3-compatible, Azure Blob,
GCS (D-113) — as create-only objects in one bucket, under the store
URL's prefix (`-store s3://bucket/prefix[?endpoint=…]`,
`azblob://container/prefix`, `gs://bucket/prefix`, or `mem://` for a
store that ends with the process). Every key below is relative to that
prefix:

| Prefix | Holds | Written by |
|---|---|---|
| `blobs/<hex>` | one immutable blob, its envelope as metadata | serving processes (create only) |
| `pointers/<name>/<seq>.json` | one commit record | serving processes (create only) |
| `inputs/<key>` | an input servers boot from (`-feed bucket:<key>`, D-118) | the operator |
| `gc/<hex>` | a blob a `store gc` pass is collecting, until it settles | `store gc` |

No object is ever overwritten, appended to, or renamed; the engine
assumes only the backend contract in `server/store/backend.go`. A
serving process never deletes (D-117): only `store gc` does, under its
own credentials. A live store is never a local or network-mounted
directory.

## Addressing and hash

- A hash is `sha256:<hex>` of the exact bytes —
  `cap_abi::content_hash`, the formula every module and host shares.
- A blob is the object `blobs/<hex>` (raw bytes, no extension). Its
  envelope is the object's metadata, so a blob and its envelope are
  one write.
- **The module is the hash authority for bus documents** (network,
  edit records, plans, walks): the producing module returns the
  canonical bytes and their hash, and the store asserts
  `sha256(blob) == hash`, failing loudly on mismatch. Host-owned
  documents (the manifest) are canonicalized and hashed by the host
  with the same formula; no module ever claims their identity.
- Canonical serialization of a document is its serde_json bytes
  (struct field order), so `sha256(canonical bytes)` is the document's
  identity on every host.

## The envelope

The blob's metadata key `envelope` holds (S3 `x-amz-meta-envelope`):

```json
{"kind": "edit_records", "schema_version": 2, "produced_by": "cap-sync@sha256:<hex>"}
```

- `kind` — one of `network_document`, `edit_records`,
  `diversion_draft`, `manifest`, `plans`, `policy`, `stops`,
  `stop_requests`, and the boot
  sources' `gtfs_feed` (a GTFS zip) and `street_layer` (GeoJSON) —
  external open formats, stored as read (D-115, D-118). `produced_by`
  on a `network_document` is `cap-network@sha256:…`; on `plans`,
  `cap-plans@sha256:…`; on `stops`, `cap-stops@sha256:…`; on
  `stop_requests`, `cap-requests@sha256:…`.
- `schema_version` — the payload's version, the same number the
  document carries inside it (see "Versions" below); a writer bumps it
  deliberately. Migrations happen at receive, never at read.
- `produced_by` — the producing module's hash (`cap-sync@sha256:…`),
  or `server` for host-owned documents.

## The pointer log

Each commit is one record object,
`pointers/<name>/<seq, zero-padded to 20 digits>.json`:

```json
{"seq": 2, "from": "sha256:aaa…", "to": "sha256:bbb…", "kind": "edit_records", "schema_version": 2, "produced_by": "cap-sync@sha256:…", "writer": "anonymous", "policy": null, "at_ms": 1798761600000}
```

- The pointer's current value is its highest record's `to` (the last
  key a listing of `pointers/<name>/` answers) — there is no pointer
  object. Records chain from seq 1 with no gaps.
- A pointer's name is one key segment: letters, digits, `.`, `_` and
  `-`, starting with a letter or digit, at most 128 bytes. The store
  refuses any other name.
- The pointers the server keeps: `edits`, `policy`, `manifest`,
  `plans` (the published plan set), `stops` (the stops document,
  written through `POST /stops`), `stop_requests` (the stop requests
  document, written through `POST /stop-requests`), `plan.<id>` per published pick
  (its network document; every revision stays in the log), `build`
  (each release committed from `-dist`, and each move — "Builds"), and
  `source.feed` and `source.streets` — each read of `-feed` and
  `-streets` whose bytes changed, the record's `writer`
  `import:<source>` (D-118); a `stops` commit of the agency's feed's
  differences has the writer `import:feed` (D-150). A server whose source does not answer at
  boot imports that pointer's head instead, so a replacement given
  only the bucket and the same configuration serves the same manifest.
- `from` is the hash the writer last saw; a commit lands only when
  the head's `to` equals it (empty `from` = the pointer must not exist
  yet). A failed CAS changes nothing.
- `writer` is the verified JWT `sub` when authentication is on (D-58),
  else the caller's label (`X-Writer`, default `anonymous`). It leaves
  the server only through the log endpoint, and only to `audit`-granted
  callers (redacted otherwise — an agent's choice, open as D-164). `policy` (the field) stays null — the
  policy itself is the `policy` pointer's blob (D-61), and that blob is
  never served over HTTP (`/blobs` 404s kind `policy`); clients get the
  caller's projection from `GET /permissions` instead.
A commit is two creates and a check: the blob (if absent), then
record N+1 with `If-None-Match: *` — the record lands only if no
record N+1 exists, so the conditional create is the CAS, and any
number of server processes may share one bucket — then a re-read of
its blob, re-created if missing. A process caches each head and reads
at most 2 s behind another process's commit. A crash between the
creates orphans the blob.

`store gc <url>` (offline, D-117) collects orphans: blobs no record
names (a build's record names its files too — "Builds"), older than
the grace (7 days). A pass lists the records, then
the blobs; moves each orphan to `gc/<hex>` (a copy, then the delete);
then settles — lists the records again, restores every trashed blob a
record now names, and empties `gc/`. The settle step closes the race
a commit's own check cannot: a commit that reuses an aged orphan after
the pass listed the records, lands its record and passes its check,
and then has the orphan deleted under it (`server/store/store.tla`,
`store-race.cfg`). A pass that crashed leaves `gc/`; the next one
settles it first. `store verify <url>` reads a whole store without
writing and exits non-zero on the first break: a key under `pointers/`
that is no record, a gap, a record that does not chain, a named blob
that is missing, does not hash to its name, or carries no envelope.

## The HTTP surface (store mode, `-store <url>`)

- `GET /pointers/{name}` → `{"name","hash","seq","envelope","at_ms","as_of_ms"}`
  — `at_ms` the head record's commit time, `as_of_ms` when the bucket
  last confirmed it is the head; `"stale": true` while the bucket does
  not answer (below).
- `GET /pointers/{name}/log?since=<hash>` → `{"entries":[…]}` — the
  records after the latest record whose `to` is `since`; an unknown `since`
  answers `{"entries": [], "current": <head>, "whole": true}` and the
  client takes the whole current document.
- `POST /pointers/{name}` body `{"from": "<hash>", …}` → `200` with the
  new head, or `409` with `{"current": "<hash>"}`. For
  `edit_records` the body is `{"from", "client": <document>}` and the
  server runs cap-sync's `sync_edits` (receive-clamped, D-56) and
  stores the canonical converged bytes; other kinds take
  `{"from", "document": <value>}` verbatim.
- `GET /blobs/{hash}` → the raw bytes; ETag is the full `sha256:…`
  hash (304 on match); `X-Document-Kind` and `X-Schema-Version` carry
  the envelope. With `Accept: application/gzip` the same bytes come
  gzipped as `Content-Type: application/gzip` (transport only, never
  `Content-Encoding`), for a page that registers a document
  compressed; `GET /blobs/{hash}.gz` answers the same by path, the
  layout a static bundle carries (docs/architecture/static-deployment.md). A `policy` blob is never served.
- `POST /plans` body: a network document, any serialization, gzipped
  or not → `200 {plan_id, network_hash, plans_hash, warnings}` (the
  network's warnings, then, with a stops document, each stop it visits
  not in service over its period — `stop_not_in_service`), `422
  {issues, refusals}` (the network does not validate, or runs no
  service in its period — D-76) or `422 {issues}` (cap-plans refuses
  the pick: the chain, or a new, restored or moved pick dated before
  the server's today), or `400`.
- `POST /stops` body `{"from": "<hash or empty>", "stops": <document>}` →
  `200 {stops_hash}`, `422 {refusals}` (cap-stops `validate`), or `409
  {current, seq}`. The server commits the module's canonical document
  to `stops`; no pick changes (D-157). `POST /pointers/stops` is `400`.
- `POST /stop-requests` body `{"from": "<hash or empty>", "requests":
  <document>}` → `200 {requests_hash}`, `422 {refusals}` (cap-requests
  `validate`), or `409 {current, seq}`. The server commits the module's
  canonical document to `stop_requests`. `POST /pointers/stop_requests`
  is `400`.
- `POST /attachments` body the file, `Content-Type` its media type (a
  photo — JPEG, PNG, WebP, HEIC — or a PDF, at most 25 MB) → `200
  {hash, type, bytes}`, `413`, `415`, or `501 {configured: false}`
  (no `-attachments`). `GET /attachments/sha256:<hex>` → the bytes with
  their type (signed-in only when sign-in is on: `401` otherwise). The
  attachments' bucket holds `attachments/<hex>`, each object's media
  type in its metadata; a record names a file by `sha256:<hex>`.
- `POST /stop-requests/work-order` body `{"request", "action": "open" |
  "refresh"}` → `200 {requests_hash, work_order}` (the work order
  system's answer on the request, the log's writer
  `work-orders:<system>`), `501 {configured: false}` with no adapter
  (`-work-orders`), `404`, `409 {current, seq}`, or `422 {refusals}`.
- `GET /feed?date=YYYY-MM-DD` → the GTFS zip in force on the date
  (default the server's today; `application/zip`, ETag its hash):
  cap-network `export` of the pick in force, carrying the stops'
  history over its period (cap-stops `export_versions`), or the `-feed`
  zip before the first pick takes effect.
- `POST /plans/retire` body `{"id"}` → `200 {plans_hash}`, or `422
  {issues}` (the pick has taken effect, or no pick has the id), `404`
  when nothing is published (D-75). The server commits the module's canonical
  network to `plan.<id>`, then cap-plans' next set to `plans`.
- `POST /builds` body `{"build": "<hash>"}` → `200 {build, seq}`:
  moves the `build` pointer to a build its log holds — a rollback, or
  forward again; `404` for a build the log never held, `409` on a
  moved head. Builds land only from the server's `-dist` and move only
  here: `POST /pointers/build` (and any `build` or `build_file` kind)
  is `400`.
- `/edits` is the `edit_records` specialization: `GET` reads the head
  blob, `POST` runs the merge with a CAS retry loop, so the legacy
  whole-document protocol is unchanged.
- **While the bucket does not answer** (D-119) a server serves the
  heads and blobs it holds — blobs are immutable, so a cached one is
  always correct — and labels every read whose head the bucket did not
  confirm: `X-Store-Stale: true` and `X-Store-As-Of: <Unix ms>` (when
  the bucket last confirmed it) on `/manifest`, `/edits` and
  `/pointers/…`. No age makes a read fail; the pages show the age. A
  write is never acknowledged: `503` with `Retry-After`, and the client
  holds its document and resends. A read the server holds nothing for
  is `503`.

## Builds

A release is a build (D-80): every file the pages load is a blob of
kind `build_file` (its exact bytes, envelope
`{"kind":"build_file","schema_version":1,"produced_by":"server"}`), and
a `build` document names them, with the modules the server hosts
retained beside them:

```json
{"kind":"build","schema_version":1,"files":[{"path":"index.html","hash":"sha256:…","type":"text/html; charset=utf-8"},{"path":"sw.js","hash":"sha256:…","type":"text/javascript; charset=utf-8"}],"server":[{"role":"sync","hash":"sha256:…"}]}
```

Files are in path order and modules in role order, so one tree is one
document and one hash. A path is the file's path under the web root;
the server serves it at `/<path>` with `type`. The `build` pointer logs
every build the server committed and every move (`POST /builds`); the
manifest's `build_hash` is its head.

A `build` blob is the one kind that names other blobs, and the store
reads it: a build record names its document AND every hash the
document lists. GC keeps them (`store gc`), and `store verify` checks
each like any named blob. A build commit creates every file, then the
document and its record, then re-reads every file and re-creates one a
GC pass removed in between, so a logged build is always whole. (The
model-checked protocol, `server/store/store.tla`, is the one-blob
commit; a build runs that order for each of its files.)

## Invariants the schema cannot say

- Dates cross every boundary as `"YYYY-MM-DD"` strings; times as u32
  seconds since service-day midnight (values past 86400 are
  legitimate — after-midnight trips); the store's own timestamps are
  `at_ms`, Unix epoch milliseconds from the host clock.
- The edit document's LWW order is a strict total order: later
  `updated_at` wins, then higher `rev`, then the lexicographically
  greater canonical record. The receive path clamps
  `updated_at = min(updated_at, now_ms)` (D-56), so the server clock
  bounds the order and redelivery is idempotent.
- An edit record carries exactly one payload, `diversion` or `cancel`
  (D-45); the absent one is omitted, never `null`, so a diversion
  record's bytes — and every edit document's hash — are the ones they
  were before the cancel payload existed (D-101). The LWW unit is the
  whole record: a later write may replace a diversion with a cancel
  under one id. A reader drops a record with neither or both.
- `status: "retired"` is the only tombstone: the record stays in the
  document, is no longer in force, and deletions propagate through
  sync. Compaction (D-57) drops retired records older than the horizon
  as an ordinary logged commit.
- Nothing compressed is ever the durable format; compression is
  transport only.

## Versions

Every document says what it is inside it (D-62, D-115), so a copy
anywhere — a blob, an export, a file another tool reads — names its
kind and version without the app:

- `network` and `edit_records` lead with `"kind"` and
  `"schema_version"` (network v2, edit document v2). A network with no
  `schema_version` is v1 — every shape written before the tag; a bare
  array of edit records is edit document v1.
- The byte kinds — a GTFS feed, a GTFS-rt snapshot, a GeoJSON layer —
  are open formats versioned by their own specs and carry no tag of
  ours.

Every persisted kind, by what a version change does to it:

| Kind | Owner | Class | Inside it today |
|---|---|---|---|
| `edit_records` | transit-sync | authored | `kind`, `schema_version` |
| `network` (authored or forked) | transit-network | authored | `kind`, `schema_version` |
| `plans` (the published set) | transit-plans | authored | `kind`, `schema_version` |
| `plan_set` (a browser's stored plans) | transit-plans | authored | each plan's network |
| `watchlist` | transit-dispatch | authored | `"version": "vN"` |
| `policy` | transit-authz | authored | `kind`, `schema_version` |
| `work_rules` | transit-runs | authored | `kind`, `version` |
| `stops` | transit-stops | authored | `kind`, `schema_version` |
| `stop_requests` | transit-requests | authored | `kind`, `schema_version` |
| `diversion_draft` | transit-dispatch | authored | — (never persisted yet) |
| a rider's device records (recent places, live-feed URLs) | the ride-now host | authored | — (`localStorage`) |
| `network` (imported) | transit-codec | derived | `kind`, `schema_version` |
| `boundary` | transit-codec | derived | `kind`, `schema_version` |
| `streets` (imported) | transit-streets | derived | `kind`, `schema_version` |
| `walks` | transit-streets | derived | `kind`, `schema_version` |
| `live` | transit-live | ephemeral | `kind`, `schema_version` (v2: a trip instance names its service `date`) |
| `manifest` | the Go server | host-owned | its own shape (below) |
| `build`, `build_file` | the Go server | host-owned | its own shape ("Builds", below) |

- *Authored* kinds are written by a person and cannot be re-derived:
  their chains are kept forever (D-66).
- *Derived* kinds are a pure function of other documents. A version
  change re-derives them from the inputs the server holds. The tag is
  there so a reader refuses a mismatch instead of misreading it.
- *Ephemeral* kinds never outlive a session. They are tagged for the
  same refusal.
- The *host-owned* manifest and builds are versioned in their own
  envelope.

A copy written before its kind's tags reads as that kind: an absent
`kind` is the kind itself, and an absent version is v1 (the network's
and the edit document's untagged v1 are older versions; every other
kind gained its tags at v1, the version it already was). Every
kernel load and every request field that takes one of these
documents checks the version the same way
(`cap_abi::parse_versioned`, `cap_abi::Current`; the tag type is
transit-version's `kind_tag!`). The watchlist's own `"version": "vN"`
is read by the kernel in both versions it has had.

A module reads the version it was built for, and only that: an older
document is refused at its load naming the migration capability, a
newer one naming both versions — never parsed and re-emitted. The
migration capability (cap-migrate) brings an older document forward,
one pure step per version, every record kept and every loss reported;
the answer is a new document under its own hash, and the old bytes
stay. It runs where a document arrives:

- the server's receive: an older `POST` body comes forward before the
  merge, and at startup an older `edits` head is committed forward as
  an ordinary logged CAS, writer `migrate:<module hash>` (the older
  blob stays under its hash);
- a browser's restore (service design brings a stored plan forward);
- an agent's load: `caps-run --load` of an older file is refused
  naming the op, which the agent runs through `--cap migrate`.

A new version rolls out server first. The server's modules learn it
first: they accept the old and the new version at receive and store
the new one. The manifest then pins those modules, and clients follow
the manifest (D-63). Every client gets its modules from that manifest,
so no client writes a version the server does not read.

The kernel's own version is separate from the documents'. It bumps
only when an existing kernel op changes meaning. A new kernel op is
listed in the manifest's `kernel_ops`, so a host checks for the op by
name, not the kernel number.

Persisted data includes URLs. The stop display's `stop` and `layout`
parameters are configured into screens and printed into QR codes, so
the URL only ever gains optional parameters. A parameter the page does
not know is ignored and kept when the page rewrites its URL.

## Worked examples

An `edit_records` blob is the canonical edit document — its kind and
version, then its records:

```json
{"kind":"edit_records","schema_version":2,"records":[{"id":"e1","diversion":{"id":"d1","route_id":"10","direction_id":null,"first_kept_stop":"A","last_kept_stop":"B","skipped_stops":["C"],"detour":null,"added_secs":300,"start_date":"2026-10-23","end_date":"2026-10-23"},"status":"submitted","rev":2,"updated_at":1000}]}
```

and a trip cancel (one service date; `start_secs` null = every
instance the trip runs that day) is a record of its own:

```json
{"id":"e2","cancel":{"id":"cancel-t10","trip_id":"t10","start_secs":null,"date":"2026-10-23"},"status":"draft","rev":1,"updated_at":2000}
```

with envelope
`{"kind":"edit_records","schema_version":2,"produced_by":"cap-sync@sha256:<hex>"}`
and, after a second commit, log record
`{"seq":2,"from":"sha256:<first>","to":"sha256:<this>","kind":"edit_records","schema_version":2,"produced_by":"cap-sync@sha256:<hex>","writer":"anonymous","policy":null,"at_ms":2000}`.

A `manifest` blob is the manifest object itself
(`{"network_hash","feed_hash","feed_version","streets_hash","walks_hash","wasm_hash","authz_hash","policy_hash","plans_hash","stops_hash","stop_requests_hash","build_hash","generated_at","publisher","timezone","services","auth"}`,
the optional ones omitted when empty),
envelope `{"kind":"manifest","schema_version":1,"produced_by":"server"}`.

A `plans` blob is the published plan set, its version inside it
(D-62), picks in (effective_from, id) order; a retired pick stays in
it as the tombstone (D-75; `status` reads `published` when absent):

```json
{"schema_version":1,"plans":[{"id":"fall-2026","name":"Fall 2026 pick","effective_from":"2026-09-13","network_hash":"sha256:5206f419…","status":"published"},{"id":"winter-2027","name":"Winter 2027 pick","effective_from":"2027-03-01","network_hash":"sha256:d122b6af…","status":"retired"}]}
```

with envelope `{"kind":"plans","schema_version":1,"produced_by":"cap-plans@sha256:<hex>"}`;
each `network_hash` is the head of that pick's `plan.<id>` pointer, a
`network_document` blob.
