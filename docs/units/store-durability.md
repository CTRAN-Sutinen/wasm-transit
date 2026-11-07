# Store durability — the sync store on object storage

The sync store (`server/store/`, `docs/FORMATS.md`) is the agency's
record of edits, policy, published picks, and the manifest. Its
production home is a managed object store — S3, Azure Blob, or GCS
(D-113); which one a deployment runs on is open (D-116). The engine
is coupled to none of them, nor to any one
S3-compatible server: it assumes only the contract below, which all
three guarantee. Ceph RGW and RustFS are dev conveniences, exactly as
the directory is today — useful only as far as they honor the
contract, and never given a code path of their own. MinIO is not used
at all (D-113). Today it is a directory on
one node, and its guarantees rest on a local POSIX filesystem: `flock`
keeps one writer process, a commit is a file append, and blobs land by
`rename`. A host crash can tear a log tail (the 2026-11-05 WSL2 crash
left NUL-padded files behind), and an object store offers none of the
three. This unit moves the engine onto objects, verifies a store
offline, and gives the agency a backup it can restore.

The engine's API does not change: the server calls `Open`, `Head`,
`CAS`, `GetBlob`, `Log`, `GC`, `Sweep`, and `Close`, and keeps every
rule it has — content-addressed blobs, the module as hash authority,
an append-only log per pointer, a commit that succeeds only from the
head the writer saw. Only what sits under it changes.

## What it serves

The north star for every choice here: a transit agency runs 24/7 or
near it, at moderate to low traffic, with high auditability needs. So
the store favors availability of reads and a provable history over
write throughput — no acknowledged write is ever lost, no serving
process can erase history, and losing any compute or cache loses
nothing.

## The design

- **The portable contract** is everything the engine may assume of a
  backend: a whole-object put that lands entirely or not at all; a
  conditional create of an absent key; a strongly consistent read
  after write and list after write within one bucket; object metadata;
  delete; an object's modified time. Cross-region replication is
  asynchronous on every provider, so one bucket takes the writes and
  any replica serves reads and backup only (D-82). An endpoint that
  breaks the contract is unfit for the store, not a case to work
  around.
- **One primitive: create an object only if its key is absent.** S3
  `PutObject` with `If-None-Match: *`, GCS `ifGenerationMatch=0`,
  Azure Blob `If-None-Match: *`. A whole-object write lands entirely
  or not at all, so nothing is appended, renamed, fsync'd, or locked,
  and no write can tear. The store never overwrites an object.
- **Blobs** are `blobs/<hex>`, created if absent (an existing key is
  the idempotent case: same key, same bytes). The envelope (`kind`,
  `schema_version`, `produced_by`) is the object's metadata, so a blob
  and its envelope are one write — there is no sidecar to lose.
- **The log is one object per record**:
  `pointers/<name>/<seq, zero-padded to 20 digits>.json`, holding
  today's `LogEntry`. A commit writes the blob, then creates record
  N+1. Two writers racing from the same head both try to create N+1;
  one wins, the other's create fails, and it is a `ConflictError` —
  the conditional create is the CAS. A record's `from` is record N's
  `to`, which the writer read.
- **The head is the highest record**, found by listing the pointer's
  prefix (listing is strongly consistent within a bucket — the
  contract). The server keeps heads in memory; a conflict lists
  from its cached seq onward (`start-after`) and the writer retries.
  A read checks freshness with one GET of record seq+1 (a 404 means
  the cached head is current), coalesced to one probe per pointer per
  process every 2 s and made only when a read asks: a head is at most
  2 s behind another process's commit, and the probe's request bill
  stays bounded however many riders read (an idle night makes none).
  Several server processes may share one bucket — the conditional
  create replaces the flock.
- **A crash between the blob and the record** leaves an orphan blob,
  as today; so does a lost CAS. Nothing else is ever garbage: the log
  is append-only and every record names its blob.
- **Serving processes never delete** (D-117). Their credentials
  create, read, and list only, so a compromised server cannot erase
  history and the bucket can carry the provider's immutability
  (S3 Object Lock, Azure immutability policies, GCS bucket lock) for
  the agency's retention period. The online sweep (`-gc-interval`, GC
  at `Open`) goes.
- **`store gc <url>`** is the offline collector, run by hand or on a
  schedule with its own delete-capable credentials: it lists `blobs/`
  and deletes what no record names, past a grace of 7 days, skipping
  objects still under retention. It may run beside live servers, so a
  commit closes the remaining race itself: after its record lands it
  re-reads its blob and re-creates it if missing (the bytes are
  content-addressed, so the re-create is always the same object), and
  `store verify` catches the crash between those two steps.
- **Backends** sit behind one small interface in `server/store` (get,
  create-if-absent, list by prefix from a key, delete, an object's
  modified time): an in-memory backend that `just test` runs every
  store test over, and an S3 adapter first, then Azure Blob and GCS. One contract suite runs against
  every backend; the run against a real bucket is opt-in (the
  endpoint, bucket, and credentials from the environment) and never in
  `just test`'s required path. `-store` takes a URL (`s3://bucket/prefix`
  with an endpoint flag for any S3-compatible dev server, `azblob://…`, `gs://…`,
  `mem://`).
