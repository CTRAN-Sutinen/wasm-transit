//! gen-manifest: assemble the capability manifest after the wasm
//! builds (`just caps`) and PUBLISH the registry (`just registry`).
//! Reads each capability module, pins it by content hash, serves it
//! under that hash, and writes both surfaces hosts and agents read:
//!
//! * `web/public/caps/` — the browser's manifest + hash-addressed
//!   module files (the assembly the resolver loads);
//! * `registry/` — the REGISTRY: immutable blobs, one evidence record
//!   per capability (contracts + agent context + which gates ran when
//!   these exact bytes were published), and a registry pointer. An
//!   agent (or a human dogfooding `caps run`) pulls compute from
//!   here and verifies the bytes against the record.
//!
//! Before writing, the manifest is run through the compose-time
//! validator (cap_abi::validate): a manifest this crate assembles
//! must be provably coherent, and the generation is the first
//! consumer of that proof.

use std::fs;
use std::path::Path;
use std::process::Command;

use cap_abi::manifest::{CapManifest, Capability, MigrationSpec, OpDoc, OpExample};
use cap_abi::validate::validate_assembly;
use sha2::{Digest as _, Sha256};

/// The gates `just registry` runs before publishing. The recipe is
/// the authority; the record is the honest report of it.
const PUBLISH_GATES: &[&str] = &[
    "cargo test (workspace, includes the capability law suites)",
    "node crates/cap-abi/caps-smoke.mjs (ABI + composition + context)",
];

/// The document-kernel version this build speaks.
const KERNEL: u32 = 1;

/// The capability set this deployment assembles, mirroring the crates:
/// cap-gtfs produces feed and watchlist documents (and migrates the
/// watchlist); cap-router consumes feeds. When a crate's Request enum
/// changes, this changes — and the manifest diff is the reviewable
/// assembly change.
struct ModuleSpec {
    id: &'static str,
    file: &'static str,
    ops: &'static [&'static str],
    consumes: &'static [(&'static str, &'static str)],
    speaks: &'static [(&'static str, &'static [u32])],
    migrates: &'static [(&'static str, &'static [(u32, u32)])],
    /// Agent context: what this capability is FOR.
    purpose: &'static str,
    /// Per-op agent context, with worked examples (the feed hash is
    /// the REAL hash of the fixture this manifest ships, injected at
    /// generation time so examples are always current).
    op_docs: &'static [(&'static str, &'static str)],
}

