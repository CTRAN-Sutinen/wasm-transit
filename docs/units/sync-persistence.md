# Sync persistence

The Go server today merges edit documents through cap-sync
(`POST /edits`) and keeps the result in process. Persistence is
the store's two git pieces, applied server-side and per device.

## Scope

- **Blobs + pointers**: documents stored as immutable blobs under
  their content hash; a pointer names the current one.
- **Migrate-at-receive**: a document written under an older schema
  migrates when it arrives, not when it is read.
- **Pointer CAS**: a pointer moves only from the hash the writer
  last saw; one writer per prefix.
- **The store reflog**: pointer-move history, so migrations and
  rollbacks are audited facts. The journal of
  [host-discipline.md](host-discipline.md) is the same mechanism
  at per-device scope — there is one persistence design, not two.

The Go server stays scaffolding: blob and pointer storage is I/O,
and any domain rule the server needs becomes a capability module.

## Done when

- Go round-trip tests cover CAS conflicts and reflog replay
  (`go test -race`).
- A C-TRAN session syncs an edit document through a restarted
  server and resolves a real plan chain through cap-plans (this
  also pays the sync/plans line of [grade-debt.md](grade-debt.md)).
