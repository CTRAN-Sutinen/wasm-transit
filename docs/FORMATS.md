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
- the files and their hashes.

A reader maps a blob's envelope (`kind`, `schema_version`) to its
schema there. This file pins the *storage envelope* and the invariants
that the schema cannot say.
The Go host owns kinds that no Rust type generates. The `build`
document, the `derivation` record, and the `store_layout` object have
hand-written schemas, each locked in `server/schemas/` beside a frozen
example. `server/schema_test.go` checks each against its Go type and
against one the server writes, and the index lists them with the owner
`server`. The manifest's schema is the HTTP contract's
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
| `logs/<log>/entries/<seq>.json` | one entry of an append-only log | serving processes (create only) |
| `logs/<log>/checkpoints/<seq>.json` | a log's fold up to an entry | serving processes (create only) |
| `inputs/<key>` | an input servers boot from (`-feed bucket:<key>`, D-118) | the operator |
| `layout`, `README` | the layout's version and hash scheme, and the layout in plain words | serving processes (create only, once) |
| `schemas/<tag>.v<N>/<hex>.json` | a locked schema, by its sha256 | serving processes (create only) |
| `kinds/<hex>.json` | a build's kind table, by its sha256 | serving processes (create only) |
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
- **The module that writes a bus document computes its hash**
  (network, edit records, plans, walks). The module returns the
  canonical bytes and their hash. The store asserts `sha256(blob) == hash` and
  fails loudly on a mismatch. The host canonicalizes and hashes
  host-owned documents (the manifest) with the same formula. No module
  ever claims their identity.
