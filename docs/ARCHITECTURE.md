# Architecture — the authoritative reference

This is the ONE authoritative architecture doc. The compact tree
and ownership rules live in the root `AGENTS.md`; per-crate,
server, web, and bus detail lives here and nowhere else. The
three architecture laws (the network is the core data model;
nobody reinvents the monolith inside a module; one NEED per
module, personas assemble at wire time) are spelled out in
`crates/AGENTS.md` — that file and this one are updated together.
Status — what is extracted, what is pending — lives in
`docs/CAPS-PLAN.md`, the authoritative status doc.

## Layering (bottom up)

```
transit-core    GTFS codec: model, zip/CSV parse, calendar, queries,
                diversion patching, and the WRITE direction —
                deterministic zip bytes (src/write.rs), so a produced
                feed has one identity everywhere. Pure, no I/O.
                The edit-document TYPES (Diversion, DetourPoint)
                live in transit-sync; this crate re-exports them for
                the dying feed-side patch path only (apply_diversions,
                report_all — they die with set_diversions).
transit-network The authored network model (planner authority):
                patterns, timing profiles, service types (the
                service identifier, linked to days by its calendar
                rule), pattern schedules (one pattern's trips under
                one service type), runcut. Trips are DERIVED
                (`materialize`) — trips are never stored. GTFS is a
                codec at the boundary, not the model; no GTFS types
                leak in here. Transfer points are authored flags on
                stops, composed over their WALKING GROUP (the stop +
                siblings sharing a parent, the parent, children —
                transfers happen across a street, not at one pin).
                Every document is a SERVICE PLAN (`PlanMeta`: id,
                name, effective_from) — agencies implement a few
                picks a year at defined dates; empty identity =
                unversioned (a fresh import).
transit-router  RAPTOR earliest-arrival planning.
transit-codec   The network codec, both directions: a network document
                expanded into the derived-feed view (the publish
                path) and a GTFS feed inferred into an authored
                document (the planner's bootstrap). The import is
                VALID BY CONSTRUCTION over spec-valid feeds (the law,
                property-tested in tests/import_prop_test) and every
                resolution it applies (dropped block trips, unusable
                transfer points, …) is REPORTED as an ImportNote —
                never silent. The import also infers each block
                layover from the observed end→start gap (exactly
                feasible — the model's LayoverInfeasible law holds
                with equality); the planner re-authors from there.
                A LIBRARY shared by cap-network and transit-ops's
                tests — the extraction pattern: crate boundary
                first, capability wrapper second, monolith deletion
                last.
transit-analysis Planner analysis over an authored network document:
                stats, service-type overview/detail, population
                coverage. A LIBRARY shared by transit-ops (parse-only
                coverage handle) and the cap-analysis module.
transit-dispatch The operator's DAY, model-native: patch_day (skip/
                shift over MATERIALIZED stop times — the day, never
                the calendar), edit validation against the NETWORK,
                the board and station board (walking-group pooling),
                the AUTHORED block chain with the layover ripple
                (layover_remaining_secs goes negative when a
                connection is eaten). A LIBRARY shared by the
                cap-dispatch module — the monolith's board and
                block-chain ops are deleted, the UI reads them here.
                Holds no GTFS layer (the justfile arch gate enforces
                it).
transit-rt      prost types from the official GTFS-rt proto, the
                OVERVIEW view (delays, vehicles) over them, and the
                PROGRESS join (one trip's predicted times; the
                ranked late view with the DELAYED-BUT-DARK flag) —
                LIBRARIES shared by cap-rt and any host that holds
                the bytes (and, for the join, the static feed).
transit-sync   THE sync document: EditRecord, FeedManifest, EditReport,
                the Diversion edit document (THE DISPATCHER'S OWN —
                it lives in the sync contract, not the GTFS codec),
                pure validation/merge logic. Shared by every host.
transit-streets Street routing graph for detour authoring: GeoJSON in,
                snap (nearest street position) + shortest-path route
                out (one-way aware — TIGER `OneWay` and OSM `oneway`
                conventions; turn restrictions are a non-goal until a
                restriction table ships in the data). Pure reference
                geometry — cap-streets registers parsed graphs under
                their content hash, never mixed with feed state.
transit-ops     dispatch(Request) -> Response: the host-agnostic seam
                + per-feed registry (feed_id keyed). The codec ops,
                analysis ops, rt decode, the dispatch reads
                (active_services / blocks / block_trips / feed_info)
                AND the day views (departures / block_trips) are
                DELETED — they live in cap-network, cap-analysis,
                cap-rt, cap-gtfs, and cap-dispatch over the shared
                libraries; the board and block chains read the bus.
                What remains: the parsed-feed op surface the map and
                the planner still need (stops, routes, route/stop
                sequences, geojson, planning — all date-scoped by
                the edit document via set_diversions, the last
                diversion consumer), the plans chain in src/plans.rs
                (plans_resolve / plans_validate / plans_periods —
                cross-document domain logic, the same standing as
                transit-sync's merge), and the parse-only coverage
                handle. The street ops are DELETED — cap-streets owns
                the detour editor's graph.
transit-wasm    THE artifact: flat import-free C ABI over dispatch.
                No wasm-bindgen — do not reintroduce host glue.
```

