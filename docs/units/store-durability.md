# Store durability — surviving the host, and restoring from a copy

The sync store (`server/store/`, `docs/FORMATS.md`) is the agency's
record of edits, policy, published picks, and the manifest, and it
lives in one directory on one node. The TLA+ model proves the engine's
interleavings under the assumption that a log append and a rename are
atomic and durable. A host crash can break that assumption. A crashed
WSL2 VM (2026-11-05) left NUL-padded and truncated files behind. This
unit makes the store survive a torn write, verifies it offline, and
gives the agency a backup it can restore.

## Scope

- **A torn log tail.** A crash during the append can leave the last
  line of `pointers/<name>.log` partial or NUL-padded. `store.Open`
  then refuses the whole log ("pointer X log line: …"), and the
  server does not start. Recovery: the last record counts only if it
  parses, chains (`seq`, `from`), and names a blob whose bytes hash
  to it. A torn tail is truncated back to the last valid record,
  fsync'd, logged loudly, and its bytes kept beside the log
  (`<name>.log.torn-<at_ms>`), never silently dropped. Only the tail
  recovers: a bad record mid-log is not a crash artifact, and the
  store still refuses it.
- **A blob without its sidecar.** `putBlobLocked` returns early when
  the blob file exists (`store.go`, the idempotent path). A crash
  between the blob write and the sidecar write, followed by a commit
  of the same bytes, therefore commits a pointer to a blob that
  `GetBlob` answers `ErrNotFound`. The idempotent path writes a
  missing sidecar.
- **Directory durability.** `writeFileDurably` fsyncs the file and
  renames it, but never fsyncs the directory, so the rename itself
  can be lost on power failure. Fsync `blobs/` after a rename, and
  `pointers/` when a log file is created.
- **Heads verified at open.** Each pointer's head blob is hashed at
  `Open` (C-TRAN's largest blob is ~10 MB). A head that does not hash
  to its name fails loudly and names the pointer, the way a mid-log
  break does.
- **The model.** `store.tla` gains a torn append (a crash that leaves
  a partial record) and the recovery step, and the sidecar as its own
  write. The invariants stay: every pointer names a durable, readable
  blob, and recovery never moves a head past a valid record.
- **Verify offline.** A `store verify` command (Go, no wasm) checks a
  store directory: every log chains, every named blob exists, hashes
  to its name, and has its sidecar. It exits non-zero on the first
  break. The backup and restore below both run it.
- **Backup and restore** (Blocked: D-82, where the copies go and how
  often). The store's shape makes a consistent copy simple. Logs are
  append-only and blobs are written before the record that names
  them, so copying the logs first and then the blobs gives a copy in
  which every named blob is present. Blobs copy incrementally by
  name. Restore points the server at the copy and runs `store
  verify`. A restore drill is part of the deployment runbook.

## Order

1. The sidecar fix and directory fsync (small, independent).
2. Torn-tail recovery with its Go tests, then the TLA+ model.
3. `store verify`, then backup and restore.

## Done when

- Go: a log with a partial last line, and one NUL-padded at its end,
  opens to the last valid record, keeps the torn bytes beside it, and
  logs it; a bad record mid-log is still refused; a blob whose
  sidecar was lost is readable after the same bytes are committed
  again; a head that does not hash to its name fails at `Open`.
- `just spec` model-checks the torn-append model clean.
- `store verify` passes on a healthy store and fails on each
  deliberately broken fixture in its own test.
- A backup of a C-TRAN-sized store, restored into a fresh directory,
  verifies, and a server started on it serves the same manifest,
  plan set, and edit document. D-82 is decided, and the runbook names
  the drill.
