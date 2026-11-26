# UI: runcutting

A generated UI for cutting a service plan's work, in two layers:

- **Blocking**: chain a plan's trips into vehicle blocks with
  layovers and deadheads, and answer the peak vehicle requirement.
  This is inside the network model (`docs/ROADMAP.md` 2).
- **Runs**: cut blocks into operator runs under work rules. This is
  the runs layer (`docs/ROADMAP.md` 7): its own documents and
  library beside the network (D-16). The work-rule document is on
  the bus; the runs cut is [runs-layer.md](runs-layer.md).

It follows the generated-UI procedure (`docs/architecture/generated-uis.md`). The blocking questions are at step 2.

## Questions (blocking), in the order asked

1. What blocks run on a day, and how are they shaped (span, gaps)?
2. How many vehicles does the plan need at peak, and how does a
   proposal change that?
3. Does each block's cycle still work after a change?
4. Chain these trips into a block, or move a trip to another block.
5. What does the deadhead between two trips cost?

## Op inventory (blocking; draft, before the session)

| # | op(s) | state |
|---|---|---|
| 1 | runcut `blocks`, `block_trips`; runs `pieces`, `cut` | op exists: where a block is cut, at its reliefs under the rules |
| 2 | runcut `fleet_summary`, `suggest_blocks` | op exists: a proposal's vehicle need is `suggest_blocks`' added_vehicles; fleet summary per service type, QAF-3 |
| 3 | author `apply` issues (`scheduling_constraints_hash`), runcut `recovery` | op exists: a wait is judged against the recovery required there, and the recovery view shows required vs slack |
| 4 | author `apply` (`add_block`, `add_block_trip`, `remove_block_trip`, `set_layover`), runcut `suggest_blocks` | op exists: a suggested chaining, adopted through its edits |
| 5 | none | gap: deadhead estimation over cap-streets (`docs/ROADMAP.md` 2, 6) |

## Questions (runs)

Runs `cut` answers which runs a cut makes, with each run's pieces,
timeline, pay, and broken rules. Runs `pieces` answers the pieces
alone, `compose` suggests runs from any pieces, and `judge` judges a
set of runs that anyone composed: a planner, or an operator at the
bid. The UI reads `judge` for a run composed by hand. Run edits have
no op yet. They are [runs-layer.md](runs-layer.md)'s runs document,
and this UI reads them when they land.

## Done when

- The session has run, and the inventory names an op or a finding
  for every question.
- The answer graph table is in this file.
- The UI lands per step 8 with `e2e/ui-runcut.spec.mjs`, and this
  file is deleted, or edited down to its blocked questions.
