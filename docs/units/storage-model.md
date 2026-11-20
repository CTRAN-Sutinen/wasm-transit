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
(D-197). The plans log holds the published plan set and the frozen
versions, and the server freezes and adopts through it (D-191;
`docs/architecture/storage.md`). The documents the server derives from
its inputs are stored in the record when they are made (D-199;
`docs/FORMATS.md`, "Derivations"). The kind table governs every
write, and the store describes itself (D-202, D-203; `docs/FORMATS.md`,
"The kind table", "The store describes itself").

## Scope, in order

1. **The work rules in the plans log** (D-191). The server holds no
   work rules today: they live in the planner's browser. This needs a
   write path for them (their records, answered by the capability that
   owns transit-runs' work rules), and the server serving them by hash.
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
   - An attachment that a record names is a gc root (D-232,
     `integrity.md`). gc never deletes it.
3. **The policy log.** The policy moves into a restricted log that the
   server never serves openly.
4. **Permissions per change.** The receive path knows which records a
   write changed. The kind's module sorts each change into change
   classes (for stops: a survey, a planned event, an in-service event).
   cap-authz maps roles to classes.

## Done when

Each step gets its own Done when as it starts. The file is deleted when
the last step lands.
