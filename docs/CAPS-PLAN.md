# Capability bus — the next unit of work

This doc is the plan for the flagged work from the C-TRAN dogfood
session (2026-10-25) and the standing capability-bus queue. It is
written to be picked up cold, in a different session, by a different
agent. Read AGENTS.md first — especially the capability-bus section
and the dogfood-QA expectation. The bus's own state:

- **Done**: cap-abi (ABI macro + manifest contract + compose-time
  validator + `gen-manifest`, which also publishes `registry/`),
  cap-gtfs, cap-router, cap-analysis (the first REAL extraction —
  the monolith's analysis ops are deleted), cap-network (the second —
  the codec: import/validate/export, the monolith's network ops are
  deleted), migrations (watchlist v1→v2), agent context (purpose +
  op docs + worked examples — the network examples are computed from
  the REAL fixture at generation time), the resolver
  (web/src/capabilities.svelte.ts), the registry (`registry/`,
  published by `just registry`), and the agent runner
  (`tools/caps-run.mjs`).
- **The pattern for every extraction** (AGENTS.md): crate boundary
  first, capability wrapper second, monolith op deletion last.
- **The QA feed**: `testdata/ctran-gtfs.zip` — the real C-TRAN
  GTFS. NOT COMMITTED (licensed agency data; fetch from
  c-tran.com). The dogfood session that used it found four real
  defects that every fixture-scale gate missed.

## The working state this unit must reach: ALL work through the caps

QA is done when a planner's whole session runs on capabilities +
`caps-run` ALONE — **no one-off scripts**. The founding dogfood
session violated this three times, and each violation was a
missing piece of this plan:

- the bootstrap scratch script → item 1 (cap-network; the flow
  becomes `caps run gtfs` + `caps run network`);
- hash-copying between sessions → item 3 (result references);
- a Node summarizer to read a 199 KiB answer → item 4 (query
  shaping — right-size at the op).

**The rule for whoever runs these sessions: a one-off script is
never tooling — it is a FINDING.** If a question needs bespoke
glue to ask, that is a missing op (right-shape the response) or a
missing runner feature (route it into this doc). The mirror
principle extends to the web UI in the same way: a question easy
via the CLI but absent from the UI is a UI gap (item 5); a
question hard via the CLI is an op gap. Both signals land here.

## 1. Extract the network codec (cap-network) — DONE (2026-10-25)

Landed per the standard pattern, one change: `transit-codec` (the
moved codec + its fixpoint laws, `codec_prop_test` and
`import_prop_test`), `cap-network` (ops `load_feed`, `import`,
`load_document`, `validate`, `export`), the monolith ops DELETED
(`network_import` / `network_load` / `network_validate` gone from
Request/Response, wasm.ts, the corpus, and the ABI smoke — which now
pins that the retired ops error through the standard outcome), and
transit-core gained the WRITE half of the codec (`src/write.rs`:
deterministic zip bytes, so the export path produces a real GTFS
artifact whose content hash is its identity). The web planner's
bootstrap, validation, and materialize cross the bus
(capabilities.svelte.ts `networkCap`; derive uses the cached feed
bytes, materialize exports → zip → the same LoadFeed a parsed zip
uses) and degrade loudly: `validation off` is a chip state, not a
claim of validity.

