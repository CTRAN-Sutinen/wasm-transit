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
    its age, and a read with no answer says since when. A read that
    answered with something the page refused (a module's refusal, not
    a `ReadFailed`) is `refused`, and its label gives the reason. A
    changed document changes the input's `feeds` in one request for
    each poll. `read(...ids)` reads inputs now (a write moved their
    head). `stop()` ends the reads: a page that applies a new build
    stops them, because the old build's worker hands over only when no
    request is in flight. The laws are `src/uis/inputs.test.mjs`.
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
   persona is one example of who has the need). The rider,
   dispatcher, and planner cuts are examples, not the right cuts. A
   persona never becomes a type, an enum, a layout, or a branch in
   code. Write the questions
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
   its description (`generated-uis/<id>.md`), and the unit keeps the
   rows that are still open.

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
   in the descriptor's `session` field. Describe the UI in its own file,
   `generated-uis/<id>.md`, and add its link to "The UIs today". The
   file opens with the line "`<id>` (`src/uis/<id>/`) …", which the
   failure-table gate reads. Give it
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

## The UIs today

Each UI has one file in `generated-uis/`: its layout, its host, and its
failure table. The failure-table gate reads them (`tools/failure-tables.mjs`).

- [service-design](generated-uis/service-design.md)
- [service-change](generated-uis/service-change.md)
- [ride-now](generated-uis/ride-now.md)
- [stop-display](generated-uis/stop-display.md)
- [disruptions](generated-uis/disruptions.md)
- [stops](generated-uis/stops.md)
