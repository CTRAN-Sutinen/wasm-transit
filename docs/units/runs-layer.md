# The runs layer

Cut blocks into operator runs under an agreement's work rules
(`docs/ROADMAP.md` 7). The layer lives beside the network (D-16): its
documents and library are its own (`transit-runs`), and it reads the
network by hash. The work rules, the scheduling constraints, required
recovery, and suggested blocking are on the bus
(`crates/transit-runs/src/lib.rs` and cap-runcut). The runs cut is
what is left. It closes QAF-11, QAF-12, QAF-14, and QAF-15.

The model takes TODS's concepts (D-126). A run is an ordered list of
run events: its trips and its other activities (report, deadhead,
break, relief, clear). A run is keyed by service and run id, and a run
id is unique within its service. A deadhead is a non-revenue trip. A
garage or a relief point is an internal location. Nothing here reads
TODS files. That is the codec's job (step 7).

## Scope, in order

1. **The runs cut.** A library function from a day's blocks, the work
   rules, and the scheduling constraints to runs. Each run is one or
   more pieces of blocks, relieved at relief points, else at a block's
   trip ends. Each run carries its timeline of activities, its class,
   its pay by the minute under the premiums and `combine`, its
   guarantees and travel, and every limit it breaks and every break
   rule it misses. The cut judges limits over a run. Limits over a
   rolling 24 hours or a week wait for a roster. The cut reports the
   mix, the uncovered work, and the work left over, and it follows the
   document's objective. Two sets of options give two cuts to compare.
   A cut for a date outside the rules' `source` period warns. This
   closes QAF-11 and QAF-15.
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

Where the runs ops go is a rule 3 question, judged when step 1 lands.
The runcut answers what a vehicle does today, and a run is a person's
work. The runs cut is most likely its own capability over the same
documents.

## Testing

C-TRAN has no TODS, so the tests use generated data. C-TRAN's GTFS has
real blocks (every trip carries a `block_id`), so a real weekday
serves as input for a session.

- Property tests in transit-runs over generated blocks and the
  generated work rules of `labor_rules_prop_test`. Every piece of
  every block that runs is in exactly one run, or listed as uncovered.
  A run's pay is at least each of its guarantees. Under `highest`, no
  minute is paid above the highest rate that applies to it, and under
  `sum` the rates add. A run that breaks a hard limit names it. A
  measure's total equals the sum of its activities. The same input
  gives the same bytes.
- The paired readings in `crates/fixtures` (`work_rules.rs`). Where
  two readings differ, the runs differ in the way that the reading
  predicts: platform time limits a run with long layovers that driving
  time does not, the state's breaks add a meal to a run over five
  hours, and travel between pieces pays a split run whose second piece
  starts away from where the first ended.
- Hand-cut examples: a small day of blocks, cut and paid by hand under
  the C-TRAN fixture, in `crates/fixtures`.
- The codec round-trips generated runs.

## Done when

- A runs cut on the bus, with its property tests, its op and runner
  gate, and a C-TRAN session reporting the run count, the class mix,
  and the broken rules for a weekday, with the rules named. QAF-11 and
  QAF-15 are deleted with that pin.
- The run view (QAF-14) and run legality (QAF-12) answered, with
  their findings deleted.
- A UI reads them (`docs/units/ui-runcut.md`).
- Generated runs export as GTFS + TODS and import back to the same
  documents (a codec round-trip property), and the export is refused
  without the policy's grant.
