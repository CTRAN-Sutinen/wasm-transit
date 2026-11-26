# The runs layer

Cut blocks into operator runs under an agreement's work rules
(`docs/ROADMAP.md` 7). The layer lives beside the network (D-16): its
documents and library are its own (`transit-runs`), and it reads the
network by hash. The work rules, the scheduling constraints, required
recovery, suggested blocking, and the runs cut are on the bus
(`crates/transit-runs/src/lib.rs`, cap-runcut, and cap-runs). What is
left closes QAF-12, QAF-14, and QAF-134.

The model takes TODS's concepts (D-126). A run is an ordered list of
run events: its trips and its other activities (report, deadhead,
break, relief, clear). A run is keyed by service and run id, and a run
id is unique within its service. A deadhead is a non-revenue trip. A
garage or a relief point is an internal location. Nothing here reads
TODS files. That is the codec's job (step 7).

## Scope, in order

1. **The cut's class mix** (QAF-134). On C-TRAN the cut leaves most
   pieces as part-time runs, and class A falls under its minimum.
   Pairing weighs a full-time run against two part-time runs by pay
   alone. It should also weigh the document's objective and its mix,
   and say what each costs.
2. **The run view.** Report and clear, platform, pay, pieces, and
   deadhead per run (QAF-14). Deadhead needs a garage location as data
   and cap-streets for the distance (QAF-17, `docs/ROADMAP.md` 2 and
   6). Until then, a run's deadhead is reported as unknown, never as
   zero.
3. **Run legality.** Judge a run that a planner cut by hand against
   the rules (QAF-12).
4. **Run edits.** Cut, join, or move a piece. These are edits to the
   runs document, which follows the network's edit discipline.
5. **Suggested blocking, extended.** Today a suggestion keeps
   authored blocks as they are and chains only the trips no block
   carries, into new blocks, with no deadheads (D-109). Still open:
   appending a trip to an authored block's idle end, a full
   re-blocking, chaining frequency trips, and deadheads once a base
   exists (QAF-17).
6. **A page.** The engine loads neither document. No UI holds them,
   shows a wait's recovery (runcut `recovery`), or reads author
   `apply`'s scheduling constraints. This lands with
   `docs/units/ui-runcut.md`.
7. **The GTFS + TODS codec** (D-126): export the network with its
   runs layer as GTFS plus TODS's supplements and `run_events`, and
   import the same, losslessly, in transit-codec beside the GTFS
   codec. The export carries internal data and is served only under
   policy.

## Testing

C-TRAN has no TODS, so the tests use generated data. C-TRAN's GTFS has
real blocks (every trip carries a `block_id`), so a real weekday
serves as input for a session.

- Hand-cut examples: a small day of blocks, cut and paid by hand under
  the C-TRAN fixture, in `crates/fixtures`.
- The codec round-trips generated runs.

## Done when

- A C-TRAN weekday's cut meets the C-TRAN rules' mix, or names the
  pay it would cost to, and QAF-134 is deleted with that pin.
- The run view (QAF-14) and run legality (QAF-12) answered, with
  their findings deleted.
- A UI reads them (`docs/units/ui-runcut.md`).
- Generated runs export as GTFS + TODS and import back to the same
  documents (a codec round-trip property), and the export is refused
  without the policy's grant.
