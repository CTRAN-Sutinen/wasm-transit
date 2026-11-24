# Storage — the agency's record

The agency's record is one bucket in object storage. The bucket holds
immutable, content-addressed objects, the pointer logs that name them,
and the append-only logs of what people edit. Every other copy of a document is a working copy or a copy of
the record. Examples are a page's IndexedDB, the bus worker's
registries, a server's head and blob caches (in memory, and on its
disk with `-store-cache`), an export, and a backup.
Who owns that data and the contract a write meets are in
[agency-data.md](agency-data.md). The bytes at rest are in
[`docs/FORMATS.md`](../FORMATS.md): keys, envelope, records, the commit
protocol, builds, and the invariants no schema can say. The server that
reads and writes the bucket is in [server.md](server.md). This file is
the model those three share. It says what the record is, what it holds
by shape, how it is written and read, and where it is not yet what it
has to be.

To see it first: the architecture page of `just docs` ("Where things
are kept") maps where each kind of document lives, drawn from the kind
table. It then steps through the files that one saved note makes (a
log), one new feed makes (pointer histories), and one published
service change makes (both: a frozen network in its plan's pointer
history, named by a plans log entry).

## The record

With `-store <url>` the server keeps the record in object storage.
The URL takes one of four forms: `s3://bucket/prefix` (S3 or an
S3-compatible endpoint, named by `?endpoint=`), `azblob://container/prefix`,
`gs://bucket/prefix`, or `mem://`. The `mem://` store is gone when the
process exits. Which provider a deployment runs on is the agency's
choice. The engine favors none (D-113, D-116).

The engine (`server/store/`) runs over one small backend interface
(`backend.go`: get, stat, create-if-absent, list from a key, delete).
The engine assumes only that contract. It assumes no append, rename,
fsync, or lock. The in-memory backend and the S3, Azure Blob and GCS
adapters (`s3.go`, `azure.go`, `gcs.go`) all pass one contract suite
(`contract_test.go`). `just store-contract` runs that suite and every
engine law against local SeaweedFS, Azurite and fake-gcs-server, or
against one store's address (an operator's own bucket).

- **Blobs** are `blobs/<hex>` objects. Each blob is immutable. Each
  carries its typed envelope (kind, schema version, producer) as
  metadata.
- **Pointers** are logs. Each commit is one record object,
  `pointers/<name>/<seq>.json`. The pointer's value is its highest
  record. A commit creates a blob. Then it creates record N+1 only if
  that key is absent. That create is the CAS on the `from` hash. Last,
  it re-reads its blob. Every record names its writer and time, so the
  log is the audit trail.
- **Logs** are the record of what people edit, one log for each
  consistency domain (D-190). Each entry is one transaction,
  `logs/<log>/entries/<seq>.json`, and holds each record it changed:
  the hash before and the new state. The current document is the fold
  of the entries, which the owning module computes. A checkpoint,
  `logs/<log>/checkpoints/<seq>.json`, is the fold up to an entry. A
  commit is the same conditional create as a pointer record. The engine
  then reads the entry back and compares the bytes. The store has three
  logs, all kept by cap-log, which never reads a record: `disruptions`
  (detours and trip cancels), `stops` (the stops and the stop
  requests), and `plans` (the frozen versions and the published plan
  set). Their rules are in `docs/FORMATS.md`, "The logs".
- **Many processes, one bucket.** Several server processes may share
  one bucket. Each caches heads. Each reads at most 2 s behind another
  process's commit. The model (`store.tla`, `just spec`) checks processes
  that share a bucket, with the offline collector beside them. A Go
  test runs two server copies that write the same logs at the same
  time: every write lands once, in one order, and both copies serve
  the same heads (`server/resilience_test.go`).
- **A copy's disk cache is a working copy.** With `-store-cache`, a
  serving process keeps what it read and confirmed on its own disk,
  and boots on it read-only while the bucket does not answer. It never
  writes the cache back to the bucket, and the bucket wins the first
  time it answers ([server.md](server.md), "The disk cache").
- **Nothing serving deletes.** A serving process opens the store with
  `store.OpenServing`. It creates, reads and lists. It never deletes
  (D-117). The offline tools are in `server/cmd/store` (`just store …`).
  `gc` collects orphans with its own delete-capable credentials.
  `verify` reads a whole store and exits non-zero on the first break.
  `dump` writes every object to a folder, at its key, to read a store
  with no object storage tools. `copy` carries a directory store that
  an older build wrote into a bucket, then verifies it.