Dogfood acceptance met on the real C-TRAN feed, zero monolith:
`caps-run network --load ctran.zip` → import (2.5 MB document,
notes reported) → analysis on the derived document (1,773 trips /
29 routes / 886 stops / 646.5 revenue hours — the founding
session's numbers, reproduced through capabilities alone) → export
(1.1 MB spec-shaped zip, identity asserted when cap-gtfs loads it).
The published artifact is a first-class feed document.

Two findings from that session, both routing into item 3: chaining
import → analysis → export required hand-copying `network_hash`
between `caps-run` invocations and building a request file with the
2.5 MB document embedded — result references
(`$import.network_hash`, `$load.feed_hash`) remove both, and now
that a session's whole flow is capability-side the need is daily,
not occasional.

## 1b. Extract the realtime decoder (cap-rt) — DONE (2026-10-25)

Found in the dispatcher dogfood session (below): the dispatcher's
live view — delays, vehicles — was monolith-only, so the dispatcher
persona could not be assembled at all. Landed per the standard
pattern: `transit-rt` (the overview view over the prost types, the
shared library), `cap-rt` (`decode`, bulk-bytes op 0, a pure decoder
over ephemeral payloads), the monolith's `decode_rt` DELETED (the
ABI smoke pins the retired op). The web rider/dispatcher rt flows
(load, poll, vehicle map, delayed board) cross the bus and degrade
loudly (schedule times stay, last good positions stay, the status
says the capability is off). gen-manifest decodes the REAL e2e
fixture payload for the worked example; the runner gate loads the
fixture on the bulk seam and greps the known +120 s delay.
Real-data dogfood: the C-TRAN TripUpdates + VehiclePositions feeds
decoded through `caps-run rt --load` (see the dispatcher session
below). The RT feed URLs are NOT committed (agency data policy
mirrors the static feed; fetch from gtfs-rt.c-tran.org).

## 1c. Dispatcher dogfood session findings (2026-10-25, C-TRAN)

A dispatcher session through `caps-run` alone. The questions and
where the bus answers them:

- **"What leaves Fisher's Landing today?"** — the parent stop (106)
  answers ZERO departures; the nine bays answer everything
  (item 2, reproduced live). A dispatcher must poll ten ids and
  merge by hand. The watchlist IS a workaround (a v2 document with
  the bays + `watchlist_board` = a one-call multi-stop board), but a
  dispatcher should not have to build a document to see a station.
- **"What service runs today? Which blocks are out? What trips does
  block X run?"** — was the dispatcher persona's core gap:
  `active_services`, `blocks` (with date), and `block_trips`
  existed ONLY in the monolith. DONE as the dispatch reads on
  cap-gtfs (2026-10-25): all four ops (`active_services`,
  `blocks`, `block_trips`, `feed_info` — the domain code already
  lived in transit-core, so it was a Stage-B-only extraction) are
  on the bus, gated (caps-smoke + a chained runner session in
  `just test`), and dogfooded live: C-TRAN Sunday = two service
  keys, 41 vehicles scheduled, and the +6:39 route-092 bus's
  whole 15-hour block chain with layovers, hash-free. REMAINS:
  the monolith keeps its copies until the dispatcher UI
  (BlocksPanel, the block picker) rewires through the bus —
  that deletion is the extraction's Stage C.
