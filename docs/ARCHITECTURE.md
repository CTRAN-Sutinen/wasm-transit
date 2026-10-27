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
                live in transit-sync; the feed-side patch path
                (apply_diversions, report_all) lost its last caller
                with set_diversions (U6) — only report_all remains,
                behind the sync seam's feed-scoped validation.
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
                unversioned (a fresh import). AUTHORING lives here
                too (`edit.rs`): typed, atomic `NetworkEdit`s
                answer the NEXT document (never mutating the last);
                removals refuse while referenced. Two levels:
                entity upserts/removes, and GESTURES — one edit per
                thing a planner does, each carrying its coherence
                rule (timing aligned with timepoints, starts sorted
                and unique, layovers only between trips, ids minted
                and reported, the plan frame merged field-wise). Validation
                answers issues (gate: a block trip its schedule
                does not run is one) and WARNINGS that never gate
                (unassigned trips; a pick with no service in its
                first week).
transit-router  RAPTOR earliest-arrival planning over the network
                model's materialized day (patterns grouped by actual
                stop sequence — an edited trip rides its own
                pattern); GTFS-free since U5.
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
                A LIBRARY under cap-network — the extraction
                pattern: crate boundary first, capability wrapper
                second. The C-TRAN import regressions live in its
                own test suite (tests/import_regression_test).
transit-analysis Planner analysis over an authored network document:
                stats, service-type overview/detail, population
                coverage. A LIBRARY under the cap-analysis module;
                the planner's regression numbers live in its own
                test suite (tests/regression_test).
transit-dispatch The materialized day's views, model-native: patch_day
                (skip/shift over MATERIALIZED stop times — the day,
                never the calendar), edit validation against the
                NETWORK, THE board (stop_board: walking-group
                pooling for a stop with nothing of its own,
                realtime-first over the live document — rows
                ordered by when the bus actually leaves, each with
                its expected time, delay, and status), the day's
                blocks list, the service keys
                running on a date, and the AUTHORED block chain with
                the layover ripple (layover_remaining_secs goes
                negative when a connection is eaten). A LIBRARY
                shared by the cap-board, cap-runcut, and cap-calendar
                modules; the UI reads them here. Holds no GTFS
                layer (the justfile arch gate enforces it).
transit-rt      prost types from the official GTFS-rt proto, the
                OVERVIEW view (delays, vehicles) over them, and the
                PROGRESS join (one trip's predicted times; the
                ranked late view with the DELAYED-BUT-DARK flag) —
                LIBRARIES shared by cap-rt and any host that holds
                the bytes (and, for the join, the static feed) —
                and the BOUNDARY RESOLUTION (live_day): a snapshot
                plus the static feed plus the import's trip map →
                THE live document, the one place agency trip ids
                meet the model.
transit-live    THE live document: one rt moment in model vocabulary
                (delays per derived trip and stop, vehicles on
                derived trips, every status classified under the
                on-time window the document records — one window,
                never a threshold per view), the classification
                rule, and the status summary. Pure data; no GTFS
                layer (arch gate). Minted by cap-rt, read by
                cap-board — no host computes a delay.
transit-sync   THE sync document: EditRecord, FeedManifest, EditReport,
                the Diversion edit document (THE DISPATCHER'S OWN —
                it lives in the sync contract, not the GTFS codec),
                pure validation/merge logic (merge is a
                property-tested semilattice; submit transitions
                drafts with the host-passed clock). Shared by every
                host — cap-sync is the seam the Go server calls.
transit-plans  THE service-plan chain: resolve (which pick is in
                force), periods (each member's governing window),
                validate (chain invariants) over the plan set's
                metadata — DERIVED, never stored, deterministic and
                order-insensitive. Cross-document logic, the
                counterpart of transit-sync's cross-record merge;
                cap-plans is the seam.
transit-streets Street routing graph for detour authoring: GeoJSON in,
                snap (nearest street position) + shortest-path route
                out (one-way aware — TIGER `OneWay` and OSM `oneway`
                conventions; turn restrictions are a non-goal until a
                restriction table ships in the data). Pure reference
                geometry — cap-streets registers parsed graphs under
                their content hash, never mixed with feed state.
cap-sync        The sync capability: the converged state of an edit
                document pair — LWW merge + the submit transition,
                the protocol's one step (the op behind POST /edits).
                Stateless; validation is NOT here (load_edits
                answers it against the network in every model-side
                module — one question, one answer).
cap-plans       The plan-chain capability: which service plan
                governs a date — resolve / periods / validate over
                the plan set's metadata (transit-plans is the
                library). Pure ops, no registries: the host owns
                the plan set and pushes its metadata per call.
cap-author      The authoring capability: what does this network
                become after these edits? apply (typed edits,
                atomic) answers the next document under its hash,
                registered for the next change, with its issues
                and warnings. transit-network's edit module is the
                library; `created` names the entities a change made.
                Every host authors through it — the browser's
                planner included (web/src/planner.ts sends edits and
                adopts the answer; it never edits a document).