The `manifest` pointer is the root. Publishing is one CAS on it. The
disruptions log is the multi-writer LWW edit document, and the stops
log holds the stops document and the stop requests document. The
alerts log holds the service alerts document and the alert templates
document. The notes log holds the notes document. The
`edits`, `stops`, and `stop_requests` pointers are their history from
before the logs, read only. The plans log holds the published plan set
and the frozen versions, and the `plans` pointer is the set's history
from before the log, read only. Each frozen network is the head of its
plan's `plan.<id>` pointer when it is frozen, so the pointer's log
keeps every version (D-74, D-191). Every pointer the server keeps is
listed in `docs/FORMATS.md`, "The pointer log".

**Freezing and adopting are two acts** (D-191). A freeze writes a
network version to the record by its hash, with what it is in the
agency's words (a proposal for public comment, a board packet). A
frozen version is in the record and not in force. Adopting puts a
frozen version in force from its date, through the published plan set.
Only a frozen version is adopted, and the pick names the stops log
entry it read (`stops_seq`). So every network that was ever in force is
in the record, with the proposals that led to it.

**The derived documents are in the record** (D-199). The server reads
two inputs, the GTFS zip and the street layer, and commits each read
to `source.feed` and `source.streets`. A module makes documents from
them: cap-network imports the network and its boundary, and cap-streets
imports the streets and makes the walk table over the streets and the
network. Each document is stored when it is made, as the head of the
pointer of its tag (`network`, `boundary`, `streets`, `walks`), with the
module in its envelope. A `derivation` record on `derived.network`,
`derived.streets`, or `derived.walks` names the input hashes, the
module, the documents, and the rest of the module's answer (an import's
notes, zone, and first service date). It is committed last. A boot
reads the derivation records. When a record names the hashes of the
inputs just read, and each document is stored in the version this build
writes, the server serves the stored documents and runs no module. An
input whose bytes changed is derived once. So every document the
manifest names is a blob in the bucket, and `store verify` checks the
head manifest for it (`docs/FORMATS.md`, "Derivations").

## What the record holds, by shape

The agency's data is not one shape. Each shape needs its own layout.
A kind is stored as a **document** (one blob per version, and a
pointer names the whole) or as a **log** (one entry per transaction,
and the document is the fold).

| Shape | Kinds | Writes | Growth | Layout today |
|---|---|---|---|---|
| Artifacts: large, published whole | network, streets, walk table, the GTFS feed and street layer read at boot, builds | rare: a publish, an import, a release | per revision | document |
| Config: small, whole | policy, manifest | rare | none | document |
| Collections: records by id, few writers | the published plan set (picks), the frozen versions | rare: a freeze, an adoption, a retirement | without bound (frozen versions) | log (`plans`) |
| Collections: records by id, many writers | the edit document (detours, trip cancels) | frequent, one record at a time | without bound | log (`disruptions`) |
| Collections: records by id, many writers | stops, stop requests | frequent, one record at a time | without bound | log (`stops`) |
| Collections: records by id, many writers | service alerts, alert templates | frequent, one record at a time | without bound | log (`alerts`) |
| Collections: records by id, many writers | notes | frequent, one record at a time | without bound | log (`notes`) |
| Attachments: binary | photos and site drawings (D-162) | per record | steady | not stored yet |
| Event tables: many rows, append only | none yet (heartbeats keep only the latest; TIDES, AVL and APC are not held) | n/a | n/a | n/a |

A document suits artifacts and config. It does not suit a collection.
Each commit stores the whole collection again. So the bucket grows with
the collection's size times its commits (D-121's measurements). Two
writers who change different records conflict. A log stores only what
each write changed.

## Every object explains itself

Every persisted document is plain JSON (gzip allowed) or an external
open format. It carries its kind and version inside it (D-115).
[bus.md](bus.md) states the rule under "Every document says what it is."
The blob's envelope repeats them, so a reader maps any blob to its
schema with no module. Each kind's JSON Schema and a worked example are
published in the registry (`registry/documents.json`;
[registry.md](registry.md)). `server/reader_test.go` reads a store with
nothing but object get and list, the schemas, and `docs/FORMATS.md`.

