# Capability bus — status

THE status doc: what is done, what is open. THE plan — the units
of the decomposition — lives in `docs/PLAN.md`. Architecture: `docs/ARCHITECTURE.md`. The three
laws: `crates/AGENTS.md`. Testing and the dogfood-QA rules:
`docs/TESTING.md`. Read the root AGENTS.md, then PLAN.md, then
this file's Current state before touching anything.

## Current state

The bus IS the architecture — the monolith is deleted, every op
answers through a capability module (the units are LANDED below;
what remains open is listed there). The QA feed: `testdata/ctran-gtfs.zip` (the real
C-TRAN GTFS, NOT COMMITTED — licensed data; fetch from c-tran.com)
and the rt payloads from gtfs-rt.c-tran.org. Every capability unit
is dogfooded on that real data before it is done
(docs/TESTING.md); a one-off script is never tooling — it is a
finding routed into this doc.

**Status claims carry receipts.** Every DONE line below names the
gate, e2e spec, or dogfood session that pins it — a claim without
a pin is not done, and a claim whose pin went stale (an e2e that
no longer runs, a gate that no longer exists) is FALSE
until re-pinned. Three grades, never conflated: the op is ON THE
BUS (module + the old seam's deletion), the UI READS the bus, and it is
DOGSFOODED on real data. A line that holds only the first grade
must not claim the other two.

LANDED:

- **analysis** → transit-analysis / cap-analysis. Monolith ops
  deleted with the monolith; the planner's numbers read
  the bus
  (app.spec: planner-degrades spec; caps.spec); dogfooded
  (C-TRAN: 1,773 trips / 29 routes / 886 stops / 646.5 revenue
  hours, reproduced through capabilities alone).
- **the network codec** (import / validate / export, and the
  TRIP-ID MAP — the agency-id ↔ derived-id join) → transit-codec
  / cap-network. Monolith codec ops deleted; the planner's
  bootstrap / validation / publish cross the bus (app.spec:
  network-degrades spec); the map's join law is property-tested
  (import_prop_test law 3); dogfooded (C-TRAN: 2.5 MB document,
  6,273-entry map, total and 1:1).
- **rt decode and the schedule joins** (trip_progress,
  late_trips with the delayed-but-dark flag) → transit-rt /
  cap-rt. The monolith's decode_rt deleted with the
  monolith; the UI's rt flows decode through the bus (actions.ts → rtCap;
  e2e rt specs); dogfooded (C-TRAN TripUpdates + VehiclePositions
  — the split-feed law is a permanent test because the dogfood
  caught it).
- **the dispatch reads** (active_services, blocks, feed_info,
  stops_by_ids) → cap-gtfs at extraction time. Monolith ops deleted
  (deleted with the monolith); the blocks list / chips /
  feed header read the bus
  (app.spec); dogfooded (C-TRAN Sunday:
  two service keys, 41 vehicles, the 092's 15-hour chain). U2
  moved the day-grade reads to the need modules (below): blocks →
  cap-runcut, active_services → cap-calendar.
- **the DAY VIEWS** (departures, station_departures, block_trips
  with the layover ripple) → transit-dispatch / cap-dispatch at
  extraction time — the module dissolved in U2 into the need
  modules (below).
  Monolith ops deleted with the monolith; the board and chains answer `network_hash + edits_hash + date`
  through the resolver, the rt-delay and click-through joins go
  through the trip map (app.spec: board, station board, blocks,
  the diversion ripple, and the board-degrades spec); dogfooded
  (C-TRAN: Fisher's Landing station board, block 3-SUN8006, the
  route-012 detour rippling 5 legs, 11/11 late-trip joins).
- **U1, the street graph** (streets_load, street_snap,
  street_route) → transit-streets / cap-streets. Monolith ops
  deleted with the monolith — the bulk op with the JSON op
  alike; cap-streets's dispatch test + caps-smoke + the justfile
  runner gate pin the module — load_streets / snap / route over
  the content-hash registry); the detour editor reads the bus
  (app.spec: the street-import, snapping, and routed-trace specs;
  caps.spec: cap-streets assembles) — the map's snap/route calls
  are the capability's, the wasm handle is gone. NOT dogfooded:
  the real-data path is the county soak (`just soak`,
  COUNTY_GEOJSON), which needs the licensed county export — the
  first dogfood session on it re-pins this line.

