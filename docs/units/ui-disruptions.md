# UI: running today's service through disruptions

A generated UI for keeping today's service running when it breaks:
what is late and where that bus is, which disruptions are in force,
drawing a detour along the streets, and which trips are covered or
uncovered. It follows the generated-UI procedure
(`docs/ARCHITECTURE.md`, "Generated UIs"). It is at step 2: no
session has asked these questions as one sequence.

## Questions, in the order asked

1. What is late right now, and where is that bus?
2. Will a late bus make its next trip?
3. Which disruptions are in force today?
4. Where does this detour go: which stops does it skip, and what
   path does it take on the streets?
5. What does the detour do to the trip's path, the boards, and the
   trip plans?
6. Which trips are covered and which are uncovered, and how do I
   cancel or cover one? (Blocked: D-15, for what "covered" means.)
7. Have the others working today seen this change?

## Op inventory (draft, before the session)

| # | op(s) | state |
|---|---|---|
| 1 | rt `live`, `late_trips` | op exists; no UI reads `late_trips` (`docs/units/grade-debt.md`); vehicle labels are ids, QAF-48 (Blocked: D-10) |
| 2 | none | gap, QAF-31 (no live ripple through layovers) |
| 3 | divert `list` | op exists |
| 4 | divert `draft`, streets `snap` / `route`, divert `propose` | op exists; cap-streets is not dogfooded on real data (`docs/units/grade-debt.md`, needs the county export) |
| 5 | runcut `trip_path`, board `stop_board`, router `plan_compare` with `edits_hash` | partial: QAF-50 (path drops the detour), QAF-74, QAF-75 |
| 6 | none | gap, QAF-36 (the edit document is diversions only) |
| 7 | sync `sync_edits` | op exists; not dogfooded, rides with `docs/units/sync-persistence.md` |

## Scope beyond the ops

- **Live input**, as in `docs/units/ui-ride-now.md`: a timer
  refreshes the snapshot; QAF-49 bounds the session length.
- **The edit document is shared state.** Retire is the tombstone,
  and every next document is cap-divert's answer.

## Done when

- The session has run, and the inventory names an op or a finding
  for every question.
- The answer graph table is in this file.
- The UI lands per step 8 with `e2e/ui-disruptions.spec.mjs`, and
  this file is deleted, or edited down to its blocked questions.
