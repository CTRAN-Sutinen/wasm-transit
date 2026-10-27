# Capability bus — status

THE status doc: what is done, what is open. THE plan — the
decomposition of the remaining monolith, unit by unit — lives in
`docs/PLAN.md`. Architecture: `docs/ARCHITECTURE.md`. The three
laws: `crates/AGENTS.md`. Testing and the dogfood-QA rules:
`docs/TESTING.md`. Read the root AGENTS.md, then PLAN.md, then
this file's Current state before touching anything.

## Current state

The bus IS the architecture — the monolith is legacy surface
being deleted, and its remaining ops are exactly `docs/PLAN.md`'s
table (every one has a need, a home, and a unit; U1 has landed,
the rest are OPEN below). The QA feed: `testdata/ctran-gtfs.zip` (the real
C-TRAN GTFS, NOT COMMITTED — licensed data; fetch from c-tran.com)
and the rt payloads from gtfs-rt.c-tran.org. Every capability unit
is dogfooded on that real data before it is done
(docs/TESTING.md); a one-off script is never tooling — it is a
finding routed into this doc.

**Status claims carry receipts.** Every DONE line below names the
gate, e2e spec, or dogfood session that pins it — a claim without
a pin is not done, and a claim whose pin went stale (an e2e that
no longer runs, a corpus step that no longer exists) is FALSE
until re-pinned. Three grades, never conflated: the op is ON THE
BUS (module + monolith deletion), the UI READS the bus, and it is
DOGSFOODED on real data. A line that holds only the first grade
must not claim the other two.

LANDED:

- **analysis** → transit-analysis / cap-analysis. Monolith ops
  deleted (compile + corpus); the planner's numbers read the bus
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
  cap-rt. The monolith's decode_rt deleted (ABI smoke pins it);
  the UI's rt flows decode through the bus (actions.ts → rtCap;
  e2e rt specs); dogfooded (C-TRAN TripUpdates + VehiclePositions
  — the split-feed law is a permanent test because the dogfood
  caught it).
- **the dispatch reads** (active_services, blocks, feed_info,
  stops_by_ids) → cap-gtfs at extraction time. Monolith ops deleted
  (ABI smoke + corpus pin the retirements); the blocks list / chips
  / feed header read the bus (app.spec); dogfooded (C-TRAN Sunday:
  two service keys, 41 vehicles, the 092's 15-hour chain). U2
  moved the day-grade reads to the need modules (below): blocks →
  cap-runcut, active_services → cap-calendar.
- **the DAY VIEWS** (departures, station_departures, block_trips
  with the layover ripple) → transit-dispatch / cap-dispatch at
  extraction time — the module dissolved in U2 into the need
  modules (below).
  Monolith ops deleted (ABI smoke + the corpus step-set pin);
  the board and chains answer `network_hash + edits_hash + date`
  through the resolver, the rt-delay and click-through joins go
  through the trip map (app.spec: board, station board, blocks,
  the diversion ripple, and the board-degrades spec); dogfooded
  (C-TRAN: Fisher's Landing station board, block 3-SUN8006, the
  route-012 detour rippling 5 legs, 11/11 late-trip joins).
- **U1, the street graph** (streets_load, street_snap,
  street_route) → transit-streets / cap-streets. Monolith ops
  deleted (ABI smoke pins every retirement, JSON op and bulk op 2
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
  route_stop_sequence, summary (ABI smoke pins every retirement;
  the corpus step-set dropped the picker/summary steps and the
  unknown-feed step moved to stops_geojson); the feed line reads
  cap-gtfs's load_feed summary + feed_info (already on the bus).
  cap-gtfs ends the unit as parse + identity alone. ON THE BUS:
  cap-directory's dispatch test, caps-smoke's directory flow, the
  justfile runner gate (one chained session: load → stops →
  names join → routes → patterns), the manifest (10 capabilities,
  compose-time self-validation). UI READS THE BUS: the comboboxes
  (DepartureBoard, TripPlanner, DiversionPanel routes), the map's
  name lookups, the diversion anchors, and the feed line —
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
  a second copy). Monolith op deleted: coverage_load (ABI smoke
  pins both retirements — the JSON op and bulk op 3); the
  artifact's bytes seam is load_feed alone. ON THE BUS:
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
  identity is `(schedule, start_secs)` — the key the codec's
  trip-id map and every day view already agree on (the legs carry
  it; the map joins through the trip map until U6 takes it).
  cap-router speaks the bus document currency: load_network /
  load_edits / plan / plan_compare, answered by hash pair + date,
  the RAPTOR graph cached per (network, edits, date); the compare
  facet is the diversion delta — the unpatched day, one
  materialization apart, baseline honestly null when no edit is
  in force. Monolith ops deleted: plan_trip, plan_trip_compare
  (ABI smoke pins both retirements; the corpus step-set dropped
  the plan_trip_compare step; the Go round-trip dropped its
  compare block — the delta facts live in cap-router's dispatch
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
  trip_segment_geojson, AND set_diversions (ABI smoke pins every
  retirement; the corpus step-set dropped the geojson and
  diversion steps and gained plans_resolve; the artifact's
  remaining UI surface is the plans chain — the GtfsHandle is
  gone from the browser host, the feed registry stays for the
  server's sync validation only). The edit document's validation
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

Model facts that hold everywhere: the import infers block
layovers from observed end→start gaps (GTFS has none; the planner
re-authors from there) and reports every resolution — never
silent; diversions produce patched copies (the base feed is
immutable); retire = tombstone.

OPEN — the units of `docs/PLAN.md`: U7 sync +
plans, U8 the GTFS substrate retires (U1 cap-streets, U2 the need
cut, U3 the directory + feed line, U4 coverage, U5 planning, and
U6 the map view are landed above). The monolith's remaining
surface is the sync seam (load_feed, drop_feed, sync_edits — the
server's POST /edits path) and the plans chain; U7 decides their
placement. transit-core is deprecated as of this cut —
apply_diversions lost its last caller with set_diversions, and
the crate itself folds into the codec in U8. U7 is placement
only — the persistence mechanism (pointer CAS, the store
reflog) is the host discipline's (`docs/PLAN.md`), not U7's to
design.

## Open findings (routed, never scripted)


- **Cross-session chaining in caps-run** — a session needs a
  document a PREVIOUS session imported (`--save` / `--op-from`, or
  a session document store). Today it is a hand-carried wrap; the
  runner exists to remove exactly this friction.
- **Stop search shaping on cap-gtfs** — `stops` returns first N
  by id; finding a station's bays means pulling the whole list and
  grepping client-side. An `include`/query shape (or `find_stops`)
  is the query-shaping pattern applied here.
- **Trip-map shaping** — the full map is 539.6 KiB on C-TRAN;
  fine for a UI (fetched once per boarding), heavy for an agent
  asking "which derived trip is agency id 757797?". Shape it when
  an agent session actually needs it.
- **stop_board — the realtime-first board** (user decision
  2026-10-25): predictions are the primary display, schedule rows
  appear where no live data covers a trip; station pooling rides
  along in the join op, and the diversion question folds into it.
  Not urgent — cap-rt owns the join machinery when it lands.

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