const MODULES: &[ModuleSpec] = &[
    ModuleSpec {
        id: "gtfs",
        file: "cap_gtfs.wasm",
        ops: &["load_feed", "feed_info"],
        consumes: &[],
        speaks: &[("feed", &[1])],
        migrates: &[],
        purpose: "The FEED document: parse a GTFS static zip into a content-hash-keyed feed registry and answer the feed's own questions — what it contains at a glance (load_feed returns the summary) and its identity (feed_info.txt: version, publisher). The day views and the directory questions (pickers, names join) are NOT here: they answer over the NETWORK document (the board, runcut, calendar, and directory capabilities — one question, one answer on the bus). It has no ops that author anything.",
        op_docs: &[
            ("load_feed", "Register a GTFS zip under its content hash (idempotent). The response carries the feed's summary — stops, routes, trips, stop_times — the at-a-glance read the feed line shows. Bulk-bytes op 0; pass the raw zip to transit_invoke_bytes."),
            ("feed_info", "The feed's own metadata (version, publisher), when the feed provides feed_info.txt — the identity question: which feed is this?"),
        ],
    },
    ModuleSpec {
        id: "router",
        file: "cap_router.wasm",
        ops: &["load_network", "load_edits", "plan", "plan_compare", "itinerary_paths"],
        consumes: &[("network", "network")],
        speaks: &[("network", &[1])],
        migrates: &[],
        purpose: "Earliest-arrival trip planning (RAPTOR) over the NETWORK document and the effective day — the model's materialized trips, patched with the edits in force (the same transit-dispatch composition every day view starts from). The diversion delta is the compare facet: the unpatched day, one materialization apart. Assemble for users who need directions, not just departure times. Documents are hash-addressed; writes nothing.",
        op_docs: &[
            ("load_network", "Register the network document under its content hash (the same identity the network capability's import produces and every day view consumes; idempotent). Kernel bulk op 256 takes the bare document's JSON bytes on transit_invoke_bytes — the same answer, no request envelope; hosts send real-size networks this way."),
            ("load_edits", "Register the operator edit document against a loaded network, validated per record against it; the answer carries the reports and the document's hash. Wholesale replacement, like the host-owned store."),
            ("plan", "Earliest-arrival itinerary from stop to stop at a departure time over the materialized day (edits applied when edits_hash names a registered document); null itinerary = unreachable within max_legs. The legs speak model vocabulary: the derived trip identity is (pattern_trips, start_secs)."),
            ("plan_compare", "The diversion delta: the same plan over the patched day (active) and the unpatched materialization (baseline, null when no edit is in force on the date or no edits_hash is given), with the rider's notice when active is null — the arrival the diversion cost, or no trip within max_legs. One op whether or not edits exist."),
            ("itinerary_paths", "Where an itinerary runs, one path per leg ({kind: transit | walk, path: [{lat, lon}]}) over the same effective day it was planned on (pass the same edits_hash): a transit leg's RIDDEN segment — its trip's authored alignment sliced between the board and alight stops while the trip follows its pattern, its patched stops when diverted, a straight line as the last resort — and a walk through the stops it passes. Send the itinerary plan answered. The map draws the paths; it computes none."),
        ],
    },
    ModuleSpec {
        id: "network",
        file: "cap_network.wasm",
        ops: &["load_feed", "import", "load_network", "validate", "export", "trip_map", "agency_trip_ids"],
        consumes: &[],
        speaks: &[("feed", &[1]), ("network", &[1]), ("trip_map", &[1])],
        migrates: &[],
        purpose: "The network codec: infer an authored network document from a GTFS feed (the planner's bootstrap — patterns, service types, blocks, reported resolutions) and export one back to GTFS zip bytes (the publish path). The second capability extracted from the monolith — the codec lives in the shared transit-codec library, and the monolith keeps no copy. Assemble for planners: it is how a planner's session starts and how it publishes.",
        op_docs: &[
            ("load_feed", "Register a GTFS zip under its content hash (idempotent). Bulk-bytes op 0; pass the raw zip to transit_invoke_bytes."),
            ("import", "The planner's bootstrap: infer an authored network document from a loaded feed. Deterministic; every resolution the inference applied is reported in the notes — never silent. The inferred document registers itself, so its hash answers validate/export immediately."),
            ("load_network", "Register an authored network document (the planner's JSON document) under its content hash; idempotent. The same registration every model-side capability names load_network — one op file feeds them all. Kernel bulk op 256 takes the bare document's JSON bytes on transit_invoke_bytes — the same answer, no request envelope; hosts send real-size networks this way."),
            ("validate", "Every issue of a registered document — the authoring UI's feedback loop, the whole list, never thrown. issues gate (export refuses an invalid document; a block trip its pattern trips do not run is one — the runcut and the publish path would disagree). warnings never gate: they name what validity lets through — trips no block carries (unassigned_trips, once the document has a runcut) and a versioned plan with no service in its first week (no_service_in_plan_period: the calendars a fork inherited end before its pick date)."),
            ("export", "The publish path: materialize a registered document into a GTFS zip (deterministic bytes) and return it base64-encoded under the content hash of those bytes — the same identity any capability returns for the same bytes. Invalid documents are refused."),
            ("trip_map", "The import's trip-id map for a network: every derived trip ((pattern_trips, start_secs) — the model's identity) joined to the agency trip ids it came from. This is the join rt data and the source feed's ops are keyed by — rt feeds speak the AGENCY's ids, the model mints its own. None for a network that was not inferred from a feed (an authored document has no agency ids)."),
            ("agency_trip_ids", "The trip map SHAPED to a question: the agency trip ids behind specific derived trips ({pattern_trips, start_secs}), one list per trip in the order asked — empty for a trip the import never saw. What the radio calls a block's trips, without shipping the whole map (539 KiB on C-TRAN). Errors for a network with no map (an authored document has no agency ids)."),
        ],
    },
    ModuleSpec {
        id: "rt",
        file: "cap_rt.wasm",
        ops: &["decode", "load_feed", "trip_progress", "late_trips", "load_trip_map", "live"],
        consumes: &[("feed", "gtfs")],
        // The gtfs-rt payload: an ephemeral document (polled every
        // few seconds), versioned by the protobuf schema it speaks;
        // the static feed it joins against is the SAME document gtfs
        // serves (one zip, one identity).
        // THE LIVE DOCUMENT is minted here (model vocabulary, through
        // the network capability's trip map) and read by the board —
        // optional edges both: neither module needs the other to
        // answer its own questions, so they are spoken, not consumed.
        speaks: &[("gtfs_rt", &[1]), ("feed", &[1]), ("trip_map", &[1]), ("live", &[1])],
        migrates: &[],
        purpose: "Realtime decoding and the schedule join: turn a GTFS-rt protobuf payload (trip updates, vehicle positions) into the flat view hosts show, and answer the dispatcher's joined questions — one trip's predicted times, the ranked what's-late-now view with the delayed-but-dark flag. The third capability extracted from the monolith; the domain code lives in the shared transit-rt library. It mints THE LIVE DOCUMENT: the snapshot resolved through the import's trip map into model vocabulary (delays per derived trip and stop, vehicles on derived trips, statuses under the caller's on-time window) — the one place agency trip ids meet the model. Assemble for riders (live boards) and dispatchers (the operation view) alike.",
        op_docs: &[
            ("decode", "Decode a GTFS-rt protobuf payload into the overview (entity counts, trip updates with per-stop delays, vehicle positions), and register the snapshot under its content hash — idempotent, the same bytes are the same snapshot — so trip_progress/late_trips can address it. Bulk-bytes op 0. Garbage bytes are an error outcome, never a trap."),
            ("load_feed", "Register the STATIC feed the schedule joins come from — the same GTFS zip bytes gtfs loads, the same content-hash identity (assert equality at the host). Bulk-bytes op 1."),
            ("trip_progress", "One trip's joined board: every scheduled stop with the live delay and the PREDICTED time (schedule + delay) — the dispatcher's most-asked question in one op. A stop the update does not mention carries the last stated delay upstream of it (the GTFS-rt propagation rule); stops before the first update stay null. A trip the snapshot does not mention answers from the schedule alone; a trip the feed does not carry answers null."),
            ("load_trip_map", "Register the import's trip-id map (the network capability's trip_map answer) under its canonical content hash — the join live resolves agency trip ids through."),
            ("live", "THE LIVE DOCUMENT: one rt moment resolved into model vocabulary. rt_hash (trip updates) and/or vp_hash (vehicle positions — agencies often serve them separately) resolve through the registered trip map into delays per derived trip ((pattern_trips, start_secs)) and stop (propagated downstream, with the predicted departure), vehicles on their derived trips, and statuses (early / on_time / late / very_late / no_prediction) classified under `window` — the caller's on-time policy, default 60 s late on time, 300 s very late — which the document records so every reader classifies the same way. Trip and vehicle lateness is the trip's worst stated delay. Unresolvable rt trips are counted (unmatched_trips), never silently dropped. The response also carries the summary a status panel reads: vehicles by status and the late list worst first (late_limit, default 15) — late BUSES when positions exist, late trips (predictions) when they do not. Register the document in the board with load_live."),
            ("late_trips", "The ranked what's-late-now view: every trip whose worst stop delay reaches min_delay_secs, worst first, each carrying the located vehicle (vehicle_id, label, position) — found on the trip itself or, failing that, on another trip of its block (position_trip_id names the trip the bus is actually running: a delay propagated to a block's next trip is a late bus, not a dark one) — and the DELAYED-BUT-DARK flag (has_position false) when no vehicle of the block is visible. vp_hash names the VehiclePositions snapshot (agencies serve it as a SEPARATE feed — C-TRAN does); omit it for combined feeds."),
        ],
    },
    ModuleSpec {
        id: "analysis",
        file: "cap_analysis.wasm",
        ops: &["load_network", "load_population_layer", "network_stats", "service_types_overview", "service_type_detail", "service_type_compare", "stop_gaps", "hourly_trips", "schedule_texture", "route_overlap", "timed_transfers", "type_coverage", "unserved_points"],
        // The network document is produced by the network capability —
        // the extraction this module's own docs once called "future".
        consumes: &[("network", "network")],
        speaks: &[("network", &[1])],
        migrates: &[],
        purpose: "Planner analysis over an authored network document: network/route/stop stats, per-service-type overview and detail, population coverage. The first capability extracted from the monolith — the domain code lives in the shared transit-analysis library. Assemble for planners; riders never download it. Documents are hash-addressed (load once, query by hash — never per-op document serialization).",
        op_docs: &[
            ("load_network", "Register an authored network document (the planner's JSON document, produced or derived by the network capability) under its content hash; idempotent. Kernel bulk op 256 takes the bare document's JSON bytes on transit_invoke_bytes — the same answer, no request envelope; hosts send real-size networks this way."),
            ("load_population_layer", "Register a GeoJSON population layer under its content hash. Bulk-bytes op 0; pop_field optional (common candidates tried). The answer is the import handle — points, the picked pop_field, total population — the layer's owner never parses a second copy."),
            ("network_stats", "The network's numbers on one service date: trips, spans, revenue hours AND revenue miles, transfer points, per-route and per-stop activity, headways. Each route row carries its own revenue_hours and revenue_miles (the per-route cost basis — every cost model is per hour AND per mile; average operating speed follows from the two) and its headway as the MEDIAN gap between departures at its busiest stop (headway_stop says which) — arrivals never count. Every route of the document is a row, sorted by id: a route with no service on the date answers trips 0 and null spans (inactive routes are analysis too — filter on trips). include_routes/include_stops/include_service_types (default true) shape the response: false empties the corresponding lists — the day-shape question answers in under a KiB."),
            ("service_types_overview", "Every service type's numbers, each on its representative day — the design axis, no calendar dates in sight."),
            ("service_type_detail", "One service type expanded on its representative day: routes, stops, pattern trips, totals."),
            ("service_type_compare", "What a change does: the proposal (network_hash) against its baseline (base_hash) under one service type, each on its own representative day — both sides' trips, revenue hours and miles, and routes, then only the routes whose numbers changed (trips, span, typical headway, revenue hours and miles, patterns, stops), each with its baseline and proposal row (null on the side that does not run it), and the count of unchanged routes. Both documents must be loaded."),
            ("stop_gaps", "One stop's day folded into headway gaps: departures, the max gap and the departure it follows, and the per-hour profile (departures, max and median gap starting in the hour — a gap belongs to the hour it starts in). Gaps run between consecutive DISTINCT departure times — two routes leaving together is no gap at all — and final-stop visits are arrivals, never departures. The service-quality read of \\\"how long until the next bus?\\\". An unknown stop answers zero/null/empty, never an error."),
            ("hourly_trips", "The day's service profile: trips per hour bucketed by trip start (the service-day hour, 0..47 — after-midnight starts are legitimate), each hour row carrying every running service type's count, sorted by type id. Only hours with trips are rows — where service is thick, where it tapers. The planner's texture question (QAF-4), answered from the materialized day, not a jq fold over the document."),
            ("schedule_texture", "One service type's schedule texture on its representative day: every pattern trips entry that runs under the type, one row per schedule (sorted by id), each with headway uniformity (min/median/max gap between its starts — the median with the same four-start floor as every headway here) and clockface alignment (starts on the quarter-hour grid, start % 900 == 0). \\\"Is this headway even, and is it on a grid?\\\" Null for an unknown type (QAF-19 — lookups answer empty, never error)."),
            ("route_overlap", "The corridor read: every pair of routes that shares at least one stop on the date, sorted by (a, b) — shared stops, shared SEGMENTS (consecutive stop pairs both run, in the same order — the corridor proper), and the shared stop where the pair's departures stack highest (stack_stop, with the combined count; ties by lowest stop id). Where the Vine Green and Red run together, answered instead of inferred."),
            ("timed_transfers", "The hub's timed-transfer view: every arrival of one route followed by a departure of another within the connection window — the caller's ({min_secs, max_secs}, for a hub with widely separated bays), else the transfer-point convention (60–900 s: a rider needs a minute to move, and beyond 15 minutes it is not a timed transfer) — pooled over the stop's walking group (bays named: arrival_stop, departure_stop), sorted by arrival then routes. Who can make what at this hub, not just pooled departures. An unknown stop answers empty, never an error."),
            ("type_coverage", "Population coverage per service type against a hash-addressed layer: what share of the population each type serves, and serves frequently. Frequency is the whole-day average headway unless `window` ({start_secs, end_secs}) is given — then a stop is frequent when the worst wait anywhere in the window (edges included, departures only, every route pooled) fits frequent_secs: the planner's \"every 15 minutes, 07:00–19:00\". A midday or peak improvement is visible only through a window."),
            ("unserved_points", "The unserved WHERE of coverage: every layer point beyond radius_m of every stop with service on the date (the same served set type_coverage measures against), each with the nearest served stop and its distance — which part, and where, not just the share. Sorted by population descending, so the biggest unserved communities read first. Together with the coverage share they are complements: served + unserved conserves the layer total."),
        ],
    },
    ModuleSpec {
        id: "board",
        file: "cap_board.wasm",
        ops: &["load_network", "load_edits", "load_live", "stop_board", "load_watchlist", "migrate_watchlist", "watchlist_board"],
        // The network document is produced by the network capability;
        // the operator edit document is host-owned (wholesale-replaced
        // — the sync contract); the watchlist is the PERSONAL document
        // this capability owns (versioned, migrated here).
        consumes: &[("network", "network")],
        speaks: &[("network", &[1]), ("operator_edits", &[1]), ("watchlist", &[1, 2]), ("live", &[1])],
        migrates: &[("watchlist", &[(1, 2)])],
        purpose: "The board: when does service leave this stop — at a stop (a station pin or bay with nothing of its own answers its pooled walking group), realtime-first over the live document the rt capability mints, or for a personal saved set (the watchlist document, the same question parameterized by a personal document). Every answer is the materialized day over the NETWORK document, patched with the in-force edits. Model-native end to end; the domain code lives in the shared transit-dispatch library. Assemble for anyone who reads a departure board.",
        op_docs: &[
            ("load_network", "Register the authored NETWORK document under its content hash (idempotent) — the same document identity the network capability's import produces and analysis consumes. Assert equality at the host: one document, one truth. Kernel bulk op 256 takes the bare document's JSON bytes on transit_invoke_bytes — the same answer, no request envelope; hosts send real-size networks this way."),
            ("load_edits", "Register the OPERATOR EDIT document against a loaded network, validated per record AGAINST THE NETWORK (model vocabulary — nothing is expanded to answer it). The response carries the reports (invalid records are REPORTED, never rejected — the host owns the document) and the document's content hash, which the day views address. Wholesale replacement: push the whole document on every change."),
            ("load_live", "Register a LIVE document (the rt capability's live answer) under its canonical content hash — the same identity rt answered; assert equality at the host."),
            ("stop_board", "THE board at one stop on a service date: the network's materialized day with the in-force edits (edits_hash), REALTIME-FIRST when live_hash names a registered live document — rows ordered by when the bus actually leaves, a late bus scheduled before from_secs that has not left is still on the board, an early one that already left is not, and each row carries expected_secs, delay_secs (against the board's own patched time), and status under the live document's window (no_prediction without live data). A trip's final visit is an arrival, never a row. A stop with nothing of its own that belongs to a walking group (a station pin; a bay whose service is gone) answers the group's board with pooled: true, each row naming its bay; a plain stop never pools — a diverted-away stop shows its emptiness."),
            ("load_watchlist", "Register a watchlist document (the personal saved-set, version-tagged JSON) under its content hash."),
            ("migrate_watchlist", "Migrate a version-1 watchlist to version 2: label = note, colors dropped and reported. Pure and deterministic; the caller keeps the old bytes — migrations never destroy."),
            ("watchlist_board", "The board for a watchlist: each watched stop's departures on a service date, labelled per the document's version — the same question the per-stop board answers, parameterized by the personal document. Edits apply in force like every board."),
        ],
    },
    ModuleSpec {
        id: "runcut",
        file: "cap_runcut.wasm",
        ops: &["load_network", "load_edits", "blocks", "fleet_summary", "block_trips", "trip_stops", "trip_path"],
        // The network document is produced by the network capability;
        // the operator edit document is host-owned (wholesale-replaced
        // — the sync contract).
        consumes: &[("network", "network")],
        speaks: &[("network", &[1]), ("operator_edits", &[1])],
        migrates: &[],
        purpose: "The runcut: what does this vehicle do today — the day's blocks list (which vehicles are out, with that day's running trip counts) and one block's authored chain with the layover ripple (a diverted trip ending later eats into the vehicle's next layover). Blocks and layovers are model state GTFS never had. The materialized day over the NETWORK document, patched with the in-force edits; the domain code lives in the shared transit-dispatch library.",
        op_docs: &[
            ("load_network", "Register the authored NETWORK document under its content hash (idempotent) — the same document identity the network capability's import produces and analysis consumes. Assert equality at the host: one document, one truth. Kernel bulk op 256 takes the bare document's JSON bytes on transit_invoke_bytes — the same answer, no request envelope; hosts send real-size networks this way."),
            ("load_edits", "Register the OPERATOR EDIT document against a loaded network, validated per record AGAINST THE NETWORK (model vocabulary — nothing is expanded to answer it). The response carries the reports (invalid records are REPORTED, never rejected — the host owns the document) and the document's content hash, which the day views address. Wholesale replacement: push the whole document on every change."),
            ("blocks", "The day's blocks: every authored block with at least one trip running on the date, with that day's running trip counts — a block whose every trip is off is a vehicle still in the garage, not offered. With edits in force, skipped trips leave the count (and a block whose every trip is skipped leaves the list)."),
            ("fleet_summary", "The day's fleet in ONE answer: running blocks, the peak concurrent vehicles and the first time the peak is reached, and the platform / revenue / layover hours — the fold that used to take one block_trips call per block plus host-side concurrency glue. A vehicle is out from its block's first running start to its last running arrival (half-open spans: a bus arriving as another departs is one vehicle, not two). Platform = the fleet's out hours, revenue = the trips' own runtimes, layover = the rest; the peak is the dispatcher's fleet-need number."),
            ("block_trips", "One block's chain on a service date: the AUTHORED runcut (the model's blocks carry layovers GTFS never had), materialized for the day, with the in-force edits applied — a diverted trip ending later eats into the vehicle's next layover, and layover_remaining_secs goes negative when the connection is eaten. Each row's layover_health classifies the connection: overlap (negative — the next trip starts late), tight (under tight_layover_secs, default 300), ok. Trips of the chain that do not run on the date stay in view, marked. The answer carries the block_id it is for (many block_trips results group by the answer, never by request order). Unknown blocks answer empty, never error."),
            ("trip_stops", "One derived trip's stops on a service date — the day's trip detail, EFFECTIVE (a diverted trip answers its patched sequence). The trip's line is trip_path. A trip that does not run answers empty, never an error."),
            ("trip_path", "One derived trip's LINE on a service date ([{lat, lon}]), over the effective day: its pattern's authored alignment while the trip follows the pattern, its (patched) stops joined in order when it does not — a diverted trip draws its diversion. A trip that does not run answers an empty path, never an error."),
        ],
    },
    ModuleSpec {
        id: "calendar",
        file: "cap_calendar.wasm",
        ops: &["load_network", "load_edits", "active_services", "service_dates"],
        // The network document is produced by the network capability;
        // the operator edit document is host-owned (wholesale-replaced
        // — the sync contract).
        consumes: &[("network", "network")],
        speaks: &[("network", &[1]), ("operator_edits", &[1])],
        migrates: &[],
        purpose: "The calendar: which service keys run on a date — often several at once (a regular pattern plus an exception-added service) — and its reverse, the dates one service type runs on. Both answers come from the EFFECTIVE day, not the calendar rules alone: a key whose every trip is skipped by edits, or was never representable in the model, is not running however its calendar reads. The materialized day over the NETWORK document; the domain code lives in the shared transit-dispatch library.",
        op_docs: &[
            ("load_network", "Register the authored NETWORK document under its content hash (idempotent) — the same document identity the network capability's import produces and analysis consumes. Assert equality at the host: one document, one truth. Kernel bulk op 256 takes the bare document's JSON bytes on transit_invoke_bytes — the same answer, no request envelope; hosts send real-size networks this way."),
            ("load_edits", "Register the OPERATOR EDIT document against a loaded network, validated per record AGAINST THE NETWORK (model vocabulary — nothing is expanded to answer it). The response carries the reports (invalid records are REPORTED, never rejected — the host owns the document) and the document's content hash, which the day views address. Wholesale replacement: push the whole document on every change."),
            ("active_services", "The service keys running on a date: the distinct service types the effective day's trips run under, sorted. Departures-only when edits_hash is omitted. The day's first question — what is running today?"),
            ("service_dates", "The dates one service type runs on within an inclusive YYYY-MM-DD window — the same question active_services answers in the other direction (\"when does 1-WKDY/31 run?\"). The effective day decides each date (edits applied when edits_hash names a registered document): a type whose every trip is skipped is not running, however its calendar reads. An unknown type or an inverted window answers empty, never an error."),
        ],
    },
    ModuleSpec {
        id: "directory",
        file: "cap_directory.wasm",
        ops: &["load_network", "stops", "stops_by_ids", "routes", "route_stop_sequence", "focus", "entities", "route_detail", "stop_detail", "timetable"],
        // The network document is produced by the network capability.
        consumes: &[("network", "network")],
        speaks: &[("network", &[1])],
        migrates: &[],
        purpose: "The directory: what exists in the network — the stops (with their walking-group structure), the routes, one route's patterns with their ordered stops (list to detail). The pickers and the names join read here; they are MODEL questions, never feed reads. A route runs in directions, and each direction's stop sequences are its patterns (bay and short-turn variants), each with its own ordered stops — what a diversion editor anchors against. The planner's lists ask here too: rows of any kind narrowed by a filter, a route's composition with each pattern trips' trip count and its patterns by direction, a stop's serving routes, where an entity is on the map, and the timetable — one route, one direction, one service type, across the direction's patterns.",
        op_docs: &[
            ("load_network", "Register the authored NETWORK document under its content hash (idempotent) — the same document identity the network capability's import produces and analysis consumes. Assert equality at the host: one document, one truth. Kernel bulk op 256 takes the bare document's JSON bytes on transit_invoke_bytes — the same answer, no request envelope; hosts send real-size networks this way."),
            ("stops", "The stop list, name-sorted (the pickers' shape): every stop with its coords, its parent (the flag the station board keys off), and its transfer flag. query narrows it the way a picker's typing does — every whitespace-separated term must appear in the name or the stop id, case-insensitive (\"fisher landing\" finds Fisher's Landing and its bays). limit truncates after the sort; omit it for the whole list — a silently truncated directory would be a lie."),
            ("stops_by_ids", "Stops by id, in the request's order — the names join hosts otherwise do by hand when a view (like rt's) carries ids a human reads as names. Unknown ids are omitted."),
            ("routes", "The route list, name-sorted: every route with its mode (the model's closed set, not GTFS's numeric codes) and its authored color when there is one."),
            ("route_stop_sequence", "One route's patterns with their ordered stops (route_detail groups them by direction). An unknown route answers empty, never an error."),
            ("focus", "Where one entity is on the map ({kind: stop | route | pattern | pattern_trips | service_type | block, id}): its lines (each pattern's authored alignment, else its stops joined) and the stops it serves. A stop is its point; pattern trips their pattern; a route, a service type, or a block every pattern it runs. Null for an unknown id."),
            ("entities", "Every entity of one kind ({kind: stop | route | pattern | pattern_trips | service_type | block}), in id order, narrowed by an optional query the way a filter box's typing does (every whitespace-separated term in the name or the id, case-insensitive). Each row: id, name, context (a pattern's route; pattern trips' pattern · service type), transfer (stops), trips (pattern trips' materialized trip count; a block's chained trips)."),
            ("route_detail", "One route's composition: its patterns, the stops they visit (first-visit order), its pattern trips grouped by service type, each with the trips it runs (the same expansion the materialized day runs), and its patterns grouped by direction (the route's named directions in id order, then any patterns in none). Null for an unknown route."),
            ("timetable", "The planner's timetable: one route, one `direction` (omitted or null: its patterns in no direction), one `service_type` — every trip of the direction's patterns (bay and short-turn variants together), one row per trip by start with its identity (pattern_trips, start_secs), pattern and block, and its departure at each column (null where its pattern does not stop); the columns are every stop those patterns visit, in route order, flagged when a timepoint; the legend lists the patterns most trips first, marking headway-band ones. Rows are the materialized day's trips. With `compare_hash` (a loaded baseline) each trip is marked `added` when the baseline does not run it, and `removed` lists the baseline's trips this one no longer runs (a retimed trip is removed at its old start and added at its new — its identity is pattern_trips + start). Null for an unknown route, direction or service type."),
            ("stop_detail", "One stop's composition: the routes whose patterns visit it (id and name rows, id order). Null for an unknown stop."),
        ],
    },
    ModuleSpec {
        id: "streets",
        file: "cap_streets.wasm",
        ops: &["load_streets", "snap", "route"],
        // The street graph is reference data the HOST owns (an OSM
        // extract or open-data layer as GeoJSON lines, re-imported
        // per session); the module registers the bytes under their
        // content hash and every query addresses that hash.
        consumes: &[],
        speaks: &[("street_graph", &[1])],
        migrates: &[],
        purpose: "The detour editor's street graph: register a street-network GeoJSON (an OSM extract or an open-data layer) under its content hash and answer the detour editor's queries — snap a click to the nearest street, route between two positions along the graph so the committed detour is street geometry, never freehand chords. The domain code lives in the shared transit-streets library; the module is pure and stateless, it writes nothing.",
        op_docs: &[
            ("load_streets", "Parse a street-network GeoJSON (FeatureCollection or Feature of LineString/MultiLineString geometries; one-way conventions TIGER OneWay and OSM oneway honored) into the routing graph and register it under its content hash (idempotent — the same bytes are the same graph). Bulk-bytes op 0; pass the raw GeoJSON to transit_invoke_bytes."),
            ("snap", "The nearest street position to (lat, lon) — the projected point plus its graph address (segment, parameter along it), and how far the query point was. The detour waypoint commits to this address. Null when no street is nearby."),
            ("route", "The street geometry between two snapped positions: the shortest path along the graph, as points ending exactly at the target and starting AFTER the origin (detours chain legs — each leg's start is the previous leg's committed end; prepend `from` for a standalone trace), plus length_m: the path's length along the graph from the origin itself — \"how far by road?\" answered by the router, never a host-side sum. Empty points and null length when the positions sit on disconnected parts of the network — the caller falls back to a straight line."),
        ],
    },
    ModuleSpec {
        id: "sync",
        file: "cap_sync.wasm",
        ops: &["sync_edits"],
        consumes: &[],
        speaks: &[("operator_edits", &[1])],
        migrates: &[],
        purpose: "The sync protocol's one step: what is the converged state of this edit document pair? LWW merge per record id plus tombstones (the terminal authority is the agency scheduling system — propose, review, accept), then drafts transition to submitted with the host-passed clock. transit-sync is the domain library; the module is stateless — document storage is the host's, and validation against the network is every model-side module's load_edits answer, never this one's.",
        op_docs: &[
            ("sync_edits", "Merge the client's edit document with the server's (LWW per record id — the newer updated_at wins, tombstones propagate), then transition drafts to submitted (rev bumped, now_ms stamped). The answer is the converged document; the client adopts it wholesale. The host owns both documents and the clock."),
        ],
    },
    ModuleSpec {
        id: "author",
        file: "cap_author.wasm",
        ops: &["load_network", "apply"],
        // Authoring reads and MINTS network documents — the same
        // identity the codec's import produces and every model-side
        // capability consumes.
        consumes: &[("network", "network")],
        speaks: &[("network", &[1])],
        migrates: &[],
        purpose: "Authoring: what does this network become after these edits? A change — retime a trip, add a stop, extend a calendar, fork the plan into the next pick — is a list of typed edits applied atomically to a registered NETWORK document. The answer is the next document under its content hash (registered: chain the next change on it), with its validation issues and warnings — the authoring loop in one op. The last document is never touched (the host wholesale-replaces its copy); removals refuse while anything references the entity. The edit vocabulary lives in transit-network. Assemble for anyone who changes a network: a planner's UI, a script, an agent.",
        op_docs: &[
            ("load_network", "Register the authored NETWORK document under its content hash (idempotent) — the same identity every model-side capability returns. Kernel bulk op 256 takes the bare document's JSON bytes on transit_invoke_bytes — the same answer, no request envelope; hosts send real-size networks this way."),
            ("apply", "Apply `edits` in order, atomically, to the registered document: set_plan (fork / stamp a pick — {plan:{id,name,effective_from}}), set_name, retime_trip (move one exact-times trip {pattern_trips, from_secs, to_secs} — the blocks carrying it move with it, so a retime never strands the runcut), upsert_* / remove_* for stop, route, pattern, service_type, pattern_trips, block (upserts replace the whole entity under its id: read it from the document, change it, send it back), and the GESTURES — one edit per thing a planner does, each carrying its coherence rule: add_stop / add_route / add_pattern / add_service_type {from} / add_pattern_trips / add_block (id minted when omitted — first free s1, r1, pat1, cal1, svc1, b1 — and reported in `created`); update_stop / update_route / update_pattern / update_service_type / set_pattern_trips_refs (field-wise, references checked); set_direction {route, id, name} (add or rename one of a route's directions) / remove_direction (refused while a pattern runs in it), update_pattern {direction} (one of its route's directions, \"\" for none; a route move leaves it in none unless the same edit places it); insert_pattern_stop {index?} (before visit index, default the last stop, halfway along that leg, non-timepoint), remove_pattern_stop / toggle_timepoint (middle visits only; every timing profile stays aligned — a new timepoint takes its neighbours' midpoint), set_pattern_stop, add_timing_profile / remove_timing_profile (one always stays) / set_timing_profile / set_timepoint_time; toggle_service_day {day: mon..sun}, add_service_range / set_service_range / remove_service_range, set_service_exception (one per date, date order) / remove_service_exception; set_starts (sorted, unique) / add_start (+10 min) / remove_start (one always stays), set_band / add_band / remove_band; add_block_trip (time order; a layover stays only while its two trips stay adjacent) / remove_block_trip / set_layover {after_trip, secs | null} (only between two trips). The TIMETABLE gestures address a route + direction (+ service type) — the directory's `timetable` — and apply across every pattern of the direction: set_headway {route, direction, service_type, from_secs, to_secs, headway_secs, pattern?} (frequency by period: a trip every headway from from_secs until before to_secs; trips already in the window move to the nearest slots in order, carrying their blocks; open slots are new trips on the pattern the trip before runs, unassigned; surplus trips leave their blocks), set_span {route, direction, service_type, first_secs?, last_secs?, headway_secs?} (trim past a bound; extend to it at headway_secs, else the edge's median gap, on the edge trip's pattern), adjust_running_time {route, direction, from_secs, to_secs, delta_secs} (every pattern of the direction runs trips departing in the window delta_secs longer, negative shorter, in proportion along it; profiles split at the window; timing is the pattern's, so every service type running it changes), insert_direction_stop {route, direction, stop, after} / remove_direction_stop {route, direction, stop} (by stop id on every pattern of the direction; endpoints stay). direction null or omitted means the route's patterns in no direction; pattern trips a gesture empties are removed and new ones minted ({service_type}:{pattern}) are reported in `created`; only exact-times trips are moved. An out-of-range index or a missing reference is a named error, never a silent no-op. One edit that cannot apply (an unknown id; a removal of something still referenced — a stop a pattern visits, pattern trips a block carries) rejects the whole change as an error naming the edit's position. The answer: the next network_hash (already registered), the document itself, its issues (gate export) and warnings (never gate: unassigned trips, a pick with no service in its period), and `created` ({kind, id} per entity the change made)."),
        ],
    },
    ModuleSpec {
        id: "divert",
        file: "cap_divert.wasm",
        ops: &["load_network", "draft", "list", "propose", "retire", "restore", "sanitize"],
        // A draft is answered over the network the codec's import
        // produces; the answers are next states of the host-owned
        // operator edit document.
        consumes: &[("network", "network")],
        speaks: &[("network", &[1]), ("operator_edits", &[1])],
        migrates: &[],
        purpose: "Diversion authoring: what does this diversion draft amount to? A dispatcher's draft — a route, the pattern anchored against, two kept stops, the interior stops the detour still serves, a traced path, added minutes, dates — is answered over the NETWORK document (the route's patterns and the default, each stop's role, the skips in effect, the alignment the detour is drawn against, the traced path and its length, and the problems keeping it from being a record). propose, retire, and restore answer the edit document's next state under the envelope rules (rev bumped, host clock stamped; retirement is the tombstone); sanitize reads a stored edit document under the current schema. The host owns the documents, the clock, and the record ids. Converging two documents is the sync capability's question; validating records against the network is every day view's load_edits. Assemble for anyone who authors diversions: a dispatcher's UI, a script, an agent.",
        op_docs: &[
            ("load_network", "Register the authored NETWORK document under its content hash (idempotent) — the same identity every model-side capability returns. Kernel bulk op 256 takes the bare document's JSON bytes on transit_invoke_bytes — the same answer, no request envelope; hosts send real-size networks this way."),
            ("draft", "The editor's view of a draft: the route's patterns ({pattern, name, stop_count}), the pattern in effect (the draft's when it is the route's, else the longest), every visit with its role (first_kept / last_kept / interior / outside — the anchors are the first visit of the first kept stop and the first visit of the last kept stop after it) and whether it is skipped (an interior stop not in served_stops), the skips the record would carry, the pattern's alignment (authored path, else stop-to-stop), the traced slices flattened (repeated points dropped) with their length in metres, and problems (empty = ready). Total: a draft naming what the network lacks answers problems, never an error."),
            ("list", "The edit document's rows as the dispatcher's list shows them, in document order: route, anchor, and skipped-stop names resolved over the network (an id the network lacks shows as itself), status and in_force (retired is the tombstone), added seconds, whether a detour is drawn, the dates."),
            ("propose", "The edit document with the draft proposed as record_id: a new id appends a draft at rev 1, a known id is replaced whole with its rev bumped (its domain id kept); now_ms stamped. The record: the anchors, the skips in effect, the detour when the trace has two points or more, added_mins rounded to seconds, the dates. A draft with problems is refused, naming them."),
            ("retire", "The edit document with record_id retired — the tombstone (the record stays so sync carries the deletion), rev bumped, now_ms stamped. Retiring a retired record answers the same document. An unknown id is an error."),
            ("restore", "The edit document with record_id back in force as a draft (the next sync re-submits it), rev bumped, now_ms stamped. Restoring an in-force record answers the same document."),
            ("sanitize", "Read a stored edit document (a cache, a server response, a file): the records that parse under the current schema — defaults filled for fields added since they were written — and how many were dropped. Anything but an array is an empty document."),
        ],
    },
    ModuleSpec {
        id: "plans",
        file: "cap_plans.wasm",
        ops: &["resolve", "validate", "periods"],
        consumes: &[],
        speaks: &[("plan_set", &[1])],
        migrates: &[],
        purpose: "The service-plan chain: which plan governs a date. Service changes are PICKS — a few a year at defined dates — and the chain over the plan set's metadata is DERIVED, never stored: a pick's date is a target that moves while the pick is in development. resolve answers which pick is in force; periods windows every member; validate reports chain invariant violations. Unversioned plans are standalone: they never resolve, never flag. Pure ops over the metadata the host pushes per call.",
        op_docs: &[
            ("resolve", "Which plan is in force on the date: the latest pick at or before it, ties broken by id (deterministic, order-insensitive). Unversioned plans never resolve; no pick yet answers null."),
            ("validate", "Chain validity: unique plan ids, every versioned plan effective-dated, pick dates unique. Every violation is reported (kind-tagged); unversioned plans are standalone and never flagged."),
            ("periods", "Every chain member's governing period — [effective_from, until the next pick) — derived and sorted by pick date. The until end is computed, never stored."),
        ],
    },
];