The bucket carries its own description (D-203). Each serving process
creates, create only: `layout` (the layout's version and hash scheme),
a plain-language `README`, each build's kind table under `kinds/`, and
every schema it names under `schemas/<tag>.v<N>/`, each by its hash. A
schema marks each field that names a blob in the store
(`"format": "content-hash"`). `store verify` and `store gc` follow those
marks from every named blob and every log record, so a pick whose
network is missing is a break, and gc keeps what a log names
(`docs/FORMATS.md`, "The store describes itself"). `store verify` also
reports each broken reference of each log's head: a declared reference
whose target record is not in its log's fold. That is a finding for a
person, not a break (`docs/FORMATS.md`, "References").

The newest authored kinds keep fields they do not know. The stops and
stop requests types carry an `extra` map. A field that a newer writer
added survives a round trip through an older one.

## Writes

Every write goes through a receive path. The path checks shape,
invariants, concurrency, and attribution ([agency-data.md](agency-data.md),
"Writes are governed," which tabulates where each holds). Writes differ
by kind:

- **Edits** (`POST /edits`, `POST /pointers/edits`) merge record by
  record, with cap-sync's LWW merge per record id. Two writers who
  change different records never conflict. Each write that changes a
  record is one entry of the disruptions log. A mistake is fixed by a
  revert, which is a new entry (`POST /logs/disruptions/revert`).
- **The intent write** (`POST /logs/<log>`, D-189) is how a page writes
  the stops log, the alerts log, and the notes log. The body is what the
  person asked for: `{key, ops, read_seq, read_seqs, claimed_at_ms}`. The server
  runs each op over the documents at the head, in order, so the
  module's checks run on what the server holds, and writes what the
  ops changed as one entry.
  - The key is the device's idempotency key. Every resend carries it,
    and a key that landed answers the entry that holds it. So a write
    whose answer was lost lands once.
  - `read_seq` is the log's seq whose documents the page read (the
    manifest's `log_seqs`). The write is checked against the records it
    changes, not the whole log's head: the receive op reads the
    entries after `read_seq` (`transit_log::receive`), and a record that one of them changed and
    the write changes too is a `409` that names it (`overlap`). A change
    to another record does not refuse the write.
  - `read_seqs` are the other logs' seqs the page read, by log (the
    manifest's `log_seqs`). The server judges each reference the write
    adds against every log as of the seq its writer read (the write's
    own log at `read_seq`; a log not named, now). A reference to a
    record missing then is a `422` with `references` (and empty `refusals`): an honest page
    names only what it saw (D-233). One that held then and is broken
    now lands, with the reference as a warning.
  - `claimed_at_ms` is the device's clock when the person made a write
    that waited. The entry keeps it in its intent, beside the server's
    stamp, and no rule reads it as the record's time (D-222).
- **Stops and stop requests** are one log, written by intent (stops
  `add_event`, `correct_event`; requests `put`, `record_work_order`).
  Two writers who change different stops both land. A correction of an
  event that another writer changed since is a `409` that names the
  event. Completing a request and adding the stop's event is one entry.
  `POST /stops` and `POST /stop-requests` still take the whole document
  under `from`, and a moved head is a `409`; each such write is one
  entry too. A page uses `POST /stops` only for the first stops
  document. The work order system's answer and the feed's differences
  run over the head and need no `from`. The work order route takes a
  key too: a resend whose key landed answers the request's work order,
  and the system is not asked again.
- **Service alerts and templates** are one log, written by intent
  (alerts `put`, `end`, `put_template`). `POST /service-alerts` and
  `POST /alert-templates` still take the whole document under `from`,
  and a moved head is a `409`. Each write is one entry of the alerts
  log.
- **Notes** are written by intent (notes `write`): one note, not a
  document. The server gives `by` and `at`. `POST /notes` takes one note
  the same way, with no key. Two people who note at once never
  conflict. Each write is one entry of the notes log, and the same text
  written again is none.
- **Picks and the plan set** (`POST /plans/freeze`, `/plans/adopt`,
  `/plans`, `/plans/retire`) are each one entry of the plans log.
  cap-plans answers the next frozen versions and plan set over the head,
  and refuses what breaks the chain. `POST /plans` freezes and adopts in
  one entry. The plans log has no generic revert: a pick is corrected
  by republishing it, or retired before it takes effect.
- **A generic pointer commit** (`POST /pointers/<name>`) is governed by
  the kind table (D-202): a kind with no entry, or with its own route,
  is refused, and a pointer kind (the policy) is checked by its module
  (cap-authz `validate`), which answers the bytes stored.

Permissions are set per pointer and action by the policy (D-58..D-61).
A role may write a pointer or not. There is no finer grain. No kind is
restricted on read, except the policy itself and the logs' entries.
The policy's blob is never served: its read class in the kind table is
`restricted`. The entries name their writers, so they need the `audit`
rule.

## Reads

- A blob by hash (`/blobs/<hash>`) is immutable. Every layer caches it
  forever. The layers are the server's byte-bounded blob cache, the
  service worker, and any HTTP cache in front.
- A pointer's head (`/pointers/<name>`, `/manifest`, `/edits`) is the
  one thing that can be stale. While the bucket does not answer, a
  server serves the heads it holds, in memory or in its disk cache,
  labelled with their age (D-119, D-224; [server.md](server.md)).
- A pointer's log (`/pointers/<name>/log?since=`) is the change feed.
  It lists the commits after a hash the reader holds.
- A log's entries (`/logs/disruptions?since=<seq>`,
  `/logs/stops?since=<seq>`) are the change feed of what people edit. Each entry says what changed, who changed
  it, and which module answered it. The fold that `/edits` serves, and
  the stops and requests documents that `/blobs` serves under the
  manifest's hashes, can be stale like a head.