- **U2, the need cut of the day views** — cap-dispatch was a
  persona ("the operator's day") and cap-gtfs answered day
  questions over the feed beside it; both dissolved per
  docs/PLAN.md's criteria. cap-board (when does service leave this
  stop — departures, station_departures, the WATCHLIST moved from
  cap-gtfs with its v1→v2 migration), cap-runcut (what does this
  vehicle do today — the day's blocks list, one block's chain
  with the ripple), cap-calendar (which service keys run today —
  from the effective day). cap-gtfs keeps only the feed document
  (load_feed, stops, stops_by_ids, feed_info); its feed-side day
  reads died as duplicates (criterion 6), pinned by cap-gtfs's
  dispatch test (the retired ops answer as request errors) and
  caps-smoke's retirement loop. ON THE BUS: the three modules'
  dispatch tests, caps-smoke's board/runcut/calendar flows, the
  justfile runner gates (board: station + watchlist chained;
  runcut: blocks → edits → ripple; calendar: $TODAY), and the
  manifest (9 capabilities, compose-time self-validation). UI
  READS THE BUS: DepartureBoard rows through cap-board, chips
  through cap-calendar, BlocksPanel list + chain through
  cap-runcut, the demo composition feed → network → board →
  router (caps.spec: assembly, composition, degrade; app.spec:
  the board, station board, blocks scoping, both degrade specs —
  runcut and board, not gtfs/dispatch). NOT dogfooded: the
  C-TRAN board/chains dogfood above predates the cut — the first
  real-data session through cap-board/cap-runcut/cap-calendar
  re-pins this line. Model-native honesty note: the fixture's HOL
  key (its single trip dropped at import) does not appear in
  calendar's answer, and B2 (same reason) is not offered by the
  blocks list — the honest holes, pinned by the module tests.

- **U3, the network directory + the feed line** — the pickers
  were model questions answering over the feed; the criteria moved
  them. cap-directory (ONE question: what exists in the network?):
  stops (name-sorted, walking-group structure), stops_by_ids (the
  names join, moved from cap-gtfs), routes (the model's mode set),
  route_stop_sequence (a route's PATTERNS with ordered stops — the
  model's directions; the diversion editor picks a pattern now,
  and new edit records carry direction_id null, the documented
  both-directions case). Monolith ops deleted: stops, routes,
  route_stop_sequence, summary (deleted with the monolith); the feed line reads
  cap-gtfs's load_feed summary + feed_info (already on the bus).
  cap-gtfs ends the unit as parse + identity alone. ON THE BUS:
  cap-directory's dispatch test, caps-smoke's directory flow, the
  justfile runner gate (one chained session: load → stops →
  names join → routes → patterns), the manifest (compose-time
  self-validation; 14 capabilities today),
  compose-time self-validation). UI READS THE BUS: the comboboxes
  (DepartureBoard, TripPlanner, DiversionPanel routes), the map's
  name lookups, the diversion anchors (cap-divert's draft view
  since W3), and the feed line —
  primed once per feed (caps.spec: cap-directory assembles;
  app.spec: the picker/combobox specs, the feed line, the
  diversion flow with the pattern default preserving the longest
  anchor sequence). NOT dogfooded: the first real-data session
  through the directory re-pins this line.

- **U4, coverage** — the monolith's coverage_load was a parse-only
  DUPLICATE of the capability (criterion 6: cap-analysis already
  registered layers by content hash and answered type_coverage);
  the criteria delete the duplicate, they do not re-derive it.
  The registration answer grew into the import handle:
  load_population_layer returns points, pop_field and total_pop
  (the monolith op's exact answer shape — the import UI's status
  line and chip are unchanged, and the layer's owner never parses
  a second copy). Monolith op deleted with the monolith (the JSON op and
  bulk op 3 alike). ON THE BUS:
  cap-analysis's dispatch proptest (the handle's fields: points,
  the picked pop_field, the summed population) and caps-smoke's
  analysis flow (the handle asserted, then type_coverage by the
  hash pair). UI READS THE BUS: the population import (file and
  URL) registers through ensureAnalysisLayer — the registration
  IS the handle the DataSources chip shows — and the coverage
  refresh reads the same registration (app.spec: the
  population-layer import spec). NOT dogfooded: the first
  real-data session with a census layer re-pins this line.