fn main() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().parent().unwrap();
    let target = root.join("target/wasm32-unknown-unknown/release");
    let dist = root.join("web/public/caps");

    // Hash-addressed files: the filename carries the content hash, so
    // the HTTP cache holds each capability build forever.
    let _ = fs::remove_dir_all(&dist);
    fs::create_dir_all(&dist).expect("create caps dir");

    // The demo feed: the e2e fixture, served statically so dev and
    // e2e run the identical demo — and hashed now, so the worked
    // examples in op_docs carry the REAL document identity.
    let fixture = root.join("web/e2e/fixtures/sample-gtfs.zip");
    let feed_bytes = fs::read(&fixture).expect("fixture exists");
    let feed_hex = format!("{:x}", Sha256::digest(&feed_bytes));
    fs::copy(&fixture, dist.join("sample-gtfs.zip")).expect("copy demo feed");
    let feed_hash = format!("sha256:{feed_hex}");

    // The network capability's worked examples are computed from the
    // REAL fixture: import derives the document, export produces the
    // zip — both through the shared transit-codec library, so the
    // examples are exactly what the module returns and cannot drift.
    let net_ctx = network_example_context(&feed_bytes);

    // The day-view runner gates' document: the load_network request
    // for the REAL fixture-derived network (the same bytes the
    // examples and the smoke use — the document cannot drift from
    // the module that owns it). Every JSON-document capability's
    // gate (board, runcut, calendar) chains from it.
    if let Some(ctx) = net_ctx.as_ref() {
        let request = serde_json::json!({ "op": "load_network", "network": ctx.derived });
        let json = serde_json::to_string_pretty(&request).expect("request serializes");
        fs::write(dist.join("sample-network-load.json"), json + "\n")
            .expect("write the dispatch sample document");
    }

    let mut capabilities = Vec::with_capacity(MODULES.len());
    for module in MODULES {
        let bytes = fs::read(target.join(module.file)).expect("wasm module (run the build first)");
        let hex = format!("{:x}", Sha256::digest(&bytes));
        fs::write(dist.join(format!("{}-{hex}.wasm", module.id)), &bytes).expect("write module");
        capabilities.push(Capability {
            id: module.id.into(),
            hash: format!("sha256:{hex}"),
            url: format!("caps/{}-{hex}.wasm", module.id),
            bytes: bytes.len() as u64,
            ops: module.ops.iter().map(|s| s.to_string()).collect(),
            consumes: module
                .consumes
                .iter()
                .map(|(kind, id)| ((*kind).to_string(), (*id).to_string()))
                .collect(),
            speaks: module
                .speaks
                .iter()
                .map(|(kind, versions)| {
                    ((*kind).to_string(), versions.iter().copied().collect::<Vec<_>>())
                })
                .collect(),
            migrates: module
                .migrates
                .iter()
                .map(|(kind, steps)| {
                    (
                        (*kind).to_string(),
                        steps
                            .iter()
                            .map(|&(from, to)| MigrationSpec { from, to })
                            .collect::<Vec<_>>(),
                    )
                })
                .collect(),
            purpose: module.purpose.into(),
            op_docs: module
                .op_docs
                .iter()
                .map(|(op, description)| {
                    let example = example_for(module.id, op, &feed_hash, net_ctx.as_ref());
                    (
                        (*op).to_string(),
                        OpDoc { description: (*description).into(), example },
                    )
                })
                .collect(),
        });
        println!("cap {}: sha256:{hex} ({:.1} KiB)", module.id, bytes.len() as f64 / 1024.0);
    }

    let manifest = CapManifest { kernel: KERNEL, capabilities };

    // The compose-time proof, applied to our own assembly before it
    // is written: generation is the validator's first consumer.
    if let Err(issues) = validate_assembly(&manifest) {
        eprintln!("generated assembly is NOT valid:");
        for issue in issues {
            eprintln!("  [{}] {}: {}", issue.capability, issue.check, issue.detail);
        }
        std::process::exit(1);
    }

    let json = serde_json::to_string_pretty(&manifest).expect("manifest serializes");
    fs::write(dist.join("manifest.json"), json + "\n").expect("write manifest");

    publish_registry(&root, &manifest);

    println!("manifest: {} capabilities, kernel v{KERNEL}, assembly valid", MODULES.len());
}