- Reads are of whole documents. The derived documents a reader asks for
  by name (`/network/document`, `/network/boundary`, `/streets`,
  `/walks`) are blobs in the record, and `/blobs/<hash>` serves them by
  the manifest's hashes. `/feed` is computed by the server's modules
  from the record. No projection tier exists apart from the server.

## Where it is going

The storage model's direction is decided (D-190, D-191). The
disruptions log, the stops log, the plans log with freezing and
adopting, the stored derived documents, the kind table, and the
store's description of itself are built. The rest is
[docs/units/storage-model.md](../units/storage-model.md). It covers:

- the work rules in the plans log, and the policy's log.
- a working store for drafts, attachments, and caches, apart from the
  record.
- permissions and restricted reads per change.

The store on each provider's real account and its backups are in
[docs/units/store-durability.md](../units/store-durability.md).

## Why

One entry per choice that matters, headed by the code it governs.
**Holds** is what must stay true. Change it only with the user, after
reading **Because**.

### Object storage, create-only (`server/store/`, `backend.go`, `store.OpenServing`)
- **Holds:** the engine assumes only whole-object writes and a
  conditional create of an absent key. It assumes no append, rename,
  fsync, or lock. Serving processes never delete. `store gc` runs
  offline with its own credentials.
- **Because:** the store must survive losing a node. S3, Azure Blob, and
  GCS all provide exactly that contract. A server that cannot delete
  cannot erase history. The bucket can carry the provider's immutability
  settings.
- **Rejected:** a crash-hardened directory store. It runs on one node, so
  the disk's failure is the store's failure. Online GC in every server
  races a commit over object storage, and it needs delete rights
  everywhere. Never deleting is kept as an option for immutable buckets.
- **Decided:** D-113, D-117.

### The record is documents in a bucket, not rows in a database (this file)
- **Holds:** the record is immutable, self-describing objects in open
  formats. It is also an append-only log of who changed what. No database
  is the record for any kind.
- **Because:** an agency's data outlives its software, and it must be
  auditable. Objects that say what they are stay readable with no running
  software. Any number of processes share one bucket with no coordinator.
  Every past version is one GET. Losing any compute or cache loses
  nothing. At a transit agency's write rates (moderate to low, 24/7), the
  transactions a database offers are not what is scarce. History,
  availability of reads, and independence from one vendor are.
- **Costs:** each shape needs a layout designed for it, not one taken from
  a database. The edit document, the stops, the stop requests, the plan
  set, and the frozen versions have one (their logs). The work rules and
  the policy do not yet (`docs/units/storage-model.md`).
  Queries over the record are projections someone builds, not SQL against
  the record. A workload that needs transactions across records under
  contention needs an engine in front of the bucket.
- **Rejected:** a relational database as the record. It adds a second
  durable service to run, back up, and migrate. Its at-rest format is the
  vendor's.
- **Decided:** D-113, D-115, D-152.

### An infrastructure choice is judged by its failures, not by its parts (this file; D-113, D-116)
- **Holds:** a storage, infrastructure, or durability choice is weighed
  for an agency that runs 24/7 at moderate to low traffic, with high
  audit needs. Operational simplicity means that the part breaks
  rarely, that a failure has a bounded reach, and that the part is easy
  to restore, fix, change, roll back, and integrate with. Easy to
  monitor and troubleshoot weighs most. Never score a design by the
  number of stores or services it has.
