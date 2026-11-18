# Formats — what the store persists

This file is the at-rest contract of the sync store. The storage model
is in `docs/architecture/storage.md`. Every document is an open format,
and a reader needs no app to read it (D-115). A document is plain JSON,
or an external open format (GTFS, GTFS-rt, GeoJSON). Nothing here needs
wasm to read. The blobs are plain bytes. The envelopes and log records
are plain JSON. The hash is sha256.

The JSON Schemas pin the *document payloads*. Each version of each kind
has a schema and a worked example, frozen in the schema lock ("The
schema lock", below). `just caps` publishes them in the registry as
`registry/schemas/<tag>.v<N>.schema.json` and
`registry/examples/<tag>.v<N>.json`. The `<tag>` is the `kind` a
document carries inside it, so a reader that holds only a document
finds its schema. Each schema's `$id` is `urn:transit-document:<tag>:v<N>`,
which names no host. `registry/documents.json` indexes them. Each entry
holds:

- the document's `tag`, and its bus `kind` (the manifest's `speaks`);
- its `store_kind` (the envelope's `kind`, when the store persists it);
- its `owner`: the crate that owns its type, or `server` for a kind the
  Go host owns;
- its `schema_version`, and `written`: true for the version this build
  writes, false for an older version that it reads only through a
  migration;
- `locked`: false for a kind that has no lock yet (the watchlist);
- the files and their hashes.

A reader maps a blob's envelope (`kind`, `schema_version`) to its
schema there. This file pins the *storage envelope* and the invariants
that the schema cannot say.
The Go host owns two kinds that no Rust type generates. The `build`
document has a hand-written schema, locked in `server/schemas/` beside
a frozen example. `server/schema_test.go` checks it against the Go
type and against a build the server commits, and the index lists it
with the owner `server`. The manifest's schema is the HTTP contract's
(`server/openapi.json`). Its shape is below.

The store lives in object storage: S3 and S3-compatible, Azure Blob, or
GCS (D-113). The store holds create-only objects in one bucket, under
the store URL's prefix. The store URL is one of these:

- `-store s3://bucket/prefix[?endpoint=…]`
- `azblob://container/prefix`
- `gs://bucket/prefix`
- `mem://`, for a store that ends with the process

Every key below is relative to that prefix:

| Prefix | Holds | Written by |
|---|---|---|
| `blobs/<hex>` | one immutable blob, its envelope as metadata | serving processes (create only) |
| `pointers/<name>/<seq>.json` | one commit record | serving processes (create only) |
| `inputs/<key>` | an input servers boot from (`-feed bucket:<key>`, D-118) | the operator |
| `gc/<hex>` | a blob a `store gc` pass is collecting, until it settles | `store gc` |

No object is ever overwritten, appended to, or renamed. The engine
assumes only the backend contract in `server/store/backend.go`. A
serving process never deletes (D-117). Only `store gc` deletes, under
its own credentials. A live store is never a local or network-mounted
directory.

## Addressing and hash

- A hash is `sha256:<hex>` of the exact bytes. It is
  `cap_abi::content_hash`, the formula that every module and host
  shares.
- A blob is the object `blobs/<hex>` (raw bytes, no extension). Its
  envelope is the object's metadata, so a blob and its envelope are
  one write.
- **The module is the hash authority for bus documents** (network, edit
  records, plans, walks). The producing module returns the canonical
  bytes and their hash. The store asserts `sha256(blob) == hash` and
  fails loudly on a mismatch. The host canonicalizes and hashes
  host-owned documents (the manifest) with the same formula. No module
  ever claims their identity.
- The canonical serialization of a document is RFC 8785, the JSON
  Canonicalization Scheme (D-169): every object's keys sorted by their
  UTF-16 code units, no whitespace, numbers as ECMAScript writes them,
  and only `"`, `\`, and control characters escaped. So
  `sha256(canonical bytes)` is the document's identity on every host,
  and any implementation of the standard, in any language, computes the
  same identity for the same content (`cap_abi::jcs`).
- A number in a document is an IEEE double, as RFC 8785 reads it. A
  whole number above 2^53 loses its exact value in another tool. The
  documents hold none: times are seconds, counts are small, and ids are
  strings.
- An op's answer that carries a document a host stores or serves (an
  import's network and boundary, the streets, the walk table, a pick
  read over the stops) carries the document's canonical bytes exactly
  (`cap_abi::Canonical`).

## The envelope

The blob's metadata key `envelope` holds (S3 `x-amz-meta-envelope`):

```json
{"kind": "edit_records", "schema_version": 2, "produced_by": "cap-sync@sha256:<hex>"}
```

- `kind` is one of these:
  - `network_document`
  - `edit_records`
  - `diversion_draft`
  - `manifest`
  - `plans`
  - `policy`
  - `stops`
  - `stop_requests`
  - the boot sources' `gtfs_feed` (a GTFS zip) and `street_layer`
    (GeoJSON). These are external open formats, stored as read (D-115,
    D-118).

  `produced_by` on a `network_document` is `cap-network@sha256:…`. On
  `plans`, it is `cap-plans@sha256:…`. On `stops`, it is
  `cap-stops@sha256:…`. On `stop_requests`, it is
  `cap-requests@sha256:…`.
- `schema_version` is the payload's version. It is the same number that
  the document carries inside it (see "Versions" below). A writer bumps
  it deliberately. Migrations happen at receive, never at read.
- `produced_by` is the producing module's hash (`cap-sync@sha256:…`),
  or `server` for host-owned documents.

## The pointer log

Each commit is one record object,
`pointers/<name>/<seq, zero-padded to 20 digits>.json`:

```json
{"seq": 2, "from": "sha256:aaa…", "to": "sha256:bbb…", "kind": "edit_records", "schema_version": 2, "produced_by": "cap-sync@sha256:…", "writer": "anonymous", "policy": null, "at_ms": 1798761600000}
```

- The pointer's current value is its highest record's `to`. This is the
  last key that a listing of `pointers/<name>/` answers. There is no
  pointer object. Records chain from seq 1 with no gaps.
- A pointer's name is one key segment. It has letters, digits, `.`,
  `_` and `-`, starts with a letter or digit, and has at most 128
  bytes. The store refuses any other name.
- The server keeps these pointers:
  - `edits`.
  - `policy`.
  - `manifest`.
  - `plans` (the published plan set).
  - `stops` (the stops document, written through `POST /stops`).
  - `stop_requests` (the stop requests document, written through
    `POST /stop-requests`).
  - `plan.<id>` for each published pick (its network document). Every
    revision stays in the log.
  - `build` (each release committed from `-dist`, and each move;
    see "Builds").
  - `build.<id>` for each UI the server keeps (`-ui-dist`): each of
    that UI's bundles, and each move (D-171).
  - `source.feed` and `source.streets`. Each read of `-feed` and
    `-streets` whose bytes changed commits a record. The record's
    `writer` is `import:<source>` (D-118). A `stops` commit of the
    agency's feed's differences has the writer `import:feed` (D-150).

  A server whose source does not answer at boot imports that pointer's
  head instead. So a replacement that has only the bucket and the same
  configuration serves the same manifest.
- `from` is the hash that the writer last saw. A commit lands only when
  the head's `to` equals it. An empty `from` means that the pointer must
  not exist yet. A failed CAS changes nothing.
- `writer` is the verified JWT `sub` when authentication is on (D-58).
  Otherwise it is the caller's label (`X-Writer`, default `anonymous`).
  It leaves the server only through the log endpoint, and only to
  `audit`-granted callers. Other callers get it redacted (an agent's
  choice, open as D-164).
- `policy` (the field) stays null. The policy itself is the `policy`
  pointer's blob (D-61). The server never serves that blob over HTTP
  (`/blobs` 404s kind `policy`). Clients get the caller's
  projection from `GET /permissions` instead.

A commit is two creates and a check:

1. Create the blob, if it is absent.
2. Create record N+1 with `If-None-Match: *`. The record lands only if
   no record N+1 exists. So the conditional create is the CAS, and any
   number of server processes may share one bucket.
3. Re-read the blob. Re-create it if it is missing.

A process caches each head. It reads at most 2 s behind another
process's commit. A crash between the creates orphans the blob.

`store gc <url>` (offline, D-117) collects orphans. An orphan is a blob
that no record names and that is older than the grace (7 days). A
build's record names its files too (see "Builds"). A pass runs these
steps:

1. List the records, then the blobs.
2. Move each orphan to `gc/<hex>` (a copy, then the delete).
3. Settle: list the records again, restore every trashed blob that a
   record now names, and empty `gc/`.

The settle step closes a race that a commit's own check cannot close. A
commit can reuse an aged orphan after the pass listed the records. That
commit lands its record and passes its check. Then the pass deletes the
orphan under it (`server/store/store.tla`, `store-race.cfg`). A pass
that crashed leaves `gc/`. The next pass settles it first.

`store verify <url>` reads a whole store without writing. It exits
non-zero on the first break. A break is one of these:

- a key under `pointers/` that is no record;
- a gap;
- a record that does not chain;
- a named blob that is missing, does not hash to its name, or carries
  no envelope.

## The HTTP surface (store mode, `-store <url>`)

- `GET /pointers/{name}` answers
  `{"name","hash","seq","envelope","at_ms","as_of_ms"}`. `at_ms` is the
  head record's commit time. `as_of_ms` is when the bucket last
  confirmed that the record is the head. The answer has `"stale": true`
  while the bucket does not answer (below).
- `GET /pointers/{name}/log?since=<hash>` answers
  `{"name", "entries", "since_found", "current", "whole"}`. The entries
  are the records after the latest record whose `to` is `since`. An
  unknown `since` answers `"entries": null`, `"since_found": false`,
  and `"whole": true`, and the client takes the whole current document
  (`current`, the head's hash).
- `POST /pointers/{name}` takes the body `{"from": "<hash>", …}`. It
  answers `200` with the new head, or `409` with
  `{"current": "<hash>"}`. For `edit_records` the body is
  `{"from", "client": <document>}`. The server runs cap-sync's
  `sync_edits` (receive-clamped, D-56) and stores the canonical
  converged bytes. Other kinds take `{"from", "document": <value>}`
  verbatim.
- `GET /blobs/{hash}` answers the raw bytes. The ETag is the full
  `sha256:…` hash (304 on match). `X-Document-Kind` and
  `X-Schema-Version` carry the envelope. With
  `Accept: application/gzip`, the same bytes come gzipped as
  `Content-Type: application/gzip`. This is transport only, never
  `Content-Encoding`, for a page that registers a document compressed.
  `GET /blobs/{hash}.gz` answers the same by path. This is the layout
  that a static bundle carries
  (docs/architecture/static-deployment.md). The server never serves a
  `policy` blob.
- `POST /plans` takes a network document as the body, in any
  serialization, gzipped or not. It answers one of these:
  - `200 {plan_id, network_hash, plans_hash, warnings}`. The warnings
    are the network's warnings. With a stops document, they also name
    each stop that the pick visits and that is not in service over its
    period (`stop_not_in_service`).
  - `422 {issues, refusals}`: the network does not validate, or it runs
    no service in its period (D-76).
  - `422 {issues}`: cap-plans refuses the pick. Either the chain is
    wrong, or a new, restored or moved pick is dated before the
    server's today.
  - `400`.
- `POST /stops` takes the body `{"from": "<hash or empty>", "stops":
  <document>}`. It answers one of these:
  - `200 {stops_hash}`.
  - `422 {refusals}` (cap-stops `validate`).
  - `409 {current, seq}`.

  The server commits the module's canonical document to `stops`. No
  pick changes (D-157). `POST /pointers/stops` answers `400`.
- `POST /stop-requests` takes the body `{"from": "<hash or empty>",
  "requests": <document>}`. It answers one of these:
  - `200 {requests_hash}`.
  - `422 {refusals}` (cap-requests `validate`).
  - `409 {current, seq}`.

  The server commits the module's canonical document to
  `stop_requests`. `POST /pointers/stop_requests` answers `400`.
- `POST /attachments` takes the file as the body, with its media type
  as `Content-Type`. The file is a photo (JPEG, PNG, WebP, HEIC) or a
  PDF, at most 25 MB. It answers `200 {hash, type, bytes}`, `413`,
  `415`, or `501 {configured: false}` (no `-attachments`).
  `GET /attachments/sha256:<hex>` answers the bytes with their type.
  When sign-in is on, only a signed-in caller may read them, and any
  other caller gets `401`. The attachments' bucket holds
  `attachments/<hex>`, with each object's media type in its metadata. A
  record names a file by `sha256:<hex>`.
- `POST /stop-requests/work-order` takes the body
  `{"request", "action": "open" | "refresh"}`. It answers one of these:
  - `200 {requests_hash, work_order}`. The `work_order` is the work
    order system's answer on the request. The log's writer is
    `work-orders:<system>`.
  - `501 {configured: false}` with no adapter (`-work-orders`).
  - `404`.
  - `409 {current, seq}`.
  - `422 {refusals}`.
- `GET /feed?date=YYYY-MM-DD` answers the GTFS zip in force on the date.
  The default date is the server's today. The type is
  `application/zip`, and the ETag is its hash. The zip is cap-network
  `export` of the pick in force. It carries the stops' history over its
  period (cap-stops `export_versions`). Before the first pick takes
  effect, the answer is the `-feed` zip.
- `POST /plans/retire` takes the body `{"id"}`. It answers `200
  {plans_hash}`, or `422 {issues}` (the pick has taken effect, or no
  pick has the id), or `404` when nothing is published (D-75). The
  server commits cap-plans' next set to `plans`. The pick's network
  stays where it is.
- `POST /builds` takes the body `{"build": "<hash>"}`. It answers `200
  {build, seq}`. It moves the `build` pointer to a build that its log
  holds. With `?ui=<id>`, it moves that UI's pointer, `build.<id>`, and
  nothing else (`400` for an id outside a-z, 0-9, and -). The move is a rollback, or forward again. It answers `404` for
  a build that the log never held, and `409` on a moved head. Builds
  land only from the server's `-dist` and move only here.
  `POST /pointers/build` (and any `build` or `build_file` kind) answers
  `400`.
- `/edits` is the `edit_records` specialization. `GET` reads the head
  blob. `POST` runs the merge with a CAS retry loop. So the legacy
  whole-document protocol is unchanged.
- **While the bucket does not answer** (D-119), a server serves the
  heads and blobs that it holds. Blobs are immutable, so a cached blob
  is always correct. The server labels every read whose head the bucket
  did not confirm. The label is `X-Store-Stale: true` and
  `X-Store-As-Of: <Unix ms>` (when the bucket last confirmed it), on
  `/manifest`, `/edits` and `/pointers/…`. No age makes a read fail.
  The pages show the age. The server never acknowledges a write. It
  answers `503` with `Retry-After`, and the client holds its document
  and resends. A read that the server holds nothing for answers `503`.

## Builds

A release is a build (D-80). Every file that the pages load is a blob
of kind `build_file` (its exact bytes, envelope
`{"kind":"build_file","schema_version":1,"produced_by":"server"}`). A
`build` document names the files, with the modules that the server
hosts retained beside them:

```json
{"kind":"build","schema_version":1,"files":[{"path":"index.html","hash":"sha256:…","type":"text/html; charset=utf-8"},{"path":"sw.js","hash":"sha256:…","type":"text/javascript; charset=utf-8"}],"server":[{"role":"sync","hash":"sha256:…"}]}
```

Files are in path order and modules are in role order, so one tree is
one document and one hash. A path is the file's path under the web
root. The server serves the file at `/<path>` with `type`. The `build`
pointer logs every build that the server committed and every move
(`POST /builds`). The manifest's `build_hash` is its head. A UI's
bundle (`-ui-dist`) is a build of the same shape on `build.<id>`, with
no `server` modules. Its host serves it at the UI's own URL.

A `build` blob is the one kind that names other blobs, and the store
reads it. A build record names its document and every hash that the
document lists. GC keeps them (`store gc`), and `store verify` checks
each one like any named blob. A build commit runs these steps:

1. Create every file.
2. Create the document and its record.
3. Re-read every file, and re-create one that a GC pass removed in
   between.

So a logged build is always whole. The model-checked protocol
(`server/store/store.tla`) is the one-blob commit. A build runs that
order for each of its files.

## Invariants the schema cannot say

- Dates cross every boundary as `"YYYY-MM-DD"` strings. Times are u32
  seconds since service-day midnight. Values past 86400 are legitimate
  (after-midnight trips). The store's own timestamps are `at_ms`, Unix
  epoch milliseconds from the host clock.
- The edit document's LWW order is a strict total order. The later
  `updated_at` wins. Then the higher `rev` wins. Then the
  lexicographically greater canonical record wins. The receive path
  clamps `updated_at = min(updated_at, now_ms)` (D-56). So the server
  clock bounds the order, and redelivery is idempotent.
- An edit record carries exactly one payload, `diversion` or `cancel`
  (D-45). The absent payload is omitted, never `null`. So a diversion
  record's bytes, and every edit document's hash, are the ones they
  were before the cancel payload existed (D-101). The LWW unit is the
  whole record. A later write may replace a diversion with a cancel
  under one id. A reader drops a record with neither payload or with
  both.
- `status: "retired"` is the only tombstone. The record stays in the
  document and is no longer in force. Deletions propagate through sync.
  Compaction (D-57) drops retired records older than the horizon as an
  ordinary logged commit.
- Nothing compressed is ever the durable format. Compression is
  transport only.

## Versions

Every document says what it is inside it (D-62, D-115). So a copy
anywhere names its kind and version without the app. A copy can be a
blob, an export, or a file that another tool reads.

- Each of our kinds carries `"kind"` and `"schema_version"` inside it,
  except the watchlist (the table says which field). A network with no
  `schema_version` is v1. This is every shape written before the tag. A
  bare array of edit records is edit document v1. A browser's plan set
  written before its tag is a bare map of plans by store id, and a page
  reads it as the set.
- The byte kinds are a GTFS feed, a GTFS-rt snapshot, and a GeoJSON
  layer. They are open formats that their own specs version, and they
  carry no tag of ours.

Every persisted kind, by what a version change does to it:

| Kind | Owner | Class | Inside it today |
|---|---|---|---|
| `edit_records` | transit-sync | authored | `kind`, `schema_version` |
| `network` (authored or forked) | transit-network | authored | `kind`, `schema_version` |
| `plans` (the published set) | transit-plans | authored | `kind`, `schema_version` |
| `plan_set` (a browser's stored plans, by store id) | transit-network | authored | `kind`, `schema_version`; each plan's network its own |
| `watchlist` | transit-dispatch | authored | `"version": "vN"` |
| `policy` | transit-authz | authored | `kind`, `schema_version` |
| `work_rules` | transit-runs | authored | `kind`, `schema_version` |
| `stops` | transit-stops | authored | `kind`, `schema_version` |
| `stop_requests` | transit-requests | authored | `kind`, `schema_version` |
| `diversion_draft` | transit-dispatch | authored | no tag (never persisted yet) |
| a rider's device records (recent places, live-feed URLs) | the ride-now host | authored | no tag (`localStorage`) |
| `network` (imported) | transit-codec | derived | `kind`, `schema_version` |
| `boundary` | transit-codec | derived | `kind`, `schema_version` |
| `streets` (imported) | transit-streets | derived | `kind`, `schema_version` |
| `walks` | transit-streets | derived | `kind`, `schema_version` |
| `live` | transit-live | ephemeral | `kind`, `schema_version` (v2: a trip instance names its service `date`) |
| `manifest` | the Go server | host-owned | its own shape (below); its schema is the HTTP contract's |
| `build`, `build_file` | the Go server | host-owned | `kind`, `schema_version` in a build (a hand-written schema, "Builds", below); a build file is its exact bytes |

- A person writes *authored* kinds, and the system cannot re-derive
  them. Their chains are kept forever (D-66). Every record of an
  authored kind keeps the fields this build does not know through a read
  and a write (D-167), and its schema says so
  (`"additionalProperties": true`). So an added optional field needs no
  new version, and a writer on an older build keeps it. A value (a
  point, a stop's two times, a set of weekdays) is not a record and
  takes no unknown field.
- *Derived* kinds are a pure function of other documents. A version
  change re-derives them from the inputs that the server holds. The tag
  is there so that a reader refuses a mismatch instead of misreading
  it.
- *Ephemeral* kinds never outlive a session. They are tagged for the
  same refusal.
- The *host-owned* manifest and builds are versioned in their own
  envelope.

A gate (cap-abi `tests/formats_inventory_test.rs`) holds this table
and the locks to the same kinds. A kind in the table with no locked
schema names its reason there: the watchlist is locked once it carries
its kind, a diversion draft is never persisted yet, the manifest's
schema is the HTTP contract's, and a build file is its exact bytes.

A copy written before its kind's tags reads as that kind. An absent
`kind` is the kind itself, and an absent version is v1. The network's
and the edit document's untagged v1 are older versions. Every other
kind gained its tags at v1, the version it already was. Every kernel
load checks the version the same way. So does every request field that
takes one of these documents (`cap_abi::parse_versioned`,
`cap_abi::Current`; the tag type is transit-version's `kind_tag!`). The
kernel reads the watchlist's own `"version": "vN"` in both versions
that it has had.

A module reads the version it was built for, and only that version. A
module refuses an older document at its load, and the refusal names the
migration capability. It refuses a newer document too, and that refusal
names both versions. A module never parses and re-emits a document of
another version. The migration capability (cap-migrate) brings an older
document forward, one pure step per version. It keeps every record and
reports every loss. The answer is a new document under its own hash,
and the old bytes stay. It runs where a document arrives:

- The server's receive: an older `POST` body comes forward before the
  merge. At startup, an older `edits` head is committed forward as an
  ordinary logged CAS, with the writer `migrate:<module hash>`. The
  older blob stays under its hash.
- A browser's restore (service design brings a stored plan forward).
- An agent's load: `caps-run --load` of an older file is refused. The
  refusal names the op, which the agent runs through `--cap migrate`.

A new version rolls out server first. The server's modules learn it
first. They accept the old and the new version at receive and store the
new one. The manifest then pins those modules, and clients follow the
manifest (D-63). Every client gets its modules from that manifest, so
no client writes a version that the server does not read.

### The schema lock

The schemas are the agency's published spec for its data. Rust is the
one source of each schema. The lock is what the build has promised.
For each version of each kind, the crate that owns the kind keeps three
files in its `locks/` folder:

- `<tag>.v<N>.schema.json`: the JSON Schema of that version.
- `<tag>.v<N>.example.json`: a worked example in its canonical bytes
  (RFC 8785).
- `<tag>.lock.json`: the index, with the hash of each file.

`just caps` (gen-manifest) compares the build with the locks before it
writes anything, and fails on any difference. So `just test` fails
too. These are the failures:

- The version that the build writes has no lock.
- The serde type of the current version changed, and the frozen
  example shows it. The example does not read, it has a field the type
  does not know (a field was removed or renamed), or it writes other
  bytes. Other bytes are a new hash for every document of the kind, so
  they are a version change too.
- The frozen example of an older version does not migrate to the
  current version.
- A lock file does not match its index. Never edit a lock by hand.
- A version newer than the build is locked.

To change a kind, bump its version and add its migration step, then
run `just schema-lock`. That freezes the new version and keeps the
older ones. One change needs no bump (D-167): an added optional field
that writes nothing when it is absent. The frozen example then still
reads to the same bytes. `just caps` fails until `just schema-lock`
records the new schema in the same version. `just schema-lock` refuses
every other change.

Two more gates hold the schemas to the bytes the code really writes,
where `schemars` and serde can disagree (`skip_serializing_if`,
`untagged`, `flatten`). Both check with the locks in the tree:

- The real-output gate (cap-abi `tests/real_output_test.rs`, in
  `cargo test`) runs every capability's worked examples through its own
  dispatch over the fixtures. Every document an answer holds must match
  its locked schema, and every kind a worked example's recorded answer
  holds must be checked from a real answer.
- The real-data soak (`just soak`, cap-abi `tests/real_data_soak.rs`)
  runs the agency's feed through the ops a server runs on it (import,
  publish). A mismatch is a QA finding.

A deployment serves the schemas at `/schemas/`, from its page build:
the index `documents.json`, with paths relative to it, and each
version's schema and example. A rollback serves the schemas of the build
it moves to. A blob's response names its schema in a `Link` header
(`rel="describedby"`). The index holds each header's value, by a stored
blob's envelope and by a published document's tag, so the server only
copies it. A static export carries the same files.

Nothing was deployed before the lock, so each kind's baseline is the
version it had then. The older versions of the network and the edit
document have no lock. Their chains stay, and their own law tests pin
them. The watchlist is not locked until it carries its `kind`.

The kernel's own version is separate from the documents' versions. It
bumps only when an existing kernel op changes meaning. A new kernel op
is listed in the manifest's `kernel_ops`. So a host checks for the op
by name, not by the kernel number.

Persisted data includes URLs. The stop display's `stop` and `layout`
parameters are configured into screens and printed into QR codes. So
the URL only ever gains optional parameters. A page ignores a parameter
that it does not know, and keeps it when the page rewrites its URL.

## Worked examples

An `edit_records` blob is the canonical edit document: its kind and
version, then its records:

```json
{"kind":"edit_records","schema_version":2,"records":[{"id":"e1","diversion":{"id":"d1","route_id":"10","direction_id":null,"first_kept_stop":"A","last_kept_stop":"B","skipped_stops":["C"],"detour":null,"added_secs":300,"start_date":"2026-10-23","end_date":"2026-10-23"},"status":"submitted","rev":2,"updated_at":1000}]}
```

A trip cancel is a record of its own. It covers one service date.
A `start_secs` of null means every instance that the trip runs that
day:

```json
{"id":"e2","cancel":{"id":"cancel-t10","trip_id":"t10","start_secs":null,"date":"2026-10-23"},"status":"draft","rev":1,"updated_at":2000}
```

The envelope is
`{"kind":"edit_records","schema_version":2,"produced_by":"cap-sync@sha256:<hex>"}`.
After a second commit, the log record is
`{"seq":2,"from":"sha256:<first>","to":"sha256:<this>","kind":"edit_records","schema_version":2,"produced_by":"cap-sync@sha256:<hex>","writer":"anonymous","policy":null,"at_ms":2000}`.

A `manifest` blob is the manifest object itself
(`{"network_hash","feed_hash","feed_version","streets_hash","walks_hash","wasm_hash","authz_hash","policy_hash","plans_hash","stops_hash","stop_requests_hash","build_hash","generated_at","publisher","timezone","services","auth"}`,
the optional ones omitted when empty). Its envelope is
`{"kind":"manifest","schema_version":1,"produced_by":"server"}`.

A `plans` blob is the published plan set, with its version inside it
(D-62). The picks are in (effective_from, id) order. A retired pick
stays in the set as the tombstone (D-75; `status` reads `published`
when absent):

```json
{"schema_version":1,"plans":[{"id":"fall-2026","name":"Fall 2026 pick","effective_from":"2026-09-13","network_hash":"sha256:5206f419…","status":"published"},{"id":"winter-2027","name":"Winter 2027 pick","effective_from":"2027-03-01","network_hash":"sha256:d122b6af…","status":"retired"}]}
```

Its envelope is
`{"kind":"plans","schema_version":1,"produced_by":"cap-plans@sha256:<hex>"}`.
Each `network_hash` is the head of that pick's `plan.<id>` pointer, a
`network_document` blob.