- **U5, planning** — the router's feed-document input was a
  Stage-B-era shape, not an option to weigh: planning composes
  over the NETWORK document and the EFFECTIVE DAY like every
  model-side capability. transit-router builds from the model now
  (`build_from_day` over the materialized day — patterns grouped
  by ACTUAL stop sequence, so an edited trip that skips stops
  rides its own pattern; the GTFS shape is gone, the crate is
  GTFS-free, `fmt_time` inlined ahead of U8) and the derived trip
  identity is `(pattern_trips, start_secs)` (named `schedule` until
  the 2026-10-27 rename) — the key the codec's
  trip-id map and every day view already agree on (the legs carry
  it; the map joins through the trip map until U6 takes it).
  cap-router speaks the bus document currency: load_network /
  load_edits / plan / plan_compare, answered by hash pair + date,
  the RAPTOR graph cached per (network, edits, date); the compare
  facet is the diversion delta — the unpatched day, one
  materialization apart, baseline honestly null when no edit is
  in force. Monolith ops deleted: plan_trip, plan_trip_compare
  (deleted with the monolith); the delta facts live in
  cap-router's dispatch
  test: 08:15 active vs 08:10 baseline) and the feed state's
  router cache died with them. ON THE BUS: transit-router's
  raptor/oracle/FIFO suites over the model day, cap-router's
  dispatch test (identity law, plan, the diversion delta, the
  no-baseline case, honest bulk-op error), caps-smoke's router
  flow (document identity agreement with cap-network, plan, the
  delta), the justfile runner gate (one chained session:
  load → plan → load_edits → compare), and the manifest (router
  consumes network; plan/plan_compare carry REAL computed
  examples from the fixture). UI READS THE BUS: the rider planner
  boards the loaded feed through cap-network's import
  (ensureRouterBoarding) and plans/compares by hash — the
  itinerary's drawn segment joins the legs' derived identity to
  the agency trip ids through the trip map; the demo composition
  routes through the same boarding (caps.spec: the composition
  and degrade specs; app.spec: the planner, segment, and
  diversion-delta specs — 31 Playwright tests). NOT dogfooded:
  the first real-data plan session through cap-router re-pins
  this line.

- **U6, the map view** — the map is a VIEW, not a capability:
  every geojson draw left the artifact. The map renders from what
  the host already owns — the stops layer from the directory's
  stop list (coordinates included), the network document's
  authored alignments (`Pattern.path`, captured at import), the
  effective day through cap-runcut's NEW trip detail facet
  (`trip_stops`: one derived trip's stops on a date, EFFECTIVE —
  a diverted trip answers its patched sequence; the answer
  carries no geometry, the host's view code draws), the edit
  document's detour polylines (already host-drawn), and the
  trip-id map for click-throughs (`web/src/mapview.ts`, with the
  rider-side imported network document held by the host — JS
  owns documents). Monolith ops deleted: stops_geojson,
  stop_point, trip_path_geojson, route_path_geojson,
  trip_segment_geojson, AND set_diversions (deleted with the monolith). The edit document's validation
  reports — the UI's per-record display — now come from the day
  views' load_edits (the same registration every day view reads;
  memoized per document content). ON THE BUS:
  transit-dispatch's trip_stops + cap-runcut's dispatch test
  (the trip detail: authored A→B with B at 08:10, the patched
  via-C trip keeps A→B with B at 08:55, an absent trip answers
  empty), caps-smoke's runcut flow (authored vs patched trip
  detail, absent answers empty), the justfile runner gate (one
  chained session: load_network → load_edits → trip_stops), and
  the manifest (runcut's trip_stops op doc + a REAL computed
  example). UI READS THE BUS: the stops layer (directory), the
  selected trip's line (trip-id map join → trip_stops → the
  authored alignment, stop-to-stop when the effective trip
  deviates from its pattern), the itinerary's ridden segments
  (nearest-vertex slices of the authored alignment), the
  diversion editor's ghost (the selected pattern's authored
  alignment), and the apply/retire status from the day views'
  reports (app.spec: the planner segment, trip line, ghost,
  diversion flow and sync round-trip specs — 31 Playwright
  tests). NOT dogfooded: the first real-data session drawing
  the map through the host view re-pins this line.

