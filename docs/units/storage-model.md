# The storage model: a log per consistency domain, a store per lifecycle stage

The agency's record moves from whole documents per commit to
append-only logs, one kind of data at a time (D-190). Each log holds
the kinds that must change together. Each entry is one transaction.
The current state is a fold of the entries. A mistake is fixed by a
new entry, so the truth before the mistake stays in the record. Where
data lives follows its lifecycle stage: drafts in a working store, and
what the agency froze or adopted in the record. Each step starts only
after the step before it runs on real data.

## The design

**The logs.** One log per consistency domain:

| Log | Kinds | Why together |
|---|---|---|
| `disruptions` | Detours (diversions), trip cancels | Dispatch's day: its own writers, rhythm, and retention |
| `stops` | Stops, stop requests, work order status | Closing a request and adding the stop's event is one transaction |
| `plans` | Frozen versions, the plan set, work rules | Freezing and adopting. It names the other logs by `(log, seq)` and never changes them in the same commit. |
| `policy` | The policy | Restricted to read |

A new kind joins a log only if it must change atomically with that
log's kinds. Otherwise it gets a log of its own. Logs name each other
by `(log, seq)`. The logs are immutable, so those names never break.

The entry, the checkpoint, the commit, and the six rules are built:
cap-log keeps the disruptions log and the stops log (`docs/FORMATS.md`,
"The logs"; `docs/architecture/storage.md`). Each later log is kept by
cap-log too, and each of its kinds' capabilities answers its records
(D-197). The documents the server derives from its inputs are stored in
the record when they are made (D-199; `docs/FORMATS.md`,
"Derivations").

## Scope, in order

1. **The plans log, and freezing** (D-191).
   - A freeze writes a network version to the record by hash, with an
     entry that says what it is (for example, a proposal for public
     comment). A frozen version can be in the record and not in force.
   - Adopting puts a frozen version in force from a date, through an
     entry that changes the plan set. Only a frozen version can be
     adopted. An adoption names the `(stops, seq)` it read.
   - The plan set and the work rules move into this log.
2. **The working store, and drafts.**
   - A second store holds what may be deleted: drafts, attachments,
     and caches. The server keeps create, read, and list rights to the
     record, and gc keeps delete rights to the working store only.
   - A draft is a root that lists one blob for each route and the
     shared parts, committed by CAS on the draft's pointer. One planner
     owns a draft. Another planner forks it. A draft has no automatic
     rebase until two people need to edit one draft at once.
   - A draft is a transitory record. gc deletes it once it is frozen
     or abandoned, on the working store's schedule.
3. **The policy log.** The policy moves into a restricted log that the
   server never serves openly.
4. **The kind table, and governed writes.** One table for each build,
   generated beside the registry's documents. It gives each kind's
   log, module, validation op, merge rule, and read class. The receive
   path reads it. A kind with no entry is refused. Today a pointer
   commit for any kind except the edits, the picks, the stops, the stop
   requests, the builds, and the derived documents is stored as given
   (`docs/architecture/agency-data.md`, "Writes are governed"). This
   step is independent of the logs and can start at any time.
5. **The store describes itself.** At setup the store creates, create
   only, a `layout` object that names its layout version and hash
   scheme, the build's locked schemas under `schemas/` by content
   hash, and a plain-language README of the layout. Each schema marks
   every field that holds a content hash (`"format": "content-hash"`).
   `store gc` and `store verify` follow references by that mark, and
   an outside reader learns the links from the same schemas.
6. **Permissions per change.** The receive path knows which records a
   write changed. The kind's module sorts each change into change
   classes (for stops: a survey, a planned event, an in-service event).
   cap-authz maps roles to classes.

## Done when

Each step gets its own Done when as it starts. The file is deleted when
the last step lands.