- **Tonight's second session (2026-10-25 evening) found:**
  two late trips with delays but NO vehicle report (a fact a
  dispatcher acts on — an op that surfaces "delayed but
  dark" vehicles would make it checkable); route 050 with five
  of six buses late (the fleet-level pattern, again — a
  per-route delay roll-up op is the shaping fix); and the
  trip_progress gap confirmed exactly: the 092's predicted
  20:00:39 arrival at bay 9001 was computable ONLY by
  hand-joining the station board and the rt delay across two
  sessions.
- **UI-mirror session (2026-10-25 evening, the live dev instance
  through a /tmp Playwright spec — same four questions, same
  feeds):**
  * the STATION BOARD gap is real for humans: picking
    "Fisher's Landing Transit Center" (the station pin) rendered
    an EMPTY board. DONE (2026-10-25, the fallback wiring): the
    board keeps its per-stop path (diversion-patched, monolith)
    and, when that path answers nothing at a STATION-LIKE stop
    (a parent pin or a bay — `StopSummary` now carries
    `parent_station` so the UI stops guessing), falls back to
    cap-gtfs's `station_departures` through the bus — rows tagged
    with the concrete bay, honestly noted as the station board.
    Plain stops NEVER pool (a diverted-away stop must show its
    emptiness, not its base schedule — the diversion e2e caught
    exactly that flaw in the first cut, before the gate). DECISION
    (user, 2026-10-25) for where the board ends up: **realtime
    first, schedule as the fallback** — predictions are the primary
    display, schedule rows appear where no live data covers a
    trip. That sets the bus board's shape: a predictions-first
    join op (cap-rt owns the join machinery — a `stop_board` op
    over feed + snapshot, station pooling riding along), with the
    diversion question folded into it rather than solved around
    it. Not urgent — it lands with the dispatcher rewire unit.
  * the UI is AHEAD of the CLI on delays: the departure board
    joins live rt delays onto its rows in presentation — the
    exact join the CLI must hand-craft. That is the strongest
    argument yet for `trip_progress`: the answer's shape is
    already proven on screen, the op just has to exist so both
    surfaces (and agents) get it in one call.
  * the block panel's `tight`/`overlap` connection flags are
    presentation over `layover_secs` today — fine, but when
    `trip_progress` lands, the same flag belongs IN the op where
    every surface gets it.
  * the feed's identity has NO surface: `feed_version` /
    publisher never render (the op answers on the bus; the UI
    shows only counts). Fold into the dispatcher UI rewire.
  * caveat, stated honestly: the UI's blocks/board answers run
    through the MONOLITH, not the bus — the Stage C rewire
    (BlocksPanel, board, block picker through
    capabilities.svelte.ts) is what makes the mirror literal.
- **"Is my bus on time? Where is it?"** — was `decode_rt`,
  monolith-only; DONE as cap-rt (item 1b). The ranked follow-up is
  DONE too: `late_trips` ranks by worst stop delay, carries the
  vehicle's label and position, and raises the DELAYED-BUT-DARK
  flag when no position names the trip. The live dogfood caught a
  real design flaw the fixture gates missed: C-TRAN serves
  TripUpdates and VehiclePositions as SEPARATE feeds, so positions
  looked up in the delays snapshot alone marked every late trip
  dark — the op gained `vp_hash` (the positions snapshot, omitted
  for combined feeds), and the split-feed law is a permanent test.
  Per-route fleet patterns read straight off the ranked list
  (route 050's five late trips).
- **"When does trip X actually reach stop Y?"** — was the
  most-asked dispatch question, and NOTHING answered it on the bus
  (or in the monolith): rt gives the delay, the static feed holds
  the schedule, and no op joined them. DONE (2026-10-25): cap-rt
  gained `load_feed` (bulk op 1 — the SAME feed document identity
  gtfs returns; the manifest carries the registry edge rt → gtfs)
  and `trip_progress` (every scheduled stop with the live delay
  and the PREDICTED time in one answer; a trip the snapshot does
  not mention answers from the schedule alone). The join lives in
  the shared transit-rt library (`progress.rs`). Dogfooded live:
  the 092's board, one op.
- **Stop names by id** — DONE: cap-gtfs `stops_by_ids` (request
  order, unknown ids omitted). No more pulling 901 rows and
  joining by hand.
- **"Which feed is this? What version?"** — was monolith-only;
  DONE as part of the dispatch reads (`feed_info` on cap-gtfs,
  dogfooded: C-TRAN answers its own version on the bus).
- **Stop search** — `stops` returns first N by id; finding Fisher's
  bays means pulling the whole list and grepping client-side. An
  `include`/query shape on `stops` (or a `find_stops` op) is the
  item-4 pattern applied to gtfs. OPEN (the last read-shaping gap
  in 1c's ledger).
- **Cross-session chaining** — the session hand-copied feed hashes
  into every invocation (item 3, again — daily, not occasional).
  OPEN: a `--save`/`--op-from` runner pair or a session document
  store; today the analysis flow still embeds the derived document
  in a request file.
- **The clock** — hosts own the clock, so every op takes explicit
  dates; the runner has no `$TODAY` (an agent computing "today" by
  hand is fine once, drifts twice). DONE: `$TODAY` and `$NOW_SECS`
  resolve host-side in the worker (with `--load [N:]FILE` for
  modules with a second bulk op — cap-rt's feed join); the
  `just test` gate exercises `$TODAY` on every run.

## 2. Parent stations answer zero departures (cap-gtfs) — DONE (2026-10-25)

Landed as a NEW OP (the deliberate choice: `departures` stays the
per-stop board — a bay query is precise and the watchlist pins bays
on purpose; a station query is a different question):
`station_departures` pools the walking group (the stop, its parent,
siblings sharing that parent, the parent's children — the same
group definition transfer points compose over), merged by time,
each row tagged with its concrete bay; ANY member of the group
answers the same board. The group query lives in transit-core
(`Gtfs::walking_group`) — shared by the capability and the manifest
generator. The e2e fixture grew a station pin + two bays with a
shuttle route (the ripple: counts, corpus, planner-derive
expectations — all regenerated). Tests: the union oracle (the pooled
board IS the sorted union of per-stop departures over the group,
computed independently), limit-after-merge, plain-stop identity, the
transit-core group law. Dogfood: C-TRAN stop 106 answers the union
of its bays, each row tagged (`$load.feed_hash` chained, no
hand-copied hashes). The UI wiring (rider/dispatcher board at a
station) remains open — noted under item 5's follow-up below.

**Finding.** `caps run gtfs --op '{departures, stop_id: "106"}'`
(Fisher's Landing Transit Center) returns EMPTY. The bays (9000+)
carry the trips. The model HAS the walking-group concept (transfer
points compose over parents/siblings), but the departures op is
per-stop-id.

**What.** Pool the walking group in cap-gtfs departures (the
parent, siblings sharing a parent, and children — the same group
definition transfer points use), tagged per row with the concrete
stop so callers can see which bay. Decide deliberately: change
`departures` semantics, or add `station_departures` — the planner
UI has the same gap either way.

**Tests.** A fixture with a parent + two bays (extend
`web/e2e/make_fixtures.py`), the property-law style oracle for the
pooling, a dispatch test, and the C-TRAN repro: stop 106 must
return the union of its bays.

## 3. Runner ergonomics: result references (caps-run) — DONE (2026-10-25)

**Finding.** Chained ops need a hash only the PREVIOUS op's output
contains. Every placeholder-hash mistake costs a full document
load (2.5 MB zip, seconds) — twice in one founding session, and
twice again in the cap-network dogfood (import → analysis → export
chains by `network_hash`).

Landed in the worker (resolved BEFORE the bytes cross the ABI):
`$load.<path>` (the last --load result), `$<opname>.<path>` (the
last result tagged with that op — `$import.network_hash` is the
network flow's everyday shape), `$N.<path>` (the Nth result,
loads included); dotted paths, numeric segments index arrays;
references resolve into the `ok` payload (the document the session
prints); unresolvable references fail the session loudly. The
`just test` gates are chained sessions now — every feed hash in
the gates was replaced by `$load.feed_hash`, so the gates verify
the runner's reference resolution on every run. Syntax documented
in the runner header. REMAINING (the dispatcher session's
finding): `$TODAY`/`$NOW_SECS` placeholders (hosts own the clock,
and an agent computing "today" by hand drifts), and cross-session
chaining (the C-TRAN analysis session still embeds the derived
document in a request file — a `--save`/`--op-from` runner pair
or a session document store would close it).

## 4. Query shaping on analysis ops (cap-analysis) — DONE (2026-10-25)

**Finding.** `network_stats` on C-TRAN returns 199 KiB — every
route and every stop inline — for a question whose answer is five
numbers. The UI wants the full payload; an agent wants the summary.

Landed as include flags (`include_routes`, `include_stops`,
`include_service_types` — all default TRUE, so the UI's payload is
unchanged; `false` empties the corresponding list while every
summary number stays). The shaping law is tested (empty lists on
request, identical summaries, lean < rich on the wire). TS bindings
regenerated; op_docs carry the flags; the worked example ASKS the
day-shape question (all flags false). And the analysis ops' worked
examples are filled in — every one computed from the REAL
fixture-derived document through the shared transit-analysis
library at generation time, so they cannot drift (load_network,
network_stats, service_types_overview, service_type_detail,
type_coverage; load_population_layer is bulk-bytes, description
carries the contract). Dogfood acceptance on the real C-TRAN
network: the Wednesday day-shape answer is **249 bytes** (was
199 KiB).

## 5. The day-shape question deserves a screen (web) — DONE (2026-10-25)

**Finding (product-level).** The dogfood session answered "what
does a Wednesday look like?" in one op: 1,773 trips, 29 routes,
886 stops, 646.5 revenue hours, span 4:17–25:10. The web planner
had NO screen that answers this — the `network_stats` op was dead
UI surface for its entire life (never called from web/src), while
the CLI found it in one command.

Landed as the day-summary header of the planner's overview screen:
a date picker (defaults to today — JS owns the clock) and the
picked day's shape on one line of chips — trips, routes, stops,
revenue hours, span, transfer points — rendered through
`analysisCap.networkStats` (the shaped form: lists off, summary
numbers only) like the rest of the planner's numbers. The empty
day is an honest message ("no service runs"), and the degraded
state is the bus's: no capability, no numbers, and the screen says
so. e2e pins the fixture's day (5 trips / 3 routes / 5 stops /
0.7 revenue hours / span 08:00:00–08:50:00). FOLLOW-UP (from
item 2): the rider/dispatcher board should surface
`station_departures` when the selected stop is a station pin —
the capability answers it, the UI still queries per-stop.

## 6. UI/agent coherence — user-declared assembly and the projection artifacts

**Why this unit.** The thesis is "ship capabilities to match the
user," but the UI currently presumes what the user needs: screens
PULL capabilities (`ensure(["analysis"])` lives in a component).
The agent side hand-assembles a harness every session — the
founding session's three improvisations (bootstrap script,
hash-copying, summarizer) were harness-building without a harness
artifact, and items 3 and 4 are proto-harness features seen through
an op-shaped frame. This unit makes both sides assemblies over
the same catalog, declared by their consumer.

**The anchoring chain — one writer per plane.** The UX hierarchy
maps onto the bus mechanically, and the manifest is the anchor:
it is the only artifact both surfaces consume (agents read
purpose/op_docs; the UI pins hashes through the resolver).

```
Strategy   the user's selection     (user-authored, host state)
Scope      the capability manifest  (capability author)
Structure  the document DAG         (capability author)
Skeleton   sections + preferences   (UI author — the UI manifest)
Surface    typed renderers          (UI author)
```

Coherence, stated once: a human and an agent can ask the same
question and get the same bytes, because both invoke the same ops
on the same hash-addressed documents through the same manifest.

**Decisions (argued out — hold these):**

- **Selection is host-side and user-authored.** The user declares
  their needs; personas become suggestions, not the source of
  truth. The developer's job shrinks to authoring a total catalog
  and a total rendering; the user's selection picks the subset.
- **Sections probe, they don't require.** A section declares the
  ops it could render, ordered by preference, and renders the
  maximal subset the assembled set satisfies — progressive
  enhancement at the capability layer. Probe resolves each need
  to one of three states: **unassembled** (never selected; the
  store is the fix), **starved** (assembled, no suitable document
  present; a data flow is the fix), **ready**. Starved and
  unassembled are different affordances, not one "degraded".
- **The UI manifest is a separate, host-side artifact** (op →
  `render` | `flow:...`, section preferences, labels). It does
  NOT live in the capability manifest — UI structure is a view,
  and views own their own structure ("the map is a view, not a
  fixture"). There is NO `agent_only` flag and NO manifest→UI
  coverage gate: an op with no UI entry is legal (agent compute,
  plumbing awaiting a flow). The assertion runs in the other
  direction — **referential integrity**: a UI manifest entry
  naming an unknown op fails the build (shape drift is already
  caught by the generated response types). Coverage runs as a
  DIAGNOSTIC (the unmapped-ops list — standing input to the
  UI-gap queue); diagnostics are measurements, regressions are
  assertions.
- **The UI manifest is itself a document in the store**
  (hash-chained, produced_by, rollback): a UI customization by
  an agent or user is inspectable, diffable, reversible. It fails
  SOFT: entries naming unknown ops/renderers degrade loudly at
  load instead of crashing. The law that keeps it from becoming a
  homebrew UI framework: **the manifest may choose among existing
  things — ops, renderers, labels, order — never describe new
  behavior.** No conditionals, no computed values. Test for any
  proposed field: could an unknown value degrade gracefully?
- **The agent side is the same anatomy.** A harness is a
  projection whose renderer is JSON and a shell: assembly (which
  capabilities), projection (which ops, with what shaping flags),
  flow (result references — item 3), budget (the wall-clock
  guard), eventually authz (policy checked against a harness,
  where cap-authz wants to attach). Harnesses are EPHEMERAL task
  state, unlike the persistent UI projection. Harnesses are
  discovered, then crystallized: a recurring session
  improvisation graduates from finding → runner feature → data.
  Do NOT pre-build a harness framework.

**PoC stages (each independently landable):**

1. `web/src/selection.svelte.ts`: the user's chosen capability
   ids, IndexedDB-persisted, defaulting to everything in the
   manifest (nobody's experience changes day one). The resolver
   consults it: deselected = never assembled. The catalog stays
   visible even to users who want nothing from it.
2. Store surface in the DataSources modal: checkbox list over
   manifest capabilities (id, purpose, hash, status). Proves the
   convergence claim — purpose written as a pitch to agents
   reads as a pitch to users. Toggling analysis off behaves as
   `forgetCapability` does today.
3. One probe-based section: convert the network overview's
   numbers (the cap-analysis consumer) from pull to probe.
   Acceptance: the section's code names OPS, not capabilities —
   if it still says `ensure(["analysis"])`, the direction didn't
   flip. Toggle analysis → the section degrades in place and
   authoring flows on the same screen never notice; starved is
   exerciseable with no network document loaded.
4. The UI manifest + gates: the host-side op → render | flow
   map; a caps-smoke assertion that UI manifest ops ⊆ manifest
   ops; the unmapped-ops diagnostic printed by the check.
   Validate the assertion by planting a divergence (an entry
   naming a nonexistent op) and watching it fail. e2e: extend
   `web/e2e/caps.spec.mjs` (store toggle → degrade → recover).

**Not in this unit:** persona presets (selection replaces them),
question-shaped preference abstractions (flat op lists until a
second analysis capability exists), any harness framework in
caps-run.

**First customers.** Item 5's day-summary screen is the first op
rendered through the probe path; item 4's shaping flags are the
first projection-level choice an agent makes in a harness.

## Standing queue (after this unit)

- **Registry search** as a derived artifact: embed purpose +
  op_docs + worked examples; ranking carries evidence (laws,
  provenance) so "known good" is part of relevance. Endpoint of
  this road: an agent that browses the catalog and assembles its
  own harness for the task at hand — the agent-side store.
- **caps-run harness as data** (assembly/projection/flow/budget,
  per item 6's anatomy) — when the same session improvisation
  recurs, crystallize it; until then, improvisations are
  findings, not framework.
- **Fuel metering** in the runner (wasmtime) once it may point at
  untrusted registry entries; the wall-clock worker guard is the
  stopgap.
- **Sync persistence** on blobs + pointers (see the persistence
  discussions: migrate-at-receive, pointer CAS, one writer per
  prefix).
- **cap-authz** as a capability (policy document, server-side
  assembly, fail-closed laws; the validator's scope check).

## Conventions for whoever picks this up

- `just check` is the gate; `just registry` re-publishes; `just
  caps` rebuilds modules + the browser manifest.
- New ops: dispatch test + TS bindings regenerate + registry
  op_docs + smoke assertion (undocumented ops fail the build).
- Real-data QA is mandatory for anything touching feeds or
  payload size: run the C-TRAN session before declaring done.
  Fixture-scale gates passed a 64 KiB output-truncation bug; the
  real feed caught it in one command.
