# Storage — the agency's record

The agency's record is one bucket in object storage. The bucket holds
immutable, content-addressed objects and the pointer logs that name
them. Every other copy of a document is a working copy or a copy of
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
- **Many processes, one bucket.** Several server processes may share
  one bucket. Each caches heads. Each reads at most 2 s behind another
  process's commit. The model (`store.tla`, `just spec`) checks processes
  that share a bucket, with the offline collector beside them.
- **Nothing serving deletes.** A serving process opens the store with
  `store.OpenServing`. It creates, reads and lists. It never deletes
  (D-117). The offline tools are in `server/cmd/store` (`just store …`).
  `gc` collects orphans with its own delete-capable credentials.
  `verify` reads a whole store and exits non-zero on the first break.
  `copy` carries a directory store that an older build wrote into a
  bucket, then verifies it.

The `manifest` pointer is the root. Publishing is one CAS on it. The
`edits` pointer is the multi-writer LWW document. The `plans` pointer
is the published plan set. Each pick's network is its own `plan.<id>`
pointer (D-74). Every pointer the server keeps is listed in
`docs/FORMATS.md`, "The pointer log".

## What the record holds, by shape

The agency's data is not one shape. Each shape needs its own layout.
Every kind is stored today as a **document**: one blob per version,
and the pointer names the whole.

| Shape | Kinds | Writes | Growth | Layout today |
|---|---|---|---|---|
| Artifacts: large, published whole | network, streets, walk table, the GTFS feed and street layer read at boot, builds | rare: a publish, an import, a release | per revision | document |
| Config: small, whole | policy, the published plan set, manifest | rare | none | document |
| Collections: records by id, many writers | edits, stops, stop requests | frequent, one record at a time | without bound | document |
| Attachments: binary | photos and site drawings (D-162) | per record | steady | not stored yet |
| Event tables: many rows, append only | none yet (heartbeats keep only the latest; TIDES, AVL and APC are not held) | n/a | n/a | n/a |

A document suits artifacts and config. It does not suit a collection.
Each commit stores the whole collection again. So the bucket grows with
the collection's size times its commits (D-121's measurements). Two
writers who change different records conflict.

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
  change different records never conflict.
- **Stops and stop requests** (`POST /stops`, `POST /stop-requests`)
  replace the whole document under `from`. A write that names a head
  that has moved is refused with `409`. That happens even when the two
  writers changed different stops. The page then reads the document
  again and asks the person to make the change again.
- **Picks and the plan set** (`POST /plans`, `/plans/retire`) commit the
  module's canonical documents by CAS.
- **Any other pointer commit** stores the document as given, unchecked.
  This is the gap in agency-data.md's table.

Permissions are set per pointer and action by the policy (D-58..D-61).
A role may write a pointer or not. There is no finer grain. No kind is
restricted on read, except the policy itself. The policy's blob is
never served.

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
- Reads are of whole documents. The derived documents a reader asks for
  by name (`/network/document`, `/network/boundary`, `/streets`,
  `/walks`, `/feed`) are computed by the server's modules from the
  record. The server serves them beside the record. No projection tier
  exists apart from the server.

## Where it is going

The storage model's direction is decided (D-190, D-191) and not yet
built. The plan is
[docs/units/storage-model.md](../units/storage-model.md). It covers:

- append-only logs, one for each consistency domain, as the record for
  what people edit, starting with the edits log.
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
  a database. Collections do not have one yet (`docs/units/storage-model.md`).
  Queries over the record are projections someone builds, not SQL against
  the record. A workload that needs transactions across records under
  contention needs an engine in front of the bucket.
- **Rejected:** a relational database as the record. It adds a second
  durable service to run, back up, and migrate. Its at-rest format is the
  vendor's.
- **Decided:** D-113, D-115, D-152.
