# Storage — the agency's record

The agency's record is one bucket in object storage: immutable,
content-addressed objects and the pointer logs that name them. Every
other copy of a document — a page's IndexedDB, the bus worker's
registries, a server's head and blob caches, an export, a backup — is
a working copy or a copy of the record, never the record. Who owns
that data and the contract a write meets are
[agency-data.md](agency-data.md); the bytes at rest (keys, envelope,
records, the commit protocol, builds, the invariants no schema can
say) are [`docs/FORMATS.md`](../FORMATS.md); the server that reads and
writes the bucket is [server.md](server.md). This file is the model
those three share: what the record is, what it holds by shape, how it
is written and read, and where it is not yet what it has to be.

## The record

With `-store <url>` the server keeps the record in object storage
(D-113): `s3://bucket/prefix` (S3 or an S3-compatible endpoint,
`?endpoint=` naming it), `azblob://container/prefix`,
`gs://bucket/prefix`, or `mem://` (gone with the process). Which
provider a deployment runs on is the agency's (D-116); the engine
favors none.

The engine (`server/store/`) runs over one small backend interface
(`backend.go`: get, stat, create-if-absent, list from a key, delete)
and assumes only its contract — no append, rename, fsync, or lock. The
in-memory backend and the S3, Azure Blob and GCS adapters (`s3.go`,
`azure.go`, `gcs.go`) all pass one contract suite (`contract_test.go`;
`just store-contract` runs it and every engine law against local
SeaweedFS, Azurite and fake-gcs-server).

- **Blobs** are `blobs/<hex>` objects, immutable, each with its typed
  envelope (kind, schema version, producer) as metadata.
- **Pointers** are logs: each commit is one record object
  `pointers/<name>/<seq>.json`, and the pointer's value is its highest
  record. A commit is a blob create plus a conditional create of
  record N+1 — the CAS on the `from` hash — then a re-read of its
  blob. Every record names its writer and time, so the log is the
  audit trail.
- **Many processes, one bucket.** Several server processes may share
  one bucket; each caches heads and reads at most 2 s behind another's
  commit. The model (`store.tla`, `just spec`) checks processes
  sharing a bucket with the offline collector beside them.
- **Nothing serving deletes.** A serving process opens the store with
  `store.OpenServing`: it creates, reads and lists, and never deletes
  (D-117). The offline tools are `server/cmd/store` (`just store …`):
  `gc` collects orphans with its own delete-capable credentials,
  `verify` reads a whole store and exits non-zero on the first break,
  and `copy` carries a directory store an older build wrote into a
  bucket and verifies it.

The `manifest` pointer is the root (publishing is one CAS on it); the
`edits` pointer is the multi-writer LWW document; the `plans` pointer
is the published plan set, and each pick's network is its own
`plan.<id>` pointer (D-74). Every pointer the server keeps is listed
in `docs/FORMATS.md`, "The pointer log".

## What the record holds, by shape

The agency's data is not one shape, and each shape wants its own
layout. Every kind is stored today as a **document**: one blob per
version, the pointer naming the whole.

| Shape | Kinds | Writes | Growth | Layout today |
|---|---|---|---|---|
| Artifacts — large, published whole | network, streets, walk table, the GTFS feed and street layer read at boot, builds | rare: a publish, an import, a release | per revision | document |
| Config — small, whole | policy, the published plan set, manifest | rare | none | document |
| Collections — records by id, many writers | edits, stops, stop requests | frequent, one record at a time | without bound | document |
| Attachments — binary | photos and site drawings (D-162) | per record | steady | not stored yet |
| Event tables — many rows, append only | none yet (heartbeats keep only the latest; TIDES, AVL and APC are not held) | — | — | — |

A document suits artifacts and config. It does not suit a collection:
a commit stores the whole collection again, so the bucket grows with
the collection's size times its commits (D-121's measurements), and
two writers who change different records conflict.

## Every object explains itself

Every persisted document is plain JSON (gzip allowed) or an external
open format, and carries its kind and version inside it (D-115;
[bus.md](bus.md), "Every document says what it is"). The blob's
envelope repeats them, so a reader maps any blob to its schema with no
module. Each kind's JSON Schema and a worked example are published in
the registry (`registry/documents.json`; [registry.md](registry.md));
`server/reader_test.go` reads a store with nothing but object get and
list, the schemas, and `docs/FORMATS.md`. The schemas are not yet
stored in the bucket itself.

