# The storage model: a log per consistency domain, a store per lifecycle stage

The agency's record moves from whole documents per commit to
append-only logs, one kind of data at a time (D-190). Each log holds
the kinds that must change together. Each entry is one transaction.
The current state is a fold of the entries. A mistake is fixed by a
new entry, so the truth before the mistake stays in the record. Where
data lives follows its lifecycle stage: drafts in a working store,
what the agency froze or adopted in the record, and event data in the
agency's lake. The work starts with the edits log alone. Each later
step starts only after the step before it runs on real data.

## The design

**The logs.** One log per consistency domain:

| Log | Kinds | Why together |
|---|---|---|
| `edits` | Detours (diversions), trip cancels | Dispatch's day: its own writers, rhythm, and retention |
| `stops` | Stops, stop requests, work order status | Closing a request and adding the stop's event is one transaction |
| `plans` | Frozen versions, the plan set, work rules | Freezing and adopting. It names the other logs by `(log, seq)` and never changes them in the same commit. |
| `policy` | The policy | Restricted to read |

A new kind joins a log only if it must change atomically with that
log's kinds. Otherwise it gets a log of its own. Logs name each other
by `(log, seq)`. The logs are immutable, so those names never break.

**An entry.** `logs/<log>/entries/<seq>.json`, with `seq` zero-padded
to 20 digits. A commit is a create-if-absent on `seq+1`, as the pointer
log is today. An entry holds:

- the log's name and the entry's `seq`;
- the writer (the verified sign-in subject, as today), and the time;
- the module and its build that answered the entry;
- the intent: the op or endpoint, and its arguments or a hash of them;
- for each record it changed: the kind, the id, the hash before, and
  the new state, inline;
- the entry it reverts or supersedes, if any.

The entry is a document kind of its own. Its type is in Rust, its
schema is locked, and the module that owns the log's kinds answers
the entry. The host adds the sequence, the writer, and the time, and
commits it.

**The current state.** A checkpoint is the fold of a log up to a
`seq`: the log's document as it is today (for edits, the
`edit_records` document), with a header that names the log and the
`seq`. Checkpoints are written at
`logs/<log>/checkpoints/<seq>.json`. A server boots from the newest
checkpoint and folds the entries after it. Any past state is the same
fold, stopped at an earlier `seq`.

**The rules.**

1. Every write passes the receive path into one entry. A write that
   changes nothing writes no entry.
2. A lost race re-reads the head, runs the receive path again, and
   retries.
3. A fold never crashes on one entry. It quarantines an entry it
   cannot apply, raises an alarm, and continues.
4. A gap in the sequence stops writes to that log. A no-op entry that
   names the cause closes the gap.
5. A revert is a new entry that writes back the states before entry
   N. If a later entry changed one of those records, the revert is
   refused and names the record, because a person decides it.
6. After a create, the engine reads the entry back and compares the
   bytes. A store that ignored the condition is caught at once.

## Scope, in order

1. **The edits log.** The first step, and the only one that starts
   now.
   - The entry kind: its Rust type, schema, and lock. cap-sync answers
     the entry from the head and an incoming document: the records the
     LWW merge changed, with their hashes before. The merge itself
     does not change.
   - The server writes `POST /edits` and the `edit_records` pointer
     commit as entries. `GET /edits` serves the fold. A page's
     protocol does not change: it still sends a whole document, and the
     merge is still idempotent, so a resend writes no entry.
   - `GET /logs/edits?since=<seq>` answers the entries after a `seq`.
     `POST /logs/edits/revert` reverts one entry, for a caller with
     the `audit` grant. The HTTP contract expands and nothing is
     removed (D-81).
   - The first entry on an existing store copies the current `edits`
     head, with writer `migrate:edits`, and names that head's hash.
     The old `edits` pointer stays, read-only.
   - Checkpoints, the boot from the newest checkpoint, quarantine, and
     the gap rule.
   - `store verify` checks each log: no gaps, each entry readable, and
     each checkpoint equal to a fold from the checkpoint before it.
   - D-120 (whether the live state drops records whose dates have
     passed) applies to the fold only. The entries keep every record
     either way.