## server/

Go + wazero host: scaffolding only (HTTP, manifest, proxy). It must
never make a domain decision; the host reads the feed's own
feed_info.txt version field (one boundary CSV field — I/O, not
domain). (Reentrancy and the Go-host serialization live in
`docs/CONVENTIONS.md`.)

## web/

Svelte 5 runes + Vite, TypeScript end to end. ONE shell: a sidebar
(persona tabs + the active workflow) beside a PERSISTENT map canvas
that never remounts on a persona switch; persona-owned layers
DERIVE from app.persona. The map is A VIEW, not a fixture: data
screens hide it and the rail takes the full width
(`.map-side.hidden`; entity and service-type screens frame things
on it). Data sources (feed, rt, streets, population layer) is a
MODAL, not a floating panel. The Planner persona is a WORKSPACE of
screens with a back stack, ROOTED AT SERVICE PLANS (the chain: in
force / upcoming / superseded, fork = the service-change workflow);
below that the network overview (per-service-type analysis —
planners design types, not calendar dates) and drill-downs per
entity. There is no planner drawer.

File ownership:

- `src/wasm.ts` wraps the artifact (GtfsHandle + every op);
  components never see the ABI.
- `src/capabilities.svelte.ts` is the bus resolver — manifest
  validation, demand-driven load, byte verification against pinned
  hashes, per-capability degradation, and the IMMUTABLE document
  store (every version hash-chained by produced_by; `current` is a
  pointer; migrations move it forward, rollback moves it back;
  IndexedDB-persisted). CapabilityDemo.svelte is the visible story
  (assembly, hash handoff, degraded state, migrate/rollback) in the
  DataSources modal.
- `src/planner.ts` owns authoring mutations (clone → edit →
  replace → revalidate) and the screen stack.
- `src/state.svelte.ts` owns the documents (edit document, plan
  set — both IndexedDB-persisted, wholesale-replaced).
- MapView owns all map state (`window.__map`).

Frontend conventions and gotchas: `docs/CONVENTIONS.md`.

## The capability bus

The architecture: horizontal capability modules, one NEED per
module, assembled per user at WIRE time from a manifest —
capability bytes differ per user while the DOCUMENT and the
contracts stay identical everywhere. The monolith is legacy
surface being deleted: what remains of it and where each piece
goes is `docs/PLAN.md` (the plan); where the work stands is
`docs/CAPS-PLAN.md` (the status authority).

