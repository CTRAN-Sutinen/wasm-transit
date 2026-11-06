# The runs layer

Cut blocks into operator runs under the work rules (`docs/ROADMAP.md`
7). The layer lives beside the network (D-16): its documents and
library are its own (`transit-runs`), and it reads the network by
hash. The work-rule document (`work_rules`, D-50), required recovery
(D-9), and suggested blocking (D-46) are on the bus
(`docs/ARCHITECTURE.md`, transit-runs and cap-runcut). The runs cut
itself is what is left. It closes QAF-11, QAF-12, QAF-14, and QAF-15.

## Scope, in order

1. **The runs cut.** A library function from a day's blocks and the
   work rules to runs. Each run is one or more pieces of blocks,
   relieved at relief points (the rules' `relief_points`, else a
   block's trip ends). Each run carries its report and clear times
   and places, its platform time and spread, its meal break, and its
   pay in time (the guarantee, overtime, and spread premium). Any
   rule a run breaks is named on the run, never hidden. Laws first:
   every piece of every block that runs on the day is in exactly one
   run; no piece is shorter than the minimum or longer than the
   maximum; a run's pay is at least its guarantee. A cut that leaves
   work uncovered says which work. This closes QAF-11 and QAF-15
   (where a block may be cut).
2. **The run view.** Report and clear, platform, pay, pieces, and
   deadhead per run (QAF-14). Deadhead needs a garage location as
   data and cap-streets for the distance (QAF-17,
   `docs/ROADMAP.md` 2 and 6). Until then, a run's deadhead is
   reported as unknown, never as zero.
3. **Run legality.** Judge a run that a planner cut by hand against
   the rules (QAF-12: the rules now have a home, but nothing judges a
   run yet).
4. **Run edits.** Cut, join, or move a piece. These are edits to the
   runs document, which follows the network's edit discipline
   (`docs/units/ui-runcut.md` question 9).
5. **Suggested blocking, extended.** Today a suggestion keeps
   authored blocks as they are and chains only the trips no block
   carries, into new blocks, with no deadheads (D-109). Still open:
   appending a trip to an authored block's idle end, a full
   re-blocking, chaining frequency trips, and deadheads once a base
   exists (QAF-17).
6. **A page.** The engine has no `work_rules` loader, and no UI holds
   a work-rule document or reads recovery or suggested blocking
   (`docs/units/grade-debt.md`). This lands with
   `docs/units/ui-runcut.md`.

Where the runs ops go is a Law 3 question, judged when step 1 lands:
the runcut's question is "what does this vehicle do today", and a
run is a person's work. The runs cut is most likely its own
capability over the same documents.

## Done when

- A runs cut on the bus, with its laws (step 1), its op and runner
  gate, and a C-TRAN session reporting the run count and the rule
  breaks for a weekday, with the cut's work rules named. QAF-11 and
  QAF-15 are deleted with that pin.
- The run view (QAF-14) and run legality (QAF-12) answered, with
  their findings deleted.
- A UI reads them (`docs/units/ui-runcut.md`).
