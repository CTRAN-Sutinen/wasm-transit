# Store durability — the sync store on object storage

The sync store (`server/store/`; the layout and protocol in
`docs/FORMATS.md`, the engine in `docs/ARCHITECTURE.md`, server
section) runs over object storage — S3, Azure Blob, GCS (D-113) —
with offline GC, boot sources, and degraded mode. Which provider a
deployment runs on is open (D-116); the engine favors none. What is
left: the contract run on each provider's real account, the edits
layout, and a backup the agency can restore.

The north star for every choice here: a transit agency runs 24/7 or
near it, at moderate to low traffic, with high auditability needs. So
the store favors availability of reads and a provable history over
write throughput — no acknowledged write is ever lost, no serving
process can erase history, and losing any compute or cache loses
nothing.

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

## Contract runs

`just store-contract` runs the contract suite and every engine law
(20 laws) against each endpoint; `just test` runs them on `mem://`.
The local endpoints are for validation only — none is a production
choice, and none has a code path of its own.

| Endpoint | Release | Date | Result |
|---|---|---|---|
| SeaweedFS (S3, podman) | 4.50 | 2026-11-07 | all pass (racing creates ~4 s against ~ms in memory) |
| Azurite (Azure Blob, podman) | 3.37.0 | 2026-11-07 | all pass |
| fake-gcs-server (GCS, podman) | `sha256:797ce226…` (2026-08-24) | 2026-11-07 | all pass |

## Scope

1. **The real accounts.** The contract suite against S3, Azure Blob
   and GCS proper (`STORE_TEST_URL` and each provider's credentials),
   recorded above — no one of them privileged (D-116). Azure lists
   from a key with `StartFrom` and filters client-side; a run against
   a real account says whether a pointer's refresh stays one page.
2. **The edits layout** (Blocked: D-121): whole per commit, or
   record by record.
3. **Backup and restore** (Blocked: D-82, where the copies go and how
   often). Cross-region replication is asynchronous on every provider,
   so one bucket takes the writes and any replica serves reads and
   backup only. Every object is create-only, so a copy taken records
   first, then the blobs they name, is consistent, and blobs copy
   incrementally by key. Restore points the server at the copy and
   runs `store verify`. The bucket can carry the provider's
   immutability (S3 Object Lock, Azure immutability policies, GCS
   bucket lock) for the agency's retention period — serving processes
   never delete, and `store gc` keeps what retention refuses. A
   restore drill is part of the deployment runbook.

## Done when

- The contract suite passes on each managed provider's real account,
  recorded under "Contract runs".
- A backup of a C-TRAN-sized store, restored into a fresh bucket,
  verifies, and a server started on it serves the same manifest, plan
  set, and edit document. D-82 is decided, and the runbook names the
  drill.
- D-121 is decided and the edits layout matches it.
