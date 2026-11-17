# UI: reviewing the network

A generated UI that reviews how a plan's network performs. It never
changes a plan (D-180). The planner picks a plan, or a pick, or a
fork made in service-change, and a baseline. The UI then shows what
each service group delivers, who the network reaches, where routes
share a corridor, and where riders wait longest. Between two plans,
it shows who gains and who loses. A weak spot links to service-change,
which is the one UI that changes and publishes a plan
([ui-service-change.md](ui-service-change.md)).

The UI is `web/src/uis/service-design/`. Its name is open (Blocked:
D-181 for the title and the id only). Today it still holds the views
it had when it also changed plans. Taking them out is the first part
of the work below.

## Questions, in the order asked

Orient
1. Which plans exist, which is in force today, and which picks are
   coming? Which plan is under review, against which baseline?
2. What does each service group deliver and cost (trips, routes,
   revenue hours and miles, peak vehicles), against the baseline?
3. How is the service shaped across the day?

Diagnose the network
4. Who does the network reach, and reach frequently, in the hours
   that matter?
5. Where do routes share a corridor? Are their departures spread
   along it or stacked?
6. Where do riders wait longest for the next bus?
7. How many people ride, and which trips run late?

Judge one plan against another
8. Who gains and who loses, in coverage and in connections?

## Op inventory

| # | op(s) | state |
|---|---|---|
| 1 | plans `resolve` / `periods` | answered (PickTimeline); the plan under review is chosen, never forked here |
| 2 | analysis `service_groups`, `service_type_compare`; runcut `suggest_blocks` | answered (GroupTable, GroupLedger): each group a combination of the types that run together, with its date spans (D-55); peak vehicles from the authored blocks and the trips no block carries chained in (D-46); no dollar cost (D-51); hours against a budget, Blocked: D-108 |
| 3 | analysis `hourly_trips` (on the group's first date; every date of a combination answers alike) | answered (DayProfile) |
| 4 | analysis `type_coverage` (`window`), `unserved_points` | answered (CoveragePanel, CoverageMap) for the selected group, over any weighted layer, points or polygons, its field and label the planner's (D-47) |
| 5 | analysis `route_overlap` | answered (CorridorTable); whether departures are spread along a shared segment has no op |
| 6 | analysis `longest_waits` | answered: every stop ranked by its longest wait in the hour window (LongestWaits, D-134) |
| 7 | none | data gap: no ridership (D-53: to be built in, later); adherence history is `docs/ROADMAP.md` 3, read as TIDES (D-127) |
| 8 | analysis `coverage_compare` | on the bus (gained and lost between two named sides, D-99), not in the UI: which combination answers to which across the plans waits on D-43's matching rule (QAF-20); connections made and broken wait on the same rule (Blocked: D-43), then compare `hub_connections` across the two plans |

## What comes out

These views answer questions that service-change answers. They come
out of this UI, with their nodes, components, and ops in `ui.json`:

- forking a plan (PlanList's fork), every edit gesture (GesturePanel,
  the dragged trip and the inserted stop on StringLine), and
  `author.apply`;
- the route view: DirectionTabs, FocusMap, StringLine, TimetableGrid,
  RunTimes, and the frequency grid with its RouteHeader and HourGaps;
- the hubs' HubMatrix, NearMisses, and StopWaits;
- the proposal's ProposalScore, Redline, and ChangeLedger;
- Readiness, export, and publish (`network.validate`,
  `network.export`, `plans.validate`, `POST /plans`).

Two specs drive this UI to publish and to open the published
network (`e2e/published-picks.spec.mjs`, "a planner publishes a fork
from service design"; `e2e/published-network.spec.mjs`, "service
design opens the published network as a plan"). They move to
service-change in the same change. The first fails today: its "Too
late" fork, dated 2026-07-01, has no service in its period, so the
server refuses it for that (`no_service_in_plan_period`,
`server/plans_test.go`) before it checks the date. The moved test
dates the fork inside the fixture's calendar.

## Answer graph

Inputs: `active`, `compare`, `plans` (the documents), `group`,
`window` (the selection, in the URL hash), `view` (which view is
open), `layer`, `coverage-params`.

| node | reads | frame | on screen |
|---|---|---|---|
| document.active / document.compare | active / compare | — | always |
| plans.chain | plans | PickTimeline | always |
| analysis.service_groups | document.active | GroupTable, ContextBar | always |
| analysis.service_type_compare | document.active, document.compare, group | GroupLedger | always |
| runcut.suggest_blocks | document.active, group | GroupLedger | always |
| analysis.hourly_trips | document.active, group, view | DayProfile | network |
| analysis.type_coverage | document.active, group, layer, coverage-params, window, view | CoveragePanel | coverage |
| analysis.unserved_points | document.active, group, layer, coverage-params, view | CoveragePanel, CoverageMap | coverage |
| analysis.route_overlap | document.active, group, view | CorridorTable | corridors |
| analysis.longest_waits | document.active, group, window, view | LongestWaits | waits |

## Left in the views

- **Plans**: each plan's headline numbers against its parent (one
  `service_type_compare` per plan pair, a host loop today).
- **Network**: the day profile stacked by member type.
- **Coverage**: stops drawn served or frequent within the window.
- **Corridors**: shared segments drawn on a map; the stop where a
  corridor's departures stack, by name (QAF-102).
- **Links**: a route, a stop, or a corridor opens in service-change
  on the same plan.

## Done when

- `ui.json` lists no op that changes or publishes a document, and the
  shell has no control that does.
- The two publishing specs above run against service-change.
- A `caps-run` session has run questions 7 and 8, and the inventory
  names an op or a finding for each.
- Every question is answered or names its blocker in the UI, and the
  parts above are built or named in a finding.
- `e2e/ui-service-design.spec.mjs` covers each one.
- `docs/architecture/generated-uis.md` and `docs/guide/apps.md`
  describe the review. Then this file is deleted, or edited down to
  its blocked questions.
