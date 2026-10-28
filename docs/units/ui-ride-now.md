# UI: riding today

A generated UI for someone taking a trip today: find a stop, see
what leaves next and whether it is running late or early, plan a
trip, know the last bus home, and learn about a detour before it
strands them. It follows the generated-UI procedure
(`docs/ARCHITECTURE.md`, "Generated UIs"). It is at step 3: the
dogfood session has not run.

## Questions, in the order asked

1. Which stop is mine, by name or near where I am?
2. What leaves next from it, with real-time predictions?
3. Is my bus on time, late, or early?
4. How do I get from here to there, leaving now?
5. When is the last bus home?
6. Is there a detour or disruption on my trip or at my stop?
7. What leaves next from the stops I watch?

## Op inventory (draft, before the session)

| # | op(s) | state |
|---|---|---|
| 1 | directory `stops` (by name) | by name: op exists. Near me: gap, QAF-70 |
| 2 | rt `live` → board `stop_board` (`live_hash`) | op exists; rows lack long names (QAF-67), headsigns (QAF-68), bay names (QAF-69) |
| 3 | rt `live`, `trip_progress` | op exists; early reads on time, QAF-71 (Blocked: D-7) |
| 4 | router `plan` | op exists; ignores live data, QAF-73 |
| 5 | none | gap, QAF-72 (no latest-departure query) |
| 6 | board `stop_board` / router `plan_compare` with `edits_hash` | partial: QAF-74, QAF-75; no service-alerts decode (log it in the session) |
| 7 | board `watchlist_board` | op exists |

The rider findings in `docs/QA-FINDINGS.md` (QAF-66–75) answer
questions 1–6 on an earlier import. Step 3 re-runs them against the
reference import as one sequence, re-confirms those findings, and
logs what it finds for 6 and 7.

## Scope beyond the ops

- **Live input.** A live snapshot is an input the host refreshes on
  a timer; each poll registers a new document in every consumer
  (QAF-49). A rider-length session is bounded only when
  `docs/units/module-runtime.md`'s eviction lands. Record the
  polling node in the answer graph and in host-discipline.md as an
  engine requirement (step 5).
- **Where edits come from.** Detours reach a rider through the sync
  server (`docs/units/sync-persistence.md`).

## Done when

- The session has run, and the inventory names an op or a finding
  for every question.
- The answer graph table is in this file.
- The UI lands per step 8 with `e2e/ui-ride-now.spec.mjs`, and this
  file is deleted, or edited down to its blocked questions.