/// Publish the REGISTRY: immutable blobs, one evidence record per
/// capability, and the pointer. The record says what these exact
/// bytes are, what they answer, what they speak — and WHICH GATES
/// the publish recipe ran against them, at which commit. That
/// record is what makes "known good" a checkable claim for an agent
/// (or a human dogfooding `caps run`) rather than a marketing one.
fn publish_registry(root: &Path, manifest: &CapManifest) {
    let registry = root.join("registry");
    let blobs = registry.join("blobs");
    let caps_dir = registry.join("capabilities");
    let _ = fs::remove_dir_all(&registry);
    fs::create_dir_all(&blobs).expect("create registry blobs");
    fs::create_dir_all(&caps_dir).expect("create registry records");

    // Provenance-style evidence: what built these bytes, from where.
    let run = |args: &[&str]| {
        Command::new(args[0])
            .args(&args[1..])
            .current_dir(root)
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .unwrap_or_default()
    };
    let commit = run(&["git", "rev-parse", "HEAD"]);
    let dirty = !run(&["git", "status", "--porcelain"]).is_empty();
    let rustc = run(&["rustc", "--version"]);
    let evidence = serde_json::json!({
        "gates": PUBLISH_GATES,
        "recipe": "just registry",
        "source_commit": commit,
        "dirty_tree": dirty,
        "rustc": rustc,
    });

    for cap in &manifest.capabilities {
        let bytes = fs::read(root.join("web/public/caps").join(Path::new(&cap.url).file_name().unwrap()))
            .expect("module bytes (the manifest step wrote them)");
        let hex = cap.hash.strip_prefix("sha256:").expect("hash format");
        fs::write(blobs.join(format!("{hex}.wasm")), &bytes).expect("write blob");

        let record = serde_json::json!({
            "id": cap.id,
            "hash": cap.hash,
            "bytes": cap.bytes,
            "kernel": manifest.kernel,
            "ops": cap.ops,
            "consumes": cap.consumes,
            "speaks": cap.speaks,
            "migrates": cap.migrates,
            "purpose": cap.purpose,
            "op_docs": cap.op_docs,
            "blob": format!("blobs/{hex}.wasm"),
            "evidence": evidence,
        });
        let json = serde_json::to_string_pretty(&record).expect("record serializes");
        fs::write(caps_dir.join(format!("{}.json", cap.id)), json + "\n")
            .expect("write record");
    }

    // The pointer: which capabilities (at which hashes) this registry
    // currently names. Blobs are immutable; this file moves.
    let pointer = serde_json::json!({
        "kernel": manifest.kernel,
        "capabilities": manifest
            .capabilities
            .iter()
            .map(|c| serde_json::json!({ "id": c.id, "hash": c.hash }))
            .collect::<Vec<_>>(),
        "evidence": evidence,
    });
    let json = serde_json::to_string_pretty(&pointer).expect("pointer serializes");
    fs::write(registry.join("registry.json"), json + "\n").expect("write pointer");

    println!(
        "registry: {} capabilities published (blobs + records + pointer)",
        manifest.capabilities.len()
    );
}