- **Holds:** no choice ties the record to one provider (S3, Azure Blob,
  or GCS), also not in a unit's "Done when". A store that runs on a
  developer's machine is a convenience for development, not a target.
- **Because:** an agency already runs many systems, a data lake among
  them. Publishing into them is integration, not a burden. Integration
  pain comes from the data model and its access, not from the database
  engine.

### What people edit is an append-only log (`server/store/log.go`, `server/caplog.go`, `crates/transit-log`)
- **Holds:** each write that changes a record is one entry, with each
  changed record's hash before and its new state, the writer, the intent,
  and the module build. The current document is the module's fold of the
  entries. A mistake is fixed by a new entry (a revert), never by an
  overwrite. Logs name each other by `(log, seq)`, and a kind joins a log
  only if it must change atomically with that log's kinds. The fold
  quarantines an entry it cannot apply and goes on. A gap stops writes
  until an entry names its cause.
- **Because:** the agency asked for a store that breaks rarely, whose
  failures are bounded and easy to find and fix, that is cheap to run and
  back up, open in format, and works offline. A wrong entry sits beside
  the truth, so recovery is one more entry and loses nobody's later
  writes. No schema migration destroys data. Every write can be read and
  replayed, and a log is one timeline. The bucket already gives
  durability, whole-object writes, conditional create, and strong
  consistency. The engine adds only order and folding. The storage of a
  collection grows with what changed, not with its size times its
  commits (D-121).
- **Costs:** the fold and the checkpoints are this repo's code. An ad-hoc
  question needs an export. Entries keep their versions forever, so the
  migration chain must stay complete. Data in the record cannot be
  forgotten.
- **Rejected:** the whole document per commit (it grows with the
  collection's size times its commits); SQLite with Litestream (one
  writer process, a disk on the server, and a replica whose failure is
  silent); Postgres (an overwrite or a destructive migration destroys
  the truth before it, and a point-in-time restore loses later writes);
  a hash tree of records per collection (the most code of this repo's
  own); Delta Lake as the record (its failures are harder to read, and
  its writer is not in Go). Two needs reopen a database: data that must
  be deletable, or throughput that a log cannot carry. A database added
  then is fed from the logs.
- **Decided:** D-190, D-121.

