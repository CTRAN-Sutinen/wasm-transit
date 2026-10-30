# UI: comparing service proposals

A generated UI for the public to compare service-plan proposals
against today's service: what changes on my route, at my stop, and
on my trip, and who gains or loses service. It reads the same plans
the service-design UI authors; it never authors them. It follows
the generated-UI procedure (`docs/ARCHITECTURE.md`, "Generated
UIs"). It is at step 2.

## Questions, in the order asked

1. Which proposals are there, and when would each take effect?
2. What changes on my route: trips, frequency, span?
3. How often does the bus come at my stop, before and after?
4. How does my trip change: when do I leave, when do I arrive?
5. Who gains service and who loses it: coverage, and where?
6. How do I respond to a proposal? (Blocked: D-14.)

## Op inventory (draft, before the session)

| # | op(s) | state |
|---|---|---|
| 1 | plans `resolve` / `periods` | op exists; serving proposals to the public waits on the server (`docs/ROADMAP.md` 1, `docs/units/sync-persistence.md`) |
| 2 | analysis `service_type_compare`, directory `timetable` (`compare_hash`) | op exists; a retime reads retimed, never as a cut; branch directions read as one, QAF-63 |
| 3 | analysis `stop_gaps` ×2 | gap: two answers and a host diff (log it in the session) |
| 4 | router `plan` ×2 | gap: `plan_compare` compares edits, not two networks (log it in the session) |
| 5 | analysis `type_coverage` / `unserved_points` ×2 | gap, QAF-20 (no coverage delta op); QAF-18, QAF-28 |
| 6 | none | Blocked: D-14 |

Every question here compares two documents. When the session shows
that most of them need a delta op, that is one finding with a
shared fix: ops that take a baseline hash, as `timetable` does.

## Done when

- The session has run, and the inventory names an op or a finding
  for every question.
- The answer graph table is in this file.
- The UI lands per step 8 with `e2e/ui-proposal-compare.spec.mjs`,
  and this file is deleted, or edited down to its blocked questions.
