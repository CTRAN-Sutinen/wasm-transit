# Store durability — the sync store on object storage

The sync store (`server/store/`; the layout and protocol in
`docs/FORMATS.md`, the model in `docs/architecture/storage.md`) runs over object storage — S3, Azure Blob, GCS —
with offline GC, boot sources, and degraded mode. Which provider a
deployment runs on is the operator's choice, and the operator
checks it with `just store-contract URL`; the engine favors none. What
is left: a backup the agency can restore. How each shape of data is laid out in the
bucket — the edits among them — is [storage-model.md](storage-model.md).

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
the one that grows. Stored whole per commit, as the `edits` pointer
did, it reaches terabytes over a decade at a busy agency's edit rate. The disruptions
log (D-190; `docs/FORMATS.md`, "The logs") stores only the
changed records.

## Contract runs

`just store-contract` runs the contract suite and every engine law
(the pointers', the logs', and verify's) against each endpoint; `just test` runs them on `mem://`.
The local endpoints are for validation only — none is a production
choice, and none has a code path of its own. An operator runs the same
suite against their own bucket (`just store-contract URL`).

| Endpoint | Release | Date | Result |
|---|---|---|---|
| SeaweedFS (S3, podman) | 4.50 | 2026-11-18 | all pass (racing creates ~4 s against ~ms in memory) |
| Azurite (Azure Blob, podman) | 3.37.0 | 2026-11-18 | all pass |
| fake-gcs-server (GCS, podman) | `sha256:797ce226…` (2026-08-24) | 2026-11-18 | all pass |

## Scope

1. **Restore, proved.** Where the copies go and how often is the
   operator's choice, made with the provider's own tools:
   bucket versioning and replication, a periodic copy to a second
   bucket, or both. The repo's part is that a copy works. Every object
   is create-only, so a copy taken records first, then the blobs they
   name, is consistent, and blobs copy incrementally by key. Restore
   points the server at the copy and runs `store verify`. The bucket
   can carry the provider's immutability (S3 Object Lock, Azure
   immutability policies, GCS bucket lock) for the agency's retention
   period: serving processes never delete, and `store gc` keeps what
   retention refuses.

## Done when

- A backup of a C-TRAN-sized store, restored into a fresh bucket,
  verifies, and a server started on it serves the same manifest, plan
  set, and edit document. `DEPLOYMENTS-SERVER.md` names the
  drill.