/// The network examples' real context: the fixture's derived
/// document (canonical hash — the module's formula), and its
/// exported zip (content hash of the deterministic bytes).
struct NetworkExampleContext {
    /// The typed derived document — the analysis examples compute
    /// real answers over it.
    network: transit_network::Network,
    derived: serde_json::Value,
    notes: serde_json::Value,
    network_hash: String,
    trip_map: serde_json::Value,
    export_feed_hash: String,
    export_summary: serde_json::Value,
    zip_b64: String,
}

impl NetworkExampleContext {
    fn derived_network(&self) -> &transit_network::Network {
        &self.network
    }
}

fn network_example_context(feed_bytes: &[u8]) -> Option<NetworkExampleContext> {
    let gtfs = transit_core::Gtfs::from_zip_bytes(feed_bytes).ok()?;
    let outcome = transit_codec::gtfs_to_network(&gtfs);
    let canonical = serde_json::to_string(&outcome.network).ok()?;
    let network_hash = format!("sha256:{:x}", Sha256::digest(canonical.as_bytes()));
    let derived = serde_json::to_value(&outcome.network).ok()?;
    let notes = serde_json::to_value(&outcome.notes).ok()?;
    let trip_map = serde_json::to_value(&outcome.trip_map).ok()?;
    let exported = transit_codec::network_to_gtfs(&outcome.network).ok()?;
    let export_summary = serde_json::to_value(exported.summary()).ok()?;
    let zip = transit_core::write::to_zip_bytes(&exported);
    let export_feed_hash = format!("sha256:{:x}", Sha256::digest(&zip));
    let zip_b64 = transit_core::write::base64_encode(&zip);
    Some(NetworkExampleContext {
        network: outcome.network,
        derived,
        notes,
        network_hash,
        trip_map,
        export_feed_hash,
        export_summary,
        zip_b64,
    })
}

/// The parsed e2e fixture feed (for the gtfs capability's dispatch
/// read examples — real answers, same library the module uses).
fn fixture_gtfs() -> Option<transit_core::Gtfs> {
    let fixture = concat!(env!("CARGO_MANIFEST_DIR"), "/../../web/e2e/fixtures/sample-gtfs.zip");
    let bytes = std::fs::read(fixture).ok()?;
    transit_core::Gtfs::from_zip_bytes(&bytes).ok()
}

/// The street capability's examples' context: the e2e street
/// fixture parsed through the shared library, with the Elbow
/// corner snap/route answers computed over the real graph.
struct StreetsExampleContext {
    streets_hash: String,
    pos_a: transit_streets::StreetPosition,
    pos_b: transit_streets::StreetPosition,
    points: Vec<transit_streets::StreetPoint>,
    length_m: Option<f64>,
}

fn streets_example_context() -> Option<StreetsExampleContext> {
    let bytes = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../web/e2e/fixtures/streets.geojson"
    ))
    .ok()?;
    let net = transit_streets::StreetNetwork::parse(&bytes).ok()?;
    let streets_hash = format!("sha256:{:x}", Sha256::digest(&bytes));
    let pos_a = net.snap(45.5040, -73.5030)?;
    let pos_b = net.snap(45.5080, -73.5080)?;
    let (points, length_m) = match net.route_with_length(pos_a, pos_b) {
        Some((points, length)) => (points, Some(length)),
        None => (vec![], None),
    };
    Some(StreetsExampleContext { streets_hash, pos_a, pos_b, points, length_m })
}

/// The rt join examples' context: the fixture feed and the e2e rt
/// payload, both parsed through the shared libraries.
struct RtExampleContext {
    feed: transit_core::Gtfs,
    overview: transit_rt::RtOverview,
    feed_hash: String,
    rt_hash: String,
}

fn rt_example_context() -> Option<RtExampleContext> {
    let feed = fixture_gtfs()?;
    let payload = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../web/e2e/fixtures/sample-rt.pb"
    ))
    .ok()?;
    let overview = transit_rt::decode_overview(&payload).ok()?;
    let feed_hash = format!("sha256:{:x}", Sha256::digest(
        std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../web/e2e/fixtures/sample-gtfs.zip"
        ))
        .ok()?,
    ));
    let rt_hash = format!("sha256:{:x}", Sha256::digest(&payload));
    Some(RtExampleContext { feed, overview, feed_hash, rt_hash })
}

/// The live document over the fixture pair, the way the rt module
/// mints it: the trip map (and its hash), the document (and its hash).
fn live_example() -> Option<(transit_codec::TripIdMap, String, transit_live::LiveDay, String)> {
    let ctx = rt_example_context()?;
    let trip_map = transit_codec::gtfs_to_network(&ctx.feed).trip_map;
    let hash = |v: String| format!("sha256:{:x}", Sha256::digest(v.as_bytes()));
    let trip_map_hash = hash(serde_json::to_string(&trip_map).ok()?);
    let live = transit_rt::live_day(&ctx.feed, Some(&ctx.overview), None, &trip_map, transit_live::OnTimeWindow::default());
    let live_hash = hash(serde_json::to_string(&live).ok()?);
    Some((trip_map, trip_map_hash, live, live_hash))
}

