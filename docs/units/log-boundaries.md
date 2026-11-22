# Moving a log's boundary

Which kinds share a log is chosen once, and nothing can change it
later. Every other mistake in the record has an undo:

- a wrong record is reverted by a new entry;
- a wrong rule ships a fixed module;
- a wrong shape gets a new kind version and a migration step.

A wrong boundary has none. The goal: a log's boundary moves. Kinds
split into two logs, two logs merge into one, or a kind moves from one
log to another. A move keeps every entry and every reference, and
another move undoes it.

## The design

Two steps make every move.

1. **Seal.** The source log gets a last entry, the seal. The seal
   names the log that continues it and the hash of the source's
   checkpoint at the seal. The seal is an ordinary conditional create.
   It races a writer as any entry does: the seal lands at the next seq,
   or a write lands first and the seal goes after it. After the seal,
   the log refuses every write, and the answer names the log that
   continues it. Every read of the log is still served.
2. **Start.** The target log's first entry names each source as
   `(log, seq, checkpoint hash)` and holds the source's records of the
   kinds that move, as the fold holds them at the seal.

A merge seals two logs and starts one. A split seals one log and
starts two. A kind moves by a split and then a merge. The startup
migrations already write a log's first entry from the heads of the old
pointers (`migrate:stops`, `migrate:plans`; `docs/FORMATS.md`, "The
logs"). A start entry is the same thing, with a sealed log in place of
a pointer.

The rules that keep a move safe:

- **A start entry is a function of the sealed folds.** So a process
  that finds a seal with no target log writes the start entry, and two
  processes that do so write the same bytes. A crash between the two
  steps leaves a pause in writes, never a store that is half moved.
- **The idempotency keys move.** A checkpoint holds every idempotency
  key with its entry. The start entry holds the keys of the moved
  kinds. Without them, a device that resends a write that landed in
  the source log lands it again in the target.
- **The old references stay true.** `(A, seq)` at or before the seal
  names the same entry forever. No entry of `A` exists after the seal.
  A seq pin carries the entry's hash (D-232), so `store verify` proves
  that each pin across a seal names the entry it named before.
- **A write that read a sealed log is told so.** A write whose
  `read_seq` or `read_seqs` names a sealed log is a `409` that names
  the log that continues it. The page reads again.
- **A revert crosses the seal.** The revert of an entry in a sealed
  log is written in the log that continues it, and names
  `(log, seq, entry hash)`. `store revert` lists the references into
  what it drops across the seal as well.
- **A move is undone by a move.** A split is undone by a merge, never
  by deleting the target log. Both moves stay in the record.
- **A move starts only from a clean store.** Before the seal, the move
  runs `store verify` on each source log. When verify reports a break
  (a gap, a quarantined entry, a checkpoint that differs from the fold,
  or a missing blob), the move does not start. It halts with verify's
  report, and the logs stay as they are. A move that starts from a
  broken log carries the break into the log that continues it, where
  it is harder to trace. A broken reference is a finding for a person,
  not a break of the store (D-233), so it does not halt a move. The
  move's report lists the reference findings, and they stay true in
  the log that continues the source.
- **The kind table moves with the log.** The kind table names each
  kind's log (D-202). The seal names the kind table hash that the
  start entry follows. So the record is read under the kind table that
  was in force at each seq.

What a split must check (Blocked: D-241). One log is the only atomic
unit (D-190, D-232). A split puts kinds that shared a log into two
logs. Two things can cross the new boundary:

- an op that writes kinds on both sides in one entry, such as
  completing a stop request and adding the stop's event;
- a reference cycle. A cycle across logs is a design error (D-232).
  The one cycle today, a stop event's `request` and a request's
  `stop`, is in the stops log.

A reference that crosses the boundary needs nothing new: the receive
path judges it as of the seqs the writer read (D-233). A merge makes
more writes atomic, so a merge needs no check.

## Scope, in order

1. **The seal reader.** The log entry format gains the seal
   (`docs/FORMATS.md`). The store engine (`server/store/log.go`)
   refuses a write to a sealed log and serves its reads. cap-log's
   fold stops at the seal. `store verify` walks a seal to the log that
   continues it. `store.tla` models the seal, and `just spec` checks
   it. This lands in a release of its own, before any build that moves
   a log. A build without the seal reader refuses a store that holds a
   seal, as an older build refuses data that a newer one wrote.
2. **The start entry, and a move finished after a crash.** The start
   entry, its idempotency keys, and the process that finishes a seal
   with no target log. Writes to the moving kinds pause between the
   seal and the start (Blocked: D-240).
3. **The moves.** The merge first, because it needs no check. Then the
   split with its check (Blocked: D-241), and a kind moved by a split
   and a merge. Every move runs `store verify` on its source logs
   first, and halts on a break. What runs a move, and when: Blocked:
   D-242.
4. **A revert across a seal.** cap-log's `revert` takes an entry of a
   sealed log. `store revert`'s report of references follows the seal.
5. **Every seq pin carries its entry's hash** (D-232). A pick's
   `stops_seq` is a bare seq today
   (`crates/transit-plans/src/lib.rs`, `Pick`). It gains the entry's
   hash, with a new version of the plans kind and its migration step.

## Done when

- Laws in transit-log:
  - a start entry holds exactly the moved kinds' records of the sealed
    folds, for any generated log and any split of its kinds;
  - a resend whose key landed before the seal lands nothing after it;
  - a merge followed by the split that undoes it gives the same
    records and the same keys;
  - a split is refused when an op or a reference cycle crosses it, as
    D-241 decides.
- Store tests (`server/store/`): a seal that races a write; a crash
  between the seal and the start, finished by a second process with
  the same bytes; a move over a source log with a gap, a quarantined
  entry, or a checkpoint that differs from the fold halts before the
  seal, with verify's report, and writes nothing; a move over a source
  log with a broken reference runs, and its report lists the
  reference; `store verify` on a moved store, through the contract
  suite (`just store-contract`).
- A Go round trip: two server copies share a bucket. One moves a kind.
  The other copy refuses a write to the sealed log and names the log
  that continues it. A write whose `read_seqs` names the sealed log
  is a `409`.
- A page's write refused for a seal waits on the device and lands in
  the log that continues it, in an e2e spec.
- A rehearsal on a copy of a C-TRAN-sized store: a kind moved and moved
  back, `store verify` clean after each move, and the apps show the
  same stops, requests, and plans.
- `DEPLOYMENTS-SERVER.md` tells the operator what a move does: the
  pause, the release that must come first, and the rollback rule.