- **Boot from the bucket or a configured external source** (D-118):
  a server's inputs — a GTFS feed, a street layer, the policy — come
  from the bucket or a configured external URL; local files stay a dev
  convenience. A process is disposable: a replacement given the
  bucket and the same source configuration serves the same store.
- **Degraded mode** (D-119): while the bucket does not answer, a
  server serves its cached blobs (immutable, so always correct) and
  cached heads, each pointer response labelled with its head's age
  (seq and `at_ms`), and refuses every write with 503 and
  `Retry-After` — never acknowledging a write the bucket has not
  taken. The browser holds its documents and resends under LWW. No age
  limit fails a read; the pages show the age.
- **The directory store goes** when the S3 backend lands: dev runs
  `mem://`, or RustFS for a store that survives a restart. A one-time copy carries an
  existing directory store into a bucket (blobs with their sidecars as
  metadata, each log line as its record object), and verifies the
  result.

## Sizing

Measured 2026-11-06 on the C-TRAN feed and street layer
(`testdata/`), through the server's own import, publish, and merge:

| Document | Stored bytes (uncompressed — the durable format) |
|---|---|
| Network (one pick revision) | 9.58 MB (0.64 MB gzipped on the wire) |
| Streets | 22.2 MB (4.5 MB gzipped) |
| Walk table | 0.37 MB |
| Plan set, manifest, policy | under 1 KB each |
| Log record | ~365 bytes |
| Edit record | 1.5 KB average (a drawn detour 4.9 KB, a cancel 0.2 KB) |

Everything but the edit document is small: twenty pick revisions and a
few street imports are well under 1 GB a year. The edit document is
the one that grows — whole per commit (D-121) and never shedding
records whose dates have passed (D-120), it reaches terabytes over a
decade at a busy agency's edit rate; the two decisions bound it.

## Scope

1. **Probe the dev endpoints.** Whether Ceph RGW honors
   `If-None-Match: *` on `PutObject` depends on its release, and
   RustFS's support is unverified. The contract suite's conditional-
   create and list cases run against each first (create twice, the
   second must fail with 412; list after create sees the key); an
   endpoint that fails them is not a dev store, and nothing in the
   engine bends to it.
2. **The backend interface, the in-memory backend, and the contract
   suite**; the engine rewritten over the interface, every existing
   store test passing on `mem://`.
3. **The S3 adapter** (first because the dev endpoints speak the S3
   API — no production choice, D-116), its contract run recorded here; the
   server's `-store` URL; `just serve STORE=…`.
4. **The model.** `store.tla` is rewritten for N processes over
   create-only objects: invariants as today (every record names a
   durable blob, the log chains, GC never deletes a named blob, a
   failed CAS leaves authority untouched), with no torn append to
   model.
5. **Verify offline.** `store verify <url>` (Go, no wasm) reads a
   store without writing: every pointer's records chain from seq 1,
   every named blob exists, hashes to its name, and carries its
   envelope. It exits non-zero on the first break. The copy, the
   backup, and the restore run it.
6. **The directory-store copy**, then the directory store's removal.
7. **Azure Blob and GCS adapters**, each passing the contract suite.
8. **Boot sources** (D-118): `-feed`, `-streets` and `-policy` take a
   bucket key or an external URL; the import commits its result with
   the source as provenance, and a source that does not answer at boot
   falls back to the last committed import.
9. **Offline GC** (D-117): the online sweep removed, `store gc`, the
   commit's re-check of its blob, and serving credentials without
   delete.
10. **Degraded mode** (D-119): stale-labelled reads and 503 writes
    while the bucket is down, the freshness probe, and the pages
    showing a head's age.
11. **The edits layout** (Blocked: D-121): whole per commit, or
    record by record.
12. **Backup and restore** (Blocked: D-82, where the copies go and how
   often). Every object is create-only, so a copy taken records first,
   then the blobs they name, is consistent, and blobs copy
   incrementally by key. Restore points the server at the copy and
   runs `store verify`. A restore drill is part of the deployment
   runbook.

## Done when

- The contract suite passes on `mem://` in `just test`, and on each
  managed provider's real account (S3, Azure Blob, GCS — no one of
  them privileged, D-116), recorded here; the dev endpoints it ran on
  (RustFS, RGW) are recorded with their releases, as dev endpoints
  only.
- Go: two store instances on one bucket racing commits converge on one
  chained log (`-race`); a crash between blob and record leaves the
  old head readable and the orphan collectable by `store gc` past the
  grace; `store gc` racing a live commit that reuses an orphan leaves
  no record naming a missing blob.
- A server whose bucket stops answering serves the cached heads with
  their age and answers writes 503, and a client's held edit lands
  once the bucket returns (Go round-trip and a Playwright spec).
- A server booted from an empty disk with only the bucket URL and its
  source configuration serves the same manifest as the one it
  replaces.
- `just spec` model-checks the object model clean.
- `store verify` passes on a healthy store and fails on each
  deliberately broken fixture in its own test.
- The directory store is gone from `server/store`, and a directory
  store copied into a bucket verifies and serves the same manifest,
  plan set, and edit document.
- A backup of a C-TRAN-sized store, restored into a fresh bucket,
  verifies, and a server started on it serves the same manifest, plan
  set, and edit document. D-82 is decided, and the runbook names the
  drill.