Infrastructure, DONE (pinned by the same `just check` the
gates run in): cap-abi (the undocumented-ops law and the
manifest's compose-time self-validation), the resolver
(caps.spec: assembly, hash verification, degrade, migrate and
rollback), the agent runner (`just test` runs chained
$reference sessions every run), migrations (caps-smoke +
caps.spec), agent context (gen-manifest computes the worked
examples from the real fixture at generation time), query
shaping (the shaping law is tested; the 249-byte C-TRAN
day-shape answer is the dogfood record).

- **U7, sync + plans — and the monolith dies.** Placement per
  `docs/PLAN.md` (both ops are cross-document logic, the same
  standing as transit-sync's merge, so both get the library +
  module pair): **cap-sync** answers the sync protocol's one step
  (`sync_edits`: LWW merge + the submit transition, the host
  passes the clock; transit-sync is the library) — WITHOUT the
  old op's feed-scoped reports, which were a criterion-6
  duplicate of every model-side module's load_edits validation
  answer; the reports' registry (load_feed, drop_feed) died with
  them and transit-core's report_all lost its last caller.
  **cap-plans** answers the plan chain (`resolve` / `periods` /
  `validate` over the plan set's metadata; transit-plans is the
  library — the chain is derived, never stored). ON THE BUS:
  cap-sync's dispatch test (LWW, the tombstone, submit
  idempotence, pair order-insensitivity), cap-plans's dispatch
  test (the seam) + transit-plans's chain tests (the laws:
  resolution, windows, validity, order-insensitivity), caps-smoke
  (sync + plans flows), the justfile runner gates (both), the
  manifest (12 capabilities, REAL computed examples for every
  op), and the Go host: the server hosts cap-sync
  (web/pkg/cap_sync.wasm — TestSyncRoundTrip, TestEditsSyncEndpoint
  through real HTTP, TestConcurrentInvokes under -race,
  TestModuleIsTheDeployedBytes). UI READS THE BUS: the planner's
  chain view (in force, governing periods, issues) reads
  cap-plans (app.spec: the service-plans spec — resolve, derived
  periods, survive-reload); the sync protocol is unchanged for
  the browser (POST /edits; the server merges through the
  module — the Go round-trip is the pin). THE MONOLITH IS GONE:
  transit-ops and transit-wasm are deleted, with them the corpus,
  provenance record, and abi-smoke (they pinned THE one artifact's
  bytes); the monolith's regression tests moved to their owning
  crates (transit-codec's import regression suite,
  transit-analysis's regression suite), and the browser loads
  capability modules only. NOT dogfooded: no real-data session
  has synced or resolved a real plan chain through the modules
  yet.

- **The QA follow-through (2026-10-27)** — the dogfood findings
  of the 10-26/10-27 sessions, cut into units (QA-FINDINGS carries
  each finding's status and pin; this line carries the grades).
  Gate for all of it: `just check` green (host suites, caps-smoke,
  runner gates, Go -race, 31 Playwright specs), and one C-TRAN
  re-pin session per lens (QA-FINDINGS, "Session 2026-10-27
  (re-pin)").
  * Boards stop listing arrivals (QAF-29): ON THE BUS, UI READS
    THE BUS (app.spec diversion + station specs), DOGFOODED.
  * The live view judges dark per block and names the vehicle
    (QAF-30, QAF-33): ON THE BUS, DOGFOODED. The UI does not read
    late_trips yet.
  * Runner sessions (QAF-8, QAF-10 within a session, QAF-38,
    QAF-46): ordered steps, `--cap`, `--as`, numeric clock, one
    registration name; the justfile cross-capability gate.
    DOGFOODED (every re-pin session spans capabilities).
  * Analysis (QAF-7 contract, QAF-41, QAF-42, QAF-43): ON THE BUS,
    DOGFOODED; the planner screens render the new headway
    unchanged in shape — no UI reads `window` or per-route hours
    yet.
  * Validate (QAF-1 issue, QAF-40 and QAF-45 warnings): ON THE BUS,
    DOGFOODED; UI READS THE BUS for the issue (app.spec authoring
    spec — the browser retime now carries its block reference).
    The browser does not show warnings yet.
  * Authoring, cap-author (QAF-39, QAF-47): ON THE BUS (edit_test,
    dispatch_test, caps-smoke, justfile author gate), DOGFOODED
    (the 006 change through `apply`). UI READS THE BUS since W2
    (below).
  * Small op gaps (QAF-13, QAF-34, QAF-35, QAF-37): ON THE BUS,
    DOGFOODED; the map draws merged walks through their via stops
    (no e2e pins that drawing).

- **The web cleanup (2026-10-27)** — parity: `web/` is host glue
  plus render components (`docs/PLAN.md`, the web cleanup).
  * W0 the ratchet: LANDED (`just test` WEB GATES — components
    importing the resolver, a hard gate since W6 emptied it; raw
    rt `stop_updates` in web/, a hard gate since W1 emptied it;
    clone-and-mutate, a hard gate since W2).
  * W1 live data: ON THE BUS — the live document (transit-live
    live_test; transit-rt live_test over the fixture pair), cap-rt
    `load_trip_map` / `live` and cap-board `load_live` /
    `stop_board` (dispatch tests, dispatch_laws_test realtime-
    first and pooling laws, caps-smoke, the justfile live-board
    gate); `departures` / `station_departures` retired (one board
    on the bus). UI READS THE BUS: the departure board, RtPanel,
    and the map's bus colors render the live document and
    stop_board's answer (app.spec "realtime feed colors delays and
    vehicles"; the board, station, and diversion specs).
    DOGFOODED (QA-FINDINGS, "Session 2026-10-27 (live board)").
    The live document names routes (route_name, from the feed the
    import named them by). The blocks view's trip column reads
    cap-network `agency_trip_ids` and its connection flags read
    cap-runcut's `layover_health` (dispatch_laws_test, cap-network
    and cap-runcut dispatch tests, caps-smoke; app.spec blocks
    specs). W1 is DONE.
  * W2 authoring: ON THE BUS — the gesture vocabulary (transit-
    network gesture_test: minting, timing alignment, sorted starts,
    block order and layovers, calendars, field-wise plan frame;
    cap-author dispatch_test `created`; caps-smoke). UI READS THE
    BUS: every planner change is a NetworkEdit through cap-author
    (app.spec "planner authors a network" — the retime and the
    minted stop; "service plans frame service changes" — field-wise
    plan frame, a fork waits for edits in flight). DOGFOODED
    (QA-FINDINGS, "Session 2026-10-27 (authoring gestures)"). The
    W0 ratchet forbids clone-and-mutate in web/ (empty allowlist).
    The browser's `retime_trip` mirror is gone.
  * W3 operator edit records: ON THE BUS — cap-divert (transit-
    dispatch divert_test: the default pattern, interior skipped
    unless served, a revisited anchor never skipped, problems not
    errors, the flattened trace, the named rows; transit-sync
    envelope tests: propose, retire/restore without rev churn,
    sanitize; cap-divert dispatch_test; caps-smoke divert flow;
    the justfile divert gate — propose then board load_edits).
    UI READS THE BUS: the diversion editor renders cap-divert's
    draft view and list rows; Apply, Retire, Restore, and every
    stored or synced edit document go through its ops (app.spec
    "diversion reroutes the schedule…", "detour trace routes
    along the street graph…", "diversion syncs to the server and
    back", "a malformed saved record cannot wedge apply and
    retire"). DOGFOODED (QA-FINDINGS, "Session 2026-10-27
    (diversion authoring)"). The editor's longest-pattern pick,
    auto-skip, record builder, tombstone, slice dedupe, and the
    serde-defaults mirror are gone from web/; skips are now
    "every interior stop not served", so unchecking every skip
    no longer re-skips them all.
  * W4 geometry: ON THE BUS — transit-network geometry_test (the
    alignment contract, entity focus per kind), transit-dispatch
    geometry_test (a trip on its pattern draws the alignment, a
    diverted one its patched stops, the ridden segment sliced);
    cap-runcut `trip_path`, cap-router `itinerary_paths`, and
    cap-directory `focus` (dispatch tests, caps-smoke, the justfile
    geometry gate — one chained session over all three). UI READS
    THE BUS: the trip line, the itinerary, the planner drill-down,
    and the diversion ghost draw op answers; mapview.ts only shapes
    GeoJSON (app.spec itinerary and trip-line specs; "planner
    authors a network" — the WK focus draws its five patterns).
    DOGFOODED (QA-FINDINGS, "Session 2026-10-27 (geometry)";
    QAF-50 open). A block's drill-down now frames its patterns
    (it framed nothing before).
  * W5 planner numbers and search: ON THE BUS — transit-network
    listing_test (rows narrowed by every term, a route's
    composition, trip counts that are the materialized day's even
    where bands overlap); transit-router raptor_test
    `a_compared_plan_tells_the_rider_what_it_cost`; cap-directory
    `entities` and `route_detail`, cap-router `plan_compare`'s
    notice (dispatch tests, caps-smoke, the justfile planner-lists
    gate — one chained session over both). UI READS THE BUS: the
    planner's list screens and the route screen render cap-
    directory's rows; the trip planner calls `plan_compare` alone
    and shows its notice (app.spec "planner authors a network" —
    the stop filter and the route's composition; "unplannable trip
    gives inline feedback"; "diversion reroutes the schedule…" —
    the arrival the diversion cost). DOGFOODED (QA-FINDINGS,
    "Session 2026-10-27 (planner lists)"). `scheduleTripCount`,
    RouteForm's visit and grouping code, the rail's filters, and
    the plan-or-compare branch are gone from web/. The schedule
    list now shows each schedule's pattern, service type, and trip
    count; a menu jump clears the list filter.
  * W6 render-only components: ON THE BUS — transit-network
    listing_test `a_stop_lists_the_routes_serving_it`; cap-directory
    `stop_detail` (dispatch test, caps-smoke, the justfile planner-
    lists gate). UI READS THE BUS: no component imports the
    capability resolver — the W0 resolver ratchet's allowlist is
    empty, a hard gate in `just test`. The host layer makes every
    op call: the feed line (actions `refreshFeedLine`), the map's
    street snap and route (actions `snapToStreets` /
    `routeOnStreets`), the composition demo (capdemo.svelte.ts);
    the trip planner takes props and emits `onplan`; the directory
    store lives in state.svelte.ts. The planner's stop screen
    renders `stop_detail`, and its pickers, name chips, and the
    Document counts render cap-directory's `entities` rows
    (app.pickers) — app.spec "planner authors a network" (stop B
    served by route 10; the Document stop count); the caps.spec
    composition and document specs. DOGFOODED (QA-FINDINGS,
    "Session 2026-10-27 (render-only components)"; QAF-51 open).
    Earlier in W6: DepartureBoard, RtPanel, BlocksPanel, and (with
    W3) DiversionPanel went render-only. Forms still read the
    entity they edit from the plan document — JS owns the
    documents, and that read is the form's value, not an answer.
- **Generated UIs (2026-10-27)**: `/` is a picker over
  `src/uis/<id>/`, and the original app is demoted to `/app.html`
  (`docs/ARCHITECTURE.md`, web section, "Generated UIs"). No new
  ops: each UI composes existing ones. Pinned by the GENERATED UIs
  gate in `just test` (descriptor, page, and spec present; thin
  components only; no app-shell imports) and by e2e picker.spec
  (one card per `ui.json`; each card opens its page; the original
  app link boots the app).
  * `service-design`: UI READS THE BUS, pinned by e2e
    ui-service-design.spec (import → type → route → schedule, the
    fork, the change with its warning, coverage). DOGFOODED
    (QA-FINDINGS, "Session 2026-10-27 (service-design
    workbench)"): in the browser on C-TRAN, the numbers equal the
    runner's (1-WKDY rev h, coverage, block count).
  * Not done: layouts are hand-placed Svelte, not layout
    documents. MapView and the entity forms are still coupled to
    the original app's `app` state. Generated UIs replace them with
    thin components (FocusMap, ScheduleChange) rather than
    rebuilding them in place.

- **The vocabulary rename (2026-10-27)**: "schedule" is no longer a
  model word. `PatternSchedule` → `PatternTrips`, the `Schedule`
  enum → `Departures`, the document's `schedules` → `pattern_trips`,
  a trip's identity is `(pattern_trips, start_secs)`, and the edit
  kinds, entity kind, errors, op contracts, and TS bindings follow.
  Stored documents written before the rename still read: the
  modules through serde aliases, the hosts by restoring plans
  through cap-author (`readStoredNetwork`; e2e ui-service-design
  "a plan stored under the old field names", which fails without
  it). Pinned by `just check` (every crate's tests,
  caps-smoke, the runner gates, e2e); testdata/network.json uses the
  new names. The original app's on-screen labels still say
  "Schedule"; it is the demoted UI.
  * Next, decided with the user: Direction in the model and the
    TIMETABLE op are landed (the entry below); not started —
    change gestures at timetable level (frequency by period, span,
    running time by period, stops on a pattern); a
    baseline-vs-proposal diff op (QAF-20); then the service-design
    UI rebuilt on them.

- **Directions and the timetable (2026-10-27)**: a route runs in
  named directions (`Route.directions`) and a pattern in at most
  one (`Pattern.direction`; validation's `unknown_direction`); the
  gestures `set_direction` / `remove_direction` (refused while a
  pattern runs in it) and `update_pattern {direction}`. The import
  keeps GTFS direction_id, naming each direction from its trips'
  headsigns; the export writes both back. The directory answers
  patterns by direction (`route_detail.directions`) and the
  planner's timetable (`timetable`: route + direction + service
  type, every trip across the direction's patterns). ON THE BUS:
  transit-network `gesture_test`
  `a_pattern_runs_in_one_of_its_routes_directions`, `listing_test`
  `a_timetable_is_one_direction_on_one_service_type_across_its_patterns`
  and `a_route_lists_its_patterns_by_direction`; transit-codec
  `codec_prop_test` (the generator now draws direction_id and
  headsigns; the fixpoint fails with direction_id dropped on export)
  and `import_regression_test`
  `directions_are_named_from_headsigns_and_survive_the_round_trip`;
  cap-directory `dispatch_test`; caps-smoke ("a timetable is every
  trip of one direction on one service type"); the directory runner
  gate in `just test`. DOGFOODED: QA-FINDINGS "Session 2026-10-27
  (directions and the timetable)" (019 inbound: 25 trips over the
  Bay C / E / J patterns). No UI reads it yet.

Model facts that hold everywhere: the import infers block
layovers from observed end→start gaps (GTFS has none; the planner
re-authors from there) and reports every resolution — never
silent; diversions produce patched copies (the base feed is
immutable); retire = tombstone.

OPEN — the units of `docs/PLAN.md`: U8, the GTFS substrate
retires (U1 cap-streets, U2 the need cut, U3 the directory +
feed line, U4 coverage, U5 planning, U6 the map view, and U7
sync + plans are landed above). No monolith op remains —
transit-ops and transit-wasm are deleted. transit-core is
deprecated: apply_diversions lost its last caller with
set_diversions, report_all lost its last caller with U7's sync
cut, and the crate itself folds into the codec in U8. The
sync-persistence design (pointer CAS, the store reflog) remains
the host discipline's (`docs/PLAN.md`), never a unit's to design
in passing.

- **The kernel registration op (2026-10-27)** — every network
  consumer (analysis, author, board, calendar, directory, divert,
  network, router, runcut) registers the bare document on bulk op
  256, parsed straight into the model; the JSON load_network takes
  the same parse, and the nine copies of the hash formula are one
  streamed `cap_abi::canonical_hash`. ON THE BUS: cap-abi
  `kernel_test` (streamed hash ≡ the serialization's hash,
  property-tested — no identity changed), each consumer's
  `the_kernel_bulk_op_registers_the_same_identity_as_load_network`
  dispatch test, caps-smoke's kernel-bulk-op flow. UI READS IT:
  every `*Cap.loadNetwork` in capabilities.svelte.ts goes through
  `CapModule.loadNetwork` (`just e2e`, 31 specs). DOGFOODED:
  caps-run `router --load 256:` on the C-TRAN network (same hash,
  `sha256:bceda1be…`, a 6-leg plan off it); a Chromium measurement
  over C-TRAN took each consumer's wasm memory from 22.1 MB to
  5.8–8.4 MB and registration from ~595 to ~300 ms per module at 4×
  CPU throttle.

## Open findings (routed, never scripted)

- **Runtime cost has no gate** — the registration numbers above
  came from a scratch Chromium bench (8 modules over C-TRAN:
  per-module linear memory, per-op ms at 1× and 4× throttle). A
  ratchet spec (memory per consumer after load_network, ms per
  registration) belongs in `web/e2e`; without it the next envelope
  regression is invisible.
- **What registration still costs** — ~300 ms per module at 4×:
  the direct parse plus the canonical re-hash every module repeats
  for the document the import already hashed, all on the main
  thread (no Worker). cap-author is +77 KiB from the change: its
  `apply` edits carry network sub-types, so it keeps the buffered
  decoder beside the direct one.


- **Cross-session chaining in caps-run** — within a session,
  documents cross capabilities by reference (`--cap`, landed
  above); ACROSS sessions nothing persists, so every session
  re-imports (a session document store, or the journal). An
  import's response also prints the whole document — 2.7 MB on
  C-TRAN — into the agent's context; a shaping flag belongs here.
- **Trip-map shaping** — the full map is 539.6 KiB on C-TRAN.
  The forward question (derived trips → agency ids) is shaped:
  cap-network `agency_trip_ids`. The reverse ("which derived trip
  is agency id 757797?") still needs the whole map; shape it when
  an agent session actually needs it.

## Standing queue (after the units)

- **The host discipline (the composition layer):** THE plan is
  `docs/PLAN.md`'s "The host discipline (after the units)" — the
  choke-point engine (the ONE op caller), flow maps as data in
  the UI manifest, branded `VerifiedDoc` at the engine seam, the
  Merkle journal (trace / answer-cache key / migration witness),
  the store reflog, and the two gates (no op call outside the
  engine; no clocks or randomness in cap crates). Replay pays the
  dogfood debt: the U1–U4 re-pin sessions are journal replays of
  the C-TRAN session, not hand-run scripts.
- **Wire-time assembly (the mechanism the thesis depends on):**
  assemblies are per NEED and reassemble per request; the persona
  tabs (rider / dispatcher / planner) are a proof-of-concept cut,
  neither exhaustive nor the right cuts — no layout, manifest, or
  registry entry is ever keyed to a persona.
  user-declared selection (`web/src/selection.svelte.ts`,
  IndexedDB-persisted, defaulting to the whole manifest), the
  store surface in the DataSources modal, probe-based sections
  (unassembled / starved / ready — three states, not one
  "degraded"), the host-side UI manifest (op → render | flow map;
  referential integrity asserted, coverage as a diagnostic; it is
  a document in the store; it may choose among existing things,
  never describe new behavior), and the agent-side anatomy
  (harnesses = assembly / projection / flow / budget, ephemeral,
  discovered then crystallized — never pre-built). The day-summary
  screen was the first probe consumer; the shaping flags were the
  first projection choice. The anchor chain: strategy (the
  user's selection) → scope (the manifest) → structure (the
  document DAG) → skeleton (the UI manifest) → surface
  (renderers). Coherence: a human and an agent ask the same
  question and get the same bytes, through the same ops on the
  same hash-addressed documents through the same manifest.
- **Registry search** as a derived artifact (purpose + op_docs +
  worked examples embedded; ranking carries evidence) — the
  agent-side store, an agent that browses the catalog and
  assembles its own harness.
- **caps-run harness as data** — when a session improvisation
  recurs, crystallize it; until then improvisations are findings.
  The journal (the host discipline) crystallizes the runner's
  chained-session format first.
- **Fuel metering** in the runner once it may point at untrusted
  registry entries (the wall-clock guard is the stopgap).
- **cap-authz** as a capability (policy document, server-side
  assembly, fail-closed; the validator's scope check).
- **Sync persistence** — blobs + pointers, migrate-at-receive,
  pointer CAS, one writer per prefix; the store reflog rides with
  it (the host discipline, `docs/PLAN.md`).

## Conventions for whoever picks this up

Commands: the root AGENTS.md. The three laws: `crates/AGENTS.md`.
Testing expectations and the dogfood-QA rules (including the
undocumented-ops law and the all-work-through-caps rule):
`docs/TESTING.md`. This doc carries only status and the plan —
if a rule needs restating here, it lives in the wrong place.