```
cap-abi         The seam crate: the flat C ABI as a macro (every
                capability module is the same four exports), the
                capability-manifest contract (speaks/migrates/
                consumes + agent context), the ASSEMBLY VALIDATOR
                (compose-time proof: closure, version coherence,
                migration-path reachability — total and property-
                tested; gen-manifest self-validizes before writing),
                and gen-manifest (the capability set as code; the
                manifest diff IS the reviewable assembly change).
                Also caps-smoke.mjs: Node ABI smoke + the law that
                every advertised op is documented (undocumented ops
                fail the build — agent context cannot drift).
cap-gtfs        The gtfs capability: GTFS zip → content-hash feed
                registry, departures/stops queries, the dispatch
                reads (active_services, blocks, block_trips,
                feed_info, stops_by_ids), station_departures (the
                walking-group station board — a station pin's bays
                pooled, rows tagged), and the watchlist document
                (v1/v2) with its v1→v2 migration — pure,
                deterministic, loss REPORTED (MigrationNotes, never
                silent; identity-preservation is a property law —
                migrations never touch the sync key space).
                CHARTER CLOSED: parse, export, primitive boundary
                queries — nothing more goes "in GTFS."
cap-router      The router capability: RAPTOR earliest-arrival
                planning. Today it plans over a feed document
                (cap-gtfs produces it) — a TRANSITIONAL exception
                to Law 1; U5 moves it onto the network document +
                the effective day. A pure-function capability; it
                writes nothing.
cap-analysis    The analysis capability: planner numbers (stats,
                service types, coverage) over hash-keyed network and
                population-layer registries. The domain code lives
                in the shared transit-analysis library. Consumes
                network documents from cap-network (the registry
                edge the manifest carries).
cap-network     The network codec capability: load_feed (zip bytes →
                content-hash feed registry), import (the planner's
                bootstrap — the derived document registers itself
                and its hash is in the response), load_document,
                validate (the authoring feedback loop), export (the
                publish path — deterministic GTFS zip bytes under
                their content hash, base64 across the JSON seam), and
                trip_map (the import's trip-id map: derived
                (schedule, start) ↔ agency trip ids — the join rt
                data and the feed's own ids are keyed by; None for
                an authored network, the honest answer). The domain
                code lives in the shared transit-codec library — the
                monolith keeps no copy.
cap-rt          The realtime capability: decode — a GTFS-rt protobuf
                payload (bulk-bytes op 0, the ephemeral bytes an
                agency serves every few seconds) into the overview
                view (delays per trip/stop, live vehicles), each
                snapshot registered under its content hash; and the
                SCHEDULE JOIN the dispatcher actually asks for:
                load_feed (bulk op 1, the SAME feed identity
                cap-gtfs returns — the manifest's rt → gtfs edge),
                trip_progress (one trip's scheduled/delay/predicted
                board), late_trips (ranked, with the DELAYED-BUT-
                DARK flag and vp_hash for agencies that serve the
                two feeds separately — C-TRAN does).
cap-dispatch    The dispatch capability: the operator view over the
                NETWORK document (load_network — the same identity
                cap-network's import produces) plus the OPERATOR
                EDIT document (load_edits — the dispatcher's own,
                validated per record against the network, invalid
                records REPORTED never rejected). departures /
                station_departures / block_trips answer over ONE
                MATERIALIZED DAY (materialize(net, date), patched
                with the in-force edits — no derived registry, no
                GTFS expansion, no effective cache; a day is
                computed per call and a day is cheap). The chain is
                the AUTHORED runcut with the layover ripple. The
                domain code lives in the shared transit-dispatch
                library; the module holds no GTFS layer (arch
                gate). Model-native responses. The UI's departure
                board and block chains answer through this module —
                the monolith's day-view ops are deleted, and the
                rt-delay / click-through join goes through
                cap-network's trip_map.
cap-streets    The street-graph capability: the detour editor's need,
                atomically. load_streets (bulk-bytes op 0) parses a
                street-network GeoJSON into the routing graph and
                registers it under its content hash (idempotent —
                the same bytes are the same graph); snap answers the
                nearest street position (the graph address a detour
                waypoint commits to); route answers the street
                geometry between two positions (shortest path; empty
                on disconnected parts — the caller falls back to a
                straight line). The domain code lives in the shared
                transit-streets library — the monolith keeps no
                copy. Reference data the HOST owns, re-imported per
                session; the module writes nothing.
```

### Bus rules (load-bearing)

- Capabilities compose through DOCUMENTS addressed by content hash,
  never module-to-module — the host loads the same bytes into each
  module and ASSERTS the returned hashes agree (`assertSameDocument`
  is the law).
- The MODULE is the hash authority — hosts never mint document
  identity (JS JSON.stringify differs from the module's canonical
  serde bytes in float formatting; query with the hash the module
  returned).
- One Request enum per capability is the whole surface (only
  planners write networks — readers have no authoring ops by
  construction).
- `#[no_mangle]` ABI exports collide if one binary links two cap
  crates (tests assert against the hash formula instead); cap
  crates need `crate-type = ["cdylib", "rlib"]` for native tests to
  link.
- Do not let a capability grow a private document format — shared
  data crosses modules as kernel documents addressed by content hash
  only.

### The registry and the runner

The registry (`registry/`, published by `just registry`): immutable
blobs + one evidence record per capability (contracts, agent
context, which gates ran at which commit) + a pointer. It is the
artifact agents and humans pull SANDBOXED COMPUTE from —
`tools/caps-run.mjs` verifies blob bytes against the pinned hash and
runs ops behind a worker-termination wall-clock guard (`--timeout`).
Registry records and the browser manifest are BOTH generated by
`cargo run -p cap-abi --bin gen-manifest` — one source of contracts,
two surfaces. `just test` gates the runner itself (the fixture feed
hash is stable, so registry/fixture drift fails the gate).
