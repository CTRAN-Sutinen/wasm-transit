# Store durability — the sync store on object storage

The sync store (`server/store/`, `docs/FORMATS.md`) is the agency's
record of edits, policy, published picks, and the manifest. Its
production home is object storage — S3 and S3-compatible stores (Ceph
RGW; RustFS for local dev and testing), Azure Blob, and GCS (D-113).
MinIO is not a target, for dev or production (D-113). Today it is a directory on
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

## The design

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
  prefix (listing is strongly consistent on S3, GCS, Azure, and a
  single-site RGW). The server keeps heads in memory; a conflict lists
  from its cached seq onward (`start-after`) and the writer retries.
  Several server processes may share one bucket — the conditional
  create replaces the flock.
- **A crash between the blob and the record** leaves an orphan blob,
  as today. GC lists `blobs/` and deletes what no record names and
  whose last-modified time is past the grace period (24 h); the grace
  covers another process's commit in flight.
- **Backends** sit behind one small interface in `server/store` (get,
  create-if-absent, list by prefix from a key, delete, an object's
  modified time): an in-memory backend that `just test` runs every
  store test over, and an S3 adapter first (tested against the user's
  Ceph RGW), then Azure Blob and GCS. One contract suite runs against
  every backend; the run against a real bucket is opt-in (the
  endpoint, bucket, and credentials from the environment) and never in
  `just test`'s required path. `-store` takes a URL (`s3://bucket/prefix`
  with an endpoint flag for RGW and RustFS, `azblob://…`, `gs://…`,
  `mem://`).
- **The directory store goes** when the S3 backend lands: dev runs
  `mem://`, or RustFS for a store that survives a restart. A one-time copy carries an
  existing directory store into a bucket (blobs with their sidecars as
  metadata, each log line as its record object), and verifies the
  result.

## Scope

1. **Probe the targets.** Whether Ceph RGW honors `If-None-Match: *`
   on `PutObject` depends on its release, and RustFS's support is
   unverified. A probe against the user's RGW instance and a local
   RustFS (create twice, the second must fail with 412; list after
   create sees the key) runs first; without a conditional create, no
   S3 target can hold the CAS.
2. **The backend interface, the in-memory backend, and the contract
   suite**; the engine rewritten over the interface, every existing
   store test passing on `mem://`.
3. **The S3 adapter**, its contract run on RGW recorded here; the
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
8. **Backup and restore** (Blocked: D-82, where the copies go and how
   often). Every object is create-only, so a copy taken records first,
   then the blobs they name, is consistent, and blobs copy
   incrementally by key. Restore points the server at the copy and
   runs `store verify`. A restore drill is part of the deployment
   runbook.

## Done when

- The contract suite passes on `mem://` in `just test`, and on the
  user's Ceph RGW and a local RustFS, recorded here with their
  releases.
- Go: two store instances on one bucket racing commits converge on one
  chained log (`-race`); a crash between blob and record leaves the
  old head readable and the orphan collectable past the grace.
- `just spec` model-checks the object model clean.
- `store verify` passes on a healthy store and fails on each
  deliberately broken fixture in its own test.
- The directory store is gone from `server/store`, and a directory
  store copied into a bucket verifies and serves the same manifest,
  plan set, and edit document.
- The Azure Blob and GCS adapters pass the contract suite against a
  real account.
- A backup of a C-TRAN-sized store, restored into a fresh bucket,
  verifies, and a server started on it serves the same manifest, plan
  set, and edit document. D-82 is decided, and the runbook names the
  drill.