Every module is the raw rustc output with the same four flat
exports (no wasm-bindgen — do not reintroduce host glue).
```

## server/

Go + wazero host: scaffolding only (HTTP, manifest, proxy). It must
never make a domain decision; the host reads the feed's own
feed_info.txt version field (one boundary CSV field — I/O, not
domain). The one module it instantiates is cap-sync (the default
-wasm flag, web/pkg/cap_sync.wasm — rebuilt by `just caps`): the
op behind POST /edits. The manifest pins the feed hash and the
sync module's hash. (Reentrancy and the Go-host serialization
live in `docs/CONVENTIONS.md`.)

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

- `src/mapview.ts` is the map's view code — HOST code over model
  documents (the network document's stops and authored alignments,
  the directory's stop list, the effective day via cap-runcut's
  trip detail). Ruled a parity break: geometry is a model answer
  and moves into ops (`docs/PLAN.md`, the web cleanup, W4) —
  `web/` will only draw the GeoJSON it is handed.
- `src/capabilities.svelte.ts` is the bus resolver — the ONLY wasm
  loader (components never see the ABI): manifest
  validation, demand-driven load, byte verification against pinned
  hashes, per-capability degradation, and the IMMUTABLE document
  store (every version hash-chained by produced_by; `current` is a
  pointer; migrations move it forward, rollback moves it back;
  IndexedDB-persisted). CapabilityDemo.svelte is the visible story
  (assembly, hash handoff, degraded state, migrate/rollback) in the
  DataSources modal.
- `src/planner.ts` sends the planner's changes to cap-author as
  NetworkEdits (in order, each landing on the plan it was made
  against) and adopts the answered document; it also owns the
  screen stack.
- `src/state.svelte.ts` owns the documents (edit document, plan
  set — both IndexedDB-persisted, wholesale-replaced).
- MapView owns all map state (`window.__map`).

Frontend conventions and gotchas: `docs/CONVENTIONS.md`.

## The capability bus

The architecture: horizontal capability modules, one NEED per
module, assembled per user at WIRE time from a manifest —
capability bytes differ per user while the DOCUMENT and the
contracts stay identical everywhere. Every host instantiates the
modules it needs — the browser fetches them from the manifest,
the sync server hosts cap-sync. Where the remaining work stands
is `docs/CAPS-PLAN.md` (the status authority).

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
cap-gtfs        The feed-document capability: GTFS zip →
                content-hash feed registry, and the feed's OWN
                questions only — what it contains at a glance
                (load_feed's summary) and its identity (feed_info).
                The day views, the watchlist, the pickers, and the
                names join are NOT here: they answer over the NETWORK
                document (cap-board / cap-runcut / cap-calendar /
                cap-directory — one question, one answer on the bus).
                Parse + identity alone; the GTFS substrate itself
                retires in U8 (it folds into transit-codec). CHARTER
                CLOSED: parse, export, primitive boundary queries —
                nothing more goes "in GTFS."
cap-router      The router capability: RAPTOR earliest-arrival
                planning over the NETWORK document and the
                effective day — the model's materialized trips,
                patched with the edits in force (the same
                transit-dispatch composition every day view starts
                from), the RAPTOR graph cached per (network, edits,
                date). The compare facet is the diversion delta:
                the unpatched day, one materialization apart. A
                pure-function capability; it writes nothing.
cap-analysis    The analysis capability: planner numbers (stats,
                service types, coverage) over hash-keyed network and
                population-layer registries. The domain code lives
                in the shared transit-analysis library. Consumes
                network documents from cap-network (the registry
                edge the manifest carries).
cap-network     The network codec capability: load_feed (zip bytes →
                content-hash feed registry), import (the planner's
                bootstrap — the derived document registers itself
                and its hash is in the response), load_network (the
                same registration name every model-side module uses),
                validate (the authoring feedback loop: issues that
                gate export, warnings that never gate), export (the
                publish path — deterministic GTFS zip bytes under
                their content hash, base64 across the JSON seam), and
                trip_map (the import's trip-id map: derived
                (schedule, start) ↔ agency trip ids — the join rt
                data and the feed's own ids are keyed by; None for
                an authored network, the honest answer), and
                agency_trip_ids (the map shaped to a question: the
                agency ids behind given derived trips, in the order
                asked). The domain code lives in the shared
                transit-codec library.
cap-rt          The realtime capability: decode — a GTFS-rt protobuf
                payload (bulk-bytes op 0, the ephemeral bytes an
                agency serves every few seconds) into the overview
                view (delays per trip/stop, live vehicles), each
                snapshot registered under its content hash; and the
                SCHEDULE JOIN the dispatcher actually asks for:
                load_feed (bulk op 1, the SAME feed identity
                cap-gtfs returns — the manifest's rt → gtfs edge),
                trip_progress (one trip's scheduled/delay/predicted
                board; a stop the update skips carries the last
                stated delay, GTFS-rt's propagation rule), late_trips (ranked, with the DELAYED-BUT-
                DARK flag and vp_hash for agencies that serve the
                two feeds separately — C-TRAN does). Dark is judged
                per BLOCK: a delay propagated to a block's next trip
                while its bus runs the trip before is a located
                vehicle (position_trip_id names the trip it is on;
                vehicle_id rides beside the label). load_trip_map +
                live mint THE LIVE DOCUMENT (transit-live) from the
                trip-updates and/or vehicle-positions snapshots, with
                the summary a status panel reads (vehicles by
                status, the late list) — the browser's live views
                render it; none joins rt.
cap-board       The board capability — ONE QUESTION: when does
                service leave this stop? (at a stop, a station's
                pooled bays, or a personal saved set — the WATCHLIST
                document, v1/v2, migrated here: a personal
                multi-stop board is the same question parameterized
                by a personal document). load_network / load_edits
                (validated per record against the network, invalid
                records REPORTED never rejected) / load_live (cap-rt's
                live document, by hash) then stop_board (THE board:
                pooling, realtime-first order, row status — the
                module's answer, never the host's) / load_watchlist /
                migrate_watchlist / watchlist_board (a trip's FINAL
                visit is an arrival, never a row) over ONE
                MATERIALIZED DAY (materialize(net, date), patched
                with the in-force edits — no derived registry, no
                GTFS expansion, no effective cache; a day is computed
                per call and a day is cheap). The domain code lives
                in the shared transit-dispatch library; no GTFS
                layer (arch gate). The UI's departure board renders
                stop_board's answer; rows carry the derived trip's
                identity, so the click-through needs no agency join.
cap-runcut      The runcut capability — ONE QUESTION: what does
                this vehicle do today? The day's blocks LIST (the
                index facet: vehicles out, with that day's running
                trip counts; a block whose every trip is
                unrepresentable or skipped is not offered — the
                honest hole) and one block's chain (the detail
                facet: the AUTHORED runcut with the layover ripple,
                layover_remaining_secs going negative when a
                connection is eaten, and layover_health classifying
                it — overlap / tight / ok under the caller's
                tight_layover_secs) and one derived trip's EFFECTIVE
                stops on the date (the trip detail facet the map's
                click-through reads — the answer carries no geometry,
                the host's view code draws). Same document set as
                cap-board
                (load_network / load_edits + the day views); the
                domain code lives in the shared transit-dispatch
                library; no GTFS layer (arch gate). The UI's blocks
                panel and trip lines answer through this module.
                Replaces the dissolved cap-dispatch — a module named
                for a role is the monolith rebuilt sideways
                (docs/PLAN.md's criteria).
cap-calendar    The calendar capability — ONE QUESTION: which
                service keys run today? active_services over the
                EFFECTIVE day (a key whose every trip is skipped or
                unrepresentable is not running, however its calendar
                reads — the same honesty as the runcut view's empty
                hole). Same document set (load_network / load_edits
                plumbing); the domain code lives in the shared
                transit-dispatch library; no GTFS layer (arch gate).
cap-directory   The directory capability — ONE QUESTION: what
                exists in the network? The stop list (name-sorted,
                with the walking-group structure the station board
                keys on; an optional query narrows it the way a
                picker's typing does), the names join (stops_by_ids, moved from
                cap-gtfs — one question, one answer), the route list
                (the model's closed mode set), and one route's
                PATTERNS with their ordered stops — the model's
                directions, what the diversion editor anchors
                against (the GTFS direction select is gone; the
                pattern pick replaced it, and new edit records carry
                direction_id null — the documented both-directions
                case the model never matched on). The pickers are
                model questions, never feed reads; the UI's
                comboboxes and the map's name lookups read this
                module (the feed line reads cap-gtfs's load_feed
                summary + feed_info).
cap-streets    The street-graph capability: the detour editor's need,
                atomically. load_streets (bulk-bytes op 0) parses a
                street-network GeoJSON into the routing graph and
                registers it under its content hash (idempotent —
                the same bytes are the same graph); snap answers the
                nearest street position (the graph address a detour
                waypoint commits to); route answers the street
                geometry between two positions (shortest path,
                starting after `from` — detour legs chain — with
                its length along the graph; empty on disconnected
                parts — the caller falls back to a straight line). The domain code lives in the shared
                transit-streets library. Reference data the HOST
                owns, re-imported per
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
- One Request enum per capability is the whole surface (networks
  are minted only by cap-author's apply and cap-network's import —
  readers have no authoring ops by construction).
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
A session runs its steps in command-line order and may span
capabilities (`--cap` switches; each module verified and
instantiated once): documents cross between modules BY REFERENCE
(`"network":"$net.network"`), the way a host hands the same bytes to
each module, and `--as` names results so references survive
reordering. Sessions do not persist — a later session re-imports.
Registry records and the browser manifest are BOTH generated by
`cargo run -p cap-abi --bin gen-manifest` — one source of contracts,
two surfaces. `just test` gates the runner itself (the fixture feed
hash is stable, so registry/fixture drift fails the gate).