/// Worked examples: an agent reads these to learn the shapes.
/// Requests carry the REAL feed hash computed above; responses
/// carry the fixture's known answers.
fn example_for(
    capability: &str,
    op: &str,
    feed_hash: &str,
    net: Option<&NetworkExampleContext>,
) -> Option<OpExample> {
    use serde_json::json;
    let (request, response) = match (capability, op) {
        ("gtfs", "feed_info") => {
            let gtfs = fixture_gtfs()?;
            (
                json!({ "op": "feed_info", "feed_hash": feed_hash }),
                json!({ "ok": { "op": "feed_info", "info": gtfs.feed_info } }),
            )
        }
        // The router's examples compute REAL answers over the
        // fixture-derived document — the same composition the module
        // dispatches through (materialize, patch, build, query).
        ("router", "plan") => {
            let ctx = net?;
            let date = chrono::NaiveDate::from_ymd_opt(2026, 10, 23).expect("fixture date");
            let network = ctx.derived_network();
            let day = transit_network::materialize(network, date).expect("fixture materializes");
            let graph = transit_router::RouterNetwork::build_from_day(network, &day, date);
            let itinerary = transit_router::earliest_arrival(&graph, "A", "B", 25200, 4)
                .expect("query")
                .expect("A→B reachable on the fixture Friday");
            (
                json!({ "op": "plan", "network_hash": ctx.network_hash, "from": "A", "to": "B",
                        "date": "2026-10-23", "depart_secs": 25200, "max_legs": 4 }),
                json!({ "ok": { "op": "plan", "itinerary": itinerary } }),
            )
        }
        ("router", "plan_compare") => {
            let ctx = net?;
            let date = chrono::NaiveDate::from_ymd_opt(2026, 10, 23).expect("fixture date");
            let network = ctx.derived_network();
            let records = dispatch_fixture_edits();
            // The edits document's hash, computed the way the module
            // does: sha256 of the canonical serialization.
            let canonical = serde_json::to_string(&records).expect("records serialize");
            let edits_hash = format!("sha256:{:x}", Sha256::digest(canonical.as_bytes()));
            let patched =
                transit_dispatch::effective_day(network, date, &records).expect("patched day");
            let active_graph = transit_router::RouterNetwork::build_from_day(network, &patched, date);
            let active = transit_router::earliest_arrival(&active_graph, "A", "B", 25200, 4)
                .expect("query")
                .expect("A→B reachable while the diversion runs");
            let base_day = transit_network::materialize(network, date).expect("baseline day");
            let base_graph = transit_router::RouterNetwork::build_from_day(network, &base_day, date);
            let baseline = transit_router::earliest_arrival(&base_graph, "A", "B", 25200, 4)
                .expect("query")
                .expect("A→B reachable on the unpatched day");
            (
                json!({ "op": "plan_compare", "network_hash": ctx.network_hash,
                        "edits_hash": edits_hash, "from": "A", "to": "B",
                        "date": "2026-10-23", "depart_secs": 25200, "max_legs": 4 }),
                json!({ "ok": { "op": "plan_compare", "active": active, "baseline": baseline, "notice": null } }),
            )
        }
        ("router", "itinerary_paths") => {
            let ctx = net?;
            let date = chrono::NaiveDate::from_ymd_opt(2026, 10, 23).expect("fixture date");
            let network = ctx.derived_network();
            let day = transit_network::materialize(network, date).expect("fixture materializes");
            let graph = transit_router::RouterNetwork::build_from_day(network, &day, date);
            let itinerary = transit_router::earliest_arrival(&graph, "A", "B", 25200, 4).ok()??;
            let paths: Vec<_> = itinerary
                .legs
                .iter()
                .map(|leg| match leg {
                    transit_router::Leg::Transit { pattern_trips, start_secs, board_stop_id, alight_stop_id, .. } => {
                        json!({ "kind": "transit", "path": transit_dispatch::trip_segment(network, &day, pattern_trips, *start_secs, board_stop_id, alight_stop_id) })
                    }
                    transit_router::Leg::Walk { from_stop_id, via_stop_ids, to_stop_id, .. } => {
                        let stops: Vec<&str> = std::iter::once(from_stop_id).chain(via_stop_ids).chain(std::iter::once(to_stop_id)).map(String::as_str).collect();
                        json!({ "kind": "walk", "path": transit_network::stops_line(network, stops) })
                    }
                })
                .collect();
            (
                json!({ "op": "itinerary_paths", "network_hash": ctx.network_hash, "date": "2026-10-23", "itinerary": itinerary }),
                json!({ "ok": { "op": "itinerary_paths", "paths": paths } }),
            )
        }
        // The network capability's examples carry the REAL derived
        // document and exported zip of the fixture — computed by the
        // same shared library the module dispatches through.
        ("network", "import") => {
            let ctx = net?;
            (
                json!({ "op": "import", "feed_hash": feed_hash }),
                json!({ "ok": { "op": "import", "network": ctx.derived,
                    "notes": ctx.notes, "network_hash": ctx.network_hash } }),
            )
        }
        ("network", "load_network") => {
            let ctx = net?;
            (
                json!({ "op": "load_network", "network": ctx.derived }),
                json!({ "ok": { "op": "load_network", "network_hash": ctx.network_hash } }),
            )
        }
        ("network", "validate") => {
            let ctx = net?;
            (
                json!({ "op": "validate", "network_hash": ctx.network_hash }),
                json!({ "ok": { "op": "validate",
                    "issues": transit_network::validation_issues(ctx.derived_network()),
                    "warnings": transit_network::validation_warnings(ctx.derived_network()) } }),
            )
        }
        ("network", "export") => {
            let ctx = net?;
            (
                json!({ "op": "export", "network_hash": ctx.network_hash }),
                json!({ "ok": { "op": "export", "feed_hash": ctx.export_feed_hash,
                    "summary": ctx.export_summary, "zip_b64": ctx.zip_b64 } }),
            )
        }
        ("network", "agency_trip_ids") => {
            let ctx = net?;
            let map: transit_codec::TripIdMap = serde_json::from_value(ctx.trip_map.clone()).ok()?;
            let first = map.entries.first()?;
            (
                json!({ "op": "agency_trip_ids", "network_hash": ctx.network_hash,
                    "trips": [{ "pattern_trips": first.pattern_trips, "start_secs": first.start_secs }] }),
                json!({ "ok": { "op": "agency_trip_ids", "ids": [first.agency_trip_ids] } }),
            )
        }
        ("network", "trip_map") => {
            let ctx = net?;
            (
                json!({ "op": "trip_map", "network_hash": ctx.network_hash }),
                json!({ "ok": { "op": "trip_map", "map": ctx.trip_map } }),
            )
        }
        // The realtime example decodes the REAL e2e fixture payload —
        // the same bytes the browser tests replay.
        ("rt", "decode") => {
            let fixture = concat!(env!("CARGO_MANIFEST_DIR"), "/../../web/e2e/fixtures/sample-rt.pb");
            let bytes = std::fs::read(fixture).expect("rt fixture exists");
            let overview = transit_rt::decode_overview(&bytes).expect("fixture decodes");
            (
                json!({ "op": "decode", "bytes": "<the raw GTFS-rt protobuf — bulk-bytes op 0>" }),
                json!({ "ok": { "op": "decode", "overview": overview } }),
            )
        }
        // The analysis examples carry REAL answers over the fixture-
        // derived document (the same shared library the module
        // dispatches through), so the worked examples cannot drift.
        ("analysis", "load_network") => {
            let ctx = net?;
            (
                json!({ "op": "load_network", "network": ctx.derived }),
                json!({ "ok": { "op": "load_network", "network_hash": ctx.network_hash } }),
            )
        }
        ("analysis", "network_stats") => {
            let ctx = net?;
            let date = chrono::NaiveDate::from_ymd_opt(2026, 1, 5).unwrap();
            let mut stats = transit_analysis::network_stats(&ctx.derived_network(), date);
            // The example ASKS the day-shape question: no routes, no
            // stops, no service types inline — the query-shaping
            // flags in the flesh.
            let routes = std::mem::take(&mut stats.routes);
            let stops = std::mem::take(&mut stats.stops);
            let service_types = std::mem::take(&mut stats.service_types);
            let response = json!({ "ok": { "op": "network_stats", "stats": stats } });
            let _ = (routes, stops, service_types);
            (
                json!({ "op": "network_stats", "network_hash": ctx.network_hash,
                    "date": "2026-01-05", "include_routes": false, "include_stops": false, "include_service_types": false }),
                response,
            )
        }
        ("analysis", "service_types_overview") => {
            let ctx = net?;
            (
                json!({ "op": "service_types_overview", "network_hash": ctx.network_hash }),
                json!({ "ok": { "op": "service_types_overview",
                    "service_types": transit_analysis::service_type_overview(&ctx.derived_network()) } }),
            )
        }
        ("analysis", "service_type_detail") => {
            let ctx = net?;
            let detail = transit_analysis::service_type_detail(&ctx.derived_network(), "WK")
                .expect("WK exists in the fixture-derived document");
            (
                json!({ "op": "service_type_detail", "network_hash": ctx.network_hash, "service_type": "WK" }),
                json!({ "ok": { "op": "service_type_detail", "detail": detail } }),
            )
        }
        ("analysis", "service_type_compare") => {
            let ctx = net?;
            let comparison = transit_analysis::service_type_compare(&ctx.derived_network(), &ctx.derived_network(), "WK")
                .expect("WK exists in the fixture-derived document");
            (
                json!({ "op": "service_type_compare", "base_hash": ctx.network_hash, "network_hash": ctx.network_hash, "service_type": "WK" }),
                json!({ "ok": { "op": "service_type_compare", "comparison": comparison } }),
            )
        }
        // The stop's day as gaps, computed over the same fixture
        // document — a real weekday answer, not a hand-written one.
        ("analysis", "stop_gaps") => {
            let ctx = net?;
            let date = chrono::NaiveDate::from_ymd_opt(2026, 1, 5).unwrap();
            let gaps = transit_analysis::stop_gaps(&ctx.derived_network(), date, "A");
            (
                json!({ "op": "stop_gaps", "network_hash": ctx.network_hash, "date": "2026-01-05", "stop_id": "A" }),
                json!({ "ok": { "op": "stop_gaps", "gaps": gaps } }),
            )
        }
        // The day's hourly profile over the same fixture document.
        ("analysis", "hourly_trips") => {
            let ctx = net?;
            let date = chrono::NaiveDate::from_ymd_opt(2026, 1, 5).unwrap();
            (
                json!({ "op": "hourly_trips", "network_hash": ctx.network_hash, "date": "2026-01-05" }),
                json!({ "ok": { "op": "hourly_trips",
                    "hours": transit_analysis::hourly_trips(&ctx.derived_network(), date) } }),
            )
        }
        // The schedule texture over the same fixture document.
        ("analysis", "schedule_texture") => {
            let ctx = net?;
            (
                json!({ "op": "schedule_texture", "network_hash": ctx.network_hash, "service_type": "WK" }),
                json!({ "ok": { "op": "schedule_texture",
                    "texture": transit_analysis::schedule_texture(&ctx.derived_network(), "WK") } }),
            )
        }
        // The corridor read over the same fixture document.
        ("analysis", "route_overlap") => {
            let ctx = net?;
            let date = chrono::NaiveDate::from_ymd_opt(2026, 1, 5).unwrap();
            (
                json!({ "op": "route_overlap", "network_hash": ctx.network_hash, "date": "2026-01-05" }),
                json!({ "ok": { "op": "route_overlap",
                    "overlaps": transit_analysis::route_overlap(&ctx.derived_network(), date) } }),
            )
        }
        // The hub's timed transfers over the same fixture document —
        // the window the caller brings widens the default 60–900 s
        // connection bounds to catch the fixture's 20-minute meeting.
        ("analysis", "timed_transfers") => {
            let ctx = net?;
            let date = chrono::NaiveDate::from_ymd_opt(2026, 1, 5).unwrap();
            (
                json!({ "op": "timed_transfers", "network_hash": ctx.network_hash,
                    "date": "2026-01-05", "stop_id": "A",
                    "window": { "min_secs": 60, "max_secs": 1500 } }),
                json!({ "ok": { "op": "timed_transfers",
                    "transfers": transit_analysis::timed_transfers(
                        &ctx.derived_network(), date, "A",
                        Some(transit_analysis::TransferWindow { min_secs: 60, max_secs: 1500 })) } }),
            )
        }
        ("analysis", "type_coverage") => {
            let ctx = net?;
            // The same two-point layer the caps smoke uses: one point
            // at a stop (served), one far away — coverage is 50%.
            let layer_json = r#"{"type":"FeatureCollection","features":[
                {"type":"Feature","properties":{"pop":500},"geometry":{"type":"Point","coordinates":[-73.5,45.5]}},
                {"type":"Feature","properties":{"pop":500},"geometry":{"type":"Point","coordinates":[-74.0,46.0]}}
            ]}"#;
            let layer = transit_analysis::parse_pop_layer(layer_json.as_bytes(), None)
                .expect("layer parses");
            let layer_hash = format!("sha256:{:x}", Sha256::digest(layer_json.as_bytes()));
            (
                json!({ "op": "type_coverage", "network_hash": ctx.network_hash,
                    "layer_hash": layer_hash, "radius_m": 500, "frequent_secs": 3600, "min_departures": 2 }),
                json!({ "ok": { "op": "type_coverage",
                    "coverage": transit_analysis::type_coverage(&ctx.derived_network(), &layer, 500.0, 3600, 2, None) } }),
            )
        }
        // The unserved WHERE over the same fixture document and the
        // same two-point layer: the far point, named, with the
        // nearest served stop.
        ("analysis", "unserved_points") => {
            let ctx = net?;
            let layer_json = r#"{"type":"FeatureCollection","features":[
                {"type":"Feature","properties":{"pop":500},"geometry":{"type":"Point","coordinates":[-73.5,45.5]}},
                {"type":"Feature","properties":{"pop":500},"geometry":{"type":"Point","coordinates":[-74.0,46.0]}}
            ]}"#;
            let layer = transit_analysis::parse_pop_layer(layer_json.as_bytes(), None)
                .expect("layer parses");
            let layer_hash = format!("sha256:{:x}", Sha256::digest(layer_json.as_bytes()));
            let date = chrono::NaiveDate::from_ymd_opt(2026, 1, 5).unwrap();
            (
                json!({ "op": "unserved_points", "network_hash": ctx.network_hash,
                    "layer_hash": layer_hash, "radius_m": 500, "date": "2026-01-05" }),
                json!({ "ok": { "op": "unserved_points",
                    "points": transit_analysis::unserved_points(&ctx.derived_network(), date, &layer, 500.0) } }),
            )
        }
        // The rt join examples compute REAL answers over the fixture
        // pair (sample-gtfs.zip + sample-rt.pb): t10 +120 s at A /
        // +180 s at B, one vehicle on t10 — the joined board and the
        // ranked view through the same shared library the module
        // dispatches through.
        ("rt", "trip_progress") => {
            let ctx = rt_example_context()?;
            (
                json!({ "op": "trip_progress", "feed_hash": ctx.feed_hash,
                    "rt_hash": ctx.rt_hash, "trip_id": "t10" }),
                json!({ "ok": { "op": "trip_progress",
                    "progress": transit_rt::trip_progress(&ctx.feed, &ctx.overview, "t10") } }),
            )
        }
        ("rt", "load_trip_map") => {
            let (trip_map, trip_map_hash, _, _) = live_example()?;
            (
                json!({ "op": "load_trip_map", "trip_map": trip_map }),
                json!({ "ok": { "op": "load_trip_map", "trip_map_hash": trip_map_hash } }),
            )
        }
        ("rt", "live") => {
            let ctx = rt_example_context()?;
            let (_, trip_map_hash, live, live_hash) = live_example()?;
            (
                json!({ "op": "live", "feed_hash": ctx.feed_hash, "rt_hash": ctx.rt_hash,
                    "trip_map_hash": trip_map_hash,
                    "window": { "late_secs": 60, "very_late_secs": 300 } }),
                json!({ "ok": { "op": "live", "live_hash": live_hash,
                    "summary": transit_live::summarize(&live, 15), "live": live } }),
            )
        }
        ("rt", "late_trips") => {
            let ctx = rt_example_context()?;
            (
                json!({ "op": "late_trips", "feed_hash": ctx.feed_hash,
                    "rt_hash": ctx.rt_hash, "min_delay_secs": 60,
                    "vp_hash": null }),
                json!({ "ok": { "op": "late_trips",
                    "trips": transit_rt::late_trips(&ctx.feed, &ctx.overview, None, 60) } }),
            )
        }
        ("board", "load_network") => {
            let ctx = net?;
            (
                json!({ "op": "load_network", "network": ctx.derived }),
                json!({ "ok": { "op": "load_network", "network_hash": ctx.network_hash } }),
            )
        }
        ("board", "load_edits") => {
            let ctx = net?;
            let records = dispatch_fixture_edits();
            let reports = transit_dispatch::report_against_network(ctx.derived_network(), &records);
            let edits_hash = format!("sha256:{:x}", Sha256::digest(
                serde_json::to_string(&records).expect("records serialize").as_bytes(),
            ));
            (
                json!({ "op": "load_edits", "network_hash": ctx.network_hash, "records": records }),
                json!({ "ok": { "op": "load_edits", "edits_hash": edits_hash, "reports": reports } }),
            )
        }
        ("board", "load_live") => {
            let (_, _, live, live_hash) = live_example()?;
            (
                json!({ "op": "load_live", "live": live }),
                json!({ "ok": { "op": "load_live", "live_hash": live_hash } }),
            )
        }
        ("board", "stop_board") => {
            let ctx = net?;
            let (_, _, live, live_hash) = live_example()?;
            let date = chrono::NaiveDate::from_ymd_opt(2026, 10, 23).unwrap();
            let day = transit_network::materialize(ctx.derived_network(), date).expect("fixture day");
            // 08:01 at A: t10 (08:00, +120 s) has not left yet.
            (
                json!({ "op": "stop_board", "network_hash": ctx.network_hash,
                    "stop_id": "A", "date": "2026-10-23", "from_secs": 28860, "limit": 5,
                    "live_hash": live_hash }),
                json!({ "ok": { "op": "stop_board",
                    "board": transit_dispatch::stop_board(ctx.derived_network(), &day, "A", 28860, 5, Some(&live)) } }),
            )
        }
        ("board", "load_watchlist") => (
            json!({ "op": "load_watchlist", "document": { "version": "v2", "stops": [
                { "id": "A", "label": "home" }, { "id": "B", "label": "work" }
            ] } }),
            json!({ "ok": { "op": "load_watchlist", "version": 2, "watchlist_hash": "sha256:<of the document>" } }),
        ),
        ("board", "migrate_watchlist") => (
            json!({ "op": "migrate_watchlist", "document": { "version": "v1", "stops": [
                { "id": "A", "note": "home", "color": "blue" }
            ] } }),
            json!({ "ok": { "op": "migrated_watchlist", "document": { "version": "v2", "stops": [
                { "id": "A", "label": "home" }
            ] }, "notes": [ { "kind": "dropped_field", "field": "color", "count": 1 } ] } }),
        ),
        ("board", "watchlist_board") => {
            let ctx = net?;
            let date = chrono::NaiveDate::from_ymd_opt(2026, 10, 23).unwrap();
            let day = transit_network::materialize(ctx.derived_network(), date).expect("fixture day");
            let network = ctx.derived_network();
            let rows = |stop: &str| transit_dispatch::board(&network, &day, stop, 25200, 3);
            (
                json!({ "op": "watchlist_board", "watchlist_hash": "sha256:<of the document>",
                    "network_hash": ctx.network_hash, "date": "2026-10-23", "from_secs": 25200, "limit": 3 }),
                json!({ "ok": { "op": "watchlist_board", "stops": [
                    { "id": "A", "label": "home", "departures": rows("A") },
                    { "id": "B", "label": "work", "departures": rows("B") }
                ] } }),
            )
        }
        ("runcut", "load_network") => {
            let ctx = net?;
            (
                json!({ "op": "load_network", "network": ctx.derived }),
                json!({ "ok": { "op": "load_network", "network_hash": ctx.network_hash } }),
            )
        }
        ("runcut", "load_edits") => {
            let ctx = net?;
            let records = dispatch_fixture_edits();
            let reports = transit_dispatch::report_against_network(ctx.derived_network(), &records);
            let edits_hash = format!("sha256:{:x}", Sha256::digest(
                serde_json::to_string(&records).expect("records serialize").as_bytes(),
            ));
            (
                json!({ "op": "load_edits", "network_hash": ctx.network_hash, "records": records }),
                json!({ "ok": { "op": "load_edits", "edits_hash": edits_hash, "reports": reports } }),
            )
        }
        ("runcut", "blocks") => {
            let ctx = net?;
            let date = chrono::NaiveDate::from_ymd_opt(2026, 10, 23).unwrap();
            let day = transit_network::materialize(ctx.derived_network(), date).expect("fixture day");
            (
                json!({ "op": "blocks", "network_hash": ctx.network_hash, "date": "2026-10-23" }),
                json!({ "ok": { "op": "blocks",
                    "blocks": transit_dispatch::day_blocks(ctx.derived_network(), &day) } }),
            )
        }
        ("runcut", "fleet_summary") => {
            let ctx = net?;
            let date = chrono::NaiveDate::from_ymd_opt(2026, 10, 23).unwrap();
            let day = transit_network::materialize(ctx.derived_network(), date).expect("fixture day");
            (
                json!({ "op": "fleet_summary", "network_hash": ctx.network_hash, "date": "2026-10-23" }),
                json!({ "ok": { "op": "fleet_summary",
                    "summary": transit_dispatch::fleet_summary(ctx.derived_network(), &day) } }),
            )
        }
        ("runcut", "block_trips") => {
            let ctx = net?;
            let date = chrono::NaiveDate::from_ymd_opt(2026, 10, 23).unwrap();
            let records = dispatch_fixture_edits();
            let day = transit_dispatch::effective_day(ctx.derived_network(), date, &records)
                .expect("fixture day");
            (
                json!({ "op": "block_trips", "network_hash": ctx.network_hash,
                    "edits_hash": "<of the loaded edit document>", "block_id": "B1", "date": "2026-10-23" }),
                json!({ "ok": { "op": "block_trips", "block_id": "B1",
                    "trips": transit_dispatch::block_chain(ctx.derived_network(), &day, "B1", transit_dispatch::TIGHT_LAYOVER_SECS) } }),
            )
        }
        ("runcut", "trip_stops") => {
            let ctx = net?;
            let date = chrono::NaiveDate::from_ymd_opt(2026, 10, 23).unwrap();
            let records = dispatch_fixture_edits();
            let day = transit_dispatch::effective_day(ctx.derived_network(), date, &records)
                .expect("fixture day");
            // t30's derived trip: the via-C run at 08:40 — the
            // patched day keeps A→B (C skipped, +300 s).
            let trip = day
                .iter()
                .find(|t| t.stop_times.len() == 2 && t.start_secs == 8 * 3600 + 40 * 60)
                .expect("the diverted trip is in the day");
            (
                json!({ "op": "trip_stops", "network_hash": ctx.network_hash,
                    "edits_hash": "<of the loaded edit document>",
                    "pattern_trips": trip.pattern_trips, "start_secs": trip.start_secs, "date": "2026-10-23" }),
                json!({ "ok": { "op": "trip_stops",
                    "stops": transit_dispatch::trip_stops(
                        ctx.derived_network(), &day, &trip.pattern_trips, trip.start_secs) } }),
            )
        }
        ("runcut", "trip_path") => {
            let ctx = net?;
            let date = chrono::NaiveDate::from_ymd_opt(2026, 10, 23).unwrap();
            let records = dispatch_fixture_edits();
            let day = transit_dispatch::effective_day(ctx.derived_network(), date, &records).ok()?;
            // The diverted 08:40 trip draws its patched stops (A→B).
            let trip = day.iter().find(|t| t.stop_times.len() == 2 && t.start_secs == 8 * 3600 + 40 * 60)?;
            (
                json!({ "op": "trip_path", "network_hash": ctx.network_hash,
                    "edits_hash": "<of the loaded edit document>",
                    "pattern_trips": trip.pattern_trips, "start_secs": trip.start_secs, "date": "2026-10-23" }),
                json!({ "ok": { "op": "trip_path",
                    "path": transit_dispatch::trip_path(ctx.derived_network(), &day, &trip.pattern_trips, trip.start_secs) } }),
            )
        }
        ("calendar", "load_network") => {
            let ctx = net?;
            (
                json!({ "op": "load_network", "network": ctx.derived }),
                json!({ "ok": { "op": "load_network", "network_hash": ctx.network_hash } }),
            )
        }
        ("calendar", "load_edits") => {
            let ctx = net?;
            let records = dispatch_fixture_edits();
            let reports = transit_dispatch::report_against_network(ctx.derived_network(), &records);
            let edits_hash = format!("sha256:{:x}", Sha256::digest(
                serde_json::to_string(&records).expect("records serialize").as_bytes(),
            ));
            (
                json!({ "op": "load_edits", "network_hash": ctx.network_hash, "records": records }),
                json!({ "ok": { "op": "load_edits", "edits_hash": edits_hash, "reports": reports } }),
            )
        }
        ("calendar", "active_services") => {
            let ctx = net?;
            let date = chrono::NaiveDate::from_ymd_opt(2026, 10, 23).unwrap();
            let day = transit_network::materialize(ctx.derived_network(), date).expect("fixture day");
            (
                json!({ "op": "active_services", "network_hash": ctx.network_hash, "date": "2026-10-23" }),
                json!({ "ok": { "op": "active_services",
                    "services": transit_dispatch::active_services(ctx.derived_network(), &day) } }),
            )
        }
        // The reverse read, computed over the same fixture document
        // — the window crosses the fixture's calendar exception (WK
        // removed on 2026-10-29), so the example cannot drift.
        ("calendar", "service_dates") => {
            let ctx = net?;
            let from = chrono::NaiveDate::from_ymd_opt(2026, 10, 23).unwrap();
            let to = chrono::NaiveDate::from_ymd_opt(2026, 11, 3).unwrap();
            let dates = transit_dispatch::service_dates(ctx.derived_network(), &[], "WK", from, to)
                .expect("fixture window");
            (
                json!({ "op": "service_dates", "network_hash": ctx.network_hash,
                    "service_type": "WK", "from": "2026-10-23", "to": "2026-11-03" }),
                json!({ "ok": { "op": "service_dates",
                    "dates": dates.iter().map(|d| d.format("%Y-%m-%d").to_string()).collect::<Vec<_>>() } }),
            )
        }
        // The directory's examples compute REAL answers over the
        // fixture-derived document — the stop list with its walking
        // group, the route list, and route 10's patterns (the direct
        // run and the via-C run) — through the same document the
        // module addresses, so the worked examples cannot drift.
        ("directory", "load_network") => {
            let ctx = net?;
            (
                json!({ "op": "load_network", "network": ctx.derived }),
                json!({ "ok": { "op": "load_network", "network_hash": ctx.network_hash } }),
            )
        }
        ("directory", "stops") => {
            let ctx = net?;
            let network = ctx.derived_network();
            let mut stops: Vec<serde_json::Value> = network
                .stops
                .keys()
                .map(|id| {
                    let s = &network.stops[id];
                    json!({ "stop_id": id, "name": s.name, "lat": s.lat, "lon": s.lon,
                        "parent": s.parent, "transfer": s.transfer })
                })
                .collect();
            stops.sort_by(|a, b| {
                a["name"].as_str().cmp(&b["name"].as_str())
                    .then(a["stop_id"].as_str().cmp(&b["stop_id"].as_str()))
            });
            (
                json!({ "op": "stops", "network_hash": ctx.network_hash, "limit": 3 }),
                json!({ "ok": { "op": "stops", "stops": stops.into_iter().take(3).collect::<Vec<_>>() } }),
            )
        }
        ("directory", "stops_by_ids") => {
            let ctx = net?;
            let network = ctx.derived_network();
            let pick = |id: &str| network.stops.get(id).map(|s| json!({
                "stop_id": id, "name": s.name, "lat": s.lat, "lon": s.lon,
                "parent": s.parent, "transfer": s.transfer }));
            (
                json!({ "op": "stops_by_ids", "network_hash": ctx.network_hash, "ids": ["A", "ghost", "STN"] }),
                json!({ "ok": { "op": "stops_by_ids", "stops": [
                    pick("A").expect("A exists"), pick("STN").expect("STN exists"),
                ] } }),
            )
        }
        ("directory", "routes") => {
            let ctx = net?;
            let network = ctx.derived_network();
            let mut routes: Vec<serde_json::Value> = network
                .routes
                .iter()
                .map(|(id, r)| json!({ "route_id": id, "name": r.name, "mode": r.mode, "color": r.color }))
                .collect();
            routes.sort_by(|a, b| {
                a["name"].as_str().cmp(&b["name"].as_str())
                    .then(a["route_id"].as_str().cmp(&b["route_id"].as_str()))
            });
            (
                json!({ "op": "routes", "network_hash": ctx.network_hash }),
                json!({ "ok": { "op": "routes", "routes": routes } }),
            )
        }
        ("directory", "focus") => {
            let ctx = net?;
            (
                json!({ "op": "focus", "network_hash": ctx.network_hash, "kind": "route", "id": "10" }),
                json!({ "ok": { "op": "focus",
                    "focus": transit_network::entity_focus(ctx.derived_network(), transit_network::EntityKind::Route, "10") } }),
            )
        }
        ("directory", "entities") => {
            let ctx = net?;
            (
                json!({ "op": "entities", "network_hash": ctx.network_hash, "kind": "pattern_trips", "query": "wk" }),
                json!({ "ok": { "op": "entities",
                    "rows": transit_network::entity_rows(ctx.derived_network(), transit_network::EntityKind::PatternTrips, "wk") } }),
            )
        }
        ("directory", "route_detail") => {
            let ctx = net?;
            (
                json!({ "op": "route_detail", "network_hash": ctx.network_hash, "route_id": "10" }),
                json!({ "ok": { "op": "route_detail",
                    "detail": transit_network::route_detail(ctx.derived_network(), "10") } }),
            )
        }
        ("directory", "timetable") => {
            let ctx = net?;
            (
                json!({ "op": "timetable", "network_hash": ctx.network_hash, "route_id": "10", "direction": "0", "service_type": "WK" }),
                json!({ "ok": { "op": "timetable",
                    "timetable": transit_network::timetable(ctx.derived_network(), "10", Some("0"), "WK") } }),
            )
        }
        ("directory", "stop_detail") => {
            let ctx = net?;
            (
                json!({ "op": "stop_detail", "network_hash": ctx.network_hash, "stop_id": "B" }),
                json!({ "ok": { "op": "stop_detail",
                    "detail": transit_network::stop_detail(ctx.derived_network(), "B") } }),
            )
        }
        ("directory", "route_stop_sequence") => {
            let ctx = net?;
            let network = ctx.derived_network();
            let patterns: Vec<serde_json::Value> = network
                .patterns
                .iter()
                .filter(|(_, p)| p.route == "10")
                .map(|(id, p)| json!({ "pattern": id, "name": p.name, "stops": p.stops.iter().map(|ps| json!({
                    "stop_id": ps.stop,
                    "name": network.stops.get(&ps.stop).map(|s| s.name.clone()).unwrap_or_else(|| ps.stop.clone()),
                })).collect::<Vec<_>>() }))
                .collect();
            (
                json!({ "op": "route_stop_sequence", "network_hash": ctx.network_hash, "route_id": "10" }),
                json!({ "ok": { "op": "route_stop_sequence", "patterns": patterns } }),
            )
        }
        ("streets", "snap") => {
            let ctx = streets_example_context()?;
            (
                json!({ "op": "snap", "streets_hash": ctx.streets_hash, "lat": 45.5040, "lon": -73.5030 }),
                json!({ "ok": { "op": "snap", "position": ctx.pos_a } }),
            )
        }
        ("streets", "route") => {
            let ctx = streets_example_context()?;
            (
                json!({ "op": "route", "streets_hash": ctx.streets_hash, "from": ctx.pos_a, "to": ctx.pos_b }),
                json!({ "ok": { "op": "route", "points": ctx.points, "length_m": ctx.length_m } }),
            )
        }
        // The sync capability's example computes the REAL converged
        // answer over the fixture edit document (draft client vs
        // empty server) through the same library the module
        // dispatches through.
        ("sync", "sync_edits") => {
            let mut draft = dispatch_fixture_edits();
            draft[0].status = transit_sync::EditStatus::Draft;
            let now_ms: u64 = 1_770_000_000_000;
            let records = transit_sync::submit(transit_sync::merge(&draft, &[]), now_ms);
            (
                json!({ "op": "sync_edits", "client": draft, "server": [], "now_ms": now_ms }),
                json!({ "ok": { "op": "sync_edits", "records": records } }),
            )
        }
        // The plan-chain capability's examples compute REAL answers
        // through the same shared library the module dispatches
        // through — a two-pick chain over the fixture's dates.
        // Authoring examples run the REAL edit through the shared
        // library: the fixture network stamped as a pick, the answer's
        // hash computed the way every module computes it.
        ("author", "load_network") => {
            let ctx = net?;
            (
                json!({ "op": "load_network", "network": ctx.derived }),
                json!({ "ok": { "op": "load_network", "network_hash": ctx.network_hash } }),
            )
        }
        ("author", "apply") => {
            let ctx = net?;
            let edits = vec![transit_network::NetworkEdit::SetPlan {
                plan: transit_network::PlanMeta {
                    id: "fall-2026".into(),
                    name: "Fall 2026".into(),
                    effective_from: chrono::NaiveDate::from_ymd_opt(2026, 10, 19),
                },
            }, transit_network::NetworkEdit::AddStop { id: None, lat: Some(45.5), lon: Some(-73.5) }];
            let (next, created) = transit_network::apply_edits_reporting(ctx.derived_network(), &edits).ok()?;
            let canonical = serde_json::to_string(&next).ok()?;
            (
                json!({ "op": "apply", "network_hash": ctx.network_hash, "edits": edits }),
                json!({ "ok": { "op": "apply",
                    "network_hash": format!("sha256:{:x}", Sha256::digest(canonical.as_bytes())),
                    "network": next,
                    "issues": transit_network::validation_issues(&next),
                    "warnings": transit_network::validation_warnings(&next),
                    "created": created } }),
            )
        }
        // Diversion-authoring examples run the REAL draft through the
        // shared libraries over the fixture network: route 10 anchored
        // end to end on its default pattern.
        ("divert", "load_network") => {
            let ctx = net?;
            (
                json!({ "op": "load_network", "network": ctx.derived }),
                json!({ "ok": { "op": "load_network", "network_hash": ctx.network_hash } }),
            )
        }
        ("divert", "draft") => {
            let ctx = net?;
            let draft = divert_example_draft(ctx.derived_network())?;
            (
                json!({ "op": "draft", "network_hash": ctx.network_hash, "draft": draft }),
                json!({ "ok": { "op": "draft", "view": transit_dispatch::draft_view(ctx.derived_network(), &draft) } }),
            )
        }
        ("divert", "list") => {
            let ctx = net?;
            let records = dispatch_fixture_edits();
            (
                json!({ "op": "list", "network_hash": ctx.network_hash, "records": records }),
                json!({ "ok": { "op": "list", "rows": transit_dispatch::diversion_rows(ctx.derived_network(), &records) } }),
            )
        }
        ("divert", "propose") => {
            let ctx = net?;
            let draft = divert_example_draft(ctx.derived_network())?;
            let now_ms: u64 = 1_770_000_000_000;
            let diversion = transit_dispatch::diversion_of(ctx.derived_network(), &draft, &format!("div-10-{now_ms}")).ok()?;
            (
                json!({ "op": "propose", "network_hash": ctx.network_hash, "draft": draft, "records": [], "record_id": "e2", "now_ms": now_ms }),
                json!({ "ok": { "op": "propose", "records": transit_sync::propose(&[], "e2", diversion, now_ms) } }),
            )
        }
        ("divert", op @ ("retire" | "restore")) => {
            let mut records = dispatch_fixture_edits();
            if op == "restore" {
                records = transit_sync::set_retired(&records, "e1", true, 1_770_000_000_000).ok()?;
            }
            let now_ms: u64 = 1_770_000_060_000;
            let next = transit_sync::set_retired(&records, "e1", op == "retire", now_ms).ok()?;
            (
                json!({ "op": op, "records": records, "record_id": "e1", "now_ms": now_ms }),
                json!({ "ok": { "op": op, "records": next } }),
            )
        }
        ("divert", "sanitize") => {
            let mut old = serde_json::to_value(&dispatch_fixture_edits()[0]).ok()?;
            for field in ["direction_id", "skipped_stops", "detour", "added_secs"] {
                old["diversion"].as_object_mut()?.remove(field);
            }
            let stored = json!([old, { "id": "e3", "status": "draft" }]);
            let (records, dropped) = transit_sync::sanitize(&stored);
            (
                json!({ "op": "sanitize", "records": stored }),
                json!({ "ok": { "op": "sanitize", "records": records, "dropped": dropped } }),
            )
        }
        ("plans", "resolve") => {
            let plan_set = fixture_plan_set();
            let plan = transit_plans::resolve(
                &plan_set,
                chrono::NaiveDate::from_ymd_opt(2026, 10, 23).unwrap(),
            )
            .expect("fixture dates parse");
            (
                json!({ "op": "resolve", "plans": plan_set, "date": "2026-10-23" }),
                json!({ "ok": { "op": "resolve", "plan": plan } }),
            )
        }
        ("plans", "validate") => {
            let plan_set = fixture_plan_set();
            (
                json!({ "op": "validate", "plans": plan_set }),
                json!({ "ok": { "op": "validate", "issues": transit_plans::validate(&plan_set) } }),
            )
        }
        ("plans", "periods") => {
            let plan_set = fixture_plan_set();
            (
                json!({ "op": "periods", "plans": plan_set }),
                json!({ "ok": { "op": "periods", "periods": transit_plans::periods(&plan_set) } }),
            )
        }
        // Bulk-bytes ops have no JSON request example (the payload is
        // the raw zip/protobuf); the description carries the contract.
        (_, "load_feed") => return None,
        (_, "load_population_layer") => return None,
        (_, "load_streets") => return None,
        _ => return None,
    };
    Some(OpExample { request, response })
}

