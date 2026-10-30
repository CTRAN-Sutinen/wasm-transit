# UI: designing service

A generated UI for the work of a service change, from start to
finish. The planner learns what the network does today, finds where
it is weak, reshapes routes in a fork, and judges what the fork
costs and who it reaches before publishing it. The UI is six views
chained by one selection (`web/src/uis/service-design/`, described
in `docs/ARCHITECTURE.md`, "Generated UIs"). It answers the
questions below that have an op, and every frame without one says
what it waits on. What is left: the blocked questions, the
remaining parts of the views, and the `caps-run` session for the
questions the 2026-10-30 session did not run (10, 11, 14, 15, 17,
19). The views were first drawn as a clickable prototype:
https://claude.ai/artifact/LWrTKxGV8MyTnMZFZteXEG.

## Questions, in the order asked

Orient
1. Which plans exist, which is in force today, and which picks are
   coming?
2. What does each service group deliver and cost (trips, routes,
   revenue hours and miles, peak vehicles), against the baseline?
3. How is the service shaped across the day?

Diagnose the network
4. How often does each route come, hour by hour, under a service
   group: the target, and how far the trips vary around it? Where
   does the frequent network have holes?
5. What does each route cost, and how fast does it run?
6. Who does the network reach, and reach frequently, in the hours
   that matter?
7. Where do routes share a corridor? Are their departures spread
   along it or stacked?
8. Where do riders wait longest for the next bus?
9. At each hub, which connections are timed, and which just miss?
10. How many people ride, and which trips run late?

Understand one route
11. What is one route: its directions, branches, patterns, and
    stops, and where does it run?
12. What is one direction's day: every trip against time and stop,
    and where does it depart from its target, and why (traffic,
    layover, a relief, a connection)?
13. How long does the route take across the day, segment by segment,
    band by band?
14. Which vehicles does the route take, and how do its blocks cycle?

Change it
15. Fork a pick, and carry its calendar into the pick's period.
16. Change a direction's frequency, span, running time, or stops as
    TARGETS the trips flex around, nudge single trips, and watch the
    timetable move.
17. Reroute, add, or retire a route or a pattern.

Judge the proposal
18. What does the proposal change, route by route, and what does it
    cost?
19. Who gains and who loses, in coverage and in connections?
20. Is the pick ready to publish?

## Flexibility, not a grid

A real timetable is never a rigid interval. A route has a target
headway, and its trips depart from it on purpose through the day:
for traffic, for the running time of the hour, for layover and
recovery, for reliefs, and for timed connections. The UI treats
that variation as information, never as error.

- A headway is shown as its target and its observed range (shortest,
  typical, and longest gap), never as one number standing in for the
  hour.
- Running time is per time band, as the trips run it. No all-day
  figure stands in for it.
- A change states a target and a tolerance. Trips already inside the
  tolerance keep their own times. Only the trips the target needs
  are moved, added, or removed, and each new trip takes the running
  time of its band (QAF-90; D-3, D-54).
- Any trip can be nudged on its own, and a nudge is never undone by
  a later target.
- No view snaps, rounds, or regularizes an authored time.

## Op inventory

