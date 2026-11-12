# Generated UIs

A generated UI is a layout built for a set of questions, made from
THIN components over its own host layer. Each one is a directory, and the build, the picker,
and the gates discover it. Nothing central is edited to add one.

Each generated UI is its own product: it builds alone (`just build-ui
<id>`) and deploys on its own, with the server or, when it needs no
server, as a static site ([README.md](README.md), "Each UI is its own
product"; [static-deployment.md](static-deployment.md)). A new UI adds
its row to that README's table, saying what it needs the server for.

```
web/<id>.html                  the page (copy service-design.html; change the script src)
web/src/uis/<id>/ui.json       the descriptor the picker renders: id (= the directory),
                               title, summary, questions, ops ("<cap>.<op>" — the
                               capabilities they name are what its linked assembly
                               links; the kernel's loads are not ops), components,
                               session (the dogfood session: date, feed, open
                               findings), spec, generated, and `offline: true`
                               for a page that installs and works offline
                               ([updates-offline.md](updates-offline.md))
web/src/uis/<id>/main.ts       imports ../base.css (+ maplibre css if it maps), mounts
                               Shell, then awaits the host's boot (the layout paints first)
web/src/uis/<id>/Shell.svelte  PLACEMENT ONLY: reads the host's answers state, passes
                               props, routes events to host functions
web/src/uis/<id>/host.svelte.ts the ONLY file of the UI that asks the engine; owns
                               I/O, the selection, and one $state object of answers
web/e2e/ui-<id>.spec.mjs       the UI's e2e spec (fixture feed: e2e/fixtures/)
```

Shared by every generated UI:

- `src/uis/base.css`: color tokens (`--surface --text --muted
  --line --hover --accent --accent-soft --bad --warn --ok`) in light and
  dark, the page base, `.wb-scroll` (a table that scrolls in its own
  box with an opaque sticky header), and the class names the shared
  components render with (`.muted`, `.table-scroll`, `.chip`,
  `.picker-row`).
- `src/uis/kit.ts` (host layer only): the clock — `today()` and
  `nowSecs()` read the instant in the AGENCY's zone (the manifest's
  `timezone`, read once at boot by `readAgencyZone()` before the page
  picks its network; a loaded zip's by `setAgencyZone()`), never the
  device's, through `src/clock.ts`: the agency's date, and seconds from
  noon minus 12 h on it (the GTFS anchor — midnight except on a
  clock-change day; the fall-back hour before it clamps to 0);
  `previousDay()` is the same instant on the previous date's clock
  and that date's length, which a board takes as `previous` (the
  previous date's after-midnight trips, and the fall-back hour read
  truly). `deploymentFeeds()` and `fetchFeed()` read the live feeds
  `live-feeds.json` names, directly — or through the dev relay its
  `proxy` names (`/rt`, D-67). `installApp()`, `buildWaiting()`,
  `applyBuild()`, and `serverAnswers()` are the build lifecycle
  ([updates-offline.md](updates-offline.md)). `whileVisible()` is every page's poll: a hidden page makes no
  requests and re-reads on its return. The answer graph is TRANSIENT — hand-built ahead of the flow
  maps `docs/units/host-discipline.md` designs, which replace it: a
  host declares its answers as `AnswerNode`s (the inputs each reads,
  the frame that renders it, when that frame is on screen), and
  `answerGraph` answers the nodes downstream of a changed input in
  declared order, yielding between nodes so each frame paints as
  its answer lands; a request made mid-pass supersedes the pass,
  and a hidden frame's node waits until it shows. It carries no
  domain knowledge. Every op a node runs is asked of the engine
  (`src/engine.ts`).
- `src/plans.svelte.ts` (`getAllPlans`, `putPlan`,
  `setActivePlan`, `restorePlans`, `readPlan`): the service plans.
  JS owns the documents. A host reads every plan it uses through
  cap-author before using it — at boot with
  `restorePlans(readStoredNetwork)`, or on first use with a bare
  `restorePlans()` and `readPlan(id, readStoredNetwork)` (the
  service-design host: a workspace of many forks reads the two it
  opens). The UI reads documents in JavaScript, which has no serde
  aliases; only the `plan` frame, which every stored shape spells
  alike, may be read before.
- `src/basemap.ts`: the basemap style choice (tiles, and glyphs and
  sprites local or remote) and the MapLibre worker wiring, for any
  map view.

