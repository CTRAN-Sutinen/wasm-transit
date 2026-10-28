# Sync persistence

The Go server today merges edit documents through cap-sync
(`POST /edits`) and keeps the result in process. Persistence is
the store's two git pieces, applied server-side and per device.
Durable storage is not a closed system: the blobs are
self-describing and documented, so another application or a
person reads them without any of the wasm.

## Scope

- **Blobs + pointers**: documents stored as immutable blobs under
  their content hash; a pointer names the current one.
- **Migrate-at-receive**: a document written under an older schema
  migrates when it arrives, not when it is read.
- **Pointer CAS**: a pointer moves only from the hash the writer
  last saw. Writers may contend per pointer: a failed CAS
  re-reads the current document and answers again — the conflict
  function is the document kind's (the edit document joins, LWW
  merge; a plan forks, the chain resolves). The store stays
  generic; no single-writer invariant depends on topology.
- **The store reflog**: pointer-move history, so migrations and
  rollbacks are audited facts. The journal of
  [host-discipline.md](host-discipline.md) is the same mechanism
  at per-device scope — there is one persistence design, not two.
- **Self-describing blobs**: every stored document carries a typed
  envelope — `kind` (network_document, edit_records,
  diversion_draft, manifest), `schema_version`, `produced_by` —
  next to the bytes the content hash names. The hash names them,
  the envelope says how to read them. GTFS stays the interchange
  for published products; the envelope covers what GTFS cannot
  express (patterns, directions, timing profiles, the plan
  chain), and no second interchange format is invented.
- **Generated schemas**: JSON Schema emitted from the serde types
  (the TS-bindings rule: Rust is the source, the schema is
  generated, never hand-written) and published with the registry's
  evidence. `docs/FORMATS.md` documents the invariants the schema
  cannot (dates as "YYYY-MM-DD", times as u32 seconds past
  86400, the LWW order, tombstone semantics) with a worked
  example per kind.
- **Explicit versioning**: a `schema_version` bump is deliberate;
  old blobs stay readable under their hash (a reader pins a
  version and ignores newer ones), and migrate-at-receive is the
  system's own reader of the same rule.
- **At rest, not on the wire**: the self-describing contract binds
  what the store persists. Sync payloads and caches may compress
  for performance — decompression happens before a document is
  stored or read, and no compressed format is ever the durable
  one.
- **The commit is a pointer CAS**: blob writes are additive and
  idempotent (a torn write is an orphan, garbage to collect, not
  corruption); a change commits as ONE CAS from the hash the
  writer last saw, and the reflog entry is its audit. Sync
  redelivery is idempotent — `merge` is a pure semilattice, so
  at-least-once transport converges to the same hash; no
  exactly-once delivery is needed. Caches (server memory, device
  stores) are materializations of pointer + blobs: torn or stale
  means rebuild, never a correctness fact. Cross-pointer
  atomicity is not built until a real need lands — it would be a
  commit object (one pointer naming a set of hashes, git-tree
  style); per-pointer contention already covers multi-writer
  documents.
- **The engine is model-checked**: the store engine (writers
  contending per pointer, GC, crash points) has a TLA+ spec,
  model-checked for: every pointer, current or in the reflog,
  names a durable blob; the reflog is an append-only history
  whose from-hash is the prior pointer; GC never deletes a blob
  any pointer or reflog entry names; a failed CAS leaves
  authority untouched; and devices receiving the same records in
  any order, at-least-once, converge to the same hash. The merge
  semilattice stays in proptest — it is a pure function, and the
  spec covers the engine's interleavings, not algebra the
  property tests already own.

The Go server stays scaffolding: blob and pointer storage is I/O,
and any domain rule the server needs becomes a capability module.

## Done when

- Go round-trip tests cover CAS conflicts and reflog replay
  (`go test -race`), including idempotent redelivery of the same
  edit batch (same resulting hash) and a crash between blob write
  and pointer move (the old authority stays readable; the orphan
  is collected).
- The TLA+ spec is model-checked with no invariant violations
  at small bounds (two writers contending on one pointer, one
  GC, every crash point): TLC (explicit, exhaustive — the
  default for a state space this small), or Apalache (symbolic,
  same module) if the bounds outgrow enumeration. `just spec`
  runs it, separate from `just test`, which must stay runnable
  without Java.
- A reader outside the wasm interprets a stored blob through the
  envelope, the published schema, and `docs/FORMATS.md` alone
  (a Go test validates a blob against the generated JSON Schema
  and reads the plan chain from it — no Rust, no wasm).
- The registry publishes the schemas and the worked examples as
  evidence, and `docs/FORMATS.md` lands in the same change.
- A C-TRAN session syncs an edit document through a restarted
  server and resolves a real plan chain through cap-plans (this
  also pays the sync/plans line of [grade-debt.md](grade-debt.md)).
