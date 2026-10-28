# UI: runcutting

A generated UI for cutting a service plan's work, in two layers:

- **Blocking**: chain a plan's trips into vehicle blocks with
  layovers and deadheads, and answer the peak vehicle requirement.
  This is inside the network model (`docs/ROADMAP.md` 2).
- **Runs**: cut blocks into operator runs under work rules. This is
  the runs layer (`docs/ROADMAP.md` 7): its own documents and
  modules beside the network (D-16), none of which exist yet.

It follows the generated-UI procedure (`docs/ARCHITECTURE.md`,
"Generated UIs"). The blocking questions are at step 2; the runs
questions are at step 2 with no op to inventory, so their session
logs the layer's op gaps.

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
| 1 | runcut `blocks`, `block_trips` | op exists; no structure summary, QAF-15 |
| 2 | runcut `fleet_summary` | op exists for a day; a proposal's vehicle need is a gap, QAF-3 |
| 3 | author `apply` issues | gap: QAF-62 and QAF-26 (Blocked: D-9) |
| 4 | author `apply` (`add_block`, `add_block_trip`, `remove_block_trip`, `set_layover`) | op exists; no op proposes a chaining (log it in the session) |
| 5 | none | gap: deadhead estimation over cap-streets (`docs/ROADMAP.md` 2, 6) |

## Questions (runs), in the order asked

6. Which runs does a cut make from these blocks, and how many?
7. For each run: its pieces, report and clear times and places,
   platform time, and pay.
8. Is each run legal under the work rules?
9. Cut, join, or move a piece, and see what it does to the runs.

| # | state |
|---|---|
| 6 | gap, QAF-11 (no runs layer); block structure, QAF-15 |
| 7 | gap, QAF-14 (no run view); relief points and garages, QAF-17 |
| 8 | gap, QAF-12 (no work-rule document) |
| 9 | gap: no run edits (they follow the runs document) |

## Done when

- The session has run, and the inventory names an op or a finding
  for every question.
- The answer graph table is in this file.
- The UI lands per step 8 with `e2e/ui-runcut.spec.mjs`, and this
  file is deleted, or edited down to its blocked questions.