### One log engine for every kind, and a revert takes out what it created (`crates/transit-log`, `crates/cap-log`, `server/caplog.go`)
- **Holds:** cap-log never reads a record. A change is `{kind, version,
  id, before, after}`. Each kind's capability turns its document into
  records (its collection, and the document's own record) and back. One
  entry may hold the records of two kinds. A log that a receive op
  writes takes the whole write from one capability, which links each
  kind's library: the stops log's is cap-stops, which links
  transit-requests (`crates/transit-log/src/receive.rs`; D-228). `before` hashes the
  record as stored. A revert takes a record the reverted entry created
  out of the document. A kind whose merge would undo that (the edit
  document's LWW) writes back its own records instead: stamped to win
  the merge, and a created record retired (D-193).
- **Because:** completing a request and adding the stop's event is one
  transaction, and capabilities never call each other. A question that
  spans kinds is one op, in the capability whose need it answers, so
  the write's steps are not sequenced in a host (D-228). The shared
  library composes the entry and reads no record, so each receive op
  composes it the same way. A stop is one record, because
  an event's place among events on the same day is the order of writing.
  Hashing the stored bytes keeps the chain checkable when two builds of
  different versions write in turn. The log keeps every state, so the
  tombstone of the documents' design is no longer needed where no merge
  would bring a dropped record back.
- **Costs:** a revert is refused when any later entry changed the same
  stop, not only the same event. cap-stops links transit-requests, and
  runs the requests' write ops from that library (`put`,
  `record_work_order`) beside cap-requests, which keeps them as its own
  ops. A disruptions revert asks cap-log twice, with cap-sync between.
- **Rejected:** requests merged into cap-stops (one module, two needs:
  law 3); the server sequencing a stops write between cap-stops and
  cap-requests (a rule only that host applies: D-228); an event as a
  record (its order is not in its content); an engine of its own for
  each log (two engines to keep, and the same rules written twice).
- **Decided:** D-197, D-198, D-228.

### The derived documents are stored when they are made (`server/derived.go`)
- **Holds:** the network, the boundary, the streets, and the walk table
  are blobs in the record, each the head of its tag's pointer, with the
  module that made it in its envelope. A `derivation` record names the
  inputs and is committed last. A boot whose inputs a derivation names
  reads the stored documents and runs no module. Only the server's
  derivations write these pointers (`POST /pointers` refuses them).
- **Because:** the manifest names these documents, and an auditor needs
  to read the network a pick forked from. Made again at each boot, they
  depend on the importer's build: a release that changes the importer
  makes a different network from the same zip, so the record would name
  a document it can no longer produce. An import is rare, and costs
  C-TRAN about 5 GB at its peak; the stored documents cost a few MB for
  each import.
- **Costs:** a release that writes another version of a document
  derives it again at its first boot, and commits it. The derivation
  record is one more host-owned kind with a hand-written schema.
- **Rejected:** deriving at every boot (the record depends on the
  importer's build); naming the inputs in the pointer record (the
  commit record would change format, and the import's notes would have
  no home); one root that names its documents, as a build does (gc and
  verify would follow one more kind of reference).
- **Decided:** D-199. **Provisional:** D-200 (the derivation record).

### Freezing and adopting are two acts in one log (`crates/transit-plans`, `crates/cap-plans` `receive`, `server/planslog.go`)
- **Holds:** a freeze writes a network version to the record by hash,
  with what it is, and puts nothing in force. Only a frozen version is
  adopted, and the pick names the stops log entry it read. The published
  plan set and the frozen versions are one log, `plans`, so a publish
  (`POST /plans`) freezes and adopts in one entry. Each frozen network is
  the head of its plan's `plan.<id>` pointer when it is frozen. The plans
  log has no generic revert.
- **Because:** a service change often goes to public comment, so
  "published" is not the same as "in force". The record must hold every
  proposal and every network that was ever in force. The pointer keeps
  each frozen network named, so gc keeps it while the log names it by
  hash. A pick is corrected by its own rules (D-75, D-76): a revert of an
  adoption would put an older pick back in force without the chain's
  checks.
- **Costs:** the frozen versions grow without bound, one small record
  for each freeze. An adoption's date is the version's own: a new date
  is a new version, frozen again.
- **Rejected:** freezing as a status on a pick (a proposal that is never
  adopted would have no place); a generic revert of the plans log.
- **Decided:** D-191. **Provisional:** D-201 (how the server serves it).

### Every kind has one entry, beside the server's modules (`crates/cap-abi/src/kinds.rs`, `server/kinds.go`)
- **Holds:** gen-manifest generates one kind table for each build from
  the kind list, writes it beside the server's modules
  (`web/pkg/kinds.json`), and stops while the checked-in copy
  (`crates/cap-abi/kinds.json`) differs. The receive path refuses a pointer write of
  a kind with no entry or with its own route, and checks a pointer kind
  through its module. `/blobs` takes the never-served rule from the
  table's read class.
- **Because:** a write that skips its module makes the data wrong for
  every reader, and the rule for each kind was spread over the handler
  as special cases. The table ships with the modules that enforce it, so
  a rollback of the pages never changes what the receive path takes.
- **Costs:** a new kind is one line more in the list, and the line must
  name its route. The policy is the one kind on the generic path today.
- **Rejected:** the table in the page build (a rollback of the pages
  would change the governance); a table the Go host keeps by hand (a
  second copy beside the schemas).
- **Provisional:** D-202.

### The store describes itself (`server/store/describe.go`)
- **Holds:** each serving process creates the layout, the README, the
  kind table, and the schemas, create only, at boot. A schema marks each
  field that names a blob. `store verify` and `store gc` follow the
  marks, from blobs and from log records. A store with no description
  is read as before, and the build reading stays as its fallback.
- **Because:** the bucket is the agency's record, and it must be
  readable with no app (D-115). A reader learns each document's schema,
  and the links between documents, from the bucket alone. gc keeps a
  blob that only a log names, and verify finds a reference to a missing
  blob.
- **Costs:** gc reads each blob whose kind can name a blob, and every
  log entry, on each pass. A network is never read: its schema marks
  nothing.
- **Rejected:** marking every hash (a log record's `before` names a
  record, and a photo lives in the attachments' bucket); one schema
  object for each kind and version (an added optional field changes the
  schema in the same version, and the store never writes over).
- **Provisional:** D-203.