The component catalog. Every component is THIN: props in, events
out, formatting only; it imports neither the bus nor a UI's
host layer (the gates check both).

- Thin, usable by any generated UI: Unavailable (a frame with no
  answer, said plainly: what is missing and why — an op or decision
  it waits on, or a capability that failed, with a retry), the
  service-design set: DesignRail (the views, keyed 1–6),
  ContextBar (plan, baseline, group, route, stop, hours, health, a
  route jump, and the status line — marked when it is a refusal, an
  error, or a change that left issues), PickTimeline (plans `periods` as bars, `resolve`'s
  plan named), PlanList (the plan set, fork, feed import; a network with no plan
  frame is the "Base network"),
  GroupTable (`service_groups`: each combination with its date
  spans), GroupLedger (`service_type_compare` totals, and runcut `suggest_blocks` on the group's day: the peak with the trips no block carries chained in, D-46), DayProfile (`hourly_trips`, a
  drag sets the hour window; `network_stats` on the same date: the
  service window, stops served, transfer points), FrequencyGrid (`route_frequency`:
  hours as cells, shaded continuously by the longest wait),
  CoveragePanel (the group's `type_coverage` with its parameters and window, the
  layer's weight field and label picked from the load's `fields`, its
  resolution from the answer's reading, the group's `unserved_points` listed),
  CoverageMap (`unserved_points` drawn: points as circles, polygons as
  their shape shaded by the share unserved),
  HubMatrix (`hub_connections`), CorridorTable (`route_overlap`),
  StopWaits (`stop_gaps`: a stop's waits, a station's pooled over its
  bays), NearMisses (`missed_connections`: each near miss with the
  route's next departure), LongestWaits (`longest_waits` in the hour
  window, a row opening its hub), RouteHeader (a route's `route_frequency`
  numbers, before → after), StringLine (directory `timetable` as
  time against stop; a dragged trip is a nudge event, and a stop's
  name a pick event when the host listens), RunTimes
  (`run_secs` by start), HourGaps (a route's `route_frequency`
  hours), GesturePanel (span — beside the direction's own first and last
  trip —, running time, one trip, stops, the stop to insert picked by
  name — one event each), ChangeLedger (`service_type_compare` routes, undo and
  redo events), ProposalScore (`service_type_compare`, and
  `suggest_blocks`' authored peak → the peak with the suggestion
  adopted) and Redline (`service_type_compare`), Readiness (the document's issues and
  warnings — network `validate`'s, or cap-author's for a version it
  just authored — each a sentence with clock times
  (`designFormat.ts`'s `issueWords`), whether the plan is ready, the export
  and publish events — Publish posts the plan to `POST /plans` and
  shows the server's answer as given: published, or each refusal —
  and, on a block whose trip arrives after its next trip leaves, a
  hold event carrying the issue's own trip and time); RouteCard (`route_detail`), ScheduleChange (one
  pattern trips' starts with add and remove events), DirectionTabs
  (`route_detail` directions), TimetableGrid (directory `timetable`,
  added, retimed, and removed trips when asked against a baseline; a stop
  column click is an event), FocusMap (a
  directory `focus` drawn and framed),
  Combobox (a searchable picker: it carries its own styles, its list
  floats over the page, and a pick gives the focus back so the page's
  keys work), and the rider's set: RideMap (every
  stop, the live buses by status, a route's `focus`, a trip's
  `itinerary_paths`, the rider's location; it opens on the last fix,
  else the last network frame, and frames the network only by
  `viewport.ts`'s rules; a tapped
  stop or spot is an event), PlaceSearch (a debounced type-ahead over directory
  `stops`, and under them the geocoder's addresses when the host
  passes them — two lists, one box, D-28 — keyboard-driven), DepartureRows (a board's rows —
  `stop_board`, or a watched stop's from `watchlist_board` — with
  their expected time and status),
  StopCard (a stop at a glance), RiderBoard (one stop's board, its
  alerts, its cancelled trips, and the visits a detour skips), AlertList (the live document's
  alerts), TripPlan (the trip's ends, when, and the router's answer)
  with Itinerary (the answer as a timeline, door to door: leave → arrive,
  the trip's length, and for a trip leaving now "Leave in N min"), RouteList (directory
  `entities` for routes, filtered as typed), RouteView (one route's
  directions and their variants, as line diagrams), BusView (one
  live bus and its trip's `trip_progress`, stop by stop), and
  LiveFeeds (whether
  the times are live, and the feed URLs); and the stop display's set:
  DisplayHeader (the stop's name and id, the host's clock, live or
  scheduled), DisplayGroup (one `grouped_board` group as a station
  list's row, one route large, or a band across the screen; each
  time its `wait`, its status, and at a station its bay; a group with none left says it
  is detoured, cancelled, or when it runs next — the board's `next`,
  "Next: 5:12 am tomorrow", "Next: Monday 5:12 am" — or that nothing
  runs this week, and one with times left says when a trip of it was
  cancelled), DisplayAlerts (the alerts
  slot, always on screen), and DisplaySetup (the three layouts and
  the stop type-ahead). `rideFormat.ts` holds their formatting
  (clock times, a row's wait, status words and tones). Each
  frame whose capability can fail takes an `off` prop: a capability
  that did not assemble turns its frame off, and the frame says so.

The procedure. Every generated UI follows it in order, and a step
that is not done blocks the next. A UI's open work is its unit,
`docs/units/ui-<id>.md` (`docs/units/README.md`); the
unit is written before any code and deleted when the UI lands.

1. **Name the need and its questions.** Open the unit: the need the
   UI serves (named for the need, never a persona — a persona is
   one example of who has the need), and the questions it answers,
   in the order they are asked.
2. **Inventory the ops, from the manifest.** For each question:
   the op that answers it (`caps-run tools <cap>`), or the open
   finding that says none does (`docs/QA-FINDINGS.md`). This is
   the unit's first draft. It is a list of what to test, not yet
   a list of what works.
3. **Dogfood the questions on real data** through `caps-run`
   (`docs/TESTING.md`, the dogfood rules), one op per question.
   Each question that needs anything besides passing one op's
   answer through to the screen is an op gap, and it goes into
   `docs/QA-FINDINGS.md` as a finding. That covers a host join, a
   threshold, a derived number, a second op stitched to the first,
   bisecting inputs until an answer changes, or a rule the
   rendering would have to apply. A session re-confirms the
   findings it hits. The unit's inventory then names, per question,
   either its op (as a dogfooded fact) or its blocker (`QAF-N`,
   and `Blocked: D-N` when the fix waits on the user).
4. **Build only what is unblocked.** No host, shell, or component
   is written while the session in step 3 has not run. A blocked
   question is unblocked by an op (laws first, library first, the
   capability wrapper second — `crates/AGENTS.md`), never by host
   code. The UI may land answering only its unblocked questions:
   `ui.json` lists the questions it answers, and the unit keeps
   the blocked ones with their findings.
5. **Draw the answer graph before the layout.** In the unit, one
   row per node: the op (or a run of ops over the same inputs),
   the inputs it reads (documents, selection keys, the clock, a
   live snapshot, a visibility flag), the frame that renders it,
   and when that frame is on screen. The table is the UI's first
   draft of its flow map (`docs/units/host-discipline.md`): the
   engine will be designed from these tables, so a question the
   table cannot express (a timer, a stream, a node whose inputs
   depend on another node's answer in a way `needs` cannot say) is
   written into host-discipline.md as an engine requirement, and
   `kit.ts`'s transient runner is NOT extended to cover it.
6. **Write the host from the table.** `host.svelte.ts` declares the
   table's nodes as `AnswerNode`s and requests inputs from event
   handlers; every op a node runs is `ask`ed of the engine, over the
   `Doc`s the host holds. The only things a host may do: read and write I/O
   (files, fetch, IndexedDB), read the clock and pass it as an op
   argument, hold the selection (including repairing a selection
   the new answer no longer holds), track which frames are on
   screen, and adopt answers as-is. It never joins, filters,
   sorts, counts, thresholds, or falls back by a domain rule. A
   policy value is an op parameter whose default the op owns. The
   test: if `caps-run` or the Go server would have to re-implement
   it to get the same answer, it belongs in an op.
7. **Then the components, then the shell.** Thin components in
   `src/components/` (props in, events out, formatting only), then
   `Shell.svelte` (placement only).
8. **Land it.** Add `ui.json`, the page, and
   `e2e/ui-<id>.spec.mjs`; the host layer names its page
   (`assemble("<id>")`) and `just caps` generates its linked
   assembly from `ui.json`'s `ops`. Run the UI in the browser on the real
   feed (`just serve testdata/ctran-gtfs.zip`) and check its numbers
   against the runner session's. Record the session in the
   descriptor's `session` field, describe the UI below, and give it
   a short plain-language entry in the guide (`docs/guide/apps.md`). Delete
   the unit if no question is left blocked, naming its pins;
   otherwise edit it down to the blocked questions.

Gates (`just test`, GENERATED UIs and the WEB GATES): every `src/uis/<id>/`
has its `ui.json` (with fields complete, and id equal to the
directory), its page, and its spec. No `.svelte` file imports the
bus; no UI imports another UI's files; no component imports a
UI's host layer. The build also refuses a UI directory without
its page (`vite.config.js`), and the picker's spec
(`e2e/picker.spec.mjs`) checks one card per `ui.json`.

The UIs today: `service-design` (`src/uis/service-design/`): six
views over one selection — a rail of views (keys 1–6) and a context
bar (plan, baseline, service group, route, stop, hour window) frame
whichever is open. Plans: the pick timeline, the plan set (fork,
baseline, feed import), the service groups (the combinations of
service types that run together, each with its dates; the
day-scoped frames ask on a group's first date — any of its dates
answers alike). Network: the group's
numbers against the baseline, trips per hour (a drag sets the hour
window every view reads), and the frequency grid; a row opens the
route. Coverage: the parameters and window, the selected group's
served and frequent share (named by its members, with its dates), and
the places its stops leave unserved, listed and mapped. Hubs: a stop's connection
matrix, its waits, and its just-missed connections, the network's
longest waits in the hour window, and the shared corridors. Route: the string-line
workbench — directions, the string-line or the timetable grid,
gaps by hour, running time by start, the change ledger with undo
and redo, the route's map, and (edit mode, E) the gestures, placed above
the picture they change: span,
running time, one trip moved alone (picked or dragged), stops (one
picked by its name on the string-line or its timetable column,
dropped, or another inserted after it by name) —
each acting on one service type of the group, which the panel picks
and names, warning what else runs with it and the other groups it
runs in (D-8, D-95). The panel is fresh for each route and direction.
After a change, an undo, or a redo, the status line states the
plan's issues and warnings as the answer gave them, and the last
export and publish answers are cleared with the version they were
about. A frequency target is shown as waiting on D-54. Proposal: the
scorecard (with the vehicles the group's day needs; hours against a
budget wait on D-108), the redline, readiness (each issue in words, a broken
block's next trip held from its issue — the host's `retime_trip`),
the export, and publishing the
pick to the deployment (D-74). A frame with no answer says so in a
planner's words, and keeps the decision or finding it waits on as a
tag. The selection
lives in the URL hash. With no stored plan it opens the server's
`/network` as the base plan, which every view calls the Base
network. Its host answers through the answer
graph; a view's nodes answer only while it is open, each frame
records its own failure (`sd.failed`) and renders Unavailable with
a retry while the rest answers, and the plan's health adopts
cap-author's issues and warnings for a document it authored. After
every change it adopts, the host names its keep-set (the plan set's
current networks and the layer). Undo restores an
earlier document version the host keeps; nothing is inverted.
`service-change` (`src/uis/service-change/`): the same need as
service-design, laid out in the stages a service change took when it
was worked through the ops on C-TRAN (three proposals, assessed,
built, judged and revised): one selection bar (plan, baseline, the
plan's day types — analysis `day_types`, D-137, a trial — the hour
window, health, a route box) over five stages, each with the page's
width. Plan & calendar: the pick timeline, the plan set (open, set as
baseline, rename — author `update_plan` —, delete from the device, and
fork), and the service keys (directory `service_keys`), added
(`add_service_type`) or removed (`remove_service_type`, refused while
trips run under the key): each key's weekdays,
date ranges and exceptions, edited in place (cap-author's calendar
edits), what it carries (a route's trips moved to another key,
`set_pattern_trips_refs`), and the combinations it forms. Scan: the
frequency grid, the network map (directory `route_lines`) one click
away. Route: one route's day (closed again from its stage tab) — the string-line or the timetable, the
changes (the service-design gestures), running time by start, gaps by
hour, the trips by start with the key and the block each runs under (a
block opens on Vehicles & hours) — with the
change ledger and a stop the route serves read beside it (its
connections, near misses and waits). Vehicles & hours: runcut
`fleet_summary` and `suggest_blocks` on a date the day type runs —
the peak, platform, revenue and layover hours, the trips on no block
and the suggested blocks adopted as one change, and every running
block drawn on the day (the summary's spans), one opened to its chain
(runcut `block_trips`). Proposal: analysis `plan_compare` across every
day type (a day type's card opens it on the Scan), readiness (a block
an issue names opens on Vehicles & hours), export and publish. A
stage's reads re-run when the stage opens, so a route or block chosen
while another stage is open reads in full there.
The selection lives in the URL hash, which the page follows: back and
forward move through the stages. It has no single-key shortcuts. Its
authoring path (fork, apply, undo, export, publish) is service-design's,
copied — a UI never imports another's files.
`ride-now` (`src/uis/ride-now/`): a full-bleed map with a panel over
it (a card at the left on a wide screen, a sheet over the bottom of
the map on a phone), the live pill and the alerts button in the
corner. The map draws every stop and every live bus (colored by its
status, pointing its bearing, labeled with its route); a tapped bus,
stop, or spot opens it. The panel follows the rider: home (the
type-ahead — stops, and addresses and places when the server runs
a geocoder: a picked address is a spot, "Near 1400 NE 136th Ave" —,
"Get directions" and "Routes", the places near the rider
or a tapped spot with each one's next departures, saved stops,
recent stops — nearest by the walk along the streets when the
server publishes them), one stop (its live board; a picked departure draws
its route, marks its bus, and offers to track it), one bus (its
trip stop by stop, the stop it heads to marked, followed on the
map), the routes and one route (its directions' variants as line
diagrams; its buses stay bright on the map, the rest dim), getting
there (the ends as type-aheads, or the rider's location, or chosen
on the map — "Choose on map" per end: the next tap sets it, a stop,
or with the streets published any spot, a dropped pin named by the
geocoder's nearest address (D-37); an address typed in an end's
field is a point end the same way; "Directions
here" on a tapped spot does the same; a point end plans
as a router place over cap-streets `access`, its walks drawn along
the streets; leave now, leave at, arrive by, or the last bus; the
itinerary as a timeline, drawn on the map), and the alerts in
force. It boots from the server's published network
(`servedNetwork`, bytes it never parses; or a zip the rider loads),
then loads the server's `/streets` and `/walks` in the
background (`servedStreets`, `servedWalks`; until they land, or
where none is published, walks are straight lines and trips run
stop to stop; a hand-loaded zip walks no streets — the walk table
walks the published network), reads the edit document from
`GET /edits`, reads the live feeds the deployment names in
`public/live-feeds.json` (the rider may set their own; kit's
`fetchFeed`), and polls both and the clock every 30 s. A trip asked
for "now" is re-asked on each poll. Its boards include the previous
date's after-midnight trips (`previous`), and it follows the
deployment and installs as an app ([updates-offline.md](updates-offline.md)):
a new build is offered as "a new version is ready". Addresses come from the geocoder
the manifest lists (read once on boot; [web.md](web.md), "External services"): the
host sends the typed text and the near point or the rider's, shows
Pelias's answer as given, and a point it answers keeps the name it
was shown with; nothing keeps the query. Its questions and session
are its `ui.json`.

`stop-display` (`src/uis/stop-display/`): the screen at a stop, a
passenger information display, in three full-screen layouts over
one `grouped_board` answer — a transit center (the groups as a list,
each time naming its bay, the alerts beside it), a single route (one
group filling the screen, its type scaled to it, for the stop one
route serves), and several routes (a
street stop's two or three, each a full-width band sharing the
screen's height, its type scaled to the band); the alerts slot is
always on screen. Any layout shows any stop. The stop and the layout live in the page's URL
(`?stop=<id>&layout=center|route|routes`), so a screen reopens on its
stop; without a stop, the setup picks both. It boots from the
server's published network (`servedNetwork`, bytes it never parses;
or a zip), reads `GET /edits` and the deployment's
live feeds (`public/live-feeds.json`, kit's `fetchFeed`), and polls
them and the clock every 20 s. Its groups include the previous
date's after-midnight trips (`previous`), and it follows the
deployment and works offline ([updates-offline.md](updates-offline.md)): a new
build applies itself at a poll while the server answers and the
setup is closed. Its questions and session are its `ui.json`.
