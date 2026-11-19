# Storage — the agency's record

The agency's record is one bucket in object storage. The bucket holds
immutable, content-addressed objects, the pointer logs that name them,
and the append-only logs of what people edit. Every other copy of a document is a working copy or a copy of
the record. Examples are a page's IndexedDB, the bus worker's
registries, a server's head and blob caches, an export, and a backup.
Who owns that data and the contract a write meets are in
[agency-data.md](agency-data.md). The bytes at rest are in
[`docs/FORMATS.md`](../FORMATS.md): keys, envelope, records, the commit
protocol, builds, and the invariants no schema can say. The server that
reads and writes the bucket is in [server.md](server.md). This file is
the model those three share. It says what the record is, what it holds
by shape, how it is written and read, and where it is not yet what it
has to be.

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
  then reads the entry back and compares the bytes. The store has two
  logs: `disruptions` (detours and trip cancels, kept by cap-sync) and
  `stops` (the stops and the stop requests, kept by cap-log, which never
  reads a record). Their rules are in `docs/FORMATS.md`, "The logs".
- **Many processes, one bucket.** Several server processes may share
  one bucket. Each caches heads. Each reads at most 2 s behind another
  process's commit. The model (`store.tla`, `just spec`) checks processes
  that share a bucket, with the offline collector beside them.
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
`edits`, `stops`, and `stop_requests` pointers are their history from
before the logs, read only. The `plans` pointer
is the published plan set. Each pick's network is its own `plan.<id>`
pointer (D-74). Every pointer the server keeps is listed in
`docs/FORMATS.md`, "The pointer log".

## What the record holds, by shape

The agency's data is not one shape. Each shape needs its own layout.
A kind is stored as a **document** (one blob per version, and a
pointer names the whole) or as a **log** (one entry per transaction,
and the document is the fold).

| Shape | Kinds | Writes | Growth | Layout today |
|---|---|---|---|---|
| Artifacts: large, published whole | network, streets, walk table, the GTFS feed and street layer read at boot, builds | rare: a publish, an import, a release | per revision | document |
| Config: small, whole | policy, the published plan set, manifest | rare | none | document |
| Collections: records by id, many writers | the edit document (detours, trip cancels) | frequent, one record at a time | without bound | log (`disruptions`) |
| Collections: records by id, many writers | stops, stop requests | frequent, one record at a time | without bound | log (`stops`) |
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
The schemas are not yet stored in the bucket itself.

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
- **Stops and stop requests** are one log. `POST /logs/stops` sends ops,
  each run over the documents at the head, with the device's
  idempotency key. Two writers who change different stops both land. A
  correction of an event that another writer changed since is a `409`
  that names the event. Completing a request and adding the stop's event
  is one entry. `POST /stops` and `POST /stop-requests` still take the
  whole document under `from`, and a moved head is a `409`; each such
  write is one entry too. The work order system's answer and the feed's
  differences run over the head and need no `from`.
- **Picks and the plan set** (`POST /plans`, `/plans/retire`) commit the
  module's canonical documents by CAS.
- **Any other pointer commit** stores the document as given, unchecked.
  This is the gap in agency-data.md's table.

Permissions are set per pointer and action by the policy (D-58..D-61).
A role may write a pointer or not. There is no finer grain. No kind is
restricted on read, except the policy itself and the logs' entries.
The policy's blob is never served. The entries name their writers, so
they need the `audit` rule.

## Reads

- A blob by hash (`/blobs/<hash>`) is immutable. Every layer caches it
  forever. The layers are the server's byte-bounded blob cache, the
  service worker, and any HTTP cache in front.
- A pointer's head (`/pointers/<name>`, `/manifest`, `/edits`) is the
  one thing that can be stale. While the bucket does not answer, a
  server serves the heads it holds, labelled with their age
  (D-119; [server.md](server.md)).
- A pointer's log (`/pointers/<name>/log?since=`) is the change feed.
  It lists the commits after a hash the reader holds.
- A log's entries (`/logs/disruptions?since=<seq>`,
  `/logs/stops?since=<seq>`) are the change feed of what people edit. Each entry says what changed, who changed
  it, and which module answered it. The fold that `/edits` serves, and
  the stops and requests documents that `/blobs` serves under the
  manifest's hashes, can be stale like a head.
- Reads are of whole documents. The derived documents a reader asks for
  by name (`/network/document`, `/network/boundary`, `/streets`,
  `/walks`, `/feed`) are computed by the server's modules from the
  record. The server serves them beside the record. No projection tier
  exists apart from the server.

## Where it is going

The storage model's direction is decided (D-190, D-191). The
disruptions log and the stops log are built. The rest is
[docs/units/storage-model.md](../units/storage-model.md). It covers:

- the disruptions log moved onto cap-log (D-197).
- the logs for the other consistency domains: the plans and the policy.
- a working store for drafts, attachments, and caches, apart from the
  record.
- freezing a version into the record, apart from adopting it.
- one receive path driven by each kind's entry in the kind table.
- a bucket that carries its own schemas.
- exports for a GIS and the agency's lake.
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
  a database. The edit document has one (the disruptions log). The stops
  and the stop requests do not yet (`docs/units/storage-model.md`).
  Queries over the record are projections someone builds, not SQL against
  the record. A workload that needs transactions across records under
  contention needs an engine in front of the bucket.
- **Rejected:** a relational database as the record. It adds a second
  durable service to run, back up, and migrate. Its at-rest format is the
  vendor's.
- **Decided:** D-113, D-115, D-152.

### What people edit is an append-only log (`server/store/log.go`, `server/disruptions.go`, `transit_sync::log`)
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
  entry may hold the records of two capabilities. `before` hashes the
  record as stored. In a log that cap-log keeps, a revert takes a record
  the reverted entry created out of the document.
- **Because:** completing a request and adding the stop's event is one
  transaction, and capabilities never call each other. A log module that
  reads no record lets the server compose one entry from the changes of
  two modules without deciding anything. A stop is one record, because
  an event's place among events on the same day is the order of writing.
  Hashing the stored bytes keeps the chain checkable when two builds of
  different versions write in turn. The log keeps every state, so the
  tombstone of the documents' design is no longer needed where no merge
  would bring a dropped record back.
- **Costs:** a revert is refused when any later entry changed the same
  stop, not only the same event. A compound write runs one module per
  kind and one more to compose. The disruptions log has its own engine
  until it moves onto cap-log.
- **Rejected:** requests merged into cap-stops (one module, two needs:
  law 3); a cap-stops op that depends on transit-requests (couples two
  crates); an event as a record (its order is not in its content).
- **Decided:** D-197, D-198.