2. **The stops log** (stops, stop requests, work order status).
   - Writes go in as ops, each with an idempotency key from the
     device, so a resent outbox write lands once (D-189).
   - The stops module merges by event id (D-166). Two writers who
     change different stops both land with no `409`. One event changed
     by both writers is a conflict that names the event.
   - Completing a request and adding the stop's event is one entry.
3. **The plans log, and freezing** (D-191).
   - A freeze writes a network version to the record by hash, with an
     entry that says what it is (for example, a proposal for public
     comment). A frozen version can be in the record and not in force.
   - Adopting puts a frozen version in force from a date, through an
     entry that changes the plan set. Only a frozen version can be
     adopted. An adoption names the `(stops, seq)` it read.
   - The plan set and the work rules move into this log.
4. **The working store, and drafts.**
   - A second store holds what may be deleted: drafts, attachments,
     and caches. The server keeps create, read, and list rights to the
     record, and gc keeps delete rights to the working store only.
   - A draft is a root that lists one blob for each route and the
     shared parts, committed by CAS on the draft's pointer. One planner
     owns a draft. Another planner forks it. A draft has no automatic
     rebase until two people need to edit one draft at once.
   - A draft is a transitory record. gc deletes it once it is frozen
     or abandoned, on the working store's schedule.
5. **The policy log.** The policy moves into a restricted log that the
   server never serves openly.
6. **The kind table, and governed writes.** One table for each build,
   generated beside the registry's documents. It gives each kind's
   log, module, validation op, merge rule, and read class. The receive
   path reads it. A kind with no entry is refused. Today a pointer
   commit for any kind except the edits, the picks, the stops, and the
   stop requests is stored as given
   (`docs/architecture/agency-data.md`, "Writes are governed"). This
   step is independent of the logs and can start at any time.
7. **The store describes itself.** At setup the store creates, create
   only, a `layout` object that names its layout version and hash
   scheme, the build's locked schemas under `schemas/` by content
   hash, and a plain-language README of the layout. Each schema marks
   every field that holds a content hash (`"format": "content-hash"`).
   `store gc` and `store verify` follow references by that mark, and
   an outside reader learns the links from the same schemas.
8. **Exports for other systems.** Each export is a view: it names the
   `(log, seq)` it was built from, and is built again from the logs at
   any time. The first ones are the stops for a GIS (GeoJSON, and
   GeoPackage), and the logs and checkpoints for the agency's lake
   (Parquet). Event data (AVL, APC, TIDES) goes to the lake from the
   systems that produce it. This repo does not store it.
9. **Permissions per change.** The receive path knows which records a
   write changed. The kind's module sorts each change into change
   classes (for stops: a survey, a planned event, an in-service event).
   cap-authz maps roles to classes.

## Done when

For the edits log (scope 1):

- Laws in transit-sync and cap-sync: the fold of the entries equals the
  document that whole-document commits give for the same writes; a
  merge that changes nothing answers no entry; a revert writes back
  the states before, and is refused when a later entry changed the
  same record.
- Store laws, run by `just test` and by `just store-contract` against
  SeaweedFS, Azurite, and fake-gcs-server: entries chain from 1 with no
  gap under racing writers; a lost race retries and lands; a checkpoint
  equals the fold; a gap stops writes; an unreadable entry is
  quarantined and the fold continues; the read-back catches an
  overwritten entry.
- A Go round trip: `POST /edits`, then `GET /edits`, `GET
  /logs/edits?since=`, and a revert, with the existing edits tests
  passing unchanged.
- An existing store gets its first entry from the `edits` head, and
  `store verify` passes on it.
- `just e2e`: the disruptions page issues and reads detours unchanged.
- A dogfood session on the C-TRAN store: a server restarted on the log
  serves the same edit document. The stored bytes for a month of edits
  at the agency's rate are measured, and replace D-121's estimates in
  `docs/units/store-durability.md`.
- `docs/FORMATS.md`, `docs/architecture/storage.md`,
  `docs/architecture/server.md`, and `DEPLOYMENTS-SERVER.md` (bucket
  versioning, and the revert step) describe the edits log.

Each later step gets its own Done when as it starts. The file is
deleted when the last step lands.
