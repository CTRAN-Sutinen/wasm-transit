# UI: a service change, from what is weak to what fixes it

A generated UI for the loop of a service change, built beside
[ui-service-design.md](ui-service-design.md) to compare against it.
The need is the same; the shape is not. The service-design UI is one
view per family of answers, and the thread between them (what is
weak, what changes it, whether it worked) is the planner's to carry.
This UI keeps that thread on screen: the network drawn on a map, a
worklist of what is weak, a route opened over the map to change, the
worklist items the planner watches re-read after every change, and a
proposal judged across every day type at once.

## Questions, in the order asked

1. Which plan am I changing, against which baseline, and which day
   types does it run in its period?
2. Where does the network run, and which routes has this plan
   changed?
3. What is weak: frequency holes, long waits, connections that just
   miss at a hub, places it leaves unserved?
4. Opening one finding: what route or hub is it, and what does its
   day look like?
5. Change it: span, running time, one trip, stops — the
   service-design gestures, over the picture.
6. Did it work: the findings I watch, before → after, after each
   change; which findings the plan resolved or created.
7. What does the proposal change across every day type, and is it
   ready to publish?

## Op inventory

| # | op(s) | state |
|---|---|---|
| 1 | plans `resolve`, analysis `day_types` | `day_types` is new: the service groups arranged by the days of the week their dates fall on, inside the plan's period |
| 2 | directory `route_lines`, analysis `service_type_compare` | `route_lines` is new: every route's lines by route id |
| 3 | analysis `network_findings` | new: the worklist (D-136) |
| 4 | directory `timetable`, `route_detail`; analysis `hub_connections`, `missed_connections`, `stop_gaps` | exist |
| 5 | author `apply` | exists |
| 6 | analysis `network_findings` (`base_hash`) | new: each finding with its baseline measure, and the baseline's findings the plan resolved |
| 7 | analysis `plan_compare`, network `validate`, `export`, `POST /plans` | `plan_compare` is new: `service_type_compare` for every day type of the plan's period |

## Done when

- The four ops land with their laws, their dispatch tests, and their
  manifest docs and examples.
- A `caps-run` session on C-TRAN runs each question, and the UI's
  numbers agree with it.
- `web/e2e/ui-service-change.spec.mjs` walks the loop: a finding
  opened, a change made, the watched finding re-read.
- The UI is described in `docs/architecture/generated-uis.md` and in
  the guide; then this file is deleted.