/// The plan-chain examples' plan set: a two-pick chain around the
/// fixture's dates — the spring pick in force on the fixture Friday,
/// the fall pick taking over after it.
fn fixture_plan_set() -> Vec<transit_plans::PlanInfo> {
    vec![
        transit_plans::PlanInfo {
            id: "spring".into(),
            name: "Spring 2026".into(),
            effective_from: Some("2026-03-01".into()),
        },
        transit_plans::PlanInfo {
            id: "fall".into(),
            name: "Fall 2026".into(),
            effective_from: Some("2026-09-07".into()),
        },
    ]
}

/// The dispatch examples' edit document: the fixture diversion
/// (route 10 keeps A→B, skips C, +300 s on the fixture Friday).
/// The divert examples' draft: route 10 kept end to end on its
/// default pattern, five minutes added, on the fixture Friday.
fn divert_example_draft(network: &transit_network::Network) -> Option<transit_dispatch::DiversionDraft> {
    let mut draft = transit_dispatch::DiversionDraft {
        route_id: "10".into(),
        pattern: None,
        first_kept_stop: None,
        last_kept_stop: None,
        served_stops: vec![],
        slices: vec![],
        added_mins: 5.0,
        start_date: "2026-10-23".into(),
        end_date: "2026-10-23".into(),
    };
    let stops = transit_dispatch::draft_view(network, &draft).stops;
    draft.first_kept_stop = Some(stops.first()?.stop_id.clone());
    draft.last_kept_stop = Some(stops.last()?.stop_id.clone());
    Some(draft)
}

fn dispatch_fixture_edits() -> Vec<transit_sync::EditRecord> {
    let date = chrono::NaiveDate::from_ymd_opt(2026, 10, 23).unwrap();
    vec![transit_sync::EditRecord {
        id: "e1".into(),
        diversion: transit_sync::Diversion {
            id: "d1".into(),
            route_id: "10".into(),
            direction_id: None,
            first_kept_stop: "A".into(),
            last_kept_stop: "B".into(),
            skipped_stops: vec!["C".into()],
            detour: None,
            added_secs: 300,
            start_date: date,
            end_date: date,
        },
        status: transit_sync::EditStatus::Accepted,
        rev: 1,
        updated_at: 1,
    }]
}
