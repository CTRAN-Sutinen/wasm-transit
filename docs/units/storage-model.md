# The storage model — a layout per shape, one record

The agency's data is heterogeneous: large artifacts published whole,
small config, collections of records many people change, binary
attachments, and — later — event tables of millions of rows
(`docs/architecture/storage.md`, "What the record holds, by shape").
Today every kind is stored as one document per version, which suits
the first two and not the rest. This unit makes the storage model a
deliberate design: each shape gets a layout of its own, every layout
stays readable at rest with no running software, and any engine added
later (a row store, a read cache, a table format) fits under rules
that keep the bucket the one record.

## The rules the design keeps

1. **The bucket is the record.** No other store is the record for any
   kind. Every other store is either a projection, rebuildable from
   the bucket, or a write-through front whose commits reach the
   bucket before a write is acknowledged.
2. **Every object explains itself.** Open format, kind and version
   inside it, its schema in the bucket beside it: a stranger with read
   access and `jq` reads the agency's data.
3. **One write path.** Every change, in any layout and behind any
   engine, passes one receive path: shape, invariants, concurrency,
   permission, attribution.
4. **Views are disposable.** A cache, index, projection or replica
   names the versions it was built from, and is rebuilt from the
   bucket at any time.
5. **History is append-only.** Deletion is a tombstone; the one
   exception is data the law requires removed (D-168).

## Scope, in order

1. **The shapes, before the baseline.** Every stop event gets a stable
   id, so two writers adding events to one stop can be merged
   (Blocked: D-166). Every authored kind keeps what it does not know —
   the `extra` map the stops and stop requests types carry — or none
   does (Blocked: D-167). Both land before the schema lock freezes the
   first deployment's versions
   ([document-versioning.md](document-versioning.md), item 5).
2. **The kind table, and governed writes.** One table per build,
   generated beside the registry's documents: each kind's layout
   (document, collection, table), its module and validation op, its
   merge rule, its read class (open or restricted), and its change
   classes. The server's receive path reads it: every pointer commit
   runs its kind's validation and cap-migrate, and a kind with no entry
   is refused — today a pointer commit for any kind but the edits,
   picks, stops and stop requests is stored as given
   (`docs/architecture/agency-data.md`, "Writes are governed").
3. **Collections, record by record** (Blocked: D-121). A collection's
   pointer names a root, `{"kind":"collection","of":"<kind>",
   "schema_version":N,"records":{"<id>":"sha256:…"}}`, which splits
   into shards keyed by the first byte of the hashed id past a few
   thousand records; each record is a blob of its own, carrying its
   kind and version. A commit creates the changed records and a new
   root, then CASes the pointer as today. When the head moved:
   different records changed → the server rebases, no `409`; the same
   record changed → the kind's module merges (base, ours, theirs:
   LWW for edits, events by id for stops); only an unmergeable record
   is a `409`, naming that record, so the page shows both versions
   instead of discarding the person's change. The document form
   (`{"kind":"stops","stops":{…}}`) stays the interchange format and
   the modules' input, assembled from the tree. Edits, stops and stop
   requests move to it; `store verify` checks every root names only
   records that exist and hash to their names.
4. **The bucket describes itself.** At setup the store creates (create
   only) a `layout` object naming its layout version and hash scheme,
   the build's locked schemas under `schemas/` by content hash, and a
   plain-language README of the layout. Each kind's schema marks every
   field that holds a content hash (`"format": "content-hash"`);
   `store gc` and `store verify` follow references by that mark,
   generically — collection roots, builds, and a record's attachments
   (D-162; [stop-site-and-requests.md](stop-site-and-requests.md),
   item 2) — and an outside reader learns the links from the same
   schemas.
5. **Projections over the change feed.** A projection is a pure
   function of pointer heads run by a module; its output names its
   inputs and producer
   (`{"kind":"view","of":{"stops":"sha256:…"},"by":"cap-stops@sha256:…"}`,
   and `X-View-Of` when served), so any reader checks it against the
   heads. A projector follows `/pointers/<name>/log?since=` and a
   root diff (the record ids whose hashes differ) and recomputes only
   what changed; a page holding root R fetches only the records
   changed since R, so a device's sync is bounded by changes, not by
   history. Where projections run — the server, a read tier, a CDN —
   is the deployment's; the contract is this repo's. The first: the
   stops as GeoJSON for a GIS, the stops on a date, and the pages'
   incremental sync of edits, stops and requests.
6. **Permissions per change, restricted reads** (restricted reads
   Blocked: D-168). The receive path knows which records a write
   changed; the kind's module sorts each change into its change
   classes (stops: a survey, a planned event, an in-service event…),
   and cap-authz maps roles to classes — stewardship per field and
   role, without splitting documents along the org chart. A kind
   whose read class is restricted is never served by hash and is read
   only through authz.
7. **The rebuild drill.** A gate drops every projection, rebuilds each
   from the bucket, and compares by hash. A store that cannot pass it
   has become a record.

## Deferred, until a workload asks

- **Event tables.** High-volume append data (TIDES, AVL, APC,
  heartbeat history) as Parquet with an open table format (Apache
  Iceberg) on the same bucket, committed through a pointer — the same
  design as the record, built for rows, and queryable in place.
- **A transactional engine in front of one collection**, only where
  rules across records under sustained contention outrun the CAS and
  its retry (work bidding is the likely first; [ui-work-bid.md](ui-work-bid.md)).
  It holds working state, writes through to the collection before
  acknowledging, and passes the rebuild drill.
- **Checkpoints** in long pointer logs and an incremental `store
  verify` and `gc`, once a real account's listings measure the need
  ([store-durability.md](store-durability.md), item 1).

## Done when

- The kind table drives the receive path: a Go test commits a kind
  with no entry and is refused, and commits each kind with a broken
  invariant and gets its module's `422`.
- Edits, stops and stop requests are stored as collections: store laws
  for the root, the rebase, and the module's merge (two writers
  changing different stops both land; two changing one event get a
  `409` naming it), and `store verify` checks roots.
- A store created by the engine holds `layout`, the schemas and the
  README, and `server/reader_test.go` reads a collection with nothing
  but the bucket.
- `store gc` keeps every blob a schema-marked field names, proven by
  a store law per reference kind.
- The stops GeoJSON projection and the pages' incremental sync run on
  the C-TRAN store, dogfooded with two people saving stops at once.
- The rebuild drill runs in `just test`.
- D-121, D-166, D-167 and D-168 are decided and encoded.