The newest authored kinds keep what they do not know: the stops and
stop requests types carry an `extra` map, so a field a newer writer
added round-trips through an older one.

## Writes

Every write goes through a receive path that checks shape,
invariants, concurrency, and attribution ([agency-data.md](agency-data.md),
"Writes are governed", which tabulates where each holds). By kind:

- **Edits** (`POST /edits`, `POST /pointers/edits`) merge record by
  record: cap-sync's LWW merge per record id, so two writers who
  change different records never conflict.
- **Stops and stop requests** (`POST /stops`, `POST /stop-requests`)
  replace the whole document under `from`: a write naming a head that
  has moved is refused (`409`), even when the two writers changed
  different stops, and the page reads the document again and asks the
  person to make the change again.
- **Picks and the plan set** (`POST /plans`, `/plans/retire`) commit
  the module's canonical documents by CAS.
- **Any other pointer commit** stores the document as given, unchecked
  — the gap in agency-data.md's table.

Permissions are per pointer and action (the policy; D-58..D-61): a
role may write a pointer or not. There is no finer grain, and no kind
is restricted on read except the policy itself (its blob is never
served).

## Reads

- A blob by hash (`/blobs/<hash>`) is immutable, so it is cached
  forever by every layer — the server's byte-bounded blob cache, the
  service worker, any HTTP cache in front.
- A pointer's head (`/pointers/<name>`, `/manifest`, `/edits`) is the
  one thing that can be stale; while the bucket does not answer, a
  server serves the heads it holds, labelled with their age (D-119;
  [server.md](server.md)).
- A pointer's log (`/pointers/<name>/log?since=`) is the change feed:
  the commits after a hash the reader holds.
- Reads are of whole documents. The derived documents a reader asks
  for by name (`/network/document`, `/network/boundary`, `/streets`,
  `/walks`, `/feed`) are computed by the server's modules from the
  record and served beside it; there is no read tier of projections
  apart from the server.

## Where it is going

The storage model as a deliberate design — collections stored record
by record, one receive path driven by each kind's declared layout and
rules, a bucket that carries its own schemas, projections over the
change feed, permissions and restricted reads per change, and the rule
that keeps any future row store a projection or a write-through front
— is [docs/units/storage-model.md](../units/storage-model.md). The
store on each provider's real account and its backups are
[docs/units/store-durability.md](../units/store-durability.md).

## Why

One entry per load-bearing choice, headed by the code it governs.
**Holds** is what must stay true; change it only with the user, after
reading **Because**.

### Object storage, create-only (`server/store/`, `backend.go`, `store.OpenServing`)
- **Holds:** the engine assumes only whole-object writes and a conditional create of an absent key — no append, rename, fsync, or lock. Serving processes never delete; `store gc` runs offline with its own credentials.
- **Because:** the store must survive losing a node, and S3, Azure Blob, and GCS all provide exactly that contract. A server that cannot delete cannot erase history, and the bucket can carry the provider's immutability settings.
- **Rejected:** a crash-hardened directory store (one node, its disk's failure the store's); online GC in every server (races a commit over object storage; needs delete rights everywhere); never deleting (kept as an option for immutable buckets).
- **Decided:** D-113, D-117.

### The record is documents in a bucket, not rows in a database (this file)
- **Holds:** the record is immutable, self-describing objects in open formats and an append-only log of who changed what; no database is the record for any kind.
- **Because:** an agency's data outlives its software and must be auditable. Objects that say what they are stay readable with no running software, any number of processes share one bucket with no coordinator, every past version is one GET, and losing any compute or cache loses nothing. At a transit agency's write rates — moderate to low, 24/7 — the transactions a database offers are not what is scarce; history, availability of reads, and independence from one vendor are.
- **Costs:** a layout per shape has to be designed rather than taken from a database (collections are not yet: `docs/units/storage-model.md`); queries over the record are projections someone builds, not SQL against it; and a workload that does need transactions across records under contention needs an engine in front of the bucket.
- **Rejected:** a relational database as the record (a second durable service to run, back up and migrate; its at-rest format is the vendor's).
- **Decided:** D-113, D-115, D-152.
