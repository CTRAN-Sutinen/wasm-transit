# Generated UIs

A generated UI is a layout built for a set of questions. It is made
from thin components over its own host layer. Each one is a directory.
The build, the picker, and the gates discover it. To add one, edit
nothing central.

Each generated UI is its own product. It builds alone (`just build-ui
<id>`). It deploys on its own, with the server or, when it needs no
server, as a static site ([README.md](README.md), "Each UI is its own
product"; [static-deployment.md](static-deployment.md)). A new UI adds
its row to that README's table. The row says what the UI needs the
server for.

```
web/<id>.html                  the page (copy service-design.html; change the script src)
web/src/uis/<id>/ui.json       the descriptor the picker renders: id (= the directory),
                               title, summary, questions, ops ("<cap>.<op>"; the
                               capabilities they name are what its linked assembly
                               links; the kernel's loads are not ops), components,
                               session (the dogfood session: date, feed, open
                               findings), spec, generated, `offline: true`
                               for a page that installs and works offline
                               ([updates-offline.md](updates-offline.md)), and
                               `precache_assembly: false` for one whose assembly
                               the worker keeps on first use (a staff page, D-223)
web/src/uis/<id>/main.ts       imports ../base.css (+ maplibre css if it maps), mounts
                               Shell, then awaits the host's boot (the layout paints first)
web/src/uis/<id>/Shell.svelte  placement only: reads the host's answers state, passes
                               props, routes events to host functions
web/src/uis/<id>/host.svelte.ts the only file of the UI that asks the engine; owns
                               I/O, the selection, and one $state object of answers
web/e2e/ui-<id>.spec.mjs       the UI's e2e spec (fixture feed: e2e/fixtures/)
```

Shared by every generated UI:

- `src/uis/base.css`: color tokens (`--surface --text --muted
  --line --hover --accent --accent-soft --bad --warn --ok`) in light and
  dark, the page base, `.wb-scroll`, and the class names the shared
  components render with (`.muted`, `.table-scroll`, `.chip`,
  `.picker-row`). `.wb-scroll` is a table that scrolls in its own
  box with an opaque sticky header.
- `src/uis/kit.ts` (host layer only):
  - The clock: `today()` and `nowSecs()` read the instant in the
    agency's zone, never the device's. They read it through
    `src/clock.ts`. The zone is the manifest's `timezone`. The host's
    manifest input sets it with `setAgencyZone()` before the page
    reads its network. A loaded zip's zone is set the same way. `today()` gives the agency's date. `nowSecs()`
    gives the seconds from noon minus 12 h on that date. This is the
    GTFS anchor: it is midnight except on a clock-change day, and the
    fall-back hour before it clamps to 0.
  - `previousDay()` is the same instant on the previous date's clock
    and that date's length. A board takes it as `previous` (the
    previous date's after-midnight trips, and the fall-back hour read
    truly).
  - The readers an input uses: `readHead()` (a head the server
    serves with no hash, `GET edits`, with the server's age),
    `readHeadKept()` (the same, kept on the device for a staff page
    that boots offline), `readLiveFeeds()` (the live feeds that
    `live-feeds.json` names), and `readFeed()` (one live feed, read
    directly or through the dev relay that the config's `proxy`
    names, `/rt`, D-67). The engine reads the manifest and the
    documents it pins (`readManifest`, `servedNetwork`, `served`).
  - `askOracle()` asks an oracle the manifest lists (the geocoder) on a
    person's action, with a time limit.
  - `installApp()`, `buildWaiting()`, and `applyBuild()` are the build
    lifecycle ([updates-offline.md](updates-offline.md)).
  - `clockEvery()` moves the clock while the page is visible: the
    answers that read the time, and the outbox's sends. It reads
    nothing. A page's reads are its inputs.
  - The inputs (`src/uis/inputs.ts`, through kit's `inputGraph`) read
    each host's documents and feeds. A UI declares its inputs in its
    `ui.json` (`inputs`), as data, so the gates and the docs site read
    them. Each declaration names:
    - `id` and `name`: the reader's id, and the words a label uses
      ("the detours");
    - `from`: where it comes from;
    - `every_s`: how often it is read (null: once, at boot);
    - `limit_s`: the time limit of one read;
    - `keeps`: what the page keeps when a read fails (`last`, or
      `none` for a live feed: an old prediction is never shown as
      live);
    - `old_after_s`: the age past which the input is old;
    - `feeds`: the answer graph's inputs it changes;
    - `after`: the inputs read before it in the same poll;
    - `fails_as`: the rows of the UI's failure table that say what the
      page does while its reads fail.

    The host gives each input a reader at the top of its file: `read`
    answers the document, null for none, or throws `ReadFailed`;
    `adopt` writes it into the host's state. The kit reads every input
    with one poll while the page is visible. It stops a read at its
    time limit. A read has one of three outcomes: a document, none
    kept, or failed since a time. A failed read waits its interval, or
    the upstream's `Retry-After`, and a random part of the interval, so
    the open pages do not all read again at the same moment. Each
    input's state is in the host's `inputs` record. `inputLabel()`
    says it in the server's words: an answer from a working copy says
    its age, and a failed read says since when. A changed document
    changes the input's `feeds` in one request for each poll. The laws
    are `src/uis/inputs.test.mjs`.
  - The answer graph (`src/uis/answers.ts`, through kit's
    `answerGraph`) runs each host's flows. A UI's flows are code, not
    data (D-176). A host declares its answers as `AnswerNode`s: the
    inputs each reads, the frame that renders it, when that frame is
    on screen, the capabilities its ops ask (`caps`), a document it
    cannot answer without (`requires`), the inputs it cannot answer
    without (`inputs`), and what it clears (`clear`). `answerGraph` answers the nodes downstream of a
    changed input in declared order. It yields between nodes, so each
    frame paints as its answer lands. A request made mid-pass
    supersedes the pass. A hidden frame's node waits until the frame
    shows. The graph carries no domain knowledge. The host asks the
    engine (`src/engine.ts`) for every op that a node runs.
  - The answer graph keeps each frame's state in the host's `frames`
    record. A frame that answered has none. Otherwise it is
    unassembled (a capability in `caps` did not assemble, and the
    node does not run), starved (`requires` names a missing document,
    and the node does not run), unread (an input in `inputs` fails and
    the page holds nothing of it, and the node does not run), or
    failed (the node threw). Each of
    them runs the node's `clear`, except a failure of a node that
    keeps its last answer (`keepOnFailure`, the stop display's board).
    The next answer clears the state. `request.retry()` runs each
    failed node again. A frame with several nodes shows the first that
    has a state. A document read (frame `""`) that fails goes to the
    page's handler. `reason(state)` gives the state as a sentence for
    a prop that takes text. The laws are `src/uis/answers.test.mjs`.
- `src/plans.svelte.ts` (`getAllPlans`, `putPlan`,
  `setActivePlan`, `restorePlans`, `readPlan`): the service plans.
  JavaScript owns the documents. A host reads every plan it uses
  through cap-author before it uses the plan. It reads the set through
  cap-network at boot (`restorePlans(readStoredSet)`), and each plan
  then too (`restorePlans(readStoredSet, readStoredNetwork)`) or on
  first use (`readPlan(id, readStoredNetwork)`; the service-design
  host: a workspace of many forks reads the two it opens). The UI reads documents in JavaScript, which has no serde
  aliases. Only the `plan` frame may be read before this read, because
  every stored shape spells it alike.
- `src/basemap.ts`: the basemap style choice (tiles, and glyphs and
  sprites local or remote) and the MapLibre worker wiring, for any
  map view.

The component catalog. Every component is thin: props in, events
out, formatting only. A component imports neither the bus nor a UI's
host layer (the gates check both).

Thin components that any generated UI can use:

- Unavailable: a frame with no answer, said plainly. It states what is
  missing and why: an op or decision the frame waits on, or a
  capability that failed, with a retry.

The service-design set:

- DesignRail: the views, keyed 1–6.
- ContextBar: plan, baseline, group, route, stop, hours, health, a
  route jump, and the status line. The status line is marked when it
  is a refusal, an error, or a change that left issues.
- PickTimeline: plans `periods` as bars, with `resolve`'s plan named.
- PlanList: the plan set, fork, and feed import. A network with no plan
  frame is the "Base network".
- GroupTable: `service_groups`, each combination with its date spans.
- GroupLedger: `service_type_compare` totals, and runcut
  `suggest_blocks` on the group's day. The peak has the trips that no
  block carries chained in (D-46).
- DayProfile: `hourly_trips`. A drag sets the hour window. It also
  shows `network_stats` on the same date: the service window, stops
  served, and transfer points.
- FrequencyGrid: `route_frequency`, with hours as cells, shaded
  continuously by the longest wait.
- CoveragePanel: the group's `type_coverage` with its parameters and
  window. The layer's weight field and label are picked from the
  load's `fields`. Its resolution comes from the answer's reading. The
  group's `unserved_points` are listed.
- CoverageMap: `unserved_points` drawn. Points are circles. Polygons
  are their shape, shaded by the share unserved.
- HubMatrix: `hub_connections`.
- CorridorTable: `route_overlap`.
- StopWaits: `stop_gaps`, a stop's waits. A station's waits are pooled
  over its bays.
- NearMisses: `missed_connections`, each near miss with the route's
  next departure.
- LongestWaits: `longest_waits` in the hour window. A row opens its
  hub.
- RouteHeader: a route's `route_frequency` numbers, before → after.
- StringLine: directory `timetable` as time against stop. A dragged
  trip is a nudge event. A stop's name is a pick event when the host
  listens.
- RunTimes: `run_secs` by start.
- HourGaps: a route's `route_frequency` hours.
- GesturePanel: span (beside the direction's own first and last trip),
  running time, one trip, stops, and the stop to insert, picked by
  name. Each gesture is one event.
- ChangeLedger: `service_type_compare` routes, and undo and redo
  events.
- ProposalScore: `service_type_compare`, and `suggest_blocks`' authored
  peak → the peak with the suggestion adopted.
- Redline: `service_type_compare`.
- Readiness: the document's issues and warnings. They are network
  `validate`'s, or cap-author's for a version it just authored. Each
  is a sentence with clock times (`designFormat.ts`'s `issueWords`).
  It also shows whether the plan is ready, and the export and publish
  events. Publish posts the plan to `POST /plans` and shows the
  server's answer as given: published, or each refusal. On a block
  whose trip arrives after its next trip leaves, a hold event carries
  the issue's own trip and time.
- RouteCard: `route_detail`.
- ScheduleChange: one pattern's trips' starts, with add and remove
  events.
- DirectionTabs: `route_detail` directions.
- TimetableGrid: directory `timetable`, with added, retimed, and
  removed trips when asked against a baseline. A stop column click is
  an event.
- FocusMap: a directory `focus`, drawn and framed.
- Combobox: a searchable picker. It carries its own styles. Its list
  floats over the page. A pick gives the focus back, so the page's
  keys work.

The rider's set:

- RideMap: every stop, the live buses by status, a route's `focus`, a
  trip's `itinerary_paths`, and the rider's location. It opens on the
  last fix, else the last network frame. It frames the network only by
  `viewport.ts`'s rules. A tapped stop or spot is an event.
- PlaceSearch: a debounced type-ahead over directory `stops`. Under
  them it shows the geocoder's addresses when the host passes them.
  These are two lists in one box (D-28). It is keyboard-driven.
- DepartureRows: a board's rows (`stop_board`, or a watched stop's from
  `watchlist_board`) with their expected time and status.
- StopCard: a stop at a glance.
- RiderBoard: one stop's board, its alerts, its cancelled trips, and
  the visits a detour skips.
- AlertList: the live document's alerts.
- TripPlan: the trip's ends, when, and the router's answer. It comes
  with Itinerary: the answer as a timeline, door to door (leave →
  arrive, the trip's length). For a trip that leaves now, it shows
  "Leave in N min".
- RouteList: directory `entities` for routes, filtered as typed.
- RouteView: one route's directions and their variants, as line
  diagrams.
- BusView: one live bus and its trip's `trip_progress`, stop by stop.
- LiveFeeds: whether the times are live, and the feed URLs.

The stop display's set:

- DisplayHeader: the stop's name and id, the host's clock, and whether
  the times are live or scheduled.
- DisplayGroup: one `grouped_board` group as a station list's row, as
  one route large, or as a band across the screen. Each time shows its
  `wait`, its status, and at a station its bay. A group with no times
  left says that it is detoured, that it is cancelled, or when it runs
  next. That statement is the board's `next` ("Next: 5:12 am
  tomorrow", "Next: Monday 5:12 am"), or it says that nothing runs
  this week. A group with times left says when a trip of it was
  cancelled.
- DisplayAlerts: the alerts slot, always on screen.
- DisplaySetup: the three layouts and the stop type-ahead.

The disruptions set:

- DisruptionsMap: the live buses by status (late, very late, early, and
  the rest small), each cancelled trip at its first stop, each closed
  stop, and the stops a detour skips, from disruptions `today`. An open
  trip draws its path. A route picked dims the rest. The legend reads
  the live document's window. It frames what it draws once, and again
  when its size changes, until the reader moves it.
- RouteStatus: disruptions `today` as a card per route. Each
  disruption is a line with whether an alert explains it and its
  latest note. The route's late trips are counted.
- RouteStopSearch: one box over directory `entities` (routes) and
  `stops`. A pick is a chip with a clear button.
- DispatchBoard: rt `late_trips`, the day's detours and cancelled trips
  (`today`, sectioned by what each is), and notes `between`.
- DisruptionDetail: one disruption, a cancelled trip's stops (runcut
  `trip_stops`), and its notes (notes `on`), with a box to add a note
  and Edit on each. A disruption no alert explains offers Start alert.
- AlertsBoard: disruptions `gaps`, each with Start alert, and alerts
  `list` by tab (in force, upcoming, drafts, ended), each with Edit, and
  End alert while it is told to riders.
- AlertEditor: one alert: the template menu (alerts `templates`), the
  effect, cause, and severity, where it applies (routes and stops by
  search, the whole agency), when riders are told (Starts now, Unknown
  end), the text in each language with the blanks a fill left, the
  link, and what it explains. Every edit is an event with the whole
  alert. Alerts `put`'s refusals and `flags`' notices come back in
  words beside Save draft and Publish.
- AlertReach: what riders see of the alert (board `alert_reach`): the
  whole alert's reach today, then one card per informed entity with
  what it means in words (the fields of one entity are joined by AND;
  each entity applies on its own), the departures it alone reaches,
  the `informed_entity` as the feed publishes it, and a warning where
  it reaches nothing, names a station, names an unknown id, or names
  no single trip instance. The map shows the reached stops; hovering
  a card shows its own; a stop clicked on the map is added.
- AlertPreview: one stop's board (board `stop_board`) over a live
  document whose alerts are the feed encoded with this alert published
  (rt `encode_alerts`), and the feed downloaded as a .pb file. It
  opens in a dialog from the form's Preview.

`disruptionsFormat.ts` holds their formatting.

`rideFormat.ts` holds the formatting of the rider's set and the stop
display's set (clock times, a row's wait, status words and tones). Each
frame whose capability can fail takes an `off` prop. A capability that
did not assemble turns its frame off, and the frame says so.

The procedure. Every generated UI follows it in order. A step that is
not done blocks the next. A UI's open work is its unit,
`docs/units/ui-<id>.md` (`docs/units/README.md`). Write the unit before
any code. Delete it when the UI lands.

1. **Name the need and its questions.** Open the unit. Write the need
   that the UI serves. Name it for the need, never a persona (a
   persona is one example of who has the need). Write the questions
   that the UI answers, in the order they are asked.
2. **Inventory the ops, from the manifest.** For each question, find
   the op that answers it (`caps-run tools <cap>`). Or find the open
   finding that says no op does (`docs/QA-FINDINGS.md`). This is
   the unit's first draft. It is a list of what to test. It is not
   yet a list of what works.
3. **Dogfood the questions on real data** through `caps-run`
   (`docs/TESTING.md`, the dogfood rules), one op per question.
   A question is an op gap when it needs anything besides passing one
   op's answer through to the screen. Put each op gap into
   `docs/QA-FINDINGS.md` as a finding. These needs are op gaps:
   - a host join
   - a threshold
   - a derived number
   - a second op stitched to the first
   - bisecting inputs until an answer changes
   - a rule that the rendering would have to apply

   A session re-confirms the findings it hits. The unit's inventory
   then names, for each question, either its op (as a dogfooded fact)
   or its blocker (`QAF-N`, and `Blocked: D-N` when the fix waits on
   the user).
4. **Build only what is unblocked.** Do not write a host, shell, or
   component before the session in step 3 has run. An op unblocks a
   blocked question (laws first, library first, the capability wrapper
   second; `crates/AGENTS.md`). Host code never unblocks one. The UI
   may land answering only its unblocked questions. `ui.json` lists
   the questions that the UI answers. The unit keeps the blocked ones
   with their findings.
5. **Draw the answer graph before the layout.** In the unit, write one
   row per node. Each row has:
   - the op (or a run of ops over the same inputs)
   - the inputs it reads (documents, selection keys, the clock, a
     live snapshot, a visibility flag)
   - the frame that renders it
   - when that frame is on screen

   The host's nodes are written from this table. Some questions cannot
   be expressed in the table: a timer, a stream, or a node whose inputs
   depend on another node's answer in a way `needs` cannot say. Ask the
   user before you extend the answer graph in `kit.ts` to cover one.
   The answer graph carries no domain knowledge.

   Then write the failure table below it
   ([README.md](README.md), "Built for the outage"). Write one row for
   each way an input can fail. The inputs are the server, the bucket
   behind it, a live feed, an oracle, sign-in, an adjacent system, and
   the device's network. An input can fail in more than one way: it
   does not answer, it answers slowly, it refuses, or it answers with
   old data as if it were new (a live feed whose buses stop moving).
   Each way gets its own row. Each row has:
   - how the page knows that the input failed: a network error, a
     status code, a header, or a time inside the data. A failure that
     the page cannot detect is a finding.
   - what the page still shows, and how it says how old that is
   - what the page refuses, and how it says so
   - what the person does instead
   - how the page catches up when the input comes back
   - the pin: the test that holds the row

   What the page still shows and what it refuses follow from the
   answer graph above. The nodes that need the failed input fail or go
   stale, and every other node keeps answering.

   The agency names what its staff do instead. Ask the user, and do
   not invent a procedure. Until the agency names it, the cell is "—".
   A row the page cannot meet yet is open work in the unit. A row that
   needs an op is a finding. When the UI lands, its table moves into
   its description below, and the unit keeps the rows that are still
   open.

   Then declare the inputs in `ui.json` (`inputs`; kit's "The
   inputs", above). Each document or feed that the table's nodes read
   from outside the page is one input. Give it its interval, its time
   limit, what the page keeps when a read fails, and the rows of the
   failure table it fails as (`fails_as`). A gate in `just test`
   (`tools/failure-tables.mjs`) fails when an input names a row that
   the table does not have.
6. **Write the host from the table.** `host.svelte.ts` declares the
   table's nodes as `AnswerNode`s. It requests inputs from event
   handlers. The host `ask`s the engine for every op that a node runs,
   over the `Doc`s the host holds. A host may do only these things:
   - read its declared inputs, through a reader for each, and write
     I/O (files, IndexedDB, the outbox). A gate in `just test` finds
     no `fetch(` and no hand-made poll in a host.
   - read the clock and pass it as an op argument
   - hold the selection (including repairing a selection that the
     new answer no longer holds)
   - track which frames are on screen
   - adopt answers as-is

   A host keeps no failure state of its own. A node declares its
   `caps` and `clear`, and the answer graph records why its frame has
   no answer. The shell renders the state: a planner shell through
   `Unavailable` (`state`), a rider shell through each component's
   `off` and `error` props (the host's `off(frame)` and
   `failure(frame)`).

   A host never joins, filters, sorts, counts, thresholds, or falls
   back by a domain rule. A policy value is an op parameter. The op
   owns its default. Use this test: if `caps-run` or the Go server
   would have to re-implement it to get the same answer, it belongs in
   an op.
7. **Then the components, then the shell.** Write thin components in
   `src/components/` (props in, events out, formatting only). Then
   write `Shell.svelte` (placement only).
8. **Land it.** Add `ui.json`, the page, and `e2e/ui-<id>.spec.mjs`.
   The host layer names its page (`assemble("<id>")`). `just caps`
   generates its linked assembly from `ui.json`'s `ops`. Run the UI in
   the browser on the real feed (`just serve testdata/ctran-gtfs.zip`).
   Check its numbers against the runner session's. Record the session
   in the descriptor's `session` field. Describe the UI below. Give it
   a short plain-language entry in the guide (`docs/guide/apps.md`).
   Delete the unit if no question is left blocked, and name its pins.
   Otherwise edit the unit down to the blocked questions.

Gates (`just test`, generated UIs and the web gates): every
`src/uis/<id>/` has its `ui.json` (with fields complete, and id equal
to the directory), its page, and its spec. No `.svelte` file imports
the bus. No UI imports another UI's files. No component imports a
UI's host layer. Every op that a host asks is in its `ui.json` `ops`
list, unless it is a kernel op, and every capability a host names
has an op there (`tools/ui-ops.mjs`). An op of a capability that the
page fetches alone, only when the deployment serves it, is in
`fetched_ops` instead. `just caps` does not link those into the UI's
assembly. Ride-now lists cap-migrate's ops there: it fetches cap-migrate
only for a stored watchlist older than the board reads. An op missing from the list
would turn its frame off at runtime with no error. The build also
refuses a UI directory without its page (`vite.config.js`). The picker's spec
(`e2e/picker.spec.mjs`) checks one card per `ui.json`.

The UIs today:

`service-design` (`src/uis/service-design/`) has six
views over one selection. A rail of views (keys 1–6) and a context
bar (plan, baseline, service group, route, stop, hour window) frame
whichever view is open.

- **Plans**: the pick timeline, the plan set (fork, baseline, feed
  import), and the service groups. The service groups are the
  combinations of service types that run together, each with its dates.
  The day-scoped frames ask on a group's first date. Any of its dates
  answers alike.
- **Network**: the group's numbers against the baseline, trips per
  hour, and the frequency grid. A drag on the trips per hour sets the
  hour window that every view reads. A row opens the route.
- **Coverage**: the parameters and window, the selected group's served
  and frequent share (named by its members, with its dates), and the
  places that its stops leave unserved, listed and mapped.
- **Hubs**: a stop's connection matrix, its waits, and its just-missed
  connections. Also the network's longest waits in the hour window, and
  the shared corridors.
- **Route**: the string-line workbench. It has the directions, the
  string-line or the timetable grid, gaps by hour, running time by
  start, the change ledger with undo and redo, and the route's map. In
  edit mode (E) it also has the gestures, placed above the picture they
  change:
  - span
  - running time
  - one trip moved alone (picked or dragged)
  - stops (one picked by its name on the string-line or its timetable
    column, dropped, or another inserted after it by name)

  Each gesture acts on one service type of the group. The panel picks
  and names that service type. It warns what else runs with it and the
  other groups it runs in (D-8, D-95). The panel is fresh for each
  route and direction. After a change, an undo, or a redo, the status
  line states the plan's issues and warnings as the answer gave them.
  The last export and publish answers are cleared with the version they
  were about. A frequency target is shown as waiting on D-54.
- **Proposal**: the scorecard (with the vehicles that the group's day
  needs; hours against a budget wait on D-108), the redline, readiness,
  the export, and publishing the pick to the deployment (D-74).
  - Readiness shows each issue in words. A broken block's next trip is
    held from its issue (the host's `retime_trip`). The warnings
    include each stop that the plan visits and that the stops document
    has not in service over its period (engine `stopWarnings`, D-157).
  - The export carries the stops' history over the plan's period
    (engine `withStopVersions`).

A frame with no answer says so in a planner's words. It keeps the
decision or finding that it waits on as a tag. The selection lives in
the URL hash. With no stored plan, the UI opens the server's `/network`
as the base plan, which every view calls the Base network. Its host
answers through the answer graph. A view's nodes answer only while the
view is open. Each frame's state is the answer graph's (`sd.frames`),
and the frame renders Unavailable with a retry while the rest answers. The plan's
health adopts cap-author's issues and warnings for a document that the
UI authored. After every change that the host adopts, the host names
its keep-set (the plan set's current networks and the layer). Undo
restores an earlier document version that the host keeps. Nothing is
inverted.

Its failure table. Service change has the same table.

| Input, and how it fails | The page knows by | Still shows | Refuses | Instead | Catches up | Pin |
|---|---|---|---|---|---|---|
| The server, or the device's network: no answer | a fetch error, or no answer within the read's time limit; the worker's last copy of the manifest | the plans kept on the device, and every view and change, which run in the page. It boots offline from the worker, and says "Offline · data as of" the time the server last answered. A page never opened online says that the published network could not be read. | — | a publish waits on the device (the outbox), counted on the page | the outbox sends it when the server answers, against the plan set as it is then | e2e `offline-staff` "service design: a publish made offline…", "service change: it boots offline…" |
| A publish refused when it is sent (a pick now dated in the past, a plan that no longer validates) | `422` | everything | the publish, held with the server's reasons | — | the person sends it again or discards it | e2e `offline-staff` "service design…", "service change…" |
| The bucket: no answer (the server answers) | `503` on a publish | the same | — | the publish waits on the device | the outbox sends it again | Go `TestABucketThatStopsAnsweringServesLabelledReadsAndRefusesWrites` (the server's side); `outbox.test.mjs` "a 503 keeps the write waiting" |
| Sign-in: none, or expired | `401` | everything | — | the publish waits: "Sign in to send" | the outbox sends it once the person signs in | `outbox.test.mjs` "no valid sign-in holds the outbox…" |

`service-change` (`src/uis/service-change/`) serves the same need as
service-design. Its layout follows the stages that a service change took
when it was worked through the ops on C-TRAN (three proposals, assessed,
built, judged and revised). One selection bar sits over six stages. The
bar holds the plan, the plan's day types, the hour window, health, and a
route box. The day types come from analysis `day_types` (a trial,
D-137). Each stage has the page's width.

- **Plan & calendar**: the pick timeline and the picks (PickList,
  D-139).
  - There is no base network. The list shows every pick that the
    deployment publishes and every draft on this device, by pick date.
    The pick that runs today is marked. The engine's `publishedPlanSet`
    reads the published picks. One is fetched as a value when it opens
    (`publishedPickValue`). Plans `in_force` over the published set
    finds the running pick. Before the first pick takes effect, the
    deployment's feed is marked.
  - A published pick and the deployment's feed are read-only. A change
    goes into a fork (author `set_plan`, the fork's calendar its
    period, D-138).
  - A draft is renamed (`update_plan`). It is given its period: it
    picks on a date and, optionally, runs through a date. The fork's
    own edit sets the period (`set_plan`: weekly service carried to the
    end, the calendar cut to the period). The edit is undone like any
    change. A draft is deleted from the device.
  - An imported feed is a draft that picks on its first service date
    (network `import`'s `first_service_date`, D-140). The page opens on
    what runs today.
  - Then the service keys (directory `service_keys`). A key is added
    (`add_service_type`) or removed (`remove_service_type`, refused
    while trips run under the key). Each key's weekdays, date ranges and
    exceptions are edited in place (cap-author's calendar edits). The
    stage also shows what a key carries (a route's trips moved to
    another key, `set_pattern_trips_refs`) and the combinations that it
    forms.
- **Scan**: the frequency grid. The network map (directory
  `route_lines`) is one click away.
- **Stops**: a stop belongs to the network, not to a route, so it has a
  stage of its own.
  - Every stop is on a map over the network's lines. A planner finds a
    stop there or by name (directory `stops`).
  - One stop opens to its card (directory `stop_detail` under the day
    type's members). The card shows its record, and every pattern
    position that visits it with the trips it runs. A station pin that
    no pattern visits names the routes that its bays serve.
  - A visit opens its route on the Route stage in that direction.
  - A stop's own record and life (creating, moving, suspending,
    retiring) are the stops app's (`stops`, below; D-157).
    Consolidating stops on a plan's patterns, and a route's own map,
    are `docs/units/ui-stops.md`.
- **Route**: one route's day in one direction. The route's bar chooses
  the direction (the first by default; a direction that runs nothing
  under the day type offers the others). The stage's tab closes it
  again. The stage has:
  - the string-line or the timetable
  - the changes (the service-design gestures)
  - running time by start
  - gaps by hour
  - the trips by start, with the key and the block that each runs under
    (a block opens on Vehicles & hours)

  A stop that the route serves is read beside it (its connections, near
  misses and waits). Once a plan is compared, the change ledger is
  below it.

  With no route open, New route builds one that the plan does not run.
  This is a task in three steps:
  1. The route (a taken id is refused).
  2. Each way it runs: a name, its stops picked in order, and the time
     end to end. The way back is the way out reversed until changed.
  3. Its trips: for each way and service key, a first and last
     departure and the minutes between.

  The picker offers the plan's stops. It also offers the stops
  document's stops that are in service or on their way by the pick date
  (stops `stops_on`). A stop that the plan does not hold is taken in
  with its record (stops `records`, D-173). The stop's life stays the
  stops app's. The plan only copies its record, as a pick reads it at
  publish. Create is one change: author `add_route`, `upsert_route`,
  `set_direction`, `upsert_stop`, `add_pattern_through` and
  `set_headway`. It is refused whole beside the task. Otherwise the
  route opens.
- **Vehicles & hours**: runcut `fleet_summary` and `suggest_blocks` on a
  date that the day type runs. The stage shows the peak, platform,
  revenue and layover hours, the trips on no block, and the suggested
  blocks adopted as one change. It draws every running block on the day
  (the summary's spans). One block opens to its chain (runcut
  `block_trips`).
- **Proposal**: the plan compared against is chosen here. Nothing is
  compared until the planner chooses (D-141). Then analysis
  `plan_compare` runs across every day type (a day type's card opens it
  on the Scan). The stage also has:
  - readiness: a block that an issue names opens on Vehicles & hours.
    The warnings include the stops that the plan visits that are not in
    service over its period, as service design's do.
  - export: each stop's record on the pick date, as service design's.
  - publish.

A stage's reads re-run when the stage opens. So a route or block chosen
while another stage is open reads in full there. The comparison is the
proposal's. The other stages carry no control for it. Only the Scan (a
changed route's mark, its legend) and the Route (the ledger, the
header's and the timetable's before → after) show it, once one is
chosen. Analysis `service_type_compare` is read on those two stages
only.

The selection lives in the URL hash, which the page follows. Back and
forward move through the stages. The UI has no single-key shortcuts. Its
authoring path (fork, apply, undo, export, publish) is service-design's,
copied. A UI never imports another's files.

Its failure table is service design's. Service change also shows the
published picks. When they cannot be read and the page holds none, it
says so beside the plans.

`ride-now` (`src/uis/ride-now/`) is a full-bleed map with a panel over
it. The panel is a card at the left on a wide screen, and a sheet over
the bottom of the map on a phone. The live pill and the alerts button
are in the corner. The map draws every stop and every live bus. A bus is
colored by its status, points its bearing, and is labeled with its
route. A tapped bus, stop, or spot opens it.

The panel follows the rider through these views:

- **Home**: the type-ahead, the places near the rider or a tapped spot,
  saved stops, and recent stops.
  - The type-ahead finds stops. It also finds addresses and places when
    the server runs a geocoder. A picked address is a spot, "Near 1400
    NE 136th Ave".
  - "Get directions" and "Routes" start from here.
  - Each place near the rider or a tapped spot shows its next
    departures. When the server publishes the streets, the nearest
    places are ordered by the walk along the streets.
- **One stop**: its live board. A picked departure draws its route,
  marks its bus, and offers to track it.
- **One bus**: its trip stop by stop. The stop it heads to is marked.
  The map follows it.
- **The routes and one route**: the route's directions' variants as
  line diagrams. Its buses stay bright on the map, and the rest dim.
- **Getting there**: the ends, the time, and the itinerary.
  - Each end is a type-ahead, or the rider's location, or chosen on the
    map. "Choose on map" is per end: the next tap sets it. The tap
    sets a stop. With the streets published, it sets any spot. A
    dropped pin is named by the geocoder's nearest address (D-37).
  - An address typed in an end's field is a point end the same way.
  - "Directions here" on a tapped spot does the same.
  - A point end plans as a router place over cap-streets `access`. Its
    walks are drawn along the streets.
  - The time is leave now, leave at, arrive by, or the last bus.
  - The itinerary is a timeline, drawn on the map.
- **The alerts in force**.

The UI boots from the server's published network (`servedNetwork`,
bytes it never parses; or a zip that the rider loads). Then it loads the
server's `/streets` and `/walks` in the background (`servedStreets`,
`servedWalks`). Until they land, or where none is published, walks are
straight lines and trips run stop to stop. A hand-loaded zip walks no
streets, because the walk table walks the published network. The UI
reads the edit document from `GET /edits`. It reads the live feeds that
the deployment names in `public/live-feeds.json` (the rider may set
their own; kit's `fetchFeed`). It polls both and the clock every 30 s.
A trip asked for "now" is re-asked on each poll. Its boards include the
previous date's after-midnight trips (`previous`). It follows the
deployment and installs as an app ([updates-offline.md](updates-offline.md)).
A new build is offered as "a new version is ready".

Addresses come from the geocoder that the manifest lists (read once on
boot; [web.md](web.md), "External services"). The host sends the typed
text and the near point or the rider's. It shows Pelias's answer as
given. A point that it answers keeps the name it was shown with. Nothing
keeps the query. Its questions and session are its `ui.json`.

Its failure table.

| Input, and how it fails | The page knows by | Still shows | Refuses | Instead | Catches up | Pin |
|---|---|---|---|---|---|---|
| The server, or the device's network: no answer | a fetch error, or no answer within the read's time limit. The worker answers the manifest and the network from its last copy. | the schedule it holds. It boots offline from the worker, and its boards say they are the schedule. Detours it holds, labelled "Detours as of …: no answer since …". With none held: "Detours could not be read". | address search | — | the next poll reads the manifest and the edit document, each failed read after a random part of its interval. A new network re-pins with no reload. | e2e `deploy-builds` "both pages boot offline…", `static-deploy` "ride now boots offline…" |
| The bucket: no answer (the server answers) | `X-Store-Stale` | detours, labelled "Detours as of …" | nothing (it writes nothing) | — | the next poll | e2e `ui-ride-now` "says how old detours are…" |
| A live feed: no answer | a fetch error, or a status other than 200 | the schedule, and the feed's error in the live feeds panel | live predictions and buses | — | the next poll | none |
| A live feed: frozen | not detected (QAF-129) | old buses and predictions as live | nothing | — | — | none |
| The geocoder: no answer | a fetch error | stops by name, the device's location, and a tapped point | addresses | — | the next search | none |
| The agency's GTFS URL, in a static deployment that follows it: no answer | a fetch error | the last zip the device kept. With none: "The agency's schedule could not be read. This page will try again." | a new schedule | — | checks again in an hour | none |
| The map's tiles: no answer | not detected | the map with no base layer | nothing | — | the next tile request | none |

`stop-display` (`src/uis/stop-display/`) is the screen at a stop, a
passenger information display. It has three full-screen layouts over one
`grouped_board` answer:

- A transit center: the groups as a list, each time naming its bay, and
  the alerts beside it.
- A single route: one group fills the screen, its type scaled to it.
  This is for the stop that one route serves.
- Several routes: a street stop's two or three routes, each a
  full-width band that shares the screen's height, its type scaled to
  the band.

The alerts slot is always on screen. Any layout shows any stop. The stop
and the layout live in the page's URL
(`?stop=<id>&layout=center|route|routes`), so a screen reopens on its
stop. Without a stop, the setup picks both. The UI boots from the
server's published network (`servedNetwork`, bytes it never parses; or a
zip). It reads `GET /edits` and the deployment's live feeds
(`public/live-feeds.json`, kit's `fetchFeed`). It polls them and the
clock every 20 s. Its groups include the previous date's after-midnight
trips (`previous`). It follows the deployment and works offline
([updates-offline.md](updates-offline.md)). A new build applies itself
at a poll while the server answers and the setup is closed. Its
questions and session are its `ui.json`.

Its failure table.

| Input, and how it fails | The page knows by | Still shows | Refuses | Instead | Catches up | Pin |
|---|---|---|---|---|---|---|
| The server, or the device's network: no answer | a fetch error, or no answer within the read's time limit. The worker answers the manifest and the network from its last copy. | the schedule it holds. It boots offline from the worker, and its board says it is the schedule. Detours it holds, labelled "Detours as of …: no answer since …". With none held: "Detours could not be read". | a new build: it applies one only while the server answers (the manifest's last read answered, not from a working copy) | — | the next poll, each failed read after a random part of its interval. A new network re-pins with no reload. | e2e `deploy-builds` "both pages boot offline…", `static-deploy` "the stop display boots offline…", `ui-stop-display` "a server that never answers the edit document turns the label to stale within the read's time limit" |
| The bucket: no answer (the server answers) | `X-Store-Stale` | detours, with their age | nothing (it writes nothing) | — | the next poll | e2e `ui-stop-display` "a detour served while the server's store is not answering says its age" |
| A live feed: no answer | a fetch error, or a status other than 200 | the schedule | live predictions | — | the next poll | e2e `ui-stop-display` "a live feed that stops answering turns the board to the schedule…" |
| A live feed: frozen | not detected (QAF-129) | old predictions as live | nothing | — | — | none |
| The agency's GTFS URL, in a static deployment that follows it: no answer | a fetch error | the last zip the device kept. With none: "The agency's schedule could not be read. This screen will try again." | a new schedule | — | checks again in an hour | e2e `static-feed-url` "the stop display boots offline on the last zip it had" |

`disruptions` (`src/uis/disruptions/`) answers what is disrupted today,
and whether an alert explains it (cap-disruptions, D-213, D-218). A bar
across the top holds the date and clock, the views (Status, Alerts,
Dispatch),
whether the times are live, and sign-in. Below it, a panel sits beside
a large map that is always on screen.

- **Status**: the search, then each affected route as a card. A search
  pick narrows the day to one route or one stop (a station with its
  bays). A route's badge picks the route.
- **Alerts**: the disruptions no alert explains, each starting an
  alert, and the agency's alerts in force, upcoming, drafts, and ended.
- **Dispatch**: the trips late now, the detours, the cancelled trips
  (from the edit document and the live feed, each as given), and
  today's notes, newest first.
- **A disruption opened**: what it is and where it came from, the next
  trip, whether an alert explains it, the stops it takes away, and its
  notes, oldest first. A cancelled trip shows its every stop and draws
  its path as scheduled: the edit document's cancel takes the trip out
  of the patched day. A note is added or edited there (notes `write`,
  sent as its intent to `POST /logs/notes`; the server names the writer
  and the time, and the note keeps the id it was made with).

An alert is written on half the page, beside what riders see of it
(AlertReach). The day's map is set aside while it is open. "Starts
now" tells riders from the moment the alert is saved: the start is
stamped then. Started from a gap, it carries the gap's effect, the riders it
informs, and what it explains, and it is told from now with no end. A
template fills its cause, effect, severity, and texts (alerts `fill`
over the alert's routes, stops, and dates). Alerts `put` checks it as
it is edited. Save draft and Publish put it and send the put as its
intent to `POST /logs/alerts`. A `409` means another person changed the
same alert since the page read it; a change to another alert does not
refuse it. End alert ends it at the host's clock (alerts `end`, the time
in its arguments). Preview shows one stop's board
with the alert on it (alerts `put`, rt `encode_alerts`, rt `live`,
board `stop_board`), and downloads the feed's `.pb` file. The list is then read once the
end's second has passed, because a period holds through its end. Every
node reads one clock: the page's last poll, or the write's.

The page reads the deployment's network, the edit document (`GET
/edits`), the stops, the service alerts, and the notes (by the
manifest's hashes, each read again only when its hash moves), and the
deployment's live feeds. It polls them and the clock every 30 s while
it is visible. The view, the pick, and the open disruption live in the
URL hash. Its questions and session are its `ui.json`.

Its failure table.

| Input, and how it fails | The page knows by | Still shows | Refuses | Instead | Catches up | Pin |
|---|---|---|---|---|---|---|
| The server, or the device's network: no answer | a fetch error, or no answer within the read's time limit; the worker's last copy of the manifest | the documents last read. A reload boots from the worker on them. "Offline · data as of" the time the server last answered, and the detours and cancels labelled with the time the server last confirmed them. A page never opened online says that the network could not be read. | — | a note or an alert saved meanwhile waits on the device (the outbox), counted on the page | the outbox sends each once, in order, when the server answers | e2e `offline-staff` "the disruptions page through an outage…" |
| A write refused when it is sent (another dispatcher changed the same note or alert since the page read it, a change the module refuses) | `409`, `422` | everything | the write, held with the server's reasons | — | the person sends it again (over what the page reads then) or discards it | e2e `offline-staff` "the disruptions page: a note edit refused at send time…" |
| The bucket: no answer (the server answers) | `X-Store-Stale` on the edit document; `503` on a save | detours and cancels, labelled with their age. The stops, alerts, and notes are held by hash, so they are never old: the manifest that pins them carries the age. | — | a save waits on the device | the outbox sends it again | Go `TestABucketThatStopsAnsweringServesLabelledReadsAndRefusesWrites` (the server's side) |
| A live feed: no answer | a fetch error, or a status other than 200 | the schedule's day. The live state shows the feed's error. | the buses on the map, and lateness | — | the next poll | none |
| A live feed: frozen | not detected (QAF-129) | old buses and lateness as live | nothing | — | — | none |
| Sign-in: none, or expired | `401` | everything | — | a save waits: "Saved on this device. Sign in to send it." | the outbox sends it once the person signs in | `outbox.test.mjs` "no valid sign-in holds the outbox…" |

`stops` (`src/uis/stops/`) holds every stop's own life, apart from the
service plans, which only read it (D-157). It also holds the work asked
for at the stops (D-159). Each job has a screen the width of the page,
laid out for that job. A bar across the top holds the views (Stops,
Work, Feed review), the date read (today until chosen), a search, New
stop, sign-in, and the status line.

- **Stops**: a filter per status: planned, permitted, built, in
  service, suspended, retired (hidden until shown). Each filter has its
  count for that day (stops `stops_on`'s `counts`). The stops shown
  appear in one of two ways:
  - on a map (coloured by status) beside their list
  - as a table (`StopsTable`: name, id, status, since, until, last
    surveyed, notes, sorted by any column)

  "Nearby stops" asks the device for its location (the host's `locate`).
  It lists the stops nearest the device on the date at the statuses
  shown, each with its distance (stops `stops_near`). The device is
  drawn on the map. The map stays mounted under the other screens, so
  going to a stop and back does not rebuild it.
- **Work** (`WorkTable`): every request across the stops (requests
  `list` over the manifest's `stop_requests_hash`) as a table. The
  columns are request, stop (stops `stops_on` for the requests' stops),
  job, status, requested on, work order, and notes. The table sorts by
  any column. By default it shows the drafted, submitted, and ordered
  requests. Each status is a filter with its count, and each job is a
  filter. A row opens the request's page. The page's back link returns
  to the table.
- **Feed review** (`FeedReview`): what the agency's feed did to the
  stops record (the server records the feed's differences when it
  imports it, D-150). It shows:
  - the feed's first service date
  - the changes that the server recorded (stops `feed_events` for the
    manifest's `feed_hash`)
  - the stops in service that the feed does not list
  - the stops whose difference did not fit their history (stops
    `import_events` run without writing)

  Each stop links to its page, where a change is corrected. The page has
  no GTFS control. A feed that a planner loads into a draft plan is a
  draft, and the page never records it.

A stop (`StopPage`; stops `stop`) opens as a page. The header shows its
name, id and code, and where it stands that day. The header has these
buttons:

- Survey.
- Record change. It offers only what the stop may become that day
  (`stop`'s `next`), and a move and new details. Each has a line that
  says what it does.
- Request work.
- Report issue.

Below the header, its record is shown as headed cards in as many columns
as the screen holds:

- Location: its point on a map of the stops around it, opened at the
  stop.
- Details and amenities.
- Routes: each route and direction once, and whether the stop is a time
  point on it. It also shows the stops either side with the distance
  along the route to each ("~" where the import estimated it). The
  source is directory `stop_detail` over `servedNetwork` on the date. It
  is read only, which is why the page links the network capability too.
- Work: its requests (requests `requests_for`). An open request whose
  stop is retired today says so on its page (stops `request_warnings`,
  on the agency's today, not the date read; D-238).
- Last survey: when, by whom, the answers grouped as the Service Stop
  Worksheet groups them, and its ids in other systems.
- History: every event, newest first, each correctable.

A change (`ChangeFlow`) opens as a card under the header. A change is a
lifecycle event, new details and amenities, or the correction of an
event entered wrong (stops `correct_event`, or the event deleted after a
confirmation). Each change asks only what it needs. The card shows:

- a one-line summary of what will be saved
- the routes that the schedule still runs there, beside a suspension or
  a retirement (the schedule changes only in a service change)
- the request that the change carries out, where the stop has any (its
  event's `request`)
- why the change was refused, beside its Save (`TaskFrame`)

A move, and a new stop, take the map with a panel beside it. This is the
one job that needs a large map. A new stop (`NewStopFlow`) is placed,
then given its id, name, code, and where its life starts: planned, or in
service. A planned one may go straight on to its installation request.
An id that the document holds is refused beside the form.

A survey (`SurveyPage`, D-158) is a page of the worksheet's sections,
each under its heading: the site questions as the worksheet groups them,
then amenities, external IDs, and photos and notes. A section index is
beside it. The index shows each section's answered count and a mark
where it changed. A click goes to the section. A summary column shows
the survey date, what differs from the record, and Save, with why a save
was refused. Each question shows what the record had where the answer
differs. A list answer is a row of choices. A count is typed with − and
+. A section can be put back to the record. The survey starts from what
the stop records. It is saved as one `changed` event that carries every
answer recorded. The server names the surveyor: the person signed in
(D-163). A survey half filled is kept on the device (`localStorage`, per
stop) until it is saved or discarded, so a reload or a dropped
connection loses nothing. The stop says that a draft is kept. On a phone
the survey is the whole screen: the index is a bar of sections, and Save
is held at the bottom.

A request (`RequestPage`, D-159) is a page. The header shows its job,
id, and stop, with its status track: drafted, submitted, work order
open, completed (or cancelled, after a confirmation). Below the header
is the worksheet as headed sections (Location, Service, Utility locate,
Work order, Completion, Notes & photos), each edited in place. Beside
them is the one next step for its status:

- submit
- open the work order, where the deployment connects a work order
  system (the manifest's `services.work_orders`, D-165). The server's
  adapter answers, and the answer lands on the request. With none
  connected, enter the work order by hand (requests
  `record_work_order`, as the adapter's answer would be).
- check its status
- complete it
- when completed, record the change on the stop that carried it out (a
  change naming the request)

A new request is the same page with every section open and the job
chosen first. A field report is a repair, unless chosen otherwise. A new
stop's installation is a new stop. Its id is suggested from the stop and
the date, and is never one that the stop's requests already have. It is
saved as a draft or submitted. Requests `put` places and checks it.
`POST /stop-requests` stores it (409: read again, save again).

Where the deployment keeps attachments (the manifest's
`services.attachments`, D-162), the survey and request pages take photos
and drawings. "Take photo" opens the camera. "Add file" takes a photo or
a PDF (`Attachments`). Each is added at once and named on the record by
its hash with a caption. The history and the request show them small
(`AttachmentThumbs`), read with the signed-in person's token.

A point is clicked on the map or typed as measured in the latitude and
longitude fields. A click is kept to six decimals, about 0.1 m, because
a click knows no more. A typed point is saved as typed. The two edit one
point: a click fills the fields, and a typed pair moves the marker
(`PointFields`). "Use my location" fills them from the device's fix,
shown with its accuracy. The fix is a start and never the measurement.
Whether the point is on the earth is the receive path's check (stops
`add_event`).

Each change is one event. Stops `add_event` places and checks it.
`POST /stops` checks it again and stores it (409: the page reads the
stops again and asks for the change again). With no stops document yet,
the page shows the stops of the network that runs today (stops `seed`),
and the first change saves them. Where the deployment asks for sign-in,
the header shows who is signed in, or a Sign in button (`SignInControl`,
web.md "Signing in"). A change saved before signing in is refused in
words beside its Save, and a survey's draft is kept. The view, the
stops' mode, the date, the open stop, the open request, the task, and
the filters live in the URL hash, so the browser's Back leaves a task.

Its failure table.

| Input, and how it fails | The page knows by | Still shows | Refuses | Instead | Catches up | Pin |
|---|---|---|---|---|---|---|
| The server, or the device's network: no answer | a fetch error, or no answer within the read's time limit; the worker's last copy of the manifest | the stops and requests last read. A reload boots from the worker on them, with "Offline · data as of" the time the server last answered. A survey half filled stays on the device. A stops document that cannot be read and is not held says "Stops could not be read", never that the deployment publishes none. | — | a change, a request, a work order request, or a photo saved meanwhile waits on the device (the outbox), in order, a photo before the survey that names it | the outbox sends each when the server answers; the server runs each change again over the stops it holds | e2e `offline-staff` "the stops page: a queued change runs over a moved head…", `ui-stops` "a stops document whose read fails says so…" |
| A change refused when it is sent (another writer changed the same stop since the page read it, a change the module refuses) | `409`, `422` | everything | the change, held with the server's reasons | — | the person sends it again (over what the page reads then) or discards it | e2e `offline-staff` "the stops page…" |
| The bucket: no answer (the server answers) | `503` on a save | the same | — | the save waits on the device | the outbox sends it again | Go `TestABucketThatStopsAnsweringServesLabelledReadsAndRefusesWrites` (the server's side) |
| The network: unreadable | the read fails | the stop's record | the Routes card says the routes cannot be read, never that no route stops there | — | the next read | e2e `ui-stops` "a stop whose routes cannot be read says so…" |
| Sign-in: none, or expired | `401` | everything | — | a save waits: "Saved on this device. Sign in to send it." | the outbox sends it once the person signs in | e2e `ui-stops` "a write needs sign-in…" |
| The work order system: none connected | `501` | the request | opening a work order: "No work order system is connected. Enter the work order by hand." | the person enters the work order by hand, on the page | — | none |
| The attachments store: no answer | a fetch error | the record | — | a photo or a drawing waits on the device, named by its hash, shown from the device's copy | the outbox sends it before the survey that names it | none |