- The canonical serialization of a document is RFC 8785, the JSON
  Canonicalization Scheme (D-169): every object's keys sorted by their
  UTF-16 code units, no whitespace, numbers as ECMAScript writes them,
  and only `"`, `\`, and control characters escaped. So
  `sha256(canonical bytes)` is the document's identity on every host,
  and any implementation of the standard, in any language, computes the
  same identity for the same content (`transit_version::jcs`).
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
  - the derived documents' `boundary`, `streets`, and `walks`, and
    their `derivation` records (see "Derivations")
  - the boot sources' `gtfs_feed` (a GTFS zip) and `street_layer`
    (GeoJSON). These are external open formats, stored as read (D-115,
    D-118).

  `produced_by` on a `network_document` and a `boundary` is
  `cap-network@sha256:…`. On `streets` and `walks`, it is
  `cap-streets@sha256:…`. On
  `plans`, it is `cap-plans@sha256:…`. On `stops`, it is
  `cap-stops@sha256:…`. On `stop_requests`, it is
  `cap-requests@sha256:…`. On `policy`, it is `cap-authz@sha256:…`, the
  module that checked the write.
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
  - `edits`, read only. It is the edit document's history before the
    disruptions log ("The logs", below). No server writes it.
  - `policy`.
  - `manifest`.
  - `plans`, read only. It is the published plan set's history before
    the plans log ("The logs", below). No server writes it.
  - `stops` and `stop_requests`, read only. They are the stops
    document's and the stop requests document's history before the
    stops log ("The logs", below). No server writes them.
  - `plan.<id>` for each plan's frozen versions (its network
    documents). Each freeze commits its network as the head, so every
    version stays in the log, and the plans log names it by hash.
  - `build` (each release committed from `-dist`, and each move;
    see "Builds").
  - `build.<id>` for each UI the server keeps (`-ui-dist`): each of
    that UI's bundles, and each move (D-171).
  - `source.feed` and `source.streets`. Each read of `-feed` and
    `-streets` whose bytes changed commits a record. The record's
    `writer` is `import:<source>` (D-118). A `stops` commit of the
    agency's feed's differences has the writer `import:feed` (D-150).

  A server whose source does not answer at boot reads that pointer's
  head instead. So a replacement that has only the bucket and the same
  configuration serves the same manifest.
  - `network`, `boundary`, `streets`, and `walks`: the documents the
    server derives from its inputs, and `derived.network`,
    `derived.streets`, and `derived.walks`: the derivation records that
    name them (see "Derivations"). Only the server's derivations write
    them.
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
that nothing names and that is older than the grace (7 days). A record
names its `to` and `from`. A build's record names its files too (see
"Builds"). A field that a stored schema marks `content-hash`, in a
named blob or in a log entry's record, names its blob, and so on to the
end ("The store describes itself"). A referenced blob that is missing
stops the pass, because gc never guesses what it would have named. A
pass runs these steps:

1. List the records, then the blobs.
2. Move each orphan to `gc/<hex>` (a copy, then the delete).
3. Settle: list the records again, restore every trashed blob that is
   now named, and empty `gc/`.

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
  no envelope;
- under `logs/`: a key that is no entry or checkpoint, a gap in a log's
  entries, or an entry or checkpoint whose `kind`, `log`, or `seq`
  differs from its key, or a checkpoint past the log's last entry;
- a checkpoint whose bytes differ from the fold of the entries after
  the checkpoint before it (or from the first entry);
- an object under `schemas/` or `kinds/` whose bytes do not hash to the
  name in its key;
- a blob that a field marked `content-hash` names, in a named blob or
  in a log entry's record, and that is missing. The break names what
  refers to it;
- a head manifest that names a document no record names. The fields
  read are `feed_hash`, `network_hash`, `boundary_hash`, `streets_hash`,
  `walks_hash`, `policy_hash`, and `build_hash`. The plan set, the
  stops, and the stop requests are folds their logs keep, and the
  modules' hashes name modules.

The fold is the module's, so `store verify` runs cap-log, which keeps
every log (`-log`, default `../web/pkg/cap_log.wasm`). Every entry is a
`log_entry` document, and every checkpoint a `log_checkpoint` document.
A store with no checkpoints needs no module. The rest of verify is plain
Go. A server also checks its newest checkpoint when it boots ("The
logs").

`store dump <url> <dir>` writes every object to a folder that is absent
or empty: each at `<dir>/<key>`, its exact bytes, and an object's
metadata (a blob's envelope), when it has any, beside it at
`<dir>/<key>.meta.json`. It reads the store and writes nothing to it. A
folder is never a store: nothing reads a dump back.

## The logs

A log is the record of one consistency domain (D-190). Each entry is one
transaction. The current document is the fold of the entries, which the
module that owns the log's kinds computes. A mistake is fixed by a new
entry, so the truth before the mistake stays in the record.

The store has five logs. cap-log keeps them all (`log_entry`,
`log_checkpoint`; D-197), and never reads a record. The capability that
owns each kind turns its document into records and back:

- `disruptions`: the edit document (detours and trip cancels), whose
  records cap-sync answers.
- `stops`: the stops document and the stop requests document, in one
  log because completing a request and adding the stop's event is one
  transaction. cap-stops and cap-requests answer their records.
- `plans`: the published plan set and the frozen versions (D-191), in
  one log because publishing freezes a version and adopts it in one
  transaction. cap-plans answers both documents' records. The plans log
  has no `revert`: a pick is corrected by republishing it, or retired
  before it takes effect (D-75).
- `alerts`: the service alerts document and the alert templates
  document (D-206, D-210). cap-alerts answers both documents' records
  (`service_alert`, `alert_template`).
- `notes`: the notes document (D-214). cap-notes answers its records
  (`note`).

An entry is `logs/<log>/entries/<seq, zero-padded to 20 digits>.json`,
a `log_entry` document. It names each record's kind and version,
because one entry may change the records of two capabilities:

```json
{"at_ms":1770000060000,"changes":[{"after":[{"event":"in_service","id":"e5f3…","lat":45.69,"lon":-122.65,"name":"NE 99th St & Hwy 99"},{"event":"changed","id":"e-repair","note":"pad replaced","on":"2027-02-20"}],"before":"sha256:4c1d…","id":"6124","kind":"stop","version":1},{"after":{"completed_on":"2027-02-20","job":"repair_improve","status":"completed","stop":"6124"},"before":"sha256:9a07…","id":"R-2027-001","kind":"stop_request","version":1}],"intent":{"key":"tablet-4/17","op":"POST /logs/stops"},"kind":"log_entry","log":"stops","module":"cap-log@sha256:…","schema_version":1,"seq":2,"writer":"planner-3"}
```

- `seq` is the entry's place, from 1. It is the same as its key.
- `writer` is the verified sign-in subject, or the tool: `gc` for the
  edit document's tombstone compaction, `migrate:edits` for the first
  entry of a store that had the `edits` pointer, `migrate:-edits` for
  one seeded from the dev store's file, `migrate:stops` for the
  first entry of a store that had the `stops` or `stop_requests`
  pointer, and `migrate:plans` for the first entry of a store that had
  the `plans` pointer. That entry freezes each pick it copies, with the
  purpose "published before the plans log".
- `module` is the cap-log build that answered the entry.
- `intent` is what asked for the write: `op` (the endpoint or the op),
  `args_hash` (the hash of the request's body, or of the pointer head a
  migration copied), `key` (the writer's idempotency key), `note`
  (the cause that a gap's entry names, or the heads a migration
  copied), and `claimed_at_ms` (the device's clock when the person
  made a write that waited on the device: a claim beside the entry's
  `at_ms`, never its time; D-222).
- `changes` holds each record that the entry changed, in kind and id
  order: `{kind, version, id, before, after}`. `version` is the version
  of the record's document kind. A record in that version is valid as
  the only record of a document of that version, so its published
  schema is the document's. `before` is the canonical hash of the
  record as stored before the entry (absent for a new record). `after`
  is the record's new state, whole (absent when the entry takes the
  record out of the fold, as the compaction and a revert do).
- `reverts` is the `seq` of the entry this one reverts.

The records of each log:

- In the disruptions log, an `edit_record` record is one detour or trip
  cancel, by its id, whole.
- In the stops log, a `stop` record is one stop, by its lasting id, with
  its events whole and in their order. An event's place among events on
  the same day is the order of writing, so an event is not a record of
  its own. A `stop_request` record is one request, by its id.
- In the plans log, a `pick` record is one published pick, by its plan
  id. A `frozen_version` record is one frozen version, by its network's
  hash: the plan's id, name, and date, and what the version is
  (`purpose`). A pick names the stops log entry its adoption read
  (`stops_seq`, absent when it read no stops or was published before the
  log).
- Each document also has its own record: kind `edit_records`, `stops`,
  `stop_requests`, `plans`, or `frozen_versions`, id `document`, the document with its collection
  empty. It is a valid document of its kind and version. So a field
  outside the collection that a build does not know is kept (D-167).

A checkpoint is `logs/<log>/checkpoints/<seq>.json`, a `log_checkpoint`
document. It holds every record by kind and id, each with its version,
and every idempotency key with its entry. Checkpoints are create only.
Two folds of the same entries give the same bytes, so a checkpoint
that exists with other bytes is an error.

The rules:

1. Every write passes the receive path into one entry. A write that
   changes nothing writes no entry. So a resend is idempotent.
2. A write creates entry `head + 1` only if that key is absent: the same
   conditional create as a pointer record. A lost race reads the log
   again, runs the receive path again, and retries.
3. A fold never stops on one entry. An entry that is not JSON, names
   another `seq` than its key, or whose `before` hashes do not match the
   fold is quarantined. The server logs an alarm and names it in `GET
   /logs/<log>`. The fold goes on.
4. A missing entry below the head is a gap. A gap stops writes to the
   log, and the server serves the fold up to the entry before it. An
   entry with no changes, whose `intent.note` names the cause, closes
   the gap (`POST /logs/<log>/close-gap`).
5. A revert is a new entry. It writes back each record's state before
   entry N. A record that entry N created leaves the document, and the
   log keeps every state of it (D-198). The revert is refused when a
   later entry changed one of those records, because a person decides.
   In the disruptions log, cap-sync writes each record back with the
   next `rev` and the server's clock, so it wins the merge over a copy
   that a page still holds. It retires a record that entry N created
   instead, because the LWW merge would bring a dropped record back
   (D-193).
6. After a create, the engine reads the entry back and compares the
   bytes. A store that ignored the create's condition is caught at once.

An idempotency key in `intent.key` lands once: a write sent again with
a key the fold holds writes nothing, and the answer names the entry
that holds the key. A write that names the seq it read (`read_seq`) is
checked against the entries after it: a record that one of them changed
and the write changes too refuses the write, and a change to another
record does not (`transit_log::receive`, `unread`). A change's `before` is the hash of the record as
the entry that wrote it stored it, never of a migrated copy. A record
in an older version comes forward through its kind's migration chain
as a one-record document.

A server folds from its newest checkpoint and stores the next one after
256 more entries. At boot it checks the newest checkpoint against the
fold of the entries from the checkpoint before it (or from the empty
document). A checkpoint that differs raises an alarm, and the boot
starts from the one before it.

A store that has the `edits` pointer and no log gets entry 1 at startup:
the pointer's head, brought forward through cap-migrate, with writer
`migrate:edits` and `intent.args_hash` naming the head's hash. A store
that has the `stops` or `stop_requests` pointer and no stops log gets
entry 1 from their heads, with writer `migrate:stops` and
`intent.note` naming each head's hash. The pointers stay as they are.

## The HTTP surface

This file holds the formats at rest. The HTTP surface has its own
homes:

- The contract is `server/openapi.json`: every route, its method, its
  parameters, its request and response bodies, and its status codes,
  with a description of each operation. Go tests hold it to the real
  server, and a lock holds each change to expand, then contract
  (`docs/architecture/release-artifacts.md`, "The HTTP contract").
- What the server does behind each route (which op runs, what it
  writes, sign-in, and the store's outage) is in
  `docs/architecture/server.md`.

## The kind table

Each build has one kind table (D-202). It is generated from the kind
list, `crates/cap-abi/src/kinds.rs`: one line for each kind of document.
A kind exists only if it has a line there. The table is checked in as
`crates/cap-abi/kinds.json`, so a new kind or a changed route shows in a
review's diff. gen-manifest stops while the checked-in table differs
from the list and the locked schemas, and `just schema-lock` writes it
again. gen-manifest also writes it beside the server's modules as
`web/pkg/kinds.json` (the server's `-kinds`), with each schema it names
as `web/pkg/schemas/<hex>.json`. It is a `kind_table` document. Each
entry is one kind the store keeps, by its store kind (the envelope's
`kind`, or a log's document kind):

```json
{"store_kind":"policy","tag":"policy","versions":[{"version":1,"schema":"sha256:…"}],"write":"pointer","route":"POST /pointers/policy","pointers":["policy"],"module":"authz","validate":"validate","merge":"replace","read":"restricted"}
```

- `tag` names the kind's schema. It is empty for a kind with no schema
  of ours (a GTFS zip, a file of a build, the manifest).
- `write` is `pointer` (the generic `POST /pointers/<name>`), `route`
  (its own endpoint, which `route` names), or `server` (only the
  server writes it).
- `pointers` are the pointers it heads. `*` ends a prefix.
- `log` and `records` name the log that holds it, and each record kind
  with the document field that holds its collection.
- `module` and `validate` name the capability and op that check a
  write. The op takes the document under the kind's tag, and answers
  the canonical bytes, or what refuses it.
- `merge` is `replace` (CAS on the head read), `lww` (each record id,
  D-56), `log` (ops over a log's head), or `none`.
- `read` is `open`, `restricted` (never served by hash), or `audit`.
- `load` is the kernel's load of the kind, when a module reads it from
  the bus: its JSON op and its bulk op's code.
- `receive` names the capability whose `receive` op writes the kind's
  log: it runs a write's ops over the head and answers the entry
  (D-228). Every kind of one log names the same one.
- `intent` is `false` when a page never writes the kind's log by intent
  (`POST /logs/<log>` answers 405), and absent when it does. Only the
  kinds' routes write such a log: a freeze stores its network first, and
  `POST /edits` is the edit document's protocol. Every kind of one log
  agrees.
- `post` is the kind's own write route, which that op answers: its
  `path`, the `op` the request's body is the arguments of (an op of
  the kind's checking capability, `module`, as the receive op takes
  it), the
  `answer` field that names the document after the write, and the body
  fields it repeats (`echo`).
- `references` are the fields that name other records or things, each
  with its target, from the current version's schema (D-232; "References",
  below).

A generic pointer write is governed by its kind's entry. A kind with no
entry is refused. A kind with its own route, or that only the server
writes, is refused, and the answer names the route. A pointer kind
heads only its pointers. Its module checks it: what makes it wrong is a
`422` with the module's answer, and a consequence is accepted and named
(`warnings`). The store holds the module's canonical bytes, with the
module in the envelope.

### References

A field that names another record or thing states its target in its
kind's schema, with the keyword `x-references` (D-232). A content hash
keeps `"format": "content-hash"` beside it. The kind table collects them
as `references: [{path, target}]`, and an ETL reads the same map for its
joins. A path is a JSON Pointer in which `*` is each item of an array or
each value of an object by id, and `[tag=value]` chooses a tagged enum's
branch: `/notes/*/target[kind=edit]/id`. A target is one of these:

- a log record kind: `stop`, `stop_request`, `service_alert`,
  `edit_record`;
- a thing of the network, by its model id: `network.route`,
  `network.stop`, `network.trip`;
- `trip_instance`: a trip, its service date, and its start;
- a document by its content hash, by its kind's name in the kind list:
  `network`, `streets`, `feed`;
- `attachment`: a file by its content hash;
- `log.<name>`: an entry of a log by its seq;
- `external.<name>`: a thing outside the record. It is declared and
  never checked.

A law in each owning crate (`tests/references_law_test.rs`) lists the
kind's reference fields, so a new one fails until it is declared.

The existence check reads the same map (transit-log `broken`, cap-log
`references`; D-233). A reference whose target is a log record kind
must name a record that the target's log holds in its fold. The record
may be in any state: a retired record still exists. The path's first
segment is the collection and its second the record's id, so the rest
is read in each record. The check does not judge the other targets.
`store verify` reports each broken reference of each log's head, with
the entry that broke it: the later of the entry that last wrote the
referring record and the entry that last changed the target. A finding
names the kind table's hash, the module's hash, and each log's seq, so
it can be judged again after a release or a rollback.

A referring record in a version newer than the kind table reads (after
a rollback, D-186) is not judged, since the table's declarations may
not be its fields. The check lists it as `unjudged`, a warning, and
never as broken. A person's write that adds a reference to a record
missing from every log as of the seqs its writer read (`read_seq`,
`read_seqs`; none: now) is refused, 422 with `references` (and empty `refusals`): the log's
receive op judges it (`transit_log::receive`), since an honest writer
names only what it saw. A log with no entries is not judged. A reference the
record already had is not added, so a broken record can still be
changed or cancelled. The server's own writes are not refused. A write
that lands answers the broken references its entry touches as
`references`, beside its answer: those it wrote broken now, and those
whose target it changed. `store revert` lists the records that
name a document it drops by hash (cap-log `naming`), so an operator
sees which records still name a document that is no longer the head.

A rule of the kinds at both ends judges a reference whose target is
held (D-233). The owning library states which target states each
referrer state needs, with the reason for each refusal, and a law
covers every state. A rule reads a dated state on the write's date in
the agency's time zone, which the host gives with the write
(`today`). The stops log judges a request's stop: an open request needs
a stop that is not retired on that date (transit-stops `request_rule`,
D-238). A rule's refusal and warning have the shape of a missing
target's, with `rule`, the end of the sentence a page shows ("… names
stop B, which was retired on 2026-10-01").

Compaction never breaks a reference (D-231). The tombstone compaction
(D-57) skips a retired record that a held record of any log names
(transit-log `named`), in any state of the referrer, since the
existence check judges every held referrer. The record leaves in a
later compaction, when nothing names it.

## The store describes itself

A reader with only the bucket finds how to read it (D-203). Each serving
process creates these objects, create only, at boot:

- `layout`, a `store_layout` document: the layout's version and the
  hash scheme, `{"kind":"store_layout","schema_version":1,"hash":"sha256","readme":"README"}`.
- `README`: the layout in plain words.
- `kinds/<hex>.json`: the build's kind table, by its sha256.
- `schemas/<tag>.v<N>/<hex>.json`: each schema the table names, by its
  sha256. An added optional field keeps its version (D-167), so one
  version can have more than one schema.

An object that exists is never written again. A schema marks each field
that names a blob in this store with `"format": "content-hash"`: a
pick's and a frozen version's `network_hash` (and a frozen version's
`from_network_hash`, the version it was brought forward from), the walk table's
`streets_hash` and `network_hash`, a build's files and modules, and a
derivation's inputs and documents. A record's `before` in a log entry
is not marked, because it is the hash of a record, not of a blob. A
photo's hash is not marked, because the photo is in the attachments'
own bucket. `store verify` and `store gc` follow the marks from every
named blob, and from every log entry's records, through the kind
table's `records` to the schema of the document's collection. A store
with no description is read as before.

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

## Derivations

The server derives documents from its two inputs and stores them when
it makes them (D-199):

| Input (pointer) | Module | Documents (pointers) | Derivation record |
|---|---|---|---|
| the GTFS zip (`source.feed`) | cap-network `import` | `network`, `boundary` | `derived.network` |
| the street layer (`source.streets`) | cap-streets `import` | `streets` | `derived.streets` |
| the streets and the network | cap-streets `walks` | `walks` | `derived.walks` |

Each document is a blob in its canonical bytes, the head of its tag's
pointer. Its envelope names its kind and version, and the module that
made it. A `derivation` record names what the module made from which
inputs (envelope `{"kind":"derivation","schema_version":1,"produced_by":"server"}`):

```json
{"kind":"derivation","schema_version":1,"inputs":["sha256:…"],"module":"cap-network@sha256:…","documents":[{"tag":"network","hash":"sha256:…"},{"tag":"boundary","hash":"sha256:…"}],"answer":{"boundary_hash":"sha256:…","first_service_date":"2026-11-02","network_hash":"sha256:…","notes":[],"timezone":"America/Los_Angeles"}}
```

- `inputs` are the hashes the module read, in its order: the zip, the
  layer, or the streets and then the network.
- `answer` is the rest of the module's answer, as the module answered
  it. `GET /network` serves it with the two documents put back.
- A derivation commits its documents first and its record last. A
  crash before the record leaves the documents' pointers ahead, and the
  next boot derives again and commits the same bytes.
- A boot reads the record. When `inputs` are the hashes of the inputs
  just read, and each document's envelope has the version this build
  writes, the server serves the stored documents and runs no module.
  Otherwise it derives again, and commits.

The Go host owns the kind. Its schema and a frozen example are in
`server/schemas/derivation.v1.*`, and `server/schema_test.go` holds
them to the Go type and to a record the server commits.

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
  Compaction (D-57) drops retired records older than the horizon from
  the fold, as one entry of the disruptions log (writer `gc`). The
  entries before it keep every record.
- Nothing compressed is ever the durable format. Compression is
  transport only.

## Versions

Every document says what it is inside it (D-62, D-115). So a copy
anywhere names its kind and version without the app. A copy can be a
blob, an export, or a file that another tool reads.

- Each of our kinds carries `"kind"` and `"schema_version"` inside it.
  A network with no `schema_version` is v1. This is every shape written before the tag. A
  bare array of edit records is edit document v1. A browser's plan set
  written before its tag is a bare map of plans by store id, and a page
  reads it as the set. A watchlist written before its tags names its
  version as `"version": "vN"`.
- The byte kinds are a GTFS feed, a GTFS-rt snapshot, and a GeoJSON
  layer. They are open formats that their own specs version, and they
  carry no tag of ours.

Every persisted kind, by what a version change does to it:

| Kind | Owner | Class | Inside it today |
|---|---|---|---|
| `edit_records` | transit-sync | authored | `kind`, `schema_version` |
| `log_entry` (one entry of a log: the disruptions log or the stops log) | transit-log | authored | `kind`, `schema_version`; each change names its record's kind and version |
| `log_checkpoint` (a log's fold up to an entry) | transit-log | derived | `kind`, `schema_version`; each record its version |
| `network` (authored or forked) | transit-network | authored | `kind`, `schema_version` |
| `plans` (the published set) | transit-plans | authored | `kind`, `schema_version` |
| `frozen_versions` (every version frozen, by network hash) | transit-plans | authored | `kind`, `schema_version` |
| `plan_set` (a browser's stored plans, by store id) | transit-network | authored | `kind`, `schema_version`; each plan's network its own |
| `watchlist` | transit-rider | authored | `kind`, `schema_version` (before its tags, `"version": "vN"`) |
| `policy` | transit-authz | authored | `kind`, `schema_version` |
| `work_rules` | transit-runs | authored | `kind`, `schema_version` |
| `stops` | transit-stops | authored | `kind`, `schema_version` |
| `stop_requests` | transit-requests | authored | `kind`, `schema_version` (v2: `routes` are names the planner writes, not a reference; v1 comes forward unchanged) |
| `service_alerts` | transit-alerts | authored | `kind`, `schema_version` (v2: `explains` names a trip instance; v1 comes forward unchanged) |
| `alert_templates` | transit-alerts | authored | `kind`, `schema_version` |
| `notes` | transit-notes | authored | `kind`, `schema_version` |
| `diversion_draft` | transit-dispatch | authored | no tag (never persisted yet) |
| `recent_places` (a rider's recent places, on one device) | transit-rider | authored | `kind`, `schema_version` |
| `live_feeds` (a rider's own live-feed URLs, on one device) | transit-rider | authored | `kind`, `schema_version` |
| `network` (imported) | transit-codec | derived | `kind`, `schema_version` |
| `boundary` | transit-codec | derived | `kind`, `schema_version` |
| `streets` (imported) | transit-streets | derived | `kind`, `schema_version` |
| `walks` | transit-streets | derived | `kind`, `schema_version` |
| `live` | transit-live | ephemeral | `kind`, `schema_version` (v2: a trip instance names its service `date`) |
| `manifest` | the Go server | host-owned | its own shape (below); its schema is the HTTP contract's |
| `build`, `build_file` | the Go server | host-owned | `kind`, `schema_version` in a build (a hand-written schema, "Builds", below); a build file is its exact bytes |
| `derivation` (what a module made from which inputs) | the Go server | host-owned | `kind`, `schema_version` (a hand-written schema, "Derivations", above) |
| `store_layout` (the store's layout object) | the Go server | host-owned | `kind`, `schema_version` (a hand-written schema, "The store describes itself", above) |
| `kind_table` (a build's kind table) | generator (gen-manifest) | derived | `kind`, `schema_version` ("The kind table", above) |

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
- The *host-owned* manifest, builds, and derivations are versioned in
  their own envelope.

A gate (cap-abi `tests/formats_inventory_test.rs`) holds this table
and the locks to the same kinds. A kind in the table with no locked
schema names its reason there: the watchlist is locked once it carries
its kind, a diversion draft is never persisted yet, the manifest's
schema is the HTTP contract's, and a build file is its exact bytes.

A copy written before its kind's tags reads as that kind. An absent
`kind` is the kind itself, and an absent version is v1. The network's
and the edit document's untagged v1 are older versions. The watchlist
gained its tags at v2, the version it already was: a copy that names
`"version": "v2"` reads as v2 and is written tagged, and one that names
`"v1"` is an older version. Every watchlist from before the tags named
its version, so one with neither field is v2. Every other kind gained its tags at v1, the
version it already was. Every kernel
load checks the version the same way. So does every request field that
takes one of these documents (`cap_abi::parse_versioned`,
`cap_abi::Current`; the tag type is transit-version's `kind_tag!`). A
kind whose older copies name their version another way reads that too
(`Versioned::version_in`).

A module reads the version it was built for, and only that version. A
module refuses an older document at its load, and the refusal names the
migration capability. It refuses a newer document too, and that refusal
names both versions. A module never parses and re-emits a document of
another version. The migration capability (cap-migrate) brings an older
document forward, one pure step per version. It keeps every record and
reports every loss. The answer is a new document under its own hash,
and the old bytes stay. It runs where a document arrives:

- The server's receive: an older `POST` body comes forward before the
  merge.
- The server's startup. Only the documents in use come forward, each
  as an ordinary logged CAS with the writer `migrate:<module hash>`.
  The older blobs stay under their hashes.
  - A store's `edits` pointer comes forward once, into the disruptions
    log's first entry ("The logs"). The pointer itself is not written.
  - The networks of the picks in use come forward. cap-plans `in_use`
    names these picks: the pick in force, and every pick still to
    come. Each migrated network is published again under its own date,
    through the same path as `POST /plans`. So cap-plans writes the
    set's new `network_hash`, and the host rewrites no hash. A pick
    that stopped governing, or a retired one, stays in its version. It
    comes forward when it is published again.
  - The plan set, the policy, and the work rules have only v1. They
    join this step when a second version of their kind ships.
- A browser's restore. The planners' pages bring a stored plan
  forward, and ride-now a stored watchlist. The old record stays in
  the device's document store.
- An agent's load: `caps-run --load` of an older file is refused. The
  refusal names the op, which the agent runs through `--cap migrate`.

A new version rolls out server first. The server's modules learn it
first. They accept the old and the new version at receive and store the
new one. The manifest then pins those modules, and clients follow the
manifest (D-63). Every client gets its modules from that manifest, so
no client writes a version that the server does not read.

### Rolling a version back

A format change ships in two releases (D-79). Release A reads vN and
vN+1 and writes vN. Release B writes vN+1. So a rollback goes one
release back, to a build that reads everything the store holds. The
reader of vN+1 leaves in the release after B.

Release A declares, in the kind's `Versioned`, the version it reads
ahead and how (`AHEAD`, D-186). The crate that owns the kind keeps the
step down from vN+1 to vN (`step_down`), beside the migration step it
reverses. The change is one of two kinds:

- *Lossless*: vN+1 read down and brought forward again is the same
  document. A law in the owning crate pins this. Release A reads the
  document and writes over it as usual.
- *Read-only*: vN+1 holds what vN cannot. Release A reads the document,
  and refuses to write over it. The refusal names both versions. Edits
  to that kind pause until the deployment moves forward again.

Every reader reads ahead the same way: a kernel load, a request field
(`cap_abi::Current`), and the base of a write (`cap_abi::Base`; a held
document through `Kind::require_writable`; transit-sync's
`read_document`). A copy read ahead is held under the hash it is stored
with, so the page and the store name it alike. The laws are in cap-abi
`tests/read_ahead_test.rs`. cap-plans takes the published set the same
way: `in_force` and `in_use` read it, and `publish`, `adopt`, and
`retire` write over it. `freeze` takes the frozen versions the same way.

A page whose module refuses a document as newer, or whose write the
server refuses for its version, asks for the deployment's build
(`web/src/versions.ts`). A page with a service worker (ride-now, the
stop display) looks for the new build at once. It applies the build by
its own rule, as for any new build (D-63).

The emergency procedure is for a bad release A. It rolls the data back
as well as the code, and an operator runs it by hand:

1. Roll the code back ("Builds", above; `DEPLOYMENTS-SERVER.md`).
2. Read each pointer's log (`GET /pointers/<name>/log`), and find its
   last head from before the upgrade.
3. Move the pointer back to that head: `store revert [-writer W] <url>
   <pointer> <hash>` (`server/cmd/store`). The move is a new commit of
   that head's document, under its envelope, so the log keeps every
   commit. A pointer already at that head does not move.
4. Read the commits the move dropped, which the tool lists: every log
   entry after that head. Each one names its writer and its document
   by hash, and the documents stay in the store as plain JSON (D-115).
   A person enters their content again by hand, in the older build.
   The tool also lists `references`: the records of every log that
   still name a dropped document by its hash (D-233; `-kinds` and
   `-log` choose the kind table and the log module).

The logs are not moved back this way. An entry of the disruptions log
or the stops log is undone by its revert, and a pick in the plans log
by republishing it or retiring it (D-75).

A backup restores into a build at least as new as the backup. An older
build refuses its newer documents, naming both versions.

An archive reads two ways. As it was: its bytes and its version's
published schema. This is the default for an audit. As it is today:
brought forward through the migration chains on read, never stored.

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
records the new schema in the same version. A corrected description
(a field's doc comment) also reads to the same bytes, and is recorded
the same way, and so is a reference declaration added to a field that
had none (D-232). A changed or dropped reference target changes what
the field means, though no byte changes, so it needs a version bump.
`just schema-lock` refuses every other change. It also writes the
checked-in kind table ("The kind table").

Two more gates hold the schemas to the bytes the code really writes,
where `schemars` and serde can disagree (`skip_serializing_if`,
`untagged`, `flatten`). Both check with the locks in the tree:

- The real-output gate (`generator::checks::real_output`, which the
  generated gen-manifest crate's tests run in `cargo test`) runs every capability's worked examples through its own
  dispatch over the fixtures. Every document an answer holds must match
  its locked schema, and every kind a worked example's recorded answer
  holds must be checked from a real answer.
- The real-data soak (`just soak`, `generator::checks::soak`)
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
them.

The kernel's own version is separate from the documents' versions. It
bumps only when an existing kernel op changes meaning. A new kernel op
is listed in the manifest's `kernel_ops`. So a host checks for the op
by name, not by the kernel number.

Persisted data includes URLs. The stop display's `stop` and `layout`
parameters are configured into screens and printed into QR codes. So
the URL only ever gains optional parameters. A page ignores a parameter
that it does not know, and keeps it when the page rewrites its URL.

## Worked examples

The edit document (the fold of the disruptions log, and an
`edit_records` blob of the `edits` pointer) is canonical: its kind and
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

A blob of the `edits` pointer has the envelope
`{"kind":"edit_records","schema_version":2,"produced_by":"cap-sync@sha256:<hex>"}`.
After a second commit, its pointer record is
`{"seq":2,"from":"sha256:<first>","to":"sha256:<this>","kind":"edit_records","schema_version":2,"produced_by":"cap-sync@sha256:<hex>","writer":"anonymous","policy":null,"at_ms":2000}`.

A `manifest` blob is the manifest object itself
(`{"network_hash","feed_hash","feed_version","streets_hash","walks_hash","wasm_hash","authz_hash","policy_hash","plans_hash","stops_hash","stop_requests_hash","build_hash","generated_at","publisher","timezone","services","auth"}`,
the optional ones omitted when empty). Its envelope is
`{"kind":"manifest","schema_version":1,"produced_by":"server"}`.

The `plans` document is the published plan set, with its version inside
it (D-62). It is the plans log's fold, served by hash from `/blobs`; the
`plans` pointer's blobs are the same shape. The picks are in
(effective_from, id) order. A retired pick stays in the set as the
tombstone (D-75; `status` reads `published` when absent). A pick that an
adoption put in force names the stops log entry it read (`stops_seq`):

```json
{"schema_version":1,"plans":[{"id":"fall-2026","name":"Fall 2026 pick","effective_from":"2026-09-13","network_hash":"sha256:5206f419…","status":"published"},{"id":"winter-2027","name":"Winter 2027 pick","effective_from":"2027-03-01","network_hash":"sha256:d122b6af…","status":"retired"}]}
```

A blob of the old `plans` pointer has the envelope
`{"kind":"plans","schema_version":1,"produced_by":"cap-plans@sha256:<hex>"}`.
Each `network_hash` is a frozen version: a `network_document` blob that
the pick's `plan.<id>` pointer logs.

The `frozen_versions` document is the plans log's other fold: every
version the agency froze, by network hash (D-191):

```json
{"kind":"frozen_versions","schema_version":1,"versions":{"sha256:5206f419…":{"effective_from":"2026-09-13","name":"Fall 2026 pick","network_hash":"sha256:5206f419…","plan_id":"fall-2026","purpose":"proposal for public comment"}}}
```
