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

OPEN — the units of `docs/PLAN.md`: U3 the network directory +
the feed line (cap-directory — NEW, the pickers are model
questions), U4
coverage, U5 planning, U6 the map view, U7 sync + plans, U8 the
GTFS substrate retires (U1 cap-streets and U2 the need cut are
landed above). The rider planner still plans through the monolith
(U5); the map still draws through the monolith's geojson ops (U6);
`set_diversions` is still the edit-document plumbing for both.
transit-core is deprecated as of this cut — its day answers lost
their last bus callers, and the crate itself folds into the codec
in U8.

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
- **Fuel metering** in the runner once it may point at untrusted
  registry entries (the wall-clock guard is the stopgap).
- **cap-authz** as a capability (policy document, server-side
  assembly, fail-closed; the validator's scope check).
- **Sync persistence** — blobs + pointers, migrate-at-receive,
  pointer CAS, one writer per prefix.

## Conventions for whoever picks this up

Commands: the root AGENTS.md. The three laws: `crates/AGENTS.md`.
Testing expectations and the dogfood-QA rules (including the
undocumented-ops law and the all-work-through-caps rule):
`docs/TESTING.md`. This doc carries only status and the plan —
if a rule needs restating here, it lives in the wrong place.
