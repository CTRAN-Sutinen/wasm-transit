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
  stops_by_ids) → cap-gtfs. Monolith ops deleted (ABI smoke +
  corpus pin the retirements); the blocks list / chips / feed
  header read the bus (app.spec); dogfooded (C-TRAN Sunday: two
  service keys, 41 vehicles, the 092's 15-hour chain).
- **the DAY VIEWS** (departures, station_departures, block_trips
  with the layover ripple) → transit-dispatch / cap-dispatch.
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

OPEN — the units of `docs/PLAN.md`: U2 the day/dispatch split,
U3 pickers + feed line, U4 coverage, U5 planning, U6 the map
view, U7 sync + plans (U1 cap-streets is landed above). The rider
planner still plans through the monolith (U5); the map still
draws through the monolith's geojson ops (U6); `set_diversions`
is still the edit-document plumbing for both.

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