| # | op(s) | state |
|---|---|---|
| 1 | plans `resolve` / `periods` / `validate` | answered (PickTimeline); the groups a plan lists repeat equal-calendar families, QAF-88 |
| 2 | analysis `service_groups`, `service_type_compare` | answered (GroupTable, GroupLedger); groups count members that never run on the same day, QAF-91 (Blocked: D-55); peak vehicles, QAF-3 (Blocked: D-46); a dollar cost, Blocked: D-51 |
| 3 | analysis `hourly_trips` (the group's representative date) | answered (DayProfile); it answers per date, not per group |
| 4 | analysis `route_frequency` | answered (FrequencyGrid): each route's hours at its headway stop, the trips' own gaps; shading is continuous (D-52, encoded provisionally) |
| 5 | analysis `route_frequency` (the row's `stats`) | answered: trips, span, revenue hours and miles; speed is a derived number no op answers |
| 6 | analysis `type_coverage` (`window`), `unserved_points` | answered (CoveragePanel, CoverageMap), per service id rather than per group, QAF-89; QAF-78 (Blocked: D-6), QAF-18 and QAF-28 (Blocked: D-47) |
| 7 | analysis `route_overlap` | answered (CorridorTable); whether departures are spread along a shared segment has no op |
| 8 | analysis `stop_gaps` | answered for one stop (StopWaits); not for a station's bays, QAF-92; no network-wide ranking in a window |
| 9 | analysis `hub_connections` | answered (HubMatrix); a near-miss (arriving just after the departure) has no op |
| 10 | none | data gap: no ridership (Blocked: D-53); adherence history is `docs/ROADMAP.md` 3 |
| 11 | directory `route_detail`, `focus` | answered (DirectionTabs, FocusMap); branches read as one direction, QAF-63 (Blocked: D-44) |
| 12 | directory `timetable` (`compare_hash`) | answered (StringLine, TimetableGrid), with the doubled trips of QAF-91; stop spacing is by order — the timetable carries no distance along the pattern |
| 13 | directory `timetable` (`run_secs`) | each trip's end-to-end time answered (RunTimes); by segment and band, no op; the model keeps one profile per pattern, D-3 |
| 14 | runcut `blocks`, `block_trips`, `fleet_summary` | not in the UI: no route scoping; QAF-62 (Blocked: D-9) |
| 15 | author `apply` (`set_plan`) | answered (PlanList); QAF-54 (Blocked: D-41) |
| 16 | author `apply` (`set_span`, `adjust_running_time`, `retime_trip`, `insert_direction_stop`, `remove_direction_stop`) | answered (GesturePanel, a dragged trip on StringLine); the frequency target is Blocked: D-54, D-3 (QAF-90); the gestures act on one service id, QAF-65 (Blocked: D-8) |
| 17 | author `apply` (`add_route`, `add_pattern`, `update_pattern`, `upsert_*`, `remove_*`), streets `route` | not in the UI: no gesture draws a pattern along the streets; D-11, D-26 (a bus) |
| 18 | analysis `service_type_compare` | answered (ProposalScore, Redline) for the selected group; cost, Blocked: D-51; vehicles, QAF-3 (Blocked: D-46) |
| 19 | none | QAF-20 (Blocked: D-43); no transfer delta |
| 20 | network `validate`, author `apply` issues, network `export` | answered (Readiness); QAF-62 (Blocked: D-9), QAF-54 (Blocked: D-41) |

## Answer graph

Inputs: `active`, `compare`, `plans` (the documents), `group`,
`route`, `direction`, `stop`, `window` (the selection, in the URL
hash), `view` (which view is open), `layer`, `coverage-params`.

| node | reads | frame | on screen |
|---|---|---|---|
| document.active / document.compare | active / compare | — | always |
| analysis.service_groups | document.active | GroupTable, ContextBar | always |
| directory.routes | document.active | ContextBar (jump) | always |
| analysis.route_frequency | document.active, group, view | FrequencyGrid, RouteHeader, HourGaps | network, route |
| analysis.hourly_trips | document.active, group, view | DayProfile | network |
| analysis.service_type_compare | document.active, document.compare, group | GroupLedger, ChangeLedger, ProposalScore, Redline | always |
| directory.route_detail | document.active, route, view | DirectionTabs | route |
| directory.timetable | documents, route, direction, group, view | StringLine, TimetableGrid, RunTimes | route |
| directory.focus | document.active, route, view | FocusMap | route |
| analysis.type_coverage | document.active, layer, coverage-params, window, view | CoveragePanel | coverage |
| analysis.unserved_points | document.active, group, layer, coverage-params, view | CoveragePanel, CoverageMap | coverage |
| directory.stops | document.active, view | the hub picker | hubs |
| analysis.route_overlap | document.active, group, view | CorridorTable | hubs |
| analysis.hub_connections / analysis.stop_gaps | document.active, group, stop, view | HubMatrix / StopWaits | hubs |
| plans.chain | plans | PickTimeline | always |
| network.validate | document.active | Readiness, ContextBar (health) | always |

## Left in the views

- **Plans**: each plan's headline numbers against its parent (one
  `service_type_compare` per plan pair — a host loop today).
- **Network**: the day profile stacked by member type; sorting the
  grid by a column (a sort the host may not do — an op parameter or
  a presentation rule, to decide); speed per route.
- **Coverage**: stops drawn served or frequent within the window.
- **Hubs**: shared segments drawn on a map; the connection timeline;
  the network-wide waits ranking.
- **Route**: the pattern strip (a line diagram of branches, stops,
  and timepoints, clicked to pick a stop); the route's blocks; the
  frequency target with its preview (Blocked: D-54).
- **The bar**: the jump box finds routes only — stops, plans, and
  views too.

## Done when

- A `caps-run` session has run questions 10, 11, 14, 15, 17, and 19,
  and the inventory names an op or a finding for each.
- Every question is answered or names its blocker in the UI, and
  the parts above are built or named in a finding.
- `e2e/ui-service-design.spec.mjs` covers each one. Then this file
  is deleted, or edited down to its blocked questions.
