//! gen-manifest: assemble the capability manifest after the wasm
//! builds (`just caps`) and PUBLISH the registry (`just registry`).
//! Reads each capability module, pins it by content hash, serves it
//! under that hash, and writes both surfaces hosts and agents read:
//!
//! * `web/public/caps/` — the browser's manifest + hash-addressed
//!   module files (the assembly the bus worker loads);
//! * `registry/` — the REGISTRY: immutable blobs, one evidence record
//!   per capability (contracts + agent context + which gates ran when
//!   these exact bytes were published), and a registry pointer. An
//!   agent (or a human dogfooding `caps run`) pulls compute from
//!   here and verifies the bytes against the record. Beside them, the
//!   DOCUMENTS: every document kind's JSON Schema, generated from its
//!   serde type, and a worked example computed from the fixtures
//!   (`registry/documents.json` indexes them by kind, store kind and
//!   schema version) — what lets a reader with no Rust or wasm read a
//!   stored blob (docs/FORMATS.md).
//!
//! Before writing, the manifest is run through the compose-time
//! validator (cap_abi::validate): a manifest this crate assembles
//! must be provably coherent, and the generation is the first
//! consumer of that proof.

use std::fs;
use std::path::Path;
use std::process::Command;

use cap_abi::manifest::{CapManifest, Capability, KernelOp, LinkedAssembly, MigrationSpec, OpDoc, OpExample};

#[path = "common/assemblies.rs"]
mod common;
#[path = "common/schema_lock.rs"]
mod schema_lock;
#[path = "common/kind_table.rs"]
mod kind_table;
use cap_abi::validate::validate_assembly;
use sha2::{Digest as _, Sha256};

/// The gates `just registry` runs before publishing (it says so with
/// `CAPS_PUBLISH_GATED=1`; a plain `just caps` publishes ungated and
/// the record says that instead). The recipe is the authority; the
/// record is the honest report of it.
const PUBLISH_GATES: &[&str] = &[
    "cargo test (workspace, includes the capability law suites)",
    "node crates/cap-abi/caps-smoke.mjs (ABI + composition + context)",
    "just reproducible (two clean builds of the tree, byte for byte)",
];

/// The document-kernel version this build speaks.
const KERNEL: u32 = 1;

/// The capability set this deployment assembles, mirroring the crates:
/// what each reads and in which versions (`speaks`, the versions the
/// owning crates read), and the version steps it carries (`migrates`:
/// cap-migrate's alone). When a crate's Request enum
/// changes, this changes — and the manifest diff is the reviewable
/// assembly change.
struct ModuleSpec {
    id: &'static str,
    file: &'static str,
    ops: &'static [&'static str],
    /// The kernel document kinds it reads (`capability! { documents }`):
    /// their loads are kernel ops in every module that links it. The
    /// smoke checks this against the module's own `about`.
    documents: &'static [&'static str],
    consumes: &'static [(&'static str, &'static str)],
    speaks: &'static [(&'static str, &'static [u32])],
    migrates: &'static [(&'static str, &'static [(u32, u32)])],
    /// Agent context, from the capability's OPS.md: what it is FOR
    /// (under the `#` heading), and one `##` section per op — the
    /// op's description; its worked example is computed here
    /// (`example_for`: the REAL fixture's hashes, always current).
    docs: &'static str,
}

const MODULES: &[ModuleSpec] = &[
    ModuleSpec {
        id: "gtfs",
        file: "module_gtfs.wasm",
        ops: &["feed_info"],
        documents: &["feed"],
        consumes: &[],
        speaks: &[("feed", &[1])],
        migrates: &[],
        // Its purpose and per-op agent context: crates/cap-gtfs/OPS.md
        // (a `#` heading, the purpose, then one `##` section per op).
        docs: include_str!("../../../cap-gtfs/OPS.md"),
    },
    ModuleSpec {
        id: "router",
        file: "module_router.wasm",
        ops: &["plan", "plan_arrive_by", "plan_compare", "itinerary_paths"],
        documents: &["network", "edits", "live", "walks"],
        consumes: &[("network", "network")],
        speaks: &[("network", &[transit_network::SCHEMA_VERSION]), ("live", &[1]), ("walks", &[1])],
        migrates: &[],
        // Its purpose and per-op agent context: crates/cap-router/OPS.md
        // (a `#` heading, the purpose, then one `##` section per op).
        docs: include_str!("../../../cap-router/OPS.md"),
    },
    ModuleSpec {
        id: "network",
        file: "module_network.wasm",
        ops: &["import", "validate", "export", "document", "publishable", "stored_plans"],
        documents: &["feed", "network", "boundary"],
        consumes: &[],
        speaks: &[("feed", &[1]), ("network", &[transit_network::SCHEMA_VERSION]), ("boundary", &[1])],
        migrates: &[],
        // Its purpose and per-op agent context: crates/cap-network/OPS.md
        // (a `#` heading, the purpose, then one `##` section per op).
        docs: include_str!("../../../cap-network/OPS.md"),
    },
    ModuleSpec {
        id: "rt",
        file: "module_rt.wasm",
        ops: &["decode", "trip_progress", "late_trips", "live", "encode_alerts", "read_alerts"],
        documents: &["network", "boundary", "rt", "live", "service_alerts"],
        consumes: &[("network", "network")],
        // The gtfs-rt payload: an ephemeral document (polled every
        // few seconds), versioned by the protobuf schema it speaks;
        // it joins against the NETWORK (an rt trip id is a model trip
        // id), with the import's boundary for the published
        // stop_sequence numbering when there is one.
        // THE LIVE DOCUMENT is minted here and read by the board —
        // optional edges both: neither module needs the other to
        // answer its own questions, so they are spoken, not consumed.
        // encode_alerts writes the service alerts document as an Alerts
        // feed: GTFS-rt is its serialization.
        speaks: &[("gtfs_rt", &[1]), ("network", &[transit_network::SCHEMA_VERSION]), ("boundary", &[1]), ("live", &[1]), ("service_alerts", &[transit_alerts::ALERTS_VERSION])],
        migrates: &[],
        // Its purpose and per-op agent context: crates/cap-rt/OPS.md
        // (a `#` heading, the purpose, then one `##` section per op).
        docs: include_str!("../../../cap-rt/OPS.md"),
    },
    ModuleSpec {
        id: "analysis",
        file: "module_analysis.wasm",
        ops: &["network_stats", "service_types_overview", "service_groups", "service_type_detail", "service_type_compare", "stop_gaps", "hourly_trips", "schedule_texture", "route_overlap", "timed_transfers", "hub_connections", "missed_connections", "longest_waits", "day_types", "plan_compare", "network_findings", "route_frequency", "type_coverage", "unserved_points", "coverage_compare"],
        documents: &["network", "layer"],
        // The network document is produced by the network capability —
        // the extraction this module's own docs once called "future".
        consumes: &[("network", "network")],
        speaks: &[("network", &[transit_network::SCHEMA_VERSION])],
        migrates: &[],
        // Its purpose and per-op agent context: crates/cap-analysis/OPS.md
        // (a `#` heading, the purpose, then one `##` section per op).
        docs: include_str!("../../../cap-analysis/OPS.md"),
    },
    ModuleSpec {
        id: "board",
        file: "module_board.wasm",
        ops: &["stop_board", "grouped_board", "alert_reach", "watchlist_board"],
        documents: &["network", "edits", "live", "watchlist"],
        // The network document is produced by the network capability;
        // the operator edit document is host-owned (wholesale-replaced
        // — the sync contract); the watchlist is the PERSONAL document
        // it reads in the current version (cap-migrate migrates it).
        consumes: &[("network", "network")],
        speaks: &[("network", &[transit_network::SCHEMA_VERSION]), ("operator_edits", &[transit_sync::SCHEMA_VERSION]), ("watchlist", &[transit_rider::watchlist::SCHEMA_VERSION]), ("live", &[1])],
        migrates: &[],
        // Its purpose and per-op agent context: crates/cap-board/OPS.md
        // (a `#` heading, the purpose, then one `##` section per op).
        docs: include_str!("../../../cap-board/OPS.md"),
    },
    ModuleSpec {
        id: "runcut",
        file: "module_runcut.wasm",
        ops: &["blocks", "fleet_summary", "block_trips", "trip_stops", "trip_path", "recovery", "suggest_blocks"],
        documents: &["network", "edits", "live", "work_rules"],
        // The network document is produced by the network capability;
        // the operator edit document is host-owned (wholesale-replaced
        // — the sync contract); the live document is the rt
        // capability's, which the chain's ripple reads; the work rules
        // are host-owned (transit-runs), read by the recovery view and
        // the suggested blocking.
        consumes: &[("network", "network")],
        speaks: &[("network", &[transit_network::SCHEMA_VERSION]), ("operator_edits", &[transit_sync::SCHEMA_VERSION]), ("live", &[1]), ("work_rules", &[1])],
        migrates: &[],
        // Its purpose and per-op agent context: crates/cap-runcut/OPS.md
        // (a `#` heading, the purpose, then one `##` section per op).
        docs: include_str!("../../../cap-runcut/OPS.md"),
    },
    ModuleSpec {
        id: "calendar",
        file: "module_calendar.wasm",
        ops: &["active_services", "service_dates"],
        documents: &["network", "edits"],
        // The network document is produced by the network capability;
        // the operator edit document is host-owned (wholesale-replaced
        // — the sync contract).
        consumes: &[("network", "network")],
        speaks: &[("network", &[transit_network::SCHEMA_VERSION]), ("operator_edits", &[transit_sync::SCHEMA_VERSION])],
        migrates: &[],
        // Its purpose and per-op agent context: crates/cap-calendar/OPS.md
        // (a `#` heading, the purpose, then one `##` section per op).
        docs: include_str!("../../../cap-calendar/OPS.md"),
    },
    ModuleSpec {
        id: "directory",
        file: "module_directory.wasm",
        ops: &["about", "stops", "stops_near", "stops_by_ids", "routes", "route_stop_sequence", "focus", "route_lines", "service_keys", "entities", "route_detail", "stop_detail", "timetable"],
        documents: &["network"],
        // The network document is produced by the network capability.
        consumes: &[("network", "network")],
        speaks: &[("network", &[transit_network::SCHEMA_VERSION])],
        migrates: &[],
        // Its purpose and per-op agent context: crates/cap-directory/OPS.md
        // (a `#` heading, the purpose, then one `##` section per op).
        docs: include_str!("../../../cap-directory/OPS.md"),
    },
    ModuleSpec {
        id: "streets",
        file: "module_streets.wasm",
        ops: &["import", "walks", "access", "snap", "route"],
        documents: &["streets", "network"],
        // The streets document is the HOST's (read from a street
        // layer by import, answered with its hash); every query
        // addresses that hash. The walk table reads the network's
        // stops.
        consumes: &[("network", "network")],
        speaks: &[("streets", &[1]), ("network", &[transit_network::SCHEMA_VERSION]), ("walks", &[1])],
        migrates: &[],
        // Its purpose and per-op agent context: crates/cap-streets/OPS.md
        // (a `#` heading, the purpose, then one `##` section per op).
        docs: include_str!("../../../cap-streets/OPS.md"),
    },
    ModuleSpec {
        id: "sync",
        file: "module_sync.wasm",
        ops: &["sync_edits", "read_edits", "compact_edits", "log_records", "log_document", "log_revert"],
        documents: &[],
        consumes: &[],
        speaks: &[("operator_edits", &[transit_sync::SCHEMA_VERSION])],
        migrates: &[],
        // Its purpose and per-op agent context: crates/cap-sync/OPS.md
        // (a `#` heading, the purpose, then one `##` section per op).
        docs: include_str!("../../../cap-sync/OPS.md"),
    },
    ModuleSpec {
        id: "log",
        file: "module_log.wasm",
        ops: &["compose", "fold", "revert", "close_gap"],
        documents: &[],
        // The log is the host's to store (D-190, D-197): compose answers
        // each write's entry, fold the records and a checkpoint. It never
        // reads a record: each kind's capability turns its document into
        // records and back (log_records, log_document).
        consumes: &[],
        speaks: &[("log_entry", &[transit_log::ENTRY_VERSION]), ("log_checkpoint", &[transit_log::CHECKPOINT_VERSION])],
        migrates: &[],
        // Its purpose and per-op agent context: crates/cap-log/OPS.md.
        docs: include_str!("../../../cap-log/OPS.md"),
    },
    ModuleSpec {
        id: "authz",
        file: "module_authz.wasm",
        ops: &["authorize", "effective", "validate"],
        documents: &[],
        consumes: &[],
        speaks: &[("policy", &[1])],
        migrates: &[],
        // Its purpose and per-op agent context: crates/cap-authz/OPS.md
        // (a `#` heading, the purpose, then one `##` section per op).
        docs: include_str!("../../../cap-authz/OPS.md"),
    },
    ModuleSpec {
        id: "author",
        file: "module_author.wasm",
        ops: &["apply", "stamp_pick"],
        documents: &["network", "work_rules"],
        // Authoring reads and MINTS network documents — the same
        // identity the codec's import produces and every model-side
        // capability consumes.
        consumes: &[("network", "network")],
        speaks: &[("network", &[transit_network::SCHEMA_VERSION]), ("work_rules", &[1])],
        migrates: &[],
        // Its purpose and per-op agent context: crates/cap-author/OPS.md
        // (a `#` heading, the purpose, then one `##` section per op).
        docs: include_str!("../../../cap-author/OPS.md"),
    },
    ModuleSpec {
        id: "divert",
        file: "module_divert.wasm",
        ops: &["draft", "list", "reports", "propose", "cancel", "retire", "restore"],
        documents: &["network"],
        // A draft is answered over the network the codec's import
        // produces; the answers are next states of the host-owned
        // operator edit document.
        consumes: &[("network", "network")],
        speaks: &[("network", &[transit_network::SCHEMA_VERSION]), ("operator_edits", &[transit_sync::SCHEMA_VERSION])],
        migrates: &[],
        // Its purpose and per-op agent context: crates/cap-divert/OPS.md
        // (a `#` heading, the purpose, then one `##` section per op).
        docs: include_str!("../../../cap-divert/OPS.md"),
    },
    ModuleSpec {
        id: "plans",
        file: "module_plans.wasm",
        ops: &["resolve", "validate", "timeline", "periods", "publish", "in_force", "in_use", "retire", "freeze", "adopt", "log_records", "log_document"],
        documents: &[],
        consumes: &[],
        speaks: &[("plans", &[1]), ("frozen_versions", &[1])],
        migrates: &[],
        // Its purpose and per-op agent context: crates/cap-plans/OPS.md
        // (a `#` heading, the purpose, then one `##` section per op).
        docs: include_str!("../../../cap-plans/OPS.md"),
    },
    ModuleSpec {
        id: "stops",
        file: "module_stops.wasm",
        ops: &["validate", "stops_on", "stops_near", "stop", "seed", "add_event", "correct_event", "vocabulary", "read_records", "records", "take_stops", "read_pick", "visit_warnings", "export_versions", "export_pick", "import_events", "questions", "attribute_surveys", "feed_events", "log_records", "log_document"],
        documents: &["stops", "network"],
        // The stops document is the host's (beside the plans, D-147):
        // validate answers its canonical bytes and hash, which the
        // kernel's load_stops answers too; the date questions read it.
        // seed reads a network's stops to begin the document; add_event
        // answers the next one. A pick only reads it (D-157).
        consumes: &[("network", "network")],
        speaks: &[("stops", &[transit_stops::SCHEMA_VERSION]), ("network", &[transit_network::SCHEMA_VERSION])],
        migrates: &[],
        // Its purpose and per-op agent context: crates/cap-stops/OPS.md
        // (a `#` heading, the purpose, then one `##` section per op).
        docs: include_str!("../../../cap-stops/OPS.md"),
    },
    ModuleSpec {
        id: "requests",
        file: "module_requests.wasm",
        ops: &["validate", "requests_for", "list", "put", "work_order_call", "record_work_order", "vocabulary", "log_records", "log_document"],
        documents: &["stop_requests"],
        // The stop requests document is the host's (D-159): validate
        // answers its canonical bytes and hash, which the kernel's
        // load_stop_requests answers too; put answers the next one.
        consumes: &[],
        speaks: &[("stop_requests", &[transit_requests::SCHEMA_VERSION])],
        migrates: &[],
        // Its purpose and per-op agent context: crates/cap-requests/OPS.md.
        docs: include_str!("../../../cap-requests/OPS.md"),
    },
    ModuleSpec {
        id: "alerts",
        file: "module_alerts.wasm",
        ops: &["validate", "put", "end", "list", "flags", "validate_templates", "put_template", "templates", "fill", "receive", "log_records", "log_document"],
        documents: &["service_alerts", "alert_templates", "network"],
        // The service alerts and alert templates documents are the
        // host's (D-206, D-210): validate answers their canonical bytes
        // and hash, which the kernel's loads answer too; put, end, and
        // put_template answer the next one. flags and fill read the
        // network's names.
        consumes: &[],
        speaks: &[
            ("service_alerts", &[transit_alerts::ALERTS_VERSION]),
            ("alert_templates", &[transit_alerts::TEMPLATES_VERSION]),
            ("network", &[transit_network::SCHEMA_VERSION]),
        ],
        migrates: &[],
        // Its purpose and per-op agent context: crates/cap-alerts/OPS.md.
        docs: include_str!("../../../cap-alerts/OPS.md"),
    },
    ModuleSpec {
        id: "notes",
        file: "module_notes.wasm",
        ops: &["validate", "write", "on", "between", "receive", "log_records", "log_document"],
        documents: &["notes"],
        // The notes document is the host's (D-214): validate answers its
        // canonical bytes and hash, which the kernel's load_notes answers
        // too; write answers the next one.
        consumes: &[],
        speaks: &[("notes", &[transit_notes::SCHEMA_VERSION])],
        migrates: &[],
        // Its purpose and per-op agent context: crates/cap-notes/OPS.md.
        docs: include_str!("../../../cap-notes/OPS.md"),
    },
    ModuleSpec {
        id: "disruptions",
        file: "module_disruptions.wasm",
        ops: &["today", "gaps", "start_alert"],
        documents: &["network", "edits", "live", "stops", "service_alerts", "notes"],
        // What is disrupted on a day, and whether an alert explains it
        // (D-218): it reads every document a disruption, its alert, or
        // its note lives in, and writes none.
        consumes: &[],
        speaks: &[
            ("network", &[transit_network::SCHEMA_VERSION]),
            ("operator_edits", &[transit_sync::SCHEMA_VERSION]),
            ("live", &[2]),
            ("stops", &[transit_stops::SCHEMA_VERSION]),
            ("service_alerts", &[transit_alerts::ALERTS_VERSION]),
            ("notes", &[transit_notes::SCHEMA_VERSION]),
        ],
        migrates: &[],
        // Its purpose and per-op agent context: crates/cap-disruptions/OPS.md.
        docs: include_str!("../../../cap-disruptions/OPS.md"),
    },
    ModuleSpec {
        id: "migrate",
        file: "module_migrate.wasm",
        ops: &["migrate", "version"],
        documents: &[],
        consumes: &[],
        speaks: &[
            ("network", &[1, transit_network::SCHEMA_VERSION]),
            ("operator_edits", &[1, transit_sync::SCHEMA_VERSION]),
            ("watchlist", &[1, 2]),
            ("service_alerts", &[1, transit_alerts::ALERTS_VERSION]),
        ],
        migrates: &[
            ("network", &[(1, 2)]),
            ("operator_edits", &[(1, 2)]),
            ("watchlist", &[(1, 2)]),
            ("service_alerts", &[(1, 2)]),
        ],
        // Its purpose and per-op agent context: crates/cap-migrate/OPS.md
        // (a `#` heading, the purpose, then one `##` section per op).
        docs: include_str!("../../../cap-migrate/OPS.md"),
    },
];

/// The kernel ops, documented once for every module. The registry
/// ops' examples are COMPUTED: two registries (one durable, one
/// keep-last-N) answer them through the same kernel the macro wires
/// into every module.
/// The kernel ops — the same in every module that answers them: each
/// document kind's load (in every module whose capabilities read the
/// kind), the registry ops and `about` (in every module).
fn kernel_ops(feed_hash: &str, net: Option<&NetworkExampleContext>) -> Vec<KernelOp> {
    use cap_abi::kernel::{self, Cap, NETWORK, RT};
    use cap_abi::registry::{OP_EVICT, OP_HELD};
    // The registry ops' examples run the REAL kernel over an example
    // capability reading networks and rt snapshots.
    static EXAMPLE: Cap = Cap { id: "board", json: |_| String::new(), bytes: |_, _| String::new(), documents: &[&NETWORK, &RT] };
    let answer = |text: String| serde_json::from_str::<serde_json::Value>(&text).expect("the kernel answers JSON");
    let rt_bytes = fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../../web/e2e/fixtures/sample-rt.pb")).expect("rt fixture");
    if let Some(ctx) = net {
        NETWORK.register(ctx.network.clone()).expect("the fixture network registers");
    }
    let (rt_hash, _) = RT.register_bytes(&rt_bytes).expect("the rt fixture decodes");
    let held = answer(kernel::invoke_bytes(&[&EXAMPLE], OP_HELD, b""));
    let keep = serde_json::json!({ "keep": [rt_hash] });
    let evicted = answer(kernel::invoke_bytes(&[&EXAMPLE], OP_EVICT, keep.to_string().as_bytes()));
    let about = answer(kernel::invoke_bytes(&[&EXAMPLE], kernel::OP_ABOUT, b""));

    // A load's kind and code are its line's in the kind list.
    let load = |code: u32, name: &str, description: &str| {
        let line = cap_abi::kinds::KINDS.iter().find_map(|k| k.load.filter(|l| l.op == name));
        let line = line.unwrap_or_else(|| panic!("the kernel load {name} has no line in crates/cap-abi/src/kinds.rs"));
        assert_eq!(line.code, code, "{name}: the kind list's code");
        KernelOp {
            code,
            name: name.into(),
            kind: Some(line.id.into()),
            description: description.into(),
            example: example_for("kernel", name, feed_hash, net),
        }
    };
    vec![
        load(cap_abi::OP_LOAD_NETWORK, "load_network", "Register the NETWORK document under its content hash (idempotent) — one identity in every module that reads networks, registered once per instance however many of its capabilities read it. JSON: {\"op\": \"load_network\", \"network\": …}. Bulk: the bare document's JSON bytes, or those bytes gzipped (decoded as they parse — the text never exists whole in the module) — the same answer and hash, no request envelope; hosts send real-size networks this way, gzipped."),
        load(kernel::OP_LOAD_EDITS, "load_edits", "Register the OPERATOR EDIT document ({\"kind\": \"edit_records\", \"schema_version\", \"records\"} — wholesale-replaced by the host: push the whole document on every change) under its canonical hash, which the day views address. Only the version this module reads registers: an older document is refused naming the migration capability, a newer one naming both versions. A record the network cannot answer simply patches nothing — what each record amounts to on a network is the divert capability's reports. JSON: {\"op\": \"load_edits\", \"edits\": {…}}; bulk: the document's JSON."),
        load(kernel::OP_LOAD_LIVE, "load_live", "Register a LIVE document (the rt capability's live answer) under its canonical hash — the same identity rt answered (in a module linking rt, its live answer is registered already). Ephemeral: the last 4 stay. JSON: {\"op\": \"load_live\", \"live\": …}; bulk: its JSON."),
        load(kernel::OP_LOAD_BOUNDARY, "load_boundary", "Register an imported network's BOUNDARY (the network capability's import answer) under its canonical hash: the stop_sequence numbering the published feed — and so its rt updates — use. A network published from its own export needs none (1..n). JSON: {\"op\": \"load_boundary\", \"boundary\": …}; bulk: its JSON."),
        load(kernel::OP_LOAD_WALKS, "load_walks", "Register a WALK TABLE (the streets capability's walks answer: every walk between the network's stops along the streets) under its canonical hash — the same identity streets answered. A plan naming it by walks_hash walks its transfers along the streets; a table derived for another network is refused there. JSON: {\"op\": \"load_walks\", \"walks\": …}; bulk: its JSON."),
        load(kernel::OP_LOAD_STREETS, "load_streets", "Register a STREETS document the host holds (an import's answer, stored) under its hash — the same hash its import answered — with its size (ways, segments, distinct points). Refused unless canonical (ways of two or more points, sorted, none twice, two-ways in canonical direction). JSON: {\"op\": \"load_streets\", \"streets\": …}; bulk: the bare document's JSON bytes."),
        load(kernel::OP_LOAD_WATCHLIST, "load_watchlist", "Register a WATCHLIST document (the personal saved set, version-tagged) under its canonical hash, answering its version. JSON: {\"op\": \"load_watchlist\", \"document\": …}; bulk: its JSON."),
        load(kernel::OP_LOAD_LAYER, "load_layer", "Register a GeoJSON WEIGHTED LAYER — Point, Polygon and MultiPolygon features (people, jobs, households: any weighted geography; other geometries are skipped) — under the content hash of its bytes. The answer is the import handle — features, points, polygons, and the weights it can be read by (each property that is a JSON number, with its total) — so the layer's owner never parses a second copy. Which field is the weight, and what it means, are the coverage op's (`field`, `label`): one file read two ways is one layer. Bulk: the GeoJSON bytes; JSON: {\"op\": \"load_layer\", \"bytes\": [..]}."),
        load(kernel::OP_LOAD_FEED, "load_feed", "Register a GTFS zip under the content hash of its bytes (idempotent), answering the feed's summary — stops, routes, trips, stop_times — the at-a-glance read the feed line shows. Bulk: the raw zip."),
        load(kernel::OP_LOAD_WORK_RULES, "load_work_rules", "Register the WORK RULES (transit-runs: the rules work is cut under — recovery per route and/or service type in minutes and/or a percent of the run, the route pairs one vehicle may interline, the run rules, pay; every rule optional, D-50) under its canonical hash, answering its version. Checked and normalized at the load (rules and pairs sorted, each pair's routes in order — two orderings are one identity); refused: another version, a rule requiring nothing, one scope twice, a share over 1000 percent, a pair of one route or listed twice, a minimum piece over the maximum. JSON: {\"op\": \"load_work_rules\", \"work_rules\": …}; bulk: its JSON."),
        load(kernel::OP_LOAD_STOPS, "load_stops", "Register a STOPS document (transit-stops: every stop a list of dated events — planned, permitted, built, in_service, suspended, moved, changed, retired; D-153, D-157) under its canonical hash — the same hash the stops capability's validate answers — with how many stops it holds. Refused unless validate would pass it, every refusal named. JSON: {\"op\": \"load_stops\", \"stops\": …}; bulk: its JSON."),
        load(kernel::OP_LOAD_STOP_REQUESTS, "load_stop_requests", "Register a STOP REQUESTS document (transit-requests: the planning team's requests for work at a stop — new, reinstall, relocate/remove, repair/improve, temporary — each tracking the work order the work order system opens for it; D-159) under its canonical hash — the same hash the requests capability's validate answers — with how many requests it holds. Refused unless validate would pass it, every refusal named. JSON: {\"op\": \"load_stop_requests\", \"requests\": …}; bulk: its JSON."),
        load(kernel::OP_LOAD_SERVICE_ALERTS, "load_service_alerts", "Register a SERVICE ALERTS document (transit-alerts: the agency's alerts in the network's ids, each with its cause, effect, severity, texts in each language, periods, and informed entities; D-206) under its canonical hash, the same hash the alerts capability's validate answers, with how many alerts it holds. Refused unless validate would pass it, every refusal named. JSON: {\"op\": \"load_service_alerts\", \"alerts\": …}; bulk: its JSON."),
        load(kernel::OP_LOAD_ALERT_TEMPLATES, "load_alert_templates", "Register an ALERT TEMPLATES document (transit-alerts: the agency's standard language for alerts, with blanks; D-210) under its canonical hash, the same hash the alerts capability's validate_templates answers, with how many templates it holds. Refused unless validate_templates would pass it, every refusal named. JSON: {\"op\": \"load_alert_templates\", \"templates\": …}; bulk: its JSON."),
        load(kernel::OP_LOAD_NOTES, "load_notes", "Register a NOTES document (transit-notes: the agency's internal notes, each about a trip instance, an edit record, a service alert, or a stop, with who wrote it and when; D-214) under its canonical hash, the same hash the notes capability's validate answers, with how many notes it holds. Refused unless validate would pass it, every refusal named. JSON: {\"op\": \"load_notes\", \"notes\": …}; bulk: its JSON."),
        load(kernel::OP_LOAD_RT, "load_rt", "Register a GTFS-rt protobuf payload (a snapshot) under the content hash of its bytes, answering the hash only (the rt capability's decode answers the overview too). Ephemeral: the last 8 stay. Bulk: the raw payload."),
        KernelOp {
            code: OP_EVICT,
            name: "evict".into(),
            kind: None,
            description: "EVERY module. Drop everything the keep-set does not name, answering what dropped and what is held after. The bytes are {\"keep\": [hash, …]}: a document stays when its hash is kept; a derived entry (a held day, the router's graph, a street graph) stays only while every document it was computed from is kept. The host names what survives — it owns the documents; the module enforces — it is the hash authority. Asking for an evicted document is the unknown-hash error (`unknown <kind> <hash>: <load op> first`), never a wrong answer: register it again. Ephemeral kinds also bound themselves with no host steering — rt snapshots keep their last 8, live documents their last 4 — and so does each derivation (held days 4, router graphs 8, street and walking graphs 4).".into(),
            example: Some(OpExample { request: keep, response: evicted }),
        },
        KernelOp {
            code: OP_HELD,
            name: "held".into(),
            kind: None,
            description: "EVERY module. What the module holds, per registry (each document kind its capabilities read, then the derived entries), oldest first (the bytes are ignored) — the inspection a memory gate reads.".into(),
            example: Some(OpExample { request: serde_json::json!(null), response: held }),
        },
        KernelOp {
            code: kernel::OP_ABOUT,
            name: "about".into(),
            kind: None,
            description: "EVERY module. What the module is: the capabilities it links in index order (an assembly's capability bulk op carries its index in the high 16 bits; a JSON request names its capability as \"cap\" when the module links more than one), each with the document kinds it reads (the bytes are ignored).".into(),
            example: Some(OpExample { request: serde_json::json!(null), response: about }),
        },
    ]
}

/// A capability's OPS.md, read: the purpose (the text under the `# <id>`
/// heading) and each op's description (the text under its `## <op>`
/// heading), in the file's order. The file is the docs' one home; the
/// generator refuses one whose heading is not the capability's id, or
/// whose ops are not exactly the capability's `ops`, in order.
struct CapDocs {
    purpose: String,
    ops: Vec<(String, String)>,
}

fn read_docs(module: &ModuleSpec) -> CapDocs {
    let text = module.docs;
    let place = format!("crates/cap-{}/OPS.md", module.id);
    let mut sections: Vec<(String, Vec<&str>)> = Vec::new();
    for line in text.lines() {
        if let Some(h) = line.strip_prefix("## ") {
            sections.push((h.trim().to_string(), Vec::new()));
        } else if let Some(h) = line.strip_prefix("# ") {
            assert!(sections.is_empty(), "{place}: one `#` heading, first");
            assert_eq!(h.trim(), module.id, "{place}: the `#` heading names the capability");
            sections.push((String::new(), Vec::new()));
        } else {
            sections.last_mut().unwrap_or_else(|| panic!("{place}: starts with `# {}`", module.id)).1.push(line);
        }
    }
    let body = |lines: &[&str]| lines.join("\n").trim_matches('\n').to_string();
    let mut it = sections.into_iter();
    let (head, purpose) = it.next().unwrap_or_else(|| panic!("{place}: empty"));
    assert!(head.is_empty(), "{place}: starts with `# {}`", module.id);
    let ops: Vec<(String, String)> = it.map(|(op, lines)| (op, body(&lines))).collect();
    let names: Vec<&str> = ops.iter().map(|(op, _)| op.as_str()).collect();
    assert_eq!(names, module.ops, "{place}: one `## <op>` section per op, in `ops`' order");
    for (op, d) in &ops {
        assert!(!d.is_empty(), "{place}: `## {op}` says nothing");
    }
    CapDocs { purpose: body(&purpose), ops }
}

fn main() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().parent().unwrap();
    // The cargo profile the modules were built under (`just caps`:
    // release; `just caps-fast`: wasm-fast, the iteration loop's).
    let profile = std::env::var("CAPS_PROFILE").unwrap_or_else(|_| "release".into());
    let target = root.join("target/wasm32-unknown-unknown").join(&profile);
    let dist = root.join("web/public/caps");

    // The demo feed: the e2e fixture, served statically so dev and
    // e2e run the identical demo — and hashed now, so the worked
    // examples in op_docs carry the REAL document identity.
    let fixture = root.join("web/e2e/fixtures/sample-gtfs.zip");
    let feed_bytes = fs::read(&fixture).expect("fixture exists");
    let feed_hex = format!("{:x}", Sha256::digest(&feed_bytes));
    let feed_hash = format!("sha256:{feed_hex}");

    // The network capability's worked examples are computed from the
    // REAL fixture: import derives the document, export produces the
    // zip — both through the shared transit-codec library, so the
    // examples are exactly what the module returns and cannot drift.
    let net_ctx = network_example_context(&feed_bytes);

    // The schema lock (docs/FORMATS.md, "The schema lock"), before
    // anything is written: `--lock` (`just schema-lock`) records what
    // this build adds and stops; a plain run fails on any drift.
    let documents = document_kinds(net_ctx.as_ref(), &feed_hash);
    let locking = std::env::args().any(|a| a == "--lock");
    let locks = lock_documents(root, &documents, locking);
    let (table, table_schemas) = kind_table(root, &documents, &locks);
    lock_kind_table(root, &table, locking);
    if locking {
        return;
    }

    // Hash-addressed files: the filename carries the content hash, so
    // the HTTP cache holds each capability build forever.
    let _ = fs::remove_dir_all(&dist);
    fs::create_dir_all(&dist).expect("create caps dir");
    fs::copy(&fixture, dist.join("sample-gtfs.zip")).expect("copy demo feed");

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
        let docs = read_docs(module);
        let bytes = fs::read(target.join(module.file)).expect("wasm module (run the build first)");
        let hex = format!("{:x}", Sha256::digest(&bytes));
        fs::write(dist.join(format!("{}-{hex}.wasm", module.id)), &bytes).expect("write module");
        capabilities.push(Capability {
            id: module.id.into(),
            hash: format!("sha256:{hex}"),
            url: format!("caps/{}-{hex}.wasm", module.id),
            bytes: bytes.len() as u64,
            ops: module.ops.iter().map(|s| s.to_string()).collect(),
            documents: module.documents.iter().map(|s| s.to_string()).collect(),
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
            purpose: docs.purpose.clone(),
            op_docs: docs
                .ops
                .iter()
                .map(|(op, description)| {
                    let example = example_for(module.id, op, &feed_hash, net_ctx.as_ref());
                    let kinds = kinds_for(module.id, op, net_ctx.as_ref());
                    (
                        op.clone(),
                        OpDoc { description: description.clone(), example, kinds },
                    )
                })
                .collect(),
        });
        println!("cap {}: sha256:{hex} ({:.1} KiB)", module.id, bytes.len() as f64 / 1024.0);
    }

    // The linked assemblies: one module per generated UI.
    let mut assemblies = Vec::new();
    for assembly in common::assemblies(root).into_iter().filter(|a| a.ui.is_some()) {
        let ui = assembly.ui.clone().expect("a UI assembly");
        let file = format!("{}.wasm", assembly.name.replace('-', "_"));
        let bytes = fs::read(target.join(&file)).expect("assembly module (run the build first)");
        let hex = format!("{:x}", Sha256::digest(&bytes));
        fs::write(dist.join(format!("ui-{ui}-{hex}.wasm")), &bytes).expect("write assembly");
        println!("assembly {ui}: sha256:{hex} ({:.1} KiB: {})", bytes.len() as f64 / 1024.0, assembly.caps.join(", "));
        assemblies.push(LinkedAssembly {
            ui: ui.clone(),
            hash: format!("sha256:{hex}"),
            url: format!("caps/ui-{ui}-{hex}.wasm"),
            bytes: bytes.len() as u64,
            capabilities: assembly.caps,
        });
    }

    let manifest = CapManifest { kernel: KERNEL, kernel_ops: kernel_ops(&feed_hash, net_ctx.as_ref()), capabilities, assemblies };

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

    // The local registry follows every build, fast ones too (caps-run
    // reads it, so the loop runs the code it just changed); its
    // evidence names the profile, and only `just registry` — gated,
    // release — is a publish.
    publish_registry(root, &manifest, &documents, &locks, &profile);
    publish_kind_table(root, &table, &table_schemas);

    println!("manifest: {} capabilities, kernel v{KERNEL}, assembly valid", MODULES.len());
}

/// The pinned toolchain's channel, from rust-toolchain.toml.
fn toolchain_pin(root: &Path) -> String {
    let text = fs::read_to_string(root.join("rust-toolchain.toml")).expect("rust-toolchain.toml (the toolchain pin)");
    text.lines()
        .find_map(|l| l.trim().strip_prefix("channel").map(|v| v.trim().trim_start_matches('=').trim().trim_matches('"').to_string()))
        .expect("rust-toolchain.toml names a channel")
}

/// Publish the REGISTRY: immutable blobs, one evidence record per
/// capability, and the pointer. The record says what these exact
/// bytes are, what they answer, what they speak — and WHICH GATES
/// the publish recipe ran against them, at which commit. That
/// record is what makes "known good" a checkable claim for an agent
/// (or a human dogfooding `caps run`) rather than a marketing one.
fn publish_registry(root: &Path, manifest: &CapManifest, documents: &[DocumentKind], locks: &[schema_lock::Lock], profile: &str) {
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
    let gated = std::env::var("CAPS_PUBLISH_GATED").is_ok_and(|v| v == "1");
    assert!(!gated || profile == "release", "a gated publish takes release modules, not {profile} (unset CAPS_PROFILE)");
    // The toolchain the bytes came from: the pin (rust-toolchain.toml),
    // the compiler that ran (its release, commit, and LLVM), and cargo.
    // A gated publish refuses a compiler other than the pin — the
    // published hashes are the pinned toolchain's, rebuildable from the
    // tree alone.
    let verbose = run(&["rustc", "-vV"]);
    let field = |name: &str| {
        verbose
            .lines()
            .find_map(|l| l.strip_prefix(name).map(|v| v.trim().to_string()))
            .unwrap_or_default()
    };
    let pinned = toolchain_pin(root);
    let release = field("release:");
    if gated {
        assert_eq!(
            release, pinned,
            "rustc {release} is not the pinned toolchain {pinned} (rust-toolchain.toml)"
        );
    }
    let toolchain = serde_json::json!({
        "pinned": pinned,
        "rustc": release,
        "rustc_commit": field("commit-hash:"),
        "llvm": field("LLVM version:"),
        "cargo": run(&["cargo", "--version"]),
        "locked": true,
        "remapped_paths": ["/cargo", "/src"],
    });
    let evidence = serde_json::json!({
        "gates": if gated { PUBLISH_GATES } else { &[] },
        "recipe": if gated { "just registry" } else if profile == "release" { "just caps" } else { "just caps-fast" },
        // The cargo profile of the modules: release (whole-program LTO,
        // the published bytes) or wasm-fast (the iteration loop's).
        "profile": profile,
        "source_commit": commit,
        "dirty_tree": dirty,
        "source_digest": source_digest(root),
        "rustc": rustc,
        "toolchain": toolchain,
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
            "documents": cap.documents,
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
        // The kernel ops every module answers for the kinds its
        // capabilities read (the loads) and always (evict, held,
        // about) — what `caps-run --load <kind>:FILE` resolves.
        "kernel_ops": manifest.kernel_ops,
        "capabilities": manifest
            .capabilities
            .iter()
            .map(|c| serde_json::json!({ "id": c.id, "hash": c.hash }))
            .collect::<Vec<_>>(),
        "evidence": evidence,
    });
    let json = serde_json::to_string_pretty(&pointer).expect("pointer serializes");
    fs::write(registry.join("registry.json"), json + "\n").expect("write pointer");

    publish_documents(&registry, documents, locks, &evidence);

    println!(
        "registry: {} capabilities published (blobs + records + pointer)",
        manifest.capabilities.len()
    );
}

/// One document kind the registry describes: its line in the kind list
/// (`cap_abi::kinds::KINDS`: its name, store kind, and the crate that
/// keeps its lock), and the kind as the schema lock reads it: its tag,
/// the version this build writes, the JSON Schema generated from its
/// serde type, a worked example computed from the fixtures through the
/// owning library (so the example is exactly a document the modules
/// make), and its migration chain.
struct DocumentKind {
    line: &'static cap_abi::kinds::Declaration,
    spec: schema_lock::Kind,
}

impl DocumentKind {
    fn store_kind(&self) -> Option<&'static str> {
        self.line.store.map(|s| s.kind)
    }
}

/// A locked kind: its tag and version are the type's own
/// (`transit_version::Versioned`), and the rest is its line in the kind
/// list. A type whose tag has no line stops the run.
fn kind<T>(example: Option<serde_json::Value>, migrate: Option<schema_lock::Migrate>) -> DocumentKind
where
    T: transit_version::Versioned + schemars::JsonSchema + serde::Serialize + serde::de::DeserializeOwned,
{
    let line = cap_abi::kinds::by_tag(T::KIND)
        .unwrap_or_else(|| panic!("the tag {:?} has no line in crates/cap-abi/src/kinds.rs: a kind exists only if it has one", T::KIND));
    DocumentKind { line, spec: schema_lock::kind::<T>(T::KIND, T::READS, example, migrate) }
}

/// Check every locked kind against its lock, or (`locking`) write what
/// this build adds. Any drift ends the run with every drift named.
fn lock_documents(root: &Path, documents: &[DocumentKind], locking: bool) -> Vec<schema_lock::Lock> {
    let mut drifts = Vec::new();
    let mut locks = Vec::new();
    for doc in documents {
        let owner = doc.line.owner;
        let dir = root.join("crates").join(owner).join("locks");
        let lock = schema_lock::read_lock(&dir, doc.spec.tag).unwrap_or_else(|e| panic!("{owner}'s lock: {e}"));
        if !locking {
            drifts.extend(schema_lock::check(&doc.spec, &lock));
            locks.push(lock);
            continue;
        }
        match schema_lock::freeze(&doc.spec, &lock) {
            Ok(next) if next == lock => locks.push(lock),
            Ok(next) => {
                schema_lock::write_lock(&dir, &next).unwrap_or_else(|e| panic!("{owner}'s lock: {e}"));
                println!("schema lock: crates/{owner}/locks/ records {} v{}", doc.spec.tag, doc.spec.current);
                locks.push(next);
            }
            Err(refused) => {
                drifts.extend(refused);
                locks.push(lock);
            }
        }
    }
    if !drifts.is_empty() {
        eprintln!("schema lock: the code differs from what the documents promise (docs/FORMATS.md, \"The schema lock\"):");
        for drift in &drifts {
            eprintln!("  - {drift}");
        }
        std::process::exit(1);
    }
    locks
}

/// Bring a network of an older version forward (transit-network's chain).
fn migrate_network(doc: serde_json::Value, _from: u32) -> Result<serde_json::Value, String> {
    transit_network::migrate_to(doc, transit_network::SCHEMA_VERSION).map(|(doc, _)| doc)
}

/// Bring a watchlist of an older version forward (transit-rider's chain).
fn migrate_watchlist(doc: serde_json::Value, _from: u32) -> Result<serde_json::Value, String> {
    transit_rider::watchlist::migrate_to(doc, transit_rider::watchlist::SCHEMA_VERSION).map(|(doc, _)| doc)
}

/// The fixture watchlist: a rider's two saved stops, as the kernel's
/// load and the board's example read it.
fn fixture_watchlist() -> transit_rider::watchlist::Watchlist {
    let stop = |id: &str, label: &str| transit_rider::watchlist::WatchStop { id: id.into(), label: label.into(), extra: Default::default() };
    transit_rider::watchlist::Watchlist::new(vec![stop("A", "home"), stop("B", "work")])
}

/// Bring a service alerts document of an older version forward
/// (transit-alerts' chain).
fn migrate_alerts(doc: serde_json::Value, _from: u32) -> Result<serde_json::Value, String> {
    transit_alerts::version::migrate_to(doc, transit_alerts::ALERTS_VERSION).map(|(doc, _)| doc)
}

/// Bring an edit document of an older version forward (transit-sync's chain).
fn migrate_edits(doc: serde_json::Value, _from: u32) -> Result<serde_json::Value, String> {
    transit_sync::version::migrate_to(doc, transit_sync::SCHEMA_VERSION).map(|(doc, _)| doc)
}

/// Every JSON document kind on the bus and in the store. Not here: the
/// feed (a GTFS zip) and the rt snapshot (protobuf) are bytes with
/// their own specs; the manifest is the Go host's own document
/// (docs/FORMATS.md pins it).
fn document_kinds(net: Option<&NetworkExampleContext>, feed_hash: &str) -> Vec<DocumentKind> {
    let plans_example = net.and_then(|n| {
        let set = fixture_plan_set().iter().fold(transit_plans::PublishedPlans::empty(), |set, plan| {
            transit_plans::publish(&set, plan, &n.network_hash, fixture_today()).unwrap_or(set)
        });
        serde_json::to_value(set).ok()
    });
    let walks_example = example_for("streets", "walks", feed_hash, net)
        .and_then(|ex| ex.response.get("ok").and_then(|ok| ok.get("walks")).cloned());
    vec![
        kind::<transit_network::Network>(net.map(|n| n.derived.clone()), Some(migrate_network)),
        kind::<transit_codec::Boundary>(net.map(|n| n.boundary.clone()), None),
        kind::<transit_sync::EditDocument>(Some(value(&transit_sync::EditDocument::new(dispatch_fixture_edits()))),
            Some(migrate_edits),
        ),
        kind::<transit_log::Entry>(Some(value(&fixture_stops_log().1[1])), None),
        kind::<transit_log::Checkpoint>(Some(value(&fixture_stops_log().0)), None),
        kind::<transit_network::PlanSet>(net.map(|n| value(&transit_network::PlanSet { plans: [("plan-1".to_string(), n.derived_network().clone())].into(), ..Default::default() })),
            None,
        ),
        kind::<transit_rider::watchlist::Watchlist>(Some(value(&fixture_watchlist())),
            Some(migrate_watchlist),
        ),
        kind::<transit_plans::PublishedPlans>(plans_example, None),
        kind::<transit_plans::FrozenVersions>(net.and_then(|n| {
                let frozen = transit_plans::freeze(&transit_plans::FrozenVersions::empty(), &fixture_plan_set()[0], &n.network_hash, "proposal for public comment").ok()?;
                serde_json::to_value(frozen).ok()
            }),
            None,
        ),
        kind::<transit_rider::device::RecentPlaces>(Some(serde_json::json!({"kind": "recent_places", "schema_version": 1, "places": [
                {"id": "A", "name": "Main St & 1st Ave", "lat": 45.5, "lon": -73.5},
                {"id": "STN", "name": "Central Station"}
            ]})),
            None,
        ),
        kind::<transit_rider::device::LiveFeeds>(Some(serde_json::json!({"kind": "live_feeds", "schema_version": 1,
                "trips": "https://agency.example/gtfs-rt/trip-updates.pb", "vehicles": "", "alerts": ""})),
            None,
        ),
        kind::<transit_authz::Policy>(Some(value(&transit_authz::Policy::current(vec![transit_authz::Rule {
                name: "ops-write-edits".into(),
                roles: vec!["dispatcher".into()],
                pointers: vec!["edits".into()],
                actions: vec!["write".into()],
                extra: Default::default(),
            }]))),
            None,
        ),
        kind::<transit_streets::Streets>(streets_example_context().map(|c| value(&c.streets)), None),
        kind::<transit_streets::walking::WalkTable>(walks_example, None),
        kind::<transit_runs::WorkRules>(Some(value(&fixture_work_rules())), None),
        kind::<kind_table::KindTable>(// The governance alone: the versions are the published
            // schemas', and would move the example with every schema.
            Some(value(&kind_table::KindTable {
                kind: kind_table::KindTableKind,
                schema_version: kind_table::SCHEMA_VERSION,
                kinds: kind_table::kinds(),
                extra: Default::default(),
            })),
            None,
        ),
        kind::<transit_stops::Stops>(Some(value(&fixture_stops())), None),
        kind::<transit_requests::StopRequests>(Some(value(&fixture_requests())), None),
        kind::<transit_alerts::ServiceAlerts>(Some(value(&fixture_alerts())), Some(migrate_alerts)),
        kind::<transit_alerts::AlertTemplates>(Some(value(&fixture_templates())), None),
        kind::<transit_notes::Notes>(Some(value(&fixture_notes())), None),
        kind::<transit_live::LiveDay>(live_example().map(|(live, _)| value(&live)), None),
    ]
}

/// A worked example as a JSON value.
fn value<T: serde::Serialize>(example: &T) -> serde_json::Value {
    serde_json::to_value(example).expect("example serializes")
}

/// The schemas every deployment serves at `/schemas/`
/// (web/public/schemas/, copied into every page build): the index
/// `documents.json`, with each entry's paths relative to it, and each
/// version's schema and example beside it. A reader holding only a
/// document finds its schema by the tag and version inside it. The
/// files ride in the build, so a rollback serves the schemas of the
/// build it moves to.
///
/// The index also holds the `Link` header each blob's response sends
/// (`links`): by a stored blob's envelope (`stored`, "<store kind>
/// v<N>"), and by a published document's tag (`published`, the version
/// this build writes). The server copies the value and decides nothing.
fn publish_web_schemas(registry: &Path, index: &[serde_json::Value]) {
    let web = registry.parent().expect("the registry is in the tree").join("web/public/schemas");
    let _ = fs::remove_dir_all(&web);
    fs::create_dir_all(&web).expect("create web/public/schemas");
    let mut served = Vec::new();
    let (mut stored, mut published) = (serde_json::Map::new(), serde_json::Map::new());
    for entry in index {
        let mut entry = entry.clone();
        for (field, suffix) in [("schema", ".schema.json"), ("example", ".example.json")] {
            let Some(path) = entry[field].as_str().map(str::to_string) else { continue };
            let file = format!("{}.v{}{suffix}", entry["tag"].as_str().expect("an entry has its tag"), entry["schema_version"]);
            fs::copy(registry.join(&path), web.join(&file)).expect("copy a schema file");
            entry[field] = file.into();
        }
        let link = format!("</schemas/{}>; rel=\"describedby\"; type=\"application/schema+json\"", entry["schema"].as_str().expect("a schema"));
        if let Some(store_kind) = entry["store_kind"].as_str() {
            stored.insert(format!("{store_kind} v{}", entry["schema_version"]), link.clone().into());
        }
        if entry["written"] == true {
            published.insert(entry["tag"].as_str().expect("a tag").to_string(), link.into());
        }
        served.push(entry);
    }
    let links = serde_json::json!({ "stored": stored, "published": published });
    let json = serde_json::to_string_pretty(&serde_json::json!({ "documents": served, "links": links })).expect("the served index serializes");
    fs::write(web.join("documents.json"), json + "\n").expect("write the served index");
}

/// The Go host's document kinds with a hand-written schema, by tag and
/// version (server/schemas/).
const HOST_OWNED: &[(&str, u32)] = &[("build", 1), ("derivation", 1), ("store_layout", 1)];

/// Publish the documents: for each locked version of each kind, its
/// schema `schemas/<tag>.v<N>.schema.json` and its example
/// `examples/<tag>.v<N>.json`, copied from the lock; and the index
/// `documents.json`. Each entry names its files and their hashes, and
/// says whether this build writes that version or reads it only through
/// a migration (`written`). Files are named by the tag a document
/// carries inside it, so a reader holding only a document finds its
/// schema. Each schema's `$id` names no host. `go test ./server/...`
/// validates every example against its schema with no Rust
/// (docs/FORMATS.md).
fn publish_documents(registry: &Path, documents: &[DocumentKind], locks: &[schema_lock::Lock], evidence: &serde_json::Value) {
    let schemas = registry.join("schemas");
    let examples = registry.join("examples");
    fs::create_dir_all(&schemas).expect("create registry schemas");
    fs::create_dir_all(&examples).expect("create registry examples");
    let mut index = Vec::new();
    for (doc, lock) in documents.iter().zip(locks) {
        let tag = doc.spec.tag;
        for (&version, locked) in &lock.versions {
            let text = &locked.example;
            let stem = format!("{tag}.v{version}");
            let schema = published_schema(tag, version, &locked.schema);
            let schema_path = format!("schemas/{stem}.schema.json");
            fs::write(registry.join(&schema_path), &schema).expect("write schema");
            let mut entry = serde_json::json!({
                "kind": doc.line.name,
                "tag": tag,
                "store_kind": doc.store_kind(),
                "owner": doc.line.owner,
                "schema_version": version,
                "written": version == doc.spec.current,
                "schema": schema_path,
                "schema_hash": cap_abi::content_hash(schema.as_bytes()),
            });
            let example_path = format!("examples/{stem}.json");
            fs::write(registry.join(&example_path), text).expect("write example");
            entry["example"] = example_path.into();
            entry["example_hash"] = cap_abi::content_hash(text.as_bytes()).into();
            index.push(entry);
        }
    }
    // The host-owned kinds: no Rust type generates them, so their
    // schemas are written by hand and locked in server/schemas/ beside a
    // frozen example (server/schema_test.go holds them). The manifest's
    // schema is the HTTP contract's (server/openapi.json).
    for &(tag, version) in HOST_OWNED {
        let stem = format!("{tag}.v{version}");
        let dir = registry.parent().expect("the registry is in the tree").join("server/schemas");
        let read = |file: String| fs::read_to_string(dir.join(&file)).unwrap_or_else(|e| panic!("server/schemas/{file}: {e}"));
        let schema = published_schema(tag, version, &host_owned_schema(registry.parent().expect("the registry is in the tree"), tag, version));
        let example = read(format!("{stem}.example.json"));
        let (schema_path, example_path) = (format!("schemas/{stem}.schema.json"), format!("examples/{stem}.json"));
        fs::write(registry.join(&schema_path), &schema).expect("write schema");
        fs::write(registry.join(&example_path), &example).expect("write example");
        index.push(serde_json::json!({
            "kind": tag,
            "tag": tag,
            "store_kind": tag,
            "owner": "server",
            "schema_version": version,
            "written": true,
            "schema": schema_path,
            "schema_hash": cap_abi::content_hash(schema.as_bytes()),
            "example": example_path,
            "example_hash": cap_abi::content_hash(example.as_bytes()),
        }));
    }
    let json = serde_json::to_string_pretty(&serde_json::json!({ "documents": index, "evidence": evidence }))
        .expect("documents index serializes");
    fs::write(registry.join("documents.json"), json + "\n").expect("write documents index");
    publish_web_schemas(registry, &index);
    println!("registry: {} document versions published (schemas + worked examples)", index.len());
}

/// The kind table (`kind_table.rs`): each stored kind's line in the
/// kind list, with the versions its locked schemas give it and the
/// references its current schema declares (D-232). It is a function of
/// the list and the locks alone, so the checked-in copy
/// (`crates/cap-abi/kinds.json`) needs no module build. Each schema it
/// names is answered beside it, by hash, as published.
fn kind_table(root: &Path, documents: &[DocumentKind], locks: &[schema_lock::Lock]) -> (kind_table::KindTable, Vec<(String, String)>) {
    let mut kinds = kind_table::kinds();
    let mut schemas = Vec::new();
    let mut versions = |tag: &str, version: u32, schema: &serde_json::Value| -> Option<kind_table::VersionEntry> {
        let text = published_schema(tag, version, schema);
        let hash = cap_abi::content_hash(text.as_bytes());
        schemas.push((hash.clone(), text));
        Some(kind_table::VersionEntry { version, schema: hash })
    };
    for (doc, lock) in documents.iter().zip(locks) {
        let Some(entry) = kinds.iter_mut().find(|k| k.tag == doc.spec.tag) else { continue };
        for (&version, locked) in &lock.versions {
            entry.versions.extend(versions(doc.spec.tag, version, &locked.schema));
        }
        if let Some(current) = lock.versions.get(&doc.spec.current) {
            entry.references = transit_version::references(&current.schema);
        }
    }
    for &(tag, version) in HOST_OWNED {
        let entry = kinds.iter_mut().find(|k| k.tag == tag).unwrap_or_else(|| panic!("the host-owned {tag} has no line in the kind list"));
        let schema = host_owned_schema(root, tag, version);
        entry.versions.extend(versions(tag, version, &schema));
    }
    let table = kind_table::KindTable { kind: kind_table::KindTableKind, schema_version: kind_table::SCHEMA_VERSION, kinds, extra: Default::default() };
    (table, schemas)
}

/// The checked-in kind table, beside the list it is generated from.
const KIND_TABLE: &str = "crates/cap-abi/kinds.json";

/// Hold the checked-in kind table to the list and the locks, or
/// (`locking`, `just schema-lock`) write it. A plain run stops while it
/// differs, so a new kind, a changed route, or a new reference shows in a
/// review's diff.
fn lock_kind_table(root: &Path, table: &kind_table::KindTable, locking: bool) {
    let text = serde_json::to_string_pretty(table).expect("the kind table serializes") + "\n";
    let path = root.join(KIND_TABLE);
    if locking {
        if fs::read_to_string(&path).ok().as_deref() != Some(text.as_str()) {
            fs::write(&path, &text).expect("write the kind table");
            println!("kind table: {KIND_TABLE} records the kind list");
        }
        return;
    }
    if fs::read_to_string(&path).ok().as_deref() != Some(text.as_str()) {
        eprintln!(
            "kind table: {KIND_TABLE} differs from the kind list (crates/cap-abi/src/kinds.rs) and the locked schemas. \
             Run `just schema-lock` to write it again, and review its diff."
        );
        std::process::exit(1);
    }
}

/// The kind table written beside the server's modules as
/// `web/pkg/kinds.json` (its canonical bytes), and each schema it names
/// as `web/pkg/schemas/<hex>.json` (the server keeps them in the store,
/// D-202).
fn publish_kind_table(root: &Path, table: &kind_table::KindTable, schemas: &[(String, String)]) {
    let pkg = root.join("web/pkg");
    let dir = pkg.join("schemas");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create web/pkg/schemas");
    for (hash, text) in schemas {
        fs::write(dir.join(format!("{}.json", hash.trim_start_matches("sha256:"))), text).expect("write the schema");
    }
    let json = cap_abi::canonical_bytes(table).expect("the kind table serializes");
    fs::write(pkg.join("kinds.json"), json).expect("write web/pkg/kinds.json");
    println!("kind table: {} kinds, {} schemas (web/pkg/kinds.json)", table.kinds.len(), fs::read_dir(&dir).map(|d| d.count()).unwrap_or(0));
}

/// A schema as the registry publishes it: its `$id` names no host, in
/// pretty JSON with a final newline.
fn published_schema(tag: &str, version: u32, schema: &serde_json::Value) -> String {
    let mut schema = schema.clone();
    schema["$id"] = format!("urn:transit-document:{tag}:v{version}").into();
    serde_json::to_string_pretty(&schema).expect("schema serializes") + "\n"
}

/// A host-owned kind's schema, as `server/schemas/` keeps it.
fn host_owned_schema(root: &Path, tag: &str, version: u32) -> serde_json::Value {
    let file = root.join(format!("server/schemas/{tag}.v{version}.schema.json"));
    let text = fs::read_to_string(&file).unwrap_or_else(|e| panic!("{}: {e}", file.display()));
    serde_json::from_str(&text).expect("a host-owned schema is JSON")
}

/// The digest of the source the modules are built from — every file
/// git sees (tracked or not, ignored ones excluded) under `crates/`
/// plus the workspace `Cargo.toml` and `Cargo.lock`: sha256 over
/// `"{path}\n{sha256 hex of its bytes}\n"` per file, in path order.
/// `caps-run` recomputes it the same way and says when the registry
/// was published from another tree (QAF-77). `None` without git.
fn source_digest(root: &Path) -> Option<String> {
    let listing = Command::new("git")
        .args(["ls-files", "-co", "--exclude-standard", "-z", "--", "crates", "Cargo.toml", "Cargo.lock"])
        .current_dir(root)
        .output()
        .ok()
        .filter(|o| o.status.success())?;
    let mut paths: Vec<&str> =
        std::str::from_utf8(&listing.stdout).ok()?.split('\0').filter(|p| !p.is_empty()).collect();
    paths.sort_unstable();
    paths.dedup();
    let mut digest = Sha256::new();
    for path in paths {
        let Ok(bytes) = fs::read(root.join(path)) else { continue };
        digest.update(format!("{path}\n{:x}\n", Sha256::digest(&bytes)).as_bytes());
    }
    Some(format!("sha256:{:x}", digest.finalize()))
}

/// The network examples' real context: the fixture's derived
/// document (canonical hash — the module's formula), and its
/// exported zip (content hash of the deterministic bytes).
struct NetworkExampleContext {
    /// The typed derived document — the analysis examples compute
    /// real answers over it.
    network: transit_network::Network,
    derived: serde_json::Value,
    boundary: serde_json::Value,
    boundary_hash: String,
    notes: serde_json::Value,
    network_hash: String,
    export_feed_hash: String,
    export_summary: serde_json::Value,
    zip_b64: String,
}

impl NetworkExampleContext {
    fn derived_network(&self) -> &transit_network::Network {
        &self.network
    }
}

/// The fixture as the fall 2026 pick (from 2026-10-19), and a published
/// set: that pick, a retired pick on 2026-11-30, and the winter pick on
/// 2027-01-04.
fn pick_example(net: &NetworkExampleContext) -> Option<(transit_network::Network, transit_plans::PublishedPlans)> {
    let mut pick = net.derived_network().clone();
    pick.plan.id = "fall-2026".into();
    pick.plan.name = "Fall 2026".into();
    pick.plan.effective_from = chrono::NaiveDate::from_ymd_opt(2026, 10, 19);
    let entry = |id: &str, name: &str, from: &str, status: &str| {
        serde_json::json!({ "id": id, "name": name, "effective_from": from, "network_hash": "sha256:…", "status": status })
    };
    let published = serde_json::from_value(serde_json::json!({
        "kind": "plans", "schema_version": 1,
        "plans": [entry("fall-2026", "Fall 2026", "2026-10-19", "published"), entry("holiday-2026", "Holiday 2026", "2026-11-30", "retired"), entry("winter-2027", "Winter 2027", "2027-01-04", "published")],
    }))
    .ok()?;
    Some((pick, published))
}

fn network_example_context(feed_bytes: &[u8]) -> Option<NetworkExampleContext> {
    let outcome = transit_codec::import(&transit_codec::Feed::from_zip_bytes(feed_bytes).ok()?);
    let network_hash = cap_abi::canonical_hash(&outcome.network).ok()?;
    let derived = serde_json::to_value(&outcome.network).ok()?;
    let boundary = serde_json::to_value(&outcome.boundary).ok()?;
    let boundary_hash = cap_abi::canonical_hash(&outcome.boundary).ok()?;
    let notes = serde_json::to_value(&outcome.notes).ok()?;
    let exported = transit_codec::export(&outcome.network, &outcome.boundary).ok()?;
    let export_summary = serde_json::to_value(transit_codec::FeedSummary {
        agencies: exported.agency.len(),
        routes: exported.routes.len(),
        stops: exported.stops.len(),
        trips: exported.trips.len(),
        stop_times: exported.stop_times.len(),
    })
    .ok()?;
    let zip = exported.to_zip_bytes();
    let export_feed_hash = format!("sha256:{:x}", Sha256::digest(&zip));
    let zip_b64 = transit_codec::base64_encode(&zip);
    Some(NetworkExampleContext {
        network: outcome.network,
        derived,
        boundary,
        boundary_hash,
        notes,
        network_hash,
        export_feed_hash,
        export_summary,
        zip_b64,
    })
}

/// The parsed e2e fixture feed (for the gtfs capability's dispatch
/// read examples — real answers, same library the module uses).
fn fixture_gtfs() -> Option<transit_codec::Feed> {
    let fixture = concat!(env!("CARGO_MANIFEST_DIR"), "/../../web/e2e/fixtures/sample-gtfs.zip");
    let bytes = std::fs::read(fixture).ok()?;
    transit_codec::Feed::from_zip_bytes(&bytes).ok()
}

/// The street capability's examples' context: the e2e street
/// fixture imported through the shared library, with the Elbow
/// corner snap/route answers computed over the real graph.
struct StreetsExampleContext {
    streets: transit_streets::Streets,
    streets_hash: String,
    segments: u32,
    nodes: u32,
    pos_a: transit_streets::StreetPosition,
    pos_b: transit_streets::StreetPosition,
    points: Vec<transit_streets::StreetPoint>,
    length_m: Option<f64>,
    streets_along: Vec<transit_streets::StreetUse>,
}

fn streets_example_context() -> Option<StreetsExampleContext> {
    let bytes = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../web/e2e/fixtures/streets.geojson"
    ))
    .ok()?;
    let streets = transit_streets::geojson::import(&bytes).ok()?;
    let streets_hash = cap_abi::canonical_hash(&streets).ok()?;
    let net = transit_streets::StreetNetwork::build(&streets);
    let (segments, nodes) = (net.segment_count(), net.node_count());
    let pos_a = net.snap(45.5040, -73.5030)?;
    let pos_b = net.snap(45.5080, -73.5080)?;
    let (points, length_m, streets_along) = match net.route_along(pos_a, pos_b) {
        Some(route) => (route.points, Some(route.length_m), route.streets),
        None => (vec![], None, vec![]),
    };
    Some(StreetsExampleContext { streets, streets_hash, segments, nodes, pos_a, pos_b, points, length_m, streets_along })
}

/// The rt join examples' context: the fixture network (imported
/// through the codec, with its boundary) and the e2e rt payload, both
/// through the shared libraries.
struct RtExampleContext {
    network: transit_network::Network,
    network_hash: String,
    boundary: transit_codec::Boundary,
    boundary_hash: String,
    overview: transit_rt::RtOverview,
    rt_hash: String,
}

fn rt_example_context() -> Option<RtExampleContext> {
    let outcome = transit_codec::import(&fixture_gtfs()?);
    let payload = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../web/e2e/fixtures/sample-rt.pb"
    ))
    .ok()?;
    let overview = transit_rt::decode_overview(&payload).ok()?;
    Some(RtExampleContext {
        network_hash: cap_abi::canonical_hash(&outcome.network).ok()?,
        boundary_hash: cap_abi::canonical_hash(&outcome.boundary).ok()?,
        network: outcome.network,
        boundary: outcome.boundary,
        overview,
        rt_hash: format!("sha256:{:x}", Sha256::digest(&payload)),
    })
}

/// The live document over the fixture pair, the way the rt module
/// mints it: the document and its hash.
fn live_example() -> Option<(transit_live::LiveDay, String)> {
    let ctx = rt_example_context()?;
    let live = transit_rt::live_day(&ctx.network, &ctx.boundary, Some(&ctx.overview), None, None, transit_live::OnTimeWindow::default());
    let live_hash = cap_abi::canonical_hash(&live).ok()?;
    Some((live, live_hash))
}

/// Per-kind worked examples for an op whose argument is a tagged
/// vocabulary. cap-author `apply`: one edit per kind, applied IN ORDER
/// to the fixture network through the same `apply_edits` the module
/// runs — an example that stops applying fails the build, and a kind
/// with no example (read off the type's own declaration) does too.
fn kinds_for(capability: &str, op: &str, net: Option<&NetworkExampleContext>) -> Vec<cap_abi::manifest::KindExample> {
    if (capability, op) != ("author", "apply") {
        return Vec::new();
    }
    let ctx = net.expect("the authoring kinds need the fixture network");
    let chain = edit_kind_examples();
    let mut doc = ctx.derived_network().clone();
    let mut kinds = Vec::with_capacity(chain.len());
    for (i, example) in chain.into_iter().enumerate() {
        let edit: transit_network::NetworkEdit = serde_json::from_value(example.clone())
            .unwrap_or_else(|e| panic!("edit example {i} ({example}) is not an edit: {e}"));
        doc = transit_network::apply_edits(&doc, std::slice::from_ref(&edit))
            .unwrap_or_else(|e| panic!("edit example {i} ({example}) does not apply: {e}"));
        let kind = example["edit"].as_str().expect("a tagged edit").to_string();
        kinds.push(cap_abi::manifest::KindExample { kind, example });
    }
    let declared = <transit_network::NetworkEdit as ts_rs::TS>::decl(&ts_rs::Config::default());
    let tags: std::collections::BTreeSet<&str> = declared
        .split("\"edit\": \"")
        .skip(1)
        .filter_map(|rest| rest.split('"').next())
        .collect();
    let covered: std::collections::BTreeSet<&str> = kinds.iter().map(|k| k.kind.as_str()).collect();
    assert!(!tags.is_empty(), "the edit kinds are read off NetworkEdit's declaration");
    assert_eq!(covered, tags, "every edit kind has exactly one worked example (gen_manifest edit_kind_examples)");
    assert_eq!(covered.len(), kinds.len(), "one example per edit kind");
    kinds
}

/// The edit kinds, one each, in an order that applies to the fixture
/// network (routes 10/20/40, stops A B C and the station STN with bays
/// SA SB, service types WK SUN HOL, trips t10 t11 t20 t30 t40 t41,
/// block B1 = t10 then t20): each later edit may name what an earlier
/// one made.
fn edit_kind_examples() -> Vec<serde_json::Value> {
    use serde_json::json;
    vec![
        // the plan
        json!({ "edit": "set_plan", "plan": { "id": "fall-2026", "name": "Fall 2026", "effective_from": "2026-10-19" } }),
        json!({ "edit": "update_plan", "name": "Fall 2026 service" }),
        json!({ "edit": "set_name", "name": "Sample transit" }),
        // creation gestures (an omitted id is minted and reported in `created`)
        json!({ "edit": "add_stop", "id": "D", "lat": 45.53, "lon": -73.53 }),
        json!({ "edit": "add_route", "id": "30" }),
        json!({ "edit": "add_pattern", "id": "pat-30", "route": "30" }),
        json!({ "edit": "add_pattern_through", "id": "pat-10-0-3", "route": "10", "direction": "0", "stops": ["A", "B", "C"], "run_secs": 900 }),
        json!({ "edit": "add_service_type", "id": "SAT", "from": "2026-10-19" }),
        json!({ "edit": "add_pattern_trips", "id": "SAT:pat-10-0-1", "pattern": "pat-10-0-1", "service_type": "SAT" }),
        // field gestures
        json!({ "edit": "update_stop", "id": "D", "name": "Main & 4th", "transfer": true }),
        json!({ "edit": "update_route", "id": "30", "name": "30", "mode": "bus" }),
        json!({ "edit": "set_direction", "route": "30", "id": "0", "name": "Uptown" }),
        json!({ "edit": "update_pattern", "id": "pat-30", "name": "30 via Main", "direction": "0" }),
        json!({ "edit": "update_service_type", "id": "SAT", "name": "Saturday" }),
        json!({ "edit": "set_pattern_trips_refs", "id": "SAT:pat-10-0-1", "service_type": "SAT" }),
        // a pattern's stops and timing
        json!({ "edit": "insert_pattern_stop", "pattern": "pat-10-0-2", "stop": "D", "index": 1 }),
        json!({ "edit": "set_pattern_stop", "pattern": "pat-10-0-2", "index": 1, "distance_m": 400.0 }),
        json!({ "edit": "toggle_timepoint", "pattern": "pat-10-0-2", "index": 1 }),
        json!({ "edit": "add_timing_profile", "pattern": "pat-10-0-2" }),
        json!({ "edit": "set_timing_profile", "pattern": "pat-10-0-2", "index": 1, "start_secs": 25200, "end_secs": 32400 }),
        json!({ "edit": "set_timepoint_time", "pattern": "pat-10-0-2", "profile": 1, "timepoint": 1, "secs": 300 }),
        json!({ "edit": "remove_timing_profile", "pattern": "pat-10-0-2", "index": 1 }),
        json!({ "edit": "remove_pattern_stop", "pattern": "pat-10-0-2", "index": 1 }),
        // a pattern's line: `one` gives it its own alignment, `all`
        // redraws the one it runs (every pattern on it moves; `touched`)
        json!({ "edit": "draw_alignment", "pattern": "pat-10-0-2", "redraw": "one", "id": "line-10-0-2",
            "points": [ { "lat": 45.5, "lon": -73.5 }, { "lat": 45.505, "lon": -73.505 }, { "lat": 45.51, "lon": -73.51 } ] }),
        // a service type's calendar
        json!({ "edit": "toggle_service_day", "id": "SAT", "day": "sat" }),
        json!({ "edit": "add_service_range", "id": "SAT" }),
        json!({ "edit": "set_service_range", "id": "SAT", "index": 1, "start": "2027-01-01", "end": "2027-06-30" }),
        json!({ "edit": "remove_service_range", "id": "SAT", "index": 1 }),
        json!({ "edit": "set_service_exception", "id": "SAT", "date": "2026-12-26", "state": "removed" }),
        json!({ "edit": "remove_service_exception", "id": "SAT", "date": "2026-12-26" }),
        // trips
        json!({ "edit": "retime_trip", "trip": "t30", "to_secs": 31500 }),
        json!({ "edit": "add_trip", "pattern_trips": "WK:pat-10-0-1", "id": "t12", "start_secs": 34200 }),
        json!({ "edit": "remove_trip", "trip": "t12" }),
        // a frequency trip's bands
        json!({ "edit": "upsert_pattern_trips", "id": "WK:pat-20-0-0", "pattern_trips": {
            "pattern": "pat-20-0-0", "service_type": "WK", "trips": [
                { "id": "t20", "departs": { "kind": "at", "start_secs": 30000 }, "times": [
                    { "arrival_secs": 0, "departure_secs": 0 }, { "arrival_secs": 600, "departure_secs": 600 } ] },
                { "id": "t21", "departs": { "kind": "every", "exact": false, "bands": [
                    { "start_secs": 36000, "end_secs": 43200, "headway_secs": 1200 } ] }, "times": [
                    { "arrival_secs": 0, "departure_secs": 0 }, { "arrival_secs": 600, "departure_secs": 600 } ] } ] } }),
        json!({ "edit": "add_band", "trip": "t21" }),
        json!({ "edit": "set_band", "trip": "t21", "index": 1, "start_secs": 43200, "end_secs": 50400, "headway_secs": 1800 }),
        json!({ "edit": "remove_band", "trip": "t21", "index": 1 }),
        // the timetable gestures (route + direction + service type)
        json!({ "edit": "set_headway", "route": "10", "direction": "0", "service_type": "WK",
            "from_secs": 28800, "to_secs": 32400, "headway_secs": 1200 }),
        json!({ "edit": "set_span", "route": "10", "direction": "0", "service_type": "WK", "last_secs": 36000, "headway_secs": 1800 }),
        json!({ "edit": "adjust_running_time", "route": "10", "direction": "0", "service_type": "WK",
            "from_secs": 28800, "to_secs": 32400, "delta_secs": 120 }),
        json!({ "edit": "insert_direction_stop", "route": "10", "direction": "0", "stop": "D", "after": "A" }),
        json!({ "edit": "remove_direction_stop", "route": "10", "direction": "0", "stop": "D" }),
        // the runcut (an emptied block goes)
        json!({ "edit": "add_block", "id": "B2" }),
        json!({ "edit": "add_block_trip", "block": "B2", "trip": { "trip": "t40" } }),
        json!({ "edit": "set_layover", "block": "B1", "after_trip": 0, "secs": 600 }),
        json!({ "edit": "remove_block_trip", "block": "B2", "index": 0 }),
        // whole entities: read one from the document, change it, send it back
        json!({ "edit": "upsert_stop", "id": "E", "stop": { "name": "Main & 5th", "lat": 45.54, "lon": -73.54 } }),
        json!({ "edit": "upsert_route", "id": "50", "route": { "name": "50", "mode": "bus" } }),
        json!({ "edit": "upsert_pattern", "id": "pat-50", "pattern": { "route": "50", "name": "50 Main", "stops": [
            { "stop": "A", "distance_m": 0.0, "timepoint": true }, { "stop": "B", "distance_m": 800.0, "timepoint": true } ],
            "timing": [ { "start_secs": 0, "end_secs": 86400, "at_timepoints": [0, 240] } ] } }),
        json!({ "edit": "upsert_service_type", "id": "XMAS", "service_type": { "name": "Christmas", "rule": {
            "days": { "mon": false, "tue": false, "wed": false, "thu": false, "fri": false, "sat": false, "sun": false },
            "ranges": [], "exceptions": [ { "date": "2026-12-25", "state": "added" } ] } } }),
        json!({ "edit": "upsert_block", "id": "B3", "block": { "trips": [ { "trip": "t11" } ], "layovers": [] } }),
        // removals refuse while referenced; these name what nothing references
        json!({ "edit": "remove_block", "id": "B3" }),
        json!({ "edit": "remove_service_type", "id": "XMAS" }),
        json!({ "edit": "remove_stop", "id": "E" }),
        json!({ "edit": "remove_pattern_trips", "id": "SAT:pat-10-0-1" }),
        json!({ "edit": "remove_pattern", "id": "pat-30" }),
        json!({ "edit": "remove_direction", "route": "30", "id": "0" }),
        json!({ "edit": "remove_route", "id": "30" }),
    ]
}

/// Worked examples: an agent reads these to learn the shapes.
/// Requests carry the REAL feed hash computed above; responses
/// carry the fixture's known answers.
/// The worked work rules: five minutes or a tenth of the run after
/// A stops document (D-153, D-154, D-157): one stop in service, given
/// amenities, suspended for a while, then moved; one planned, then
/// permitted.
fn fixture_stops() -> transit_stops::Stops {
    serde_json::from_value(serde_json::json!({
        "kind": "stops", "schema_version": 1,
        "stops": {
            "6124": [
                {"on": "2004-09-01", "event": "in_service", "lat": 45.6934, "lon": -122.6512, "name": "NE 99th St & Hwy 99", "code": "6124", "wheelchair": true},
                {"on": "2025-05-12", "event": "changed", "amenities": {"shelter": true, "bench": true, "bin": true}},
                {"on": "2027-03-03", "event": "suspended", "until": "2027-07-01", "note": "water main"},
                {"on": "2027-09-01", "event": "moved", "lat": 45.6936, "lon": -122.6509}
            ],
            "7001": [
                {"on": "2027-01-10", "event": "planned", "lat": 45.7, "lon": -122.66, "name": "NE 104th St"},
                {"on": "2027-05-03", "event": "permitted"}
            ]
        }
    }))
    .map(transit_stops::Stops::identified)
    .expect("the fixture stops parse")
}

/// The moment the alerts examples are asked at (Unix seconds): the
/// fixture Friday, 2026-10-23, in the morning.
const ALERTS_NOW: u64 = 1_792_756_800;

/// Two alerts over the fixture network: route 10's detour at stop C,
/// in English and Spanish, explaining the fixture's diversion; route
/// 40's reduced service in one direction, for a day; and trip t10's
/// 08:00 instance, which the live feed cancels (D-216).
fn fixture_alerts() -> transit_alerts::ServiceAlerts {
    serde_json::from_value(serde_json::json!({
        "kind": "service_alerts", "schema_version": 2,
        "alerts": {
            "detour-10": {
                "status": "published", "cause": "construction", "effect": "detour", "severity": "warning",
                "header": [{"text": "Route 10 detour at Main & 3rd"}, {"lang": "es", "text": "Desvío de la ruta 10 en Main & 3rd"}],
                "description": [{"text": "Stop C is closed. Use stop B."}],
                "communication": [{"start": ALERTS_NOW - 3600, "end": null}],
                "informed": [{"route": "10", "stop": "C"}],
                "explains": [{"kind": "edit", "id": "e1"}]
            },
            "shuttle-40": {
                "status": "published", "cause": "maintenance", "effect": "reduced_service",
                "header": [{"text": "Fewer Station Shuttle trips today"}],
                "communication": [{"start": ALERTS_NOW - 7200, "end": ALERTS_NOW + 50_000}],
                "informed": [{"route": "40", "direction": "1"}]
            },
            "t10-cancelled": {
                "status": "published", "cause": "other", "effect": "no_service",
                "header": [{"text": "The 8:00 trip on route 10 is cancelled"}],
                "communication": [{"start": ALERTS_NOW - 1800, "end": ALERTS_NOW + 3600}],
                "informed": [{"trip": "t10", "date": "2026-10-23", "start_secs": 28800}],
                "explains": [{"kind": "trip", "trip": "t10", "date": "2026-10-23", "start_secs": 28800}]
            }
        }
    }))
    .expect("the fixture alerts parse")
}

/// The write a receive op's example runs: the first entry of an empty
/// log, as the server gives it.
fn receive_write(log: &str, writer: &str, at_ms: u64) -> transit_log::Write {
    let header = transit_log::Header {
        log: log.into(),
        seq: 1,
        writer: writer.into(),
        at_ms,
        module: format!("cap-{log}@sha256:…"),
        intent: transit_log::Intent { op: format!("POST /logs/{log}"), key: Some("device-1/1".into()), ..Default::default() },
    };
    transit_log::Write { checkpoint: None, header, unread: vec![] }
}

/// A receive op's answer, as its module writes it.
fn receive_answer(received: &transit_log::Received) -> Option<serde_json::Value> {
    let mut answer = serde_json::to_value(received).ok()?;
    answer["op"] = "receive".into();
    Some(answer)
}

/// The span the notes examples ask about: 2026-10-23 in the fixture's
/// zone (America/Montreal), as Unix seconds.
const NOTES_DAY: (u64, u64) = (1_792_728_000, 1_792_814_400);

/// Two notes: one on trip t10's 08:00 instance on 2026-10-23, edited,
/// and one on the detour alert.
fn fixture_notes() -> transit_notes::Notes {
    serde_json::from_value(serde_json::json!({
        "kind": "notes", "schema_version": 1,
        "notes": {
            "n1": {
                "target": {"kind": "trip", "trip": "t10", "date": "2026-10-23", "start_secs": 28800},
                "text": "No operator. The next trip is covered.",
                "written": {"by": "dispatch-1", "at": 1_792_756_800},
                "edited": {"by": "dispatch-1", "at": 1_792_757_000}
            },
            "n2": {
                "target": {"kind": "alert", "id": "detour-10"},
                "text": "Posted. The end time follows when the crew reports.",
                "written": {"by": "comms-1", "at": 1_792_760_400}
            }
        }
    }))
    .expect("the fixture notes parse")
}

/// One template: a stop closure, with blanks, in English and Spanish.
fn fixture_templates() -> transit_alerts::AlertTemplates {
    serde_json::from_value(serde_json::json!({
        "kind": "alert_templates", "schema_version": 1,
        "templates": {
            "stop-closure": {
                "name": "Stop closure", "cause": "construction", "effect": "no_service",
                "header": [{"text": "The stop at {stop} is closed starting {start date}"}, {"lang": "es", "text": "La parada {stop} está cerrada"}],
                "description": [{"text": "Use {route} at the nearby stops until {end date}."}]
            }
        }
    }))
    .expect("the fixture templates parse")
}

/// Two stop requests: a new stop at 7001 with its work order open, and
/// a repair at 6124 drafted.
fn fixture_requests() -> transit_requests::StopRequests {
    serde_json::from_value(serde_json::json!({
        "kind": "stop_requests", "schema_version": 1,
        "requests": {
            "R-2027-014": {
                "stop": "7001", "job": "new", "status": "ordered",
                "requested_by": "Planning", "requested_on": "2027-01-04",
                "routes": ["37"], "direction": "eastbound",
                "street": "NE 104th St", "cross_street": "NE 20th Ave", "offset_ft": 60, "side": "south",
                "city": "Vancouver", "county": "Clark", "marked_by": "curb_mark",
                "locate": {"ticket": "26123456", "called_on": "2027-01-06", "utilities": ["City of Vancouver", "NW Natural"]},
                "work_order": {"system": "Maintenance", "id": "WO-88123", "status": "open", "read_on": "2027-01-07"}
            },
            "R-2027-020": {"stop": "6124", "job": "repair_improve", "status": "drafted", "note": "pad cracked at the rear door"}
        }
    }))
    .expect("the fixture requests parse")
}

/// every trip, route 10's own rule, routes 10 and 20 interlined, and an
/// eight-hour run under report and clear times.
fn fixture_work_rules() -> transit_runs::WorkRules {
    transit_runs::WorkRules { kind: Default::default(),
        schema_version: Default::default(),
        name: "Fixture rules".into(),
        recovery: vec![
            transit_runs::RecoveryRule { route: None, service_type: None, min_secs: Some(300), pct_of_run: Some(10), extra: Default::default() },
            transit_runs::RecoveryRule { route: Some("10".into()), service_type: None, min_secs: Some(240), pct_of_run: None, extra: Default::default() },
        ],
        interline: vec![transit_runs::RoutePair { a: "10".into(), b: "20".into(), extra: Default::default() }],
        runs: transit_runs::RunRules {
            max_platform_secs: Some(8 * 3600),
            report_secs: Some(900),
            clear_secs: Some(900),
            ..Default::default()
        },
        pay: None,
        extra: Default::default(),
    }
}

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
                json!({ "ok": { "op": "feed_info", "info": gtfs.info() } }),
            )
        }
        // The router's examples compute REAL answers over the
        // fixture-derived document — the same composition the module
        // dispatches through (materialize, patch, build, query).
        ("kernel", "load_walks") => {
            // A two-walk table over the fixture's station bays.
            let net = net?;
            let walk = |from: &str, to: &str| transit_streets::walking::Walk { from: from.into(), to: to.into(), walk_secs: 95 };
            let table = transit_streets::walking::WalkTable { kind: Default::default(), schema_version: Default::default(),
                streets_hash: "sha256:…".into(),
                network_hash: net.network_hash.clone(),
                speed_mps: transit_streets::walking::WALK_SPEED_MPS,
                max_walk_secs: transit_streets::walking::MAX_WALK_SECS,
                walks: vec![walk("SA", "SB"), walk("SB", "SA")],
            };
            let walks_hash = cap_abi::canonical_hash(&table).ok()?;
            (
                json!({ "op": "load_walks", "walks": table }),
                json!({ "ok": { "op": "load_walks", "walks_hash": walks_hash } }),
            )
        }
        ("router", "plan") => {
            // Over the fixture's live document: the legs carry the
            // predicted times and say how far they moved.
            let ctx = net?;
            let (live, live_hash) = live_example()?;
            let date = chrono::NaiveDate::from_ymd_opt(2026, 10, 23).expect("fixture date");
            let network = ctx.derived_network();
            let day = transit_network::materialize(network, date).expect("fixture materializes");
            let graph = transit_router::RouterNetwork::build(network, &day, date, Some(&live));
            let itinerary = transit_router::earliest_arrival(&graph, "A", "B", 25200, 4)
                .expect("query")
                .expect("A→B reachable on the fixture Friday");
            (
                json!({ "op": "plan", "network_hash": ctx.network_hash, "from": "A", "to": "B",
                        "date": "2026-10-23", "depart_secs": 25200, "max_legs": 4, "live_hash": live_hash }),
                json!({ "ok": { "op": "plan", "itinerary": itinerary } }),
            )
        }
        ("router", "plan_arrive_by") => {
            // No deadline: the last bus home from A to B on the
            // fixture Friday.
            let ctx = net?;
            let date = chrono::NaiveDate::from_ymd_opt(2026, 10, 23).expect("fixture date");
            let network = ctx.derived_network();
            let day = transit_network::materialize(network, date).expect("fixture materializes");
            let graph = transit_router::RouterNetwork::build_from_day(network, &day, date);
            let answer = transit_router::latest_departure(&graph, "A", "B", None, 4).expect("query");
            (
                json!({ "op": "plan_arrive_by", "network_hash": ctx.network_hash, "from": "A", "to": "B",
                        "date": "2026-10-23", "max_legs": 4 }),
                json!({ "ok": { "op": "plan_arrive_by", "arrive_by_secs": answer.arrive_by_secs,
                                "itinerary": answer.itinerary } }),
            )
        }
        ("router", "plan_compare") => {
            let ctx = net?;
            let date = chrono::NaiveDate::from_ymd_opt(2026, 10, 23).expect("fixture date");
            let network = ctx.derived_network();
            let records = dispatch_fixture_edits();
            // The edit document's hash, as its load answers it.
            let edits_hash = cap_abi::canonical_hash(&transit_sync::EditDocument::new(records.clone())).expect("the edit document serializes");
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
            // The diversion costs the rider 5 minutes: the notice says so.
            let diverted = transit_router::diverted_instances(&base_day, &patched);
            let notice = transit_router::plan_notice(network, Some(&active), Some(&baseline), 4, &diverted);
            (
                json!({ "op": "plan_compare", "network_hash": ctx.network_hash,
                        "edits_hash": edits_hash, "from": "A", "to": "B",
                        "date": "2026-10-23", "depart_secs": 25200, "max_legs": 4 }),
                json!({ "ok": { "op": "plan_compare", "active": active, "baseline": baseline, "notice": notice } }),
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
                    transit_router::Leg::Transit { trip, start_secs, board_stop_id, alight_stop_id, .. } => {
                        json!({ "kind": "transit", "path": transit_dispatch::trip_segment(network, &day, &[], trip, *start_secs, board_stop_id, alight_stop_id) })
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
                json!({ "ok": { "op": "import", "network": ctx.derived, "boundary": ctx.boundary,
                    "notes": ctx.notes, "network_hash": ctx.network_hash,
                    "boundary_hash": ctx.boundary_hash } }),
            )
        }
        ("kernel", "load_network") => {
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
        ("network", "document") => {
            let ctx = net?;
            let document = cap_abi::canonical_bytes(&ctx.network).ok()?;
            (
                json!({ "op": "document", "network_hash": ctx.network_hash }),
                json!({ "ok": { "op": "document", "document": document,
                    "document_hash": cap_abi::content_hash(document.as_bytes()), "plan": ctx.network.plan } }),
            )
        }
        ("network", "publishable") => {
            let ctx = net?;
            let refusals = transit_network::publish_refusals(&ctx.network);
            let warnings: Vec<_> =
                transit_network::validation_warnings(&ctx.network).into_iter().filter(|w| !refusals.contains(w)).collect();
            (
                json!({ "op": "publishable", "network_hash": ctx.network_hash }),
                json!({ "ok": { "op": "publishable", "issues": transit_network::validation_issues(&ctx.network),
                    "refusals": refusals, "warnings": warnings } }),
            )
        }
        ("network", "stored_plans") => {
            // A stored plan as it was written: its own version is read
            // when the page uses it.
            let plan = json!({ "plan": { "id": "", "name": "Spring draft" } });
            let document = json!({ "kind": "plan_set", "schema_version": 1, "plans": { "p1": plan } });
            let plans = transit_network::plan_set::stored_plans(document.clone()).ok()?;
            (json!({ "op": "stored_plans", "document": document }), json!({ "ok": { "op": "stored_plans", "plans": plans } }))
        }
        ("network", "export") => {
            let ctx = net?;
            (
                json!({ "op": "export", "network_hash": ctx.network_hash,
                    "boundary_hash": ctx.boundary_hash }),
                json!({ "ok": { "op": "export", "feed_hash": ctx.export_feed_hash,
                    "summary": ctx.export_summary, "zip_b64": ctx.zip_b64 } }),
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
        // The service groups over the same fixture document.
        ("analysis", "service_groups") => {
            let ctx = net?;
            (
                json!({ "op": "service_groups", "network_hash": ctx.network_hash }),
                json!({ "ok": { "op": "service_groups",
                    "groups": transit_analysis::service_groups(&ctx.derived_network()) } }),
            )
        }
        ("analysis", "service_type_detail") => {
            let ctx = net?;
            let detail = transit_analysis::service_type_detail(&ctx.derived_network(), &["WK".to_string()])
                .expect("WK exists in the fixture-derived document");
            (
                json!({ "op": "service_type_detail", "network_hash": ctx.network_hash, "members": ["WK"] }),
                json!({ "ok": { "op": "service_type_detail", "detail": detail } }),
            )
        }
        ("analysis", "service_type_compare") => {
            let ctx = net?;
            let comparison = transit_analysis::service_type_compare(&ctx.derived_network(), &ctx.derived_network(), &["WK".to_string()])
                .expect("WK exists in the fixture-derived document");
            (
                json!({ "op": "service_type_compare", "base_hash": ctx.network_hash, "network_hash": ctx.network_hash, "members": ["WK"] }),
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
                json!({ "op": "schedule_texture", "network_hash": ctx.network_hash, "members": ["WK"] }),
                json!({ "ok": { "op": "schedule_texture",
                    "texture": transit_analysis::schedule_texture(&ctx.derived_network(), &["WK".to_string()]) } }),
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
        // The hub's route pairs: the same connections, folded.
        ("analysis", "hub_connections") => {
            let ctx = net?;
            let date = chrono::NaiveDate::from_ymd_opt(2026, 1, 5).unwrap();
            (
                json!({ "op": "hub_connections", "network_hash": ctx.network_hash,
                    "date": "2026-01-05", "stop_id": "A",
                    "window": { "min_secs": 60, "max_secs": 1500 } }),
                json!({ "ok": { "op": "hub_connections",
                    "connections": transit_analysis::hub_connections(
                        &ctx.derived_network(), date, "A",
                        Some(transit_analysis::TransferWindow { min_secs: 60, max_secs: 1500 })) } }),
            )
        }
        // The hub's near misses over the same fixture document.
        ("analysis", "missed_connections") => {
            let ctx = net?;
            let date = chrono::NaiveDate::from_ymd_opt(2026, 1, 5).unwrap();
            (
                json!({ "op": "missed_connections", "network_hash": ctx.network_hash,
                    "date": "2026-01-05", "stop_id": "A", "miss_secs": 1500 }),
                json!({ "ok": { "op": "missed_connections",
                    "missed": transit_analysis::missed_connections(&ctx.derived_network(), date, "A", None, Some(1500)) } }),
            )
        }
        // The network's longest waits in a morning window.
        ("analysis", "longest_waits") => {
            let ctx = net?;
            let date = chrono::NaiveDate::from_ymd_opt(2026, 1, 5).unwrap();
            let window = transit_analysis::CoverageWindow { start_secs: 7 * 3600, end_secs: 10 * 3600 };
            (
                json!({ "op": "longest_waits", "network_hash": ctx.network_hash, "date": "2026-01-05",
                    "window": { "start_secs": 25200, "end_secs": 36000 }, "limit": 5 }),
                json!({ "ok": { "op": "longest_waits",
                    "waits": transit_analysis::longest_waits(&ctx.derived_network(), date, Some(window), Some(5)) } }),
            )
        }
        ("analysis", "day_types") => {
            let ctx = net?;
            (
                json!({ "op": "day_types", "network_hash": ctx.network_hash, "from": "2026-01-01" }),
                json!({ "ok": { "op": "day_types",
                    "day_types": transit_analysis::day_types(&ctx.derived_network(), chrono::NaiveDate::from_ymd_opt(2026, 1, 1), None) } }),
            )
        }
        // The fixture against itself: every day type, nothing changed.
        ("analysis", "plan_compare") => {
            let ctx = net?;
            let n = ctx.derived_network();
            (
                json!({ "op": "plan_compare", "base_hash": ctx.network_hash, "network_hash": ctx.network_hash, "from": "2026-01-01" }),
                json!({ "ok": { "op": "plan_compare",
                    "day_types": transit_analysis::plan_compare(&n, &n, chrono::NaiveDate::from_ymd_opt(2026, 1, 1), None) } }),
            )
        }
        ("analysis", "network_findings") => {
            let ctx = net?;
            let n = ctx.derived_network();
            let params = transit_analysis::FindingParams { wait_secs: Some(900), limit: Some(5), ..Default::default() };
            (
                json!({ "op": "network_findings", "network_hash": ctx.network_hash, "base_hash": ctx.network_hash,
                    "members": ["WK"], "params": { "wait_secs": 900, "limit": 5 } }),
                json!({ "ok": { "op": "network_findings",
                    "findings": transit_analysis::network_findings(&n, Some(&n), &["WK".to_string()], &params) } }),
            )
        }
        ("analysis", "route_frequency") => {
            let ctx = net?;
            (
                json!({ "op": "route_frequency", "network_hash": ctx.network_hash, "members": ["WK"] }),
                json!({ "ok": { "op": "route_frequency",
                    "frequency": transit_analysis::route_frequency(&ctx.derived_network(), &["WK".to_string()]) } }),
            )
        }
        ("analysis", "type_coverage") => {
            let ctx = net?;
            // The same two-point layer the caps smoke uses: one point
            // at a stop (served), one far away — the WK combination
            // covers 50%.
            let layer_json = r#"{"type":"FeatureCollection","features":[
                {"type":"Feature","properties":{"pop":500},"geometry":{"type":"Point","coordinates":[-73.5,45.5]}},
                {"type":"Feature","properties":{"pop":500},"geometry":{"type":"Point","coordinates":[-74.0,46.0]}}
            ]}"#;
            let layer = transit_analysis::parse_layer(layer_json.as_bytes()).expect("layer parses");
            let weights = layer.weights(Some("pop"), Some("people")).expect("pop reads");
            let layer_hash = format!("sha256:{:x}", Sha256::digest(layer_json.as_bytes()));
            (
                json!({ "op": "type_coverage", "network_hash": ctx.network_hash,
                    "layer_hash": layer_hash, "members": ["WK"], "field": "pop", "label": "people",
                    "radius_m": 500, "frequent_secs": 3600, "min_departures": 2 }),
                json!({ "ok": { "op": "type_coverage",
                    "layer": weights.reading.clone(),
                    "coverage": transit_analysis::type_coverage(&ctx.derived_network(), &weights, &["WK".to_string()], 500.0, 3600, 2, None) } }),
            )
        }
        // The unserved WHERE over the same fixture document and the
        // same two-point layer, under the WK combination: the far
        // point, named, with the nearest served stop.
        ("analysis", "unserved_points") => {
            let ctx = net?;
            let layer_json = r#"{"type":"FeatureCollection","features":[
                {"type":"Feature","properties":{"pop":500},"geometry":{"type":"Point","coordinates":[-73.5,45.5]}},
                {"type":"Feature","properties":{"pop":500},"geometry":{"type":"Point","coordinates":[-74.0,46.0]}}
            ]}"#;
            let layer = transit_analysis::parse_layer(layer_json.as_bytes()).expect("layer parses");
            let weights = layer.weights(None, None).expect("one weight");
            let layer_hash = format!("sha256:{:x}", Sha256::digest(layer_json.as_bytes()));
            (
                json!({ "op": "unserved_points", "network_hash": ctx.network_hash,
                    "layer_hash": layer_hash, "radius_m": 500, "members": ["WK"] }),
                json!({ "ok": { "op": "unserved_points",
                    "unserved": transit_analysis::unserved_points(&ctx.derived_network(), &weights, &["WK".to_string()], 500.0),
                    "layer": weights.reading } }),
            )
        }
        // The comparison over the fixture document against itself, WK
        // on both sides, with a polygon layer: one square at a stop
        // (served whole) and one far away — nothing gained or lost,
        // each side the same.
        ("analysis", "coverage_compare") => {
            let ctx = net?;
            let layer_json = r#"{"type":"FeatureCollection","features":[
                {"type":"Feature","properties":{"jobs":300},"geometry":{"type":"Polygon","coordinates":[[[-73.501,45.4995],[-73.499,45.4995],[-73.499,45.5005],[-73.501,45.5005],[-73.501,45.4995]]]}},
                {"type":"Feature","properties":{"jobs":100},"geometry":{"type":"Point","coordinates":[-74.0,46.0]}}
            ]}"#;
            let layer = transit_analysis::parse_layer(layer_json.as_bytes()).expect("layer parses");
            let weights = layer.weights(Some("jobs"), Some("jobs")).expect("jobs reads");
            let layer_hash = format!("sha256:{:x}", Sha256::digest(layer_json.as_bytes()));
            let wk = ["WK".to_string()];
            let network = ctx.derived_network();
            (
                json!({ "op": "coverage_compare", "base_hash": ctx.network_hash, "network_hash": ctx.network_hash,
                    "layer_hash": layer_hash, "field": "jobs", "label": "jobs",
                    "base_members": ["WK"], "members": ["WK"],
                    "radius_m": 500, "frequent_secs": 3600, "min_departures": 2 }),
                json!({ "ok": { "op": "coverage_compare",
                    "layer": weights.reading.clone(),
                    "comparison": transit_analysis::coverage_compare(&network, &network, &weights, &wk, &wk, 500.0, 3600, 2, None) } }),
            )
        }
        // The rt join examples compute REAL answers over the fixture
        // pair (sample-gtfs.zip + sample-rt.pb): t10 +120 s at A /
        // +180 s at B, one vehicle on t10 — the joined board and the
        // ranked view through the same shared library the module
        // dispatches through.
        ("kernel", "load_boundary") => {
            let ctx = rt_example_context()?;
            (
                json!({ "op": "load_boundary", "boundary": ctx.boundary }),
                json!({ "ok": { "op": "load_boundary", "boundary_hash": ctx.boundary_hash } }),
            )
        }
        ("rt", "trip_progress") => {
            let ctx = rt_example_context()?;
            (
                json!({ "op": "trip_progress", "network_hash": ctx.network_hash,
                    "boundary_hash": ctx.boundary_hash, "rt_hash": ctx.rt_hash, "trip_id": "t10" }),
                json!({ "ok": { "op": "trip_progress",
                    "progress": transit_rt::trip_progress(&ctx.network, &ctx.boundary, &ctx.overview, "t10") } }),
            )
        }
        ("rt", "live") => {
            let ctx = rt_example_context()?;
            let (live, live_hash) = live_example()?;
            (
                json!({ "op": "live", "network_hash": ctx.network_hash, "boundary_hash": ctx.boundary_hash,
                    "rt_hash": ctx.rt_hash,
                    "window": { "late_secs": 60, "very_late_secs": 300 } }),
                json!({ "ok": { "op": "live", "live_hash": live_hash,
                    "summary": transit_live::summarize(&live, 15), "live": live } }),
            )
        }
        ("rt", "late_trips") => {
            let ctx = rt_example_context()?;
            (
                json!({ "op": "late_trips", "network_hash": ctx.network_hash,
                    "rt_hash": ctx.rt_hash, "min_delay_secs": 60,
                    "vp_hash": null }),
                json!({ "ok": { "op": "late_trips",
                    "trips": transit_rt::late_trips(&ctx.network, &ctx.overview, None, 60) } }),
            )
        }
        ("kernel", "load_edits") => {
            let edits = transit_sync::EditDocument::new(dispatch_fixture_edits());
            let edits_hash = cap_abi::canonical_hash(&edits).ok()?;
            (
                json!({ "op": "load_edits", "edits": edits }),
                json!({ "ok": { "op": "load_edits", "edits_hash": edits_hash } }),
            )
        }
        ("kernel", "load_live") => {
            let (live, live_hash) = live_example()?;
            (
                json!({ "op": "load_live", "live": live }),
                json!({ "ok": { "op": "load_live", "live_hash": live_hash } }),
            )
        }
        ("board", "stop_board") => {
            let ctx = net?;
            let (live, live_hash) = live_example()?;
            let date = chrono::NaiveDate::from_ymd_opt(2026, 10, 23).unwrap();
            let day = transit_network::materialize(ctx.derived_network(), date).expect("fixture day");
            // 08:01 at A: t10 (08:00, +120 s) has not left yet.
            (
                json!({ "op": "stop_board", "network_hash": ctx.network_hash,
                    "stop_id": "A", "date": "2026-10-23", "from_secs": 28860, "limit": 5,
                    "live_hash": live_hash }),
                json!({ "ok": { "op": "stop_board",
                    "board": transit_dispatch::stop_board(ctx.derived_network(), &day, "A", 28860, 5, Some(&live), None) } }),
            )
        }
        ("board", "alert_reach") => {
            // Route 10 at stop A, and the station: the first reaches
            // route 10's two departures at A, the second none at the
            // station's bays.
            let ctx = net?;
            let date = chrono::NaiveDate::from_ymd_opt(2026, 10, 23).unwrap();
            let day = transit_network::materialize(ctx.derived_network(), date).expect("fixture day");
            let alert: transit_alerts::ServiceAlert = serde_json::from_value(json!({
                "status": "published", "header": [{"text": "Main & 1st: route 10 boards across the street"}],
                "informed": [{"route": "10", "stop": "A"}, {"stop": "STN"}]
            })).ok()?;
            let network = ctx.derived_network();
            let mut all = Vec::new();
            let entities: Vec<serde_json::Value> = alert.informed.iter().map(|e| {
                let r = transit_rt::entity_reading(network, e);
                all.extend(r.scopes.clone());
                json!({ "published": r.published.as_ref().ok(), "names": r.names,
                        "reach": transit_dispatch::alert_reach(network, &day, &r.scopes, "2026-10-23") })
            }).collect();
            (
                json!({ "op": "alert_reach", "network_hash": ctx.network_hash, "date": "2026-10-23", "alert": alert }),
                json!({ "ok": { "op": "alert_reach", "entities": entities, "reach": transit_dispatch::alert_reach(network, &day, &all, "2026-10-23") } }),
            )
        }
        ("board", "grouped_board") => {
            let ctx = net?;
            let (live, live_hash) = live_example()?;
            let date = chrono::NaiveDate::from_ymd_opt(2026, 10, 23).unwrap();
            let day = transit_network::materialize(ctx.derived_network(), date).expect("fixture day");
            // 08:01 at A: the 10's next three (t10 late, +120 s), the 20's one.
            let mut board = transit_dispatch::grouped_board(ctx.derived_network(), &day, "A", 28860, 3, Some(&live), None);
            transit_dispatch::with_next_service(ctx.derived_network(), &mut board, "A", date, 28860, |on| {
                transit_dispatch::day(ctx.derived_network(), on, &[]).map(Box::new).map_err(|e| e.to_string())
            })
            .ok()?;
            (
                json!({ "op": "grouped_board", "network_hash": ctx.network_hash,
                    "stop_id": "A", "date": "2026-10-23", "from_secs": 28860, "per_group": 3,
                    "live_hash": live_hash }),
                json!({ "ok": { "op": "grouped_board", "board": board } }),
            )
        }
        ("kernel", "load_stops") => {
            let stops = fixture_stops().checked().ok()?;
            (
                json!({ "op": "load_stops", "stops": stops }),
                json!({ "ok": { "op": "load_stops", "stops_hash": cap_abi::canonical_hash(&stops).ok()?, "stops": stops.stops.len() } }),
            )
        }
        ("stops", "validate") => {
            let stops = fixture_stops().checked().ok()?;
            let document = cap_abi::canonical_bytes(&stops).ok()?;
            (
                json!({ "op": "validate", "stops": stops }),
                json!({ "ok": { "op": "validate", "refusals": [],
                    "document": document, "document_hash": cap_abi::content_hash(document.as_bytes()) } }),
            )
        }
        ("stops", "stops_on") => {
            let stops = fixture_stops().checked().ok()?;
            let date = chrono::NaiveDate::from_ymd_opt(2027, 4, 1)?;
            (
                json!({ "op": "stops_on", "stops_hash": cap_abi::canonical_hash(&stops).ok()?, "date": "2027-04-01" }),
                json!({ "ok": { "op": "stops_on", "stops": stops.on_date(date), "counts": stops.counts_on(date) } }),
            )
        }
        ("stops", "stops_near") => {
            // Standing at a point by 6124 in the field: the two nearest stops.
            let stops = fixture_stops().checked().ok()?;
            let date = chrono::NaiveDate::from_ymd_opt(2027, 4, 1)?;
            let at = stops.stop_on("6124", date)?;
            let (lat, lon) = (at.lat + 0.0002, at.lon);
            (
                json!({ "op": "stops_near", "stops_hash": cap_abi::canonical_hash(&stops).ok()?, "date": "2027-04-01", "lat": lat, "lon": lon, "limit": 2 }),
                json!({ "ok": { "op": "stops_near", "stops": stops.near(date, lat, lon, 2, None, None) } }),
            )
        }
        ("stops", "stop") => {
            let stops = fixture_stops().checked().ok()?;
            let date = chrono::NaiveDate::from_ymd_opt(2027, 7, 1)?;
            (
                json!({ "op": "stop", "stops_hash": cap_abi::canonical_hash(&stops).ok()?, "stop_id": "6124", "date": "2027-07-01" }),
                json!({ "ok": { "op": "stop", "stop": stops.stop_on("6124", date), "events": stops.stops["6124"],
                    "next": stops.next_events("6124", date).iter().map(|k| k.as_str()).collect::<Vec<_>>() } }),
            )
        }
        ("stops", "seed") => {
            let net = net?;
            let seeded = transit_stops::stop_record::seed(net.derived_network()).identified().checked().ok()?;
            (
                json!({ "op": "seed", "network_hash": net.network_hash }),
                json!({ "ok": { "op": "seed", "stops_hash": cap_abi::canonical_hash(&seeded).ok()?, "stops": seeded } }),
            )
        }
        ("stops", "add_event") => {
            // 6124 suspended for a week of paving: in its place by date.
            let stops = fixture_stops().checked().ok()?;
            let event: transit_stops::Event = serde_json::from_value(json!({"on": "2026-01-05", "event": "suspended", "until": "2026-01-12", "note": "paving"})).ok()?;
            let next = stops.add_event("6124", event.clone()).identified().checked().ok()?;
            (
                json!({ "op": "add_event", "stops_hash": cap_abi::canonical_hash(&stops).ok()?, "stop_id": "6124", "event": event }),
                json!({ "ok": { "op": "add_event", "refusals": [], "stops_hash": cap_abi::canonical_hash(&next).ok()?, "stops": next } }),
            )
        }
        ("stops", "correct_event") => {
            // 7001's permit was entered on the wrong date: the event,
            // named by its id, is corrected, and the stop is permitted
            // from 2027-04-19.
            let stops = fixture_stops().checked().ok()?;
            let event_id = stops.stops["7001"][1].id.clone();
            let event: transit_stops::Event = serde_json::from_value(json!({"on": "2027-04-19", "event": "permitted"})).ok()?;
            let next = stops.correct_event_by_id("7001", &event_id, Some(event.clone()))?.identified().checked().ok()?;
            (
                json!({ "op": "correct_event", "stops_hash": cap_abi::canonical_hash(&stops).ok()?, "stop_id": "7001", "event_id": event_id, "event": event }),
                json!({ "ok": { "op": "correct_event", "refusals": [], "stops_hash": cap_abi::canonical_hash(&next).ok()?, "stops": next } }),
            )
        }
        ("stops", "read_records") => {
            // The fixture's stops seeded, stop A renamed from 2026-10-01:
            // a pick from that date reads the new name.
            let net = net?;
            let mut stops = transit_stops::stop_record::seed(net.derived_network());
            stops = stops.add_event("A", serde_json::from_value(json!({"on": "2026-10-01", "event": "changed", "name": "A Street Station"})).ok()?);
            let stops = stops.checked().ok()?;
            let read = transit_stops::stop_record::read_records(net.derived_network(), &stops, chrono::NaiveDate::from_ymd_opt(2026, 10, 5)?);
            (
                json!({ "op": "read_records", "network_hash": net.network_hash, "stops_hash": cap_abi::canonical_hash(&stops).ok()?, "on": "2026-10-05" }),
                json!({ "ok": { "op": "read_records", "network_hash": cap_abi::canonical_hash(&read).ok()?, "network": read } }),
            )
        }
        ("stops", "read_pick") => {
            // The fall pick (from 2026-10-19) in a set whose next pick
            // is 2027-01-04, and a retired pick between that bounds
            // nothing. Stop B retires on 2026-11-02, inside the period.
            let (pick, published) = pick_example(net?)?;
            let mut stops = transit_stops::stop_record::seed(&pick);
            stops = stops.add_event("B", serde_json::from_value(json!({"on": "2026-11-02", "event": "retired"})).ok()?);
            let stops = stops.checked().ok()?;
            let read = transit_stops::stop_record::read_pick(&pick, &stops, Some(&published))?;
            (
                json!({ "op": "read_pick", "network_hash": cap_abi::canonical_hash(&pick).ok()?, "stops_hash": cap_abi::canonical_hash(&stops).ok()?, "published": published }),
                json!({ "ok": { "op": "read_pick", "period": read.period, "network_hash": cap_abi::canonical_hash(&read.network).ok()?, "network": read.network, "warnings": read.warnings } }),
            )
        }
        ("stops", "export_pick") => {
            // Stop B moves on 2026-11-02, inside the fall pick's period:
            // B@2026-10-19 before, B after, over the pick's own dates.
            let (pick, published) = pick_example(net?)?;
            let mut stops = transit_stops::stop_record::seed(&pick);
            let b = pick.stops.get("B")?;
            stops = stops.add_event("B", serde_json::from_value(json!({"on": "2026-11-02", "event": "moved", "lat": b.lat + 0.001, "lon": b.lon})).ok()?);
            let stops = stops.checked().ok()?;
            let exported = transit_stops::stop_record::export_pick(&pick, &stops, Some(&published))?;
            (
                json!({ "op": "export_pick", "network_hash": cap_abi::canonical_hash(&pick).ok()?, "stops_hash": cap_abi::canonical_hash(&stops).ok()?, "published": published }),
                json!({ "ok": { "op": "export_pick", "network_hash": cap_abi::canonical_hash(&exported).ok()?, "network": exported } }),
            )
        }
        ("stops", "records") => {
            // A stop the stops app planned (9906, from 2026-10-20): the
            // record a plan takes when its planner picks it; a stop not
            // held answers nothing.
            let stops = transit_stops::Stops::empty()
                .add_event("9906", serde_json::from_value(json!({"on": "2026-10-20", "event": "planned", "lat": 45.58, "lon": -122.348, "name": "Washougal Transit Center", "code": "9906"})).ok()?)
                .checked()
                .ok()?;
            let ids = vec!["9906".to_string(), "9999".to_string()];
            let records = transit_stops::stop_record::records(&stops, &ids, chrono::NaiveDate::from_ymd_opt(2026, 11, 13)?);
            (
                json!({ "op": "records", "stops_hash": cap_abi::canonical_hash(&stops).ok()?, "stop_ids": ids, "on": "2026-11-13" }),
                json!({ "ok": { "op": "records", "records": records } }),
            )
        }
        ("stops", "take_stops") => {
            // A new route visits 9906 (planned 2026-10-20, in the stops
            // document) and a stop the plan holds: the plan's own is
            // left, 9906 is taken with its record.
            let net = net?.derived_network().clone();
            let stops = transit_stops::Stops::empty()
                .add_event("9906", serde_json::from_value(json!({"on": "2026-10-20", "event": "planned", "lat": 45.58, "lon": -122.348, "name": "Washougal Transit Center", "code": "9906"})).ok()?)
                .checked()
                .ok()?;
            let held = net.stops.keys().next()?.clone();
            let ids = vec!["9906".to_string(), held];
            let edits = transit_stops::stop_record::take_stops(&net, &stops, &ids, chrono::NaiveDate::from_ymd_opt(2026, 11, 13)?).ok()?;
            (
                json!({ "op": "take_stops", "network_hash": cap_abi::canonical_hash(&net).ok()?, "stops_hash": cap_abi::canonical_hash(&stops).ok()?, "stop_ids": ids, "on": "2026-11-13" }),
                json!({ "ok": { "op": "take_stops", "edits": edits } }),
            )
        }
        ("stops", "visit_warnings") => {
            // Stop B retired on 2026-11-02, inside a pick's period: the
            // planner is warned from that day.
            let net = net?;
            let mut stops = transit_stops::stop_record::seed(net.derived_network());
            stops = stops.add_event("B", serde_json::from_value(json!({"on": "2026-11-02", "event": "retired"})).ok()?);
            let stops = stops.checked().ok()?;
            let (from, until) = (chrono::NaiveDate::from_ymd_opt(2026, 10, 1)?, chrono::NaiveDate::from_ymd_opt(2026, 12, 1)?);
            (
                json!({ "op": "visit_warnings", "network_hash": net.network_hash, "stops_hash": cap_abi::canonical_hash(&stops).ok()?, "from": "2026-10-01", "until": "2026-12-01" }),
                json!({ "ok": { "op": "visit_warnings", "warnings": transit_stops::stop_record::visit_warnings(net.derived_network(), &stops, from, Some(until)) } }),
            )
        }
        ("stops", "export_versions") => {
            // Stop B moved on 2026-10-15, inside an export from
            // 2026-10-01 to 2026-12-01: B@2026-10-01 before, B after,
            // and the patterns visiting it split on that date.
            let net = net?;
            let mut stops = transit_stops::stop_record::seed(net.derived_network());
            let b = net.derived_network().stops.get("B")?;
            stops = stops.add_event("B", serde_json::from_value(json!({"on": "2026-10-15", "event": "moved", "lat": b.lat + 0.001, "lon": b.lon})).ok()?);
            let stops = stops.checked().ok()?;
            let (from, until) = (chrono::NaiveDate::from_ymd_opt(2026, 10, 1)?, chrono::NaiveDate::from_ymd_opt(2026, 12, 1)?);
            let exported = transit_stops::stop_record::export_versions(net.derived_network(), &stops, from, Some(until));
            (
                json!({ "op": "export_versions", "network_hash": net.network_hash, "stops_hash": cap_abi::canonical_hash(&stops).ok()?, "from": "2026-10-01", "until": "2026-12-01" }),
                json!({ "ok": { "op": "export_versions", "network_hash": cap_abi::canonical_hash(&exported).ok()?, "network": exported } }),
            )
        }
        ("stops", "import_events") => {
            // The feed (the fixture's network) against a stops document
            // that lacks stop C: C is recorded in service on the feed's
            // first date.
            let net = net?;
            let mut stops = transit_stops::stop_record::seed(net.derived_network());
            stops.stops.remove("C");
            let stops = stops.checked().ok()?;
            let on = chrono::NaiveDate::from_ymd_opt(2026, 10, 1)?;
            let imported = transit_stops::stop_record::import_events(&stops, net.derived_network(), on);
            (
                json!({ "op": "import_events", "stops_hash": cap_abi::canonical_hash(&stops).ok()?, "network_hash": net.network_hash, "on": "2026-10-01" }),
                json!({ "ok": { "op": "import_events", "stops_hash": cap_abi::canonical_hash(&imported.stops).ok()?, "stops": imported.stops, "on": "2026-10-01",
                    "added": imported.added, "unlisted": imported.unlisted, "unapplied": imported.unapplied } }),
            )
        }
        ("stops", "feed_events") => {
            // The fixture's feed recorded stop C, which the document lacked.
            let net = net?;
            let mut stops = transit_stops::stop_record::seed(net.derived_network());
            stops.stops.remove("C");
            let on = chrono::NaiveDate::from_ymd_opt(2026, 10, 1)?;
            let imported = transit_stops::stop_record::import_feed(&stops.checked().ok()?, net.derived_network(), on, "sha256:feed").stops;
            (
                json!({ "op": "feed_events", "stops_hash": cap_abi::canonical_hash(&imported).ok()?, "feed": "sha256:feed" }),
                json!({ "ok": { "op": "feed_events", "events": transit_stops::stop_record::feed_events(&imported, "sha256:feed") } }),
            )
        }
        ("stops", "questions") => (
            json!({ "op": "questions" }),
            json!({ "ok": { "op": "questions",
                "site": transit_stops::SITE.iter().map(|(key, answer)| json!({ "key": key, "answer": answer })).collect::<Vec<_>>(),
                "amenities": transit_stops::AMENITIES } }),
        ),
        ("stops", "attribute_surveys") => {
            // 7001 surveyed: the name the page sent is replaced by the
            // signed-in writer's.
            let stops = fixture_stops().checked().ok()?;
            let event: transit_stops::Event = serde_json::from_value(json!({"on": "2027-02-01", "event": "changed",
                "surveyed_by": "", "site": {"position": "far_side", "surface": "concrete"}, "refs": {"sign": "S-1180"}})).ok()?;
            let next = stops.add_event("7001", event);
            (
                json!({ "op": "attribute_surveys", "stops_hash": cap_abi::canonical_hash(&stops).ok()?, "stops": next, "by": "Rosa Ortiz" }),
                json!({ "ok": { "op": "attribute_surveys", "stops": stops.attribute_surveys(next.clone(), "Rosa Ortiz") } }),
            )
        }
        ("kernel", "load_stop_requests") => {
            let requests = fixture_requests().checked().ok()?;
            (
                json!({ "op": "load_stop_requests", "requests": requests }),
                json!({ "ok": { "op": "load_stop_requests", "requests_hash": cap_abi::canonical_hash(&requests).ok()?, "requests": requests.requests.len() } }),
            )
        }
        ("requests", "validate") => {
            let requests = fixture_requests().checked().ok()?;
            let document = cap_abi::canonical_bytes(&requests).ok()?;
            (
                json!({ "op": "validate", "requests": requests }),
                json!({ "ok": { "op": "validate", "refusals": [], "document": document, "document_hash": cap_abi::content_hash(document.as_bytes()) } }),
            )
        }
        ("requests", "requests_for") => {
            let requests = fixture_requests().checked().ok()?;
            let listed: Vec<_> = requests.for_stop("7001").into_iter().map(|(id, r)| json!({ "id": id, "request": r })).collect();
            (
                json!({ "op": "requests_for", "requests_hash": cap_abi::canonical_hash(&requests).ok()?, "stop": "7001" }),
                json!({ "ok": { "op": "requests_for", "requests": listed } }),
            )
        }
        ("requests", "list") => {
            // The work in flight across every stop: drafted and submitted.
            let requests = fixture_requests().checked().ok()?;
            let statuses = vec!["drafted".to_string(), "submitted".to_string()];
            let listed: Vec<_> = requests.list(Some(&statuses), None).into_iter().map(|(id, r)| json!({ "id": id, "request": r })).collect();
            (
                json!({ "op": "list", "requests_hash": cap_abi::canonical_hash(&requests).ok()?, "statuses": statuses }),
                json!({ "ok": { "op": "list", "requests": listed, "counts": requests.counts() } }),
            )
        }
        ("requests", "put") => {
            // The repair at 6124 submitted.
            let requests = fixture_requests().checked().ok()?;
            let request: transit_requests::StopRequest = serde_json::from_value(json!({"stop": "6124", "job": "repair_improve", "status": "submitted", "requested_on": "2027-02-01", "note": "pad cracked at the rear door"})).ok()?;
            let next = requests.put("R-2027-020", request.clone()).checked().ok()?;
            (
                json!({ "op": "put", "requests_hash": cap_abi::canonical_hash(&requests).ok()?, "id": "R-2027-020", "request": request }),
                json!({ "ok": { "op": "put", "refusals": [], "requests_hash": cap_abi::canonical_hash(&next).ok()?, "requests": next } }),
            )
        }
        ("requests", "work_order_call") => {
            // The repair at 6124, ordered: a refresh reads WO-90001.
            let requests = fixture_requests().checked().ok()?;
            let id = "R-2027-020";
            let request: transit_requests::StopRequest = serde_json::from_value(json!({"stop": "6124", "job": "repair_improve", "status": "ordered", "requested_on": "2027-02-01",
                "work_order": {"system": "Maintenance", "id": "WO-90001", "status": "open", "read_on": "2027-02-02"}})).ok()?;
            let held = requests.put(id, request).checked().ok()?;
            let call = held.work_order_call(id, "refresh").ok()?;
            (
                json!({ "op": "work_order_call", "requests_hash": cap_abi::canonical_hash(&held).ok()?, "id": id, "action": "refresh" }),
                json!({ "ok": { "op": "work_order_call", "call": call } }),
            )
        }
        ("requests", "record_work_order") => {
            // The repair at 6124: the work order system opened WO-90001.
            let requests = fixture_requests().checked().ok()?;
            let answer = transit_requests::WorkOrder { system: Some("Maintenance".into()), id: "WO-90001".into(), status: Some("open".into()), read_on: Some("2027-02-02".into()), ..Default::default() };
            let next = requests.record_work_order("R-2027-020", answer.clone())?.checked().ok()?;
            (
                json!({ "op": "record_work_order", "requests_hash": cap_abi::canonical_hash(&requests).ok()?, "id": "R-2027-020", "work_order": answer }),
                json!({ "ok": { "op": "record_work_order", "refusals": [], "requests_hash": cap_abi::canonical_hash(&next).ok()?, "requests": next } }),
            )
        }
        ("stops", "vocabulary") => (
            json!({ "op": "vocabulary" }),
            json!({ "ok": { "op": "vocabulary", "statuses": transit_stops::Status::ALL.iter().map(|s| s.as_str()).collect::<Vec<_>>() } }),
        ),
        ("requests", "vocabulary") => (
            json!({ "op": "vocabulary" }),
            json!({ "ok": { "op": "vocabulary", "jobs": transit_requests::JOBS, "statuses": transit_requests::STATUSES, "open_statuses": transit_requests::OPEN_STATUSES,
                "sides": transit_requests::SIDES, "directions": transit_requests::DIRECTIONS, "markings": transit_requests::MARKINGS } }),
        ),
        ("kernel", "load_service_alerts") => {
            let alerts = fixture_alerts().checked().ok()?;
            (
                json!({ "op": "load_service_alerts", "alerts": alerts }),
                json!({ "ok": { "op": "load_service_alerts", "alerts_hash": cap_abi::canonical_hash(&alerts).ok()?, "alerts": alerts.alerts.len() } }),
            )
        }
        ("kernel", "load_alert_templates") => {
            let templates = fixture_templates().checked().ok()?;
            (
                json!({ "op": "load_alert_templates", "templates": templates }),
                json!({ "ok": { "op": "load_alert_templates", "templates_hash": cap_abi::canonical_hash(&templates).ok()?, "templates": templates.templates.len() } }),
            )
        }
        ("kernel", "load_notes") => {
            let notes = fixture_notes().checked().ok()?;
            (
                json!({ "op": "load_notes", "notes": notes }),
                json!({ "ok": { "op": "load_notes", "notes_hash": cap_abi::canonical_hash(&notes).ok()?, "notes": notes.notes.len() } }),
            )
        }
        ("disruptions", "today") | ("disruptions", "gaps") | ("disruptions", "start_alert") => {
            // 2026-10-23 on the fixture network: the edit document's
            // detour, the live feed over the fixture snapshot, the
            // alerts (one explains trip t10's 08:00 instance), and the
            // notes (one on that instance).
            let ctx = net?;
            let edits = transit_sync::EditDocument::new(dispatch_fixture_edits());
            let (live, live_hash) = live_example()?;
            let stops = fixture_stops().checked().ok()?;
            let alerts = fixture_alerts().checked().ok()?;
            let notes = fixture_notes().checked().ok()?;
            let inputs = transit_disruptions::Inputs {
                network: &ctx.network,
                edits: &edits.records,
                live: Some(&live),
                stops: &stops,
                alerts: &alerts,
                notes: &notes,
                date: chrono::NaiveDate::from_ymd_opt(2026, 10, 23)?,
                now: ALERTS_NOW,
            };
            let mut request = json!({ "op": op, "network_hash": ctx.network_hash, "edits_hash": cap_abi::canonical_hash(&edits).ok()?,
                "live_hash": live_hash, "stops_hash": cap_abi::canonical_hash(&stops).ok()?, "alerts_hash": cap_abi::canonical_hash(&alerts).ok()?,
                "notes_hash": cap_abi::canonical_hash(&notes).ok()?, "date": "2026-10-23", "now": ALERTS_NOW });
            let answer = if op == "start_alert" {
                // A new alert for the first gap: its effect, riders, and
                // what it explains.
                let source = transit_disruptions::gaps(&inputs).first()?.source.clone();
                request["source"] = serde_json::to_value(&source).ok()?;
                let mut answer = serde_json::to_value(transit_disruptions::start_alert(&inputs, Some(&source))).ok()?;
                answer["op"] = json!("start_alert");
                answer
            } else if op == "today" {
                let mut answer = serde_json::to_value(transit_disruptions::today(&inputs, &Default::default())).ok()?;
                answer["op"] = json!("today");
                answer
            } else {
                json!({ "op": "gaps", "gaps": transit_disruptions::gaps(&inputs) })
            };
            (request, json!({ "ok": answer }))
        }
        ("notes", "validate") => {
            let notes = fixture_notes().checked().ok()?;
            let document = cap_abi::canonical_bytes(&notes).ok()?;
            (
                json!({ "op": "validate", "notes": notes }),
                json!({ "ok": { "op": "validate", "refusals": [], "document": document, "document_hash": cap_abi::content_hash(document.as_bytes()) } }),
            )
        }
        ("notes", "write") => {
            // Customer service notes a rider's call about the cancelled trip.
            let notes = fixture_notes().checked().ok()?;
            let target = transit_notes::NoteTarget::Trip { trip: "t10".into(), date: "2026-10-23".into(), start_secs: 28800 };
            let next = notes.write("n3", target.clone(), "A rider called. Told them the next trip runs.", "service-1", 1_792_758_000).ok()?;
            (
                json!({ "op": "write", "notes_hash": cap_abi::canonical_hash(&notes).ok()?, "id": "n3", "target": target,
                        "text": "A rider called. Told them the next trip runs.", "by": "service-1", "at": 1_792_758_000u64 }),
                json!({ "ok": { "op": "write", "refusals": [], "notes_hash": cap_abi::canonical_hash(&next).ok()?, "notes": next } }),
            )
        }
        ("notes", "on") => {
            let notes = fixture_notes().checked().ok()?;
            let target = transit_notes::NoteTarget::Trip { trip: "t10".into(), date: "2026-10-23".into(), start_secs: 28800 };
            (
                json!({ "op": "on", "notes_hash": cap_abi::canonical_hash(&notes).ok()?, "target": target }),
                json!({ "ok": { "op": "on", "notes": notes.on(&target) } }),
            )
        }
        ("notes", "between") => {
            let notes = fixture_notes().checked().ok()?;
            (
                json!({ "op": "between", "notes_hash": cap_abi::canonical_hash(&notes).ok()?, "from": NOTES_DAY.0, "to": NOTES_DAY.1 }),
                json!({ "ok": { "op": "between", "notes": notes.between(NOTES_DAY.0, NOTES_DAY.1) } }),
            )
        }
        ("notes", "receive") => {
            // Dispatch notes the first note of the day in an empty log:
            // the server stamps it with the verified writer and its clock.
            let target = transit_notes::NoteTarget::Trip { trip: "t10".into(), date: "2026-10-23".into(), start_secs: 28800 };
            let text = "No operator. Trip cancelled.";
            let next = transit_notes::Notes::default().write("n1", target.clone(), text, "dispatch-1", 1_792_752_000).ok()?;
            let records = transit_log::document_records(&serde_json::to_value(&next).ok()?, "notes", "note", "notes", transit_notes::SCHEMA_VERSION).ok()?;
            let write = receive_write("notes", "dispatch-1", 1_792_752_000_000);
            let mut received = transit_log::receive(&write, &records, &cap_abi::record_hash, &|e| cap_abi::canonical_bytes(e)).ok()?;
            received.heads.insert("notes".into(), cap_abi::canonical_hash(&next).ok()?);
            (
                json!({ "op": "receive", "write": write, "ops": [{ "cap": "notes", "op": "write", "args": { "id": "n1", "target": target, "text": text } }] }),
                json!({ "ok": receive_answer(&received)? }),
            )
        }
        ("notes", "log_records") => {
            let notes = fixture_notes().checked().ok()?;
            let next = transit_log::document_records(&serde_json::to_value(&notes).ok()?, "notes", "note", "notes", transit_notes::SCHEMA_VERSION).ok()?;
            (
                json!({ "op": "log_records", "notes_hash": cap_abi::canonical_hash(&notes).ok()? }),
                json!({ "ok": { "op": "log_records", "next": next } }),
            )
        }
        ("notes", "log_document") => {
            // The notes document assembled from the log's records.
            let notes = fixture_notes().checked().ok()?;
            let records: std::collections::BTreeMap<String, serde_json::Value> =
                notes.notes.iter().map(|(id, n)| (id.clone(), json!({ "version": transit_notes::SCHEMA_VERSION, "value": n }))).collect();
            (
                json!({ "op": "log_document", "records": records }),
                json!({ "ok": { "op": "log_document", "document_hash": cap_abi::canonical_hash(&notes).ok()?, "document": cap_abi::canonical_bytes(&notes).ok()? } }),
            )
        }
        ("alerts", "validate") => {
            let alerts = fixture_alerts().checked().ok()?;
            let document = cap_abi::canonical_bytes(&alerts).ok()?;
            (
                json!({ "op": "validate", "alerts": alerts }),
                json!({ "ok": { "op": "validate", "refusals": [], "document": document, "document_hash": cap_abi::content_hash(document.as_bytes()) } }),
            )
        }
        ("alerts", "put") => {
            // Snow: an agency-wide alert, published now.
            let alerts = fixture_alerts().checked().ok()?;
            let alert: transit_alerts::ServiceAlert = serde_json::from_value(json!({
                "status": "published", "cause": "weather", "effect": "significant_delays",
                "header": [{"text": "Snow: expect delays on every route"}], "informed": [{}]
            })).ok()?;
            let next = alerts.put("snow", alert.clone()).checked().ok()?;
            (
                json!({ "op": "put", "alerts_hash": cap_abi::canonical_hash(&alerts).ok()?, "id": "snow", "alert": alert }),
                json!({ "ok": { "op": "put", "refusals": [], "alerts_hash": cap_abi::canonical_hash(&next).ok()?, "alerts": next } }),
            )
        }
        ("alerts", "end") => {
            // The detour lifts now.
            let alerts = fixture_alerts().checked().ok()?;
            let next = alerts.end("detour-10", ALERTS_NOW)?.checked().ok()?;
            (
                json!({ "op": "end", "alerts_hash": cap_abi::canonical_hash(&alerts).ok()?, "id": "detour-10", "at": ALERTS_NOW }),
                json!({ "ok": { "op": "end", "refusals": [], "alerts_hash": cap_abi::canonical_hash(&next).ok()?, "alerts": next } }),
            )
        }
        ("alerts", "list") => {
            let alerts = fixture_alerts().checked().ok()?;
            let mut answer = serde_json::to_value(alerts.list(ALERTS_NOW)).ok()?;
            answer["op"] = json!("list");
            (
                json!({ "op": "list", "alerts_hash": cap_abi::canonical_hash(&alerts).ok()?, "now": ALERTS_NOW }),
                json!({ "ok": answer }),
            )
        }
        ("alerts", "flags") => {
            // The deployment speaks English and Spanish.
            let ctx = net?;
            let alerts = fixture_alerts().checked().ok()?;
            let languages = vec!["en".to_string(), "es".to_string()];
            (
                json!({ "op": "flags", "alerts_hash": cap_abi::canonical_hash(&alerts).ok()?, "network_hash": ctx.network_hash, "languages": languages }),
                json!({ "ok": { "op": "flags", "flags": alerts.flags(ctx.derived_network(), &languages) } }),
            )
        }
        ("alerts", "validate_templates") => {
            let templates = fixture_templates().checked().ok()?;
            let document = cap_abi::canonical_bytes(&templates).ok()?;
            (
                json!({ "op": "validate_templates", "templates": templates }),
                json!({ "ok": { "op": "validate_templates", "refusals": [], "document": document, "document_hash": cap_abi::content_hash(document.as_bytes()) } }),
            )
        }
        ("alerts", "put_template") => {
            // A detour template beside the closure.
            let templates = fixture_templates().checked().ok()?;
            let template: transit_alerts::AlertTemplate = serde_json::from_value(json!({
                "name": "Detour", "cause": "construction", "effect": "detour",
                "header": [{"text": "Route {route} is on detour from {start date}"}]
            })).ok()?;
            let next = templates.put("detour", template.clone()).checked().ok()?;
            (
                json!({ "op": "put_template", "templates_hash": cap_abi::canonical_hash(&templates).ok()?, "id": "detour", "template": template }),
                json!({ "ok": { "op": "put_template", "refusals": [], "templates_hash": cap_abi::canonical_hash(&next).ok()?, "templates": next } }),
            )
        }
        ("alerts", "templates") => {
            let templates = fixture_templates().checked().ok()?;
            (
                json!({ "op": "templates", "templates_hash": cap_abi::canonical_hash(&templates).ok()? }),
                json!({ "ok": { "op": "templates", "templates": templates.offered() } }),
            )
        }
        ("alerts", "fill") => {
            // The closure at stop A, served by routes 10 and 20. The
            // Spanish header's {stop} fills; its dates are not written.
            let ctx = net?;
            let templates = fixture_templates().checked().ok()?;
            let input = transit_alerts::FillInput {
                stops: vec!["A".into()],
                routes: vec!["10".into(), "20".into()],
                informed: vec![],
                start_date: Some("2026-10-26".into()),
                end_date: Some("2026-11-20".into()),
            };
            let filled = transit_alerts::fill(&templates.templates["stop-closure"], ctx.derived_network(), &input);
            (
                json!({ "op": "fill", "templates_hash": cap_abi::canonical_hash(&templates).ok()?, "template": "stop-closure",
                        "network_hash": ctx.network_hash, "input": input }),
                json!({ "ok": { "op": "fill", "filled": filled } }),
            )
        }
        ("alerts", "receive") => {
            // The first alert, put in an empty alerts log.
            let alert = fixture_alerts().alerts.values().next()?.clone();
            let next = transit_alerts::ServiceAlerts::default().put("a1", alert.clone()).checked().ok()?;
            let records = transit_log::document_records(&serde_json::to_value(&next).ok()?, "alerts", "service_alert", "service_alerts", transit_alerts::ALERTS_VERSION).ok()?;
            let write = receive_write("alerts", "dispatch-1", ALERTS_NOW * 1000);
            let mut received = transit_log::receive(&write, &records, &cap_abi::record_hash, &|e| cap_abi::canonical_bytes(e)).ok()?;
            received.heads.insert("service_alerts".into(), cap_abi::canonical_hash(&next).ok()?);
            (
                json!({ "op": "receive", "write": write, "ops": [{ "cap": "alerts", "op": "put", "args": { "id": "a1", "alert": alert } }] }),
                json!({ "ok": receive_answer(&received)? }),
            )
        }
        ("alerts", "log_records") => {
            let alerts = fixture_alerts().checked().ok()?;
            let templates = fixture_templates().checked().ok()?;
            let mut next = transit_log::document_records(&serde_json::to_value(&alerts).ok()?, "alerts", "service_alert", "service_alerts", transit_alerts::ALERTS_VERSION).ok()?;
            next.extend(transit_log::document_records(&serde_json::to_value(&templates).ok()?, "templates", "alert_template", "alert_templates", transit_alerts::TEMPLATES_VERSION).ok()?);
            (
                json!({ "op": "log_records", "alerts_hash": cap_abi::canonical_hash(&alerts).ok()?, "templates_hash": cap_abi::canonical_hash(&templates).ok()? }),
                json!({ "ok": { "op": "log_records", "next": next } }),
            )
        }
        ("alerts", "log_document") => {
            // The alerts document assembled from the log's records.
            let alerts = fixture_alerts().checked().ok()?;
            let records: std::collections::BTreeMap<String, serde_json::Value> = alerts
                .alerts
                .iter()
                .map(|(id, a)| (id.clone(), json!({ "version": transit_alerts::ALERTS_VERSION, "value": a })))
                .collect();
            (
                json!({ "op": "log_document", "kind": "service_alerts", "records": records }),
                json!({ "ok": { "op": "log_document", "document_hash": cap_abi::canonical_hash(&alerts).ok()?, "document": cap_abi::canonical_bytes(&alerts).ok()? } }),
            )
        }
        ("rt", "read_alerts") => {
            // The fixture alerts' feed read back into the document.
            let ctx = net?;
            let alerts = fixture_alerts().checked().ok()?;
            let bytes = transit_rt::encode_alerts(ctx.derived_network(), &alerts, ALERTS_NOW).bytes;
            let read = transit_rt::read_alerts(ctx.derived_network(), &bytes).ok()?;
            (
                json!({ "op": "read_alerts", "network_hash": ctx.network_hash, "bytes": bytes }),
                json!({ "ok": { "op": "read_alerts", "alerts_hash": cap_abi::canonical_hash(&read.alerts).ok()?, "alerts": read.alerts,
                                "skipped": read.skipped, "dropped": read.dropped } }),
            )
        }
        ("rt", "encode_alerts") => {
            // The fixture alerts as the Alerts feed riders' apps read.
            let ctx = net?;
            let alerts = fixture_alerts().checked().ok()?;
            let encoded = transit_rt::encode_alerts(ctx.derived_network(), &alerts, ALERTS_NOW);
            (
                json!({ "op": "encode_alerts", "network_hash": ctx.network_hash, "alerts_hash": cap_abi::canonical_hash(&alerts).ok()?, "timestamp": ALERTS_NOW }),
                json!({ "ok": { "op": "encode_alerts", "rt_hash": cap_abi::content_hash(&encoded.bytes), "feed_b64": transit_codec::base64_encode(&encoded.bytes), "skipped": encoded.skipped } }),
            )
        }
        ("kernel", "load_work_rules") => {
            let rules = fixture_work_rules().checked().ok()?;
            let work_rules_hash = cap_abi::canonical_hash(&rules).ok()?;
            (
                json!({ "op": "load_work_rules", "work_rules": fixture_work_rules() }),
                json!({ "ok": { "op": "load_work_rules", "work_rules_hash": work_rules_hash, "version": rules.schema_version.0 } }),
            )
        }
        ("runcut", "recovery") => {
            let ctx = net?;
            let date = chrono::NaiveDate::from_ymd_opt(2026, 10, 23).unwrap();
            let day = transit_network::materialize(ctx.derived_network(), date).expect("fixture day");
            let rules = fixture_work_rules().checked().ok()?;
            (
                json!({ "op": "recovery", "network_hash": ctx.network_hash, "date": "2026-10-23",
                    "work_rules_hash": "<of the loaded work rules>" }),
                json!({ "ok": { "op": "recovery",
                    "recovery": transit_runs::recovery(ctx.derived_network(), &day, Some(&rules)) } }),
            )
        }
        ("runcut", "suggest_blocks") => {
            let ctx = net?;
            let date = chrono::NaiveDate::from_ymd_opt(2026, 10, 23).unwrap();
            let day = transit_network::materialize(ctx.derived_network(), date).expect("fixture day");
            let rules = fixture_work_rules().checked().ok()?;
            (
                json!({ "op": "suggest_blocks", "network_hash": ctx.network_hash, "date": "2026-10-23",
                    "work_rules_hash": "<of the loaded work rules>" }),
                json!({ "ok": { "op": "suggest_blocks",
                    "suggestion": transit_runs::suggest_blocks(ctx.derived_network(), &day, Some(&rules)) } }),
            )
        }
        ("kernel", "load_watchlist") => {
            let watchlist = fixture_watchlist();
            let watchlist_hash = cap_abi::canonical_hash(&watchlist).ok()?;
            (
                json!({ "op": "load_watchlist", "document": watchlist }),
                json!({ "ok": { "op": "load_watchlist", "watchlist_hash": watchlist_hash, "version": 2 } }),
            )
        }
        ("migrate", "migrate") => {
            let v1 = json!([{ "id": "e1", "status": "accepted", "rev": 1, "updated_at": 1000,
                "cancel": { "id": "c1", "trip_id": "t1", "start_secs": 28800, "date": "2026-10-23" } }]);
            let (document, notes) = transit_sync::version::migrate_to(v1.clone(), transit_sync::SCHEMA_VERSION).ok()?;
            let typed: transit_sync::EditDocument = serde_json::from_value(document.clone()).ok()?;
            let canonical = cap_abi::canonical_bytes(&typed).ok()?;
            let document_hash = cap_abi::canonical_hash(&typed).ok()?;
            (
                json!({ "op": "migrate", "kind": "edit_records", "document": v1 }),
                json!({ "ok": { "op": "migrate", "document": document, "canonical": canonical, "document_hash": document_hash,
                    "from": 1, "to": transit_sync::SCHEMA_VERSION, "notes": notes } }),
            )
        }
        ("migrate", "version") => (
            json!({ "op": "version", "kind": "watchlist", "document": { "version": "v1", "stops": [] } }),
            json!({ "ok": { "op": "version", "version": 1, "reads": 2 } }),
        ),
        ("board", "watchlist_board") => {
            let ctx = net?;
            let date = chrono::NaiveDate::from_ymd_opt(2026, 10, 23).unwrap();
            let day = transit_network::materialize(ctx.derived_network(), date).expect("fixture day");
            let network = ctx.derived_network();
            let board = |stop: &str| transit_dispatch::stop_board(&network, &day, stop, 25200, 3, None, None);
            (
                json!({ "op": "watchlist_board", "watchlist_hash": cap_abi::canonical_hash(&fixture_watchlist()).ok()?,
                    "network_hash": ctx.network_hash, "date": "2026-10-23", "from_secs": 25200, "limit": 3 }),
                json!({ "ok": { "op": "watchlist_board", "stops": [
                    { "id": "A", "label": "home", "board": board("A") },
                    { "id": "B", "label": "work", "board": board("B") }
                ] } }),
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
                    "trips": transit_dispatch::block_chain(ctx.derived_network(), &day, "B1", transit_dispatch::TIGHT_LAYOVER_SECS, None) } }),
            )
        }
        ("runcut", "trip_stops") => {
            let ctx = net?;
            let date = chrono::NaiveDate::from_ymd_opt(2026, 10, 23).unwrap();
            let records = dispatch_fixture_edits();
            let day = transit_dispatch::effective_day(ctx.derived_network(), date, &records)
                .expect("fixture day");
            // t30: the via-C run at 08:40 — the
            // patched day keeps A→B (C skipped, +300 s).
            let trip = day
                .iter()
                .find(|t| t.stop_times.len() == 2 && t.start_secs == 8 * 3600 + 40 * 60)
                .expect("the diverted trip is in the day");
            (
                json!({ "op": "trip_stops", "network_hash": ctx.network_hash,
                    "edits_hash": "<of the loaded edit document>",
                    "trip": trip.trip, "start_secs": trip.start_secs, "date": "2026-10-23" }),
                json!({ "ok": { "op": "trip_stops",
                    "stops": transit_dispatch::trip_stops(
                        ctx.derived_network(), &day, &trip.trip, trip.start_secs) } }),
            )
        }
        ("runcut", "trip_path") => {
            let ctx = net?;
            let date = chrono::NaiveDate::from_ymd_opt(2026, 10, 23).unwrap();
            let records = dispatch_fixture_edits();
            let day = transit_dispatch::effective_day(ctx.derived_network(), date, &records).ok()?;
            let diversions = transit_sync::in_force_on(&records, date);
            // The diverted 08:40 trip draws the diversion's line (A→B).
            let trip = day.iter().find(|t| t.stop_times.len() == 2 && t.start_secs == 8 * 3600 + 40 * 60)?;
            (
                json!({ "op": "trip_path", "network_hash": ctx.network_hash,
                    "edits_hash": "<of the loaded edit document>",
                    "trip": trip.trip, "start_secs": trip.start_secs, "date": "2026-10-23" }),
                json!({ "ok": { "op": "trip_path",
                    "path": transit_dispatch::trip_path(ctx.derived_network(), &day, &diversions, &trip.trip, trip.start_secs) } }),
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
        ("directory", "about") => {
            let ctx = net?;
            let network = ctx.derived_network();
            (
                json!({ "op": "about", "network_hash": ctx.network_hash }),
                json!({ "ok": { "op": "about", "name": network.name, "plan": network.plan } }),
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
        ("directory", "stops_near") => {
            // Standing on the station's Bay A: the station answers once
            // (0 m — its nearest member), then the plain stops by distance.
            let ctx = net?;
            (
                json!({ "op": "stops_near", "network_hash": ctx.network_hash, "lat": 45.515, "lon": -73.5155, "limit": 3 }),
                json!({ "ok": { "op": "stops_near",
                    "stops": transit_network::listing::stops_near(ctx.derived_network(), 45.515, -73.5155, 3, None) } }),
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
                .map(|(id, r)| json!({ "route_id": id, "name": r.label(), "long_name": r.long_name, "mode": r.mode, "color": r.color }))
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
        ("directory", "service_keys") => {
            let ctx = net?;
            (
                json!({ "op": "service_keys", "network_hash": ctx.network_hash }),
                json!({ "ok": { "op": "service_keys", "keys": transit_network::service_keys(ctx.derived_network()) } }),
            )
        }
        ("directory", "route_lines") => {
            let ctx = net?;
            (
                json!({ "op": "route_lines", "network_hash": ctx.network_hash }),
                json!({ "ok": { "op": "route_lines", "routes": transit_network::route_lines(ctx.derived_network(), None) } }),
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
                json!({ "op": "timetable", "network_hash": ctx.network_hash, "route_id": "10", "direction": "0", "members": ["WK"] }),
                json!({ "ok": { "op": "timetable",
                    "timetable": transit_network::timetable(ctx.derived_network(), "10", Some("0"), &["WK".to_string()]) } }),
            )
        }
        ("directory", "stop_detail") => {
            let ctx = net?;
            (
                json!({ "op": "stop_detail", "network_hash": ctx.network_hash, "stop_id": "B", "members": ["WK"] }),
                json!({ "ok": { "op": "stop_detail",
                    "detail": transit_network::stop_detail(ctx.derived_network(), "B", Some(&["WK".to_string()])) } }),
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
        ("streets", "walks") => {
            // The walking fixture: a residential street through the
            // station's bays, a motorway across it no rider walks.
            let bytes = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../../web/e2e/fixtures/walk-streets.geojson")).ok()?;
            let streets = transit_streets::geojson::import(&bytes).ok()?;
            let streets_hash = cap_abi::canonical_hash(&streets).ok()?;
            let net = net?;
            let stops: Vec<(String, f64, f64)> =
                net.derived_network().stops.iter().map(|(id, s)| (id.clone(), s.lat, s.lon)).collect();
            let table = transit_streets::walking::WalkTable { kind: Default::default(), schema_version: Default::default(),
                streets_hash: streets_hash.clone(),
                network_hash: net.network_hash.clone(),
                speed_mps: transit_streets::walking::WALK_SPEED_MPS,
                max_walk_secs: transit_streets::walking::MAX_WALK_SECS,
                walks: transit_streets::walking::walks(
                    &streets,
                    &stops,
                    transit_streets::walking::WALK_SPEED_MPS,
                    transit_streets::walking::MAX_WALK_SECS,
                ),
            };
            let walks_hash = cap_abi::canonical_hash(&table).ok()?;
            (
                json!({ "op": "walks", "streets_hash": streets_hash, "network_hash": net.network_hash }),
                json!({ "ok": { "op": "walks", "walks_hash": walks_hash, "walks": table } }),
            )
        }
        ("streets", "access") => {
            // From a dropped pin ~22 m off Station Rd, west of the
            // bays, with a destination east of them.
            let bytes = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../../web/e2e/fixtures/walk-streets.geojson")).ok()?;
            let streets = transit_streets::geojson::import(&bytes).ok()?;
            let streets_hash = cap_abi::canonical_hash(&streets).ok()?;
            let net = net?;
            let stops: Vec<(String, f64, f64)> =
                net.derived_network().stops.iter().map(|(id, s)| (id.clone(), s.lat, s.lon)).collect();
            let graph = transit_streets::walking::WalkGraph::new(&streets);
            let (speed, max) = (transit_streets::walking::WALK_SPEED_MPS, transit_streets::walking::MAX_WALK_SECS);
            let walks = graph.walks_from(45.5152, -73.5165, &stops, speed, max);
            let direct = graph.walks_from(45.5152, -73.5165, &[(String::new(), 45.5148, -73.5135)], speed, max).pop()?;
            (
                json!({ "op": "access", "streets_hash": streets_hash, "network_hash": net.network_hash,
                        "point": { "lat": 45.5152, "lon": -73.5165 }, "to": { "lat": 45.5148, "lon": -73.5135 } }),
                json!({ "ok": { "op": "access", "walks": walks,
                                "direct": { "walk_secs": direct.walk_secs, "path": direct.path } } }),
            )
        }
        ("kernel", "load_streets") => {
            let ctx = streets_example_context()?;
            (
                json!({ "op": "load_streets", "streets": ctx.streets }),
                json!({ "ok": { "op": "load_streets", "streets_hash": ctx.streets_hash,
                    "ways": ctx.streets.ways.len(), "segments": ctx.segments, "nodes": ctx.nodes } }),
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
                json!({ "ok": { "op": "route", "points": ctx.points, "length_m": ctx.length_m,
                    "streets": ctx.streets_along,
                    "to_confirm": ctx.streets_along.iter()
                        .filter(|s| transit_streets::bus::confirms(s.class)).collect::<Vec<_>>() } }),
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
            let document = cap_abi::canonical_bytes(&transit_sync::EditDocument::new(records.clone())).unwrap();
            let document_hash = cap_abi::canonical_hash(&transit_sync::EditDocument::new(records.clone())).unwrap();
            (
                json!({ "op": "sync_edits", "client": draft, "server": [], "now_ms": now_ms }),
                json!({ "ok": { "op": "sync_edits", "records": records, "document": document, "document_hash": document_hash } }),
            )
        }
        ("sync", "read_edits") => {
            let stored = serde_json::to_value(transit_sync::EditDocument::new(dispatch_fixture_edits())).unwrap();
            let (read, dropped) = transit_sync::read_document(&stored).ok()?;
            let records = read.records;
            let document = cap_abi::canonical_bytes(&transit_sync::EditDocument::new(records.clone())).unwrap();
            let document_hash = cap_abi::canonical_hash(&transit_sync::EditDocument::new(records.clone())).unwrap();
            (
                json!({ "op": "read_edits", "document": stored }),
                json!({ "ok": { "op": "read_edits", "records": records, "dropped": dropped, "schema_version": transit_sync::SCHEMA_VERSION, "document": document, "document_hash": document_hash } }),
            )
        }
        ("sync", "compact_edits") => {
            let mut old = dispatch_fixture_edits();
            old[0].status = transit_sync::EditStatus::Retired;
            old[0].updated_at = 1_000;
            let now_ms: u64 = 1_770_000_000_000;
            let horizon_ms: u64 = 90 * 24 * 3600 * 1000;
            let (records, dropped) = transit_sync::compact(&old, now_ms, horizon_ms);
            let document = cap_abi::canonical_bytes(&transit_sync::EditDocument::new(records.clone())).unwrap();
            let document_hash = cap_abi::canonical_hash(&transit_sync::EditDocument::new(records.clone())).unwrap();
            (
                json!({ "op": "compact_edits", "records": old, "now_ms": now_ms, "horizon_ms": horizon_ms }),
                json!({ "ok": { "op": "compact_edits", "records": records, "document": document, "document_hash": document_hash, "dropped": dropped } }),
            )
        }
        ("log", "compose") => {
            let (fold, _) = fixture_stops_log();
            let (stops, _) = fixture_log_records();
            let mut next = stops[0].clone();
            if let Some(events) = next.records.values_mut().next().and_then(|v| v.as_array_mut()) {
                events.push(json!({"id": "e-shelter", "on": "2027-03-01", "event": "changed", "amenities": {"shelter": true}}));
            }
            let header = fixture_log_header(3, 1_770_000_120_000, "POST /logs/stops", Some("tablet-4/18"));
            let answer = match transit_log::compose(&fold, header.clone(), std::slice::from_ref(&next), &log_record_hash) {
                transit_log::Composed::Entry(e) => json!({ "op": "compose", "entry": cap_abi::canonical_bytes(&e).ok()?, "changes": e.changes.len() }),
                _ => return None,
            };
            (json!({ "op": "compose", "checkpoint": fold, "header": header, "next": [next] }), json!({ "ok": answer }))
        }
        ("log", "fold") => {
            let (fold, entries) = fixture_stops_log();
            let stored: Vec<transit_log::Stored> = entries.iter().map(|e| transit_log::Stored { seq: e.seq, text: cap_abi::canonical_bytes(e).unwrap() }).collect();
            (
                json!({ "op": "fold", "log": "stops", "entries": stored }),
                json!({ "ok": { "op": "fold", "seq": fold.seq, "checkpoint": cap_abi::canonical_bytes(&fold).ok()?, "records": fold.records, "quarantined": [] } }),
            )
        }
        ("log", "revert") => {
            let (fold, entries) = fixture_stops_log();
            let stored: Vec<transit_log::Stored> = entries.iter().map(|e| transit_log::Stored { seq: e.seq, text: cap_abi::canonical_bytes(e).unwrap() }).collect();
            let before = transit_log::fold(transit_log::Checkpoint::empty("stops"), &stored[..1], &log_record_hash).ok()?.fold;
            let mut header = fixture_log_header(3, 1_770_000_120_000, "POST /logs/stops/revert", None);
            header.writer = "auditor-2".into();
            let states = transit_log::revert_states(&before, &fold, &entries[1], &log_record_hash).ok()?;
            let changes = transit_log::revert_with(&states, &[], &log_record_hash).ok()?;
            let n = changes.len();
            (
                json!({ "op": "revert", "log": "stops", "entries": stored, "target": 2, "header": header }),
                json!({ "ok": { "op": "revert", "entry": cap_abi::canonical_bytes(&transit_log::Entry::new(header, changes, Some(2))).ok()?, "changes": n, "states": states } }),
            )
        }
        ("log", "close_gap") => {
            let mut header = fixture_log_header(3, 1_770_000_120_000, "POST /logs/stops/close-gap", None);
            header.writer = "auditor-2".into();
            header.intent.note = Some("entry 3 was deleted from the bucket by hand".into());
            let entry = cap_abi::canonical_bytes(&transit_log::Entry::new(header.clone(), Vec::new(), None)).ok()?;
            (json!({ "op": "close_gap", "header": header }), json!({ "ok": { "op": "close_gap", "entry": entry } }))
        }
        ("stops", "log_records") => {
            let stops = fixture_stops().checked().ok()?;
            let (next, _) = fixture_log_records();
            (
                json!({ "op": "log_records", "stops_hash": cap_abi::canonical_hash(&stops).ok()? }),
                json!({ "ok": { "op": "log_records", "next": next } }),
            )
        }
        ("stops", "log_document") => {
            let stops = fixture_stops().checked().ok()?;
            let (mut next, _) = fixture_log_records();
            let own = next.pop()?.records.remove(transit_log::DOCUMENT_ID)?;
            let held: std::collections::BTreeMap<String, transit_log::Held> = next.pop()?.records.into_iter().map(|(k, v)| (k, transit_log::Held { version: 1, value: v })).collect();
            (
                json!({ "op": "log_document", "records": held, "document": transit_log::Held { version: 1, value: own } }),
                json!({ "ok": { "op": "log_document", "stops_hash": cap_abi::canonical_hash(&stops).ok()?, "document": cap_abi::canonical_bytes(&stops).ok()? } }),
            )
        }
        ("requests", "log_records") => {
            let requests = fixture_requests().checked().ok()?;
            let (_, next) = fixture_log_records();
            (
                json!({ "op": "log_records", "requests_hash": cap_abi::canonical_hash(&requests).ok()? }),
                json!({ "ok": { "op": "log_records", "next": next } }),
            )
        }
        ("requests", "log_document") => {
            let requests = fixture_requests().checked().ok()?;
            let (_, mut next) = fixture_log_records();
            let own = next.pop()?.records.remove(transit_log::DOCUMENT_ID)?;
            let held: std::collections::BTreeMap<String, transit_log::Held> = next.pop()?.records.into_iter().map(|(k, v)| (k, transit_log::Held { version: 1, value: v })).collect();
            (
                json!({ "op": "log_document", "records": held, "document": transit_log::Held { version: 1, value: own } }),
                json!({ "ok": { "op": "log_document", "requests_hash": cap_abi::canonical_hash(&requests).ok()?, "document": cap_abi::canonical_bytes(&requests).ok()? } }),
            )
        }
        ("sync", "log_records") => {
            let doc = transit_sync::EditDocument::new(dispatch_fixture_edits());
            let next = transit_log::document_records(&value(&doc), "records", "edit_record", transit_sync::KIND, transit_sync::SCHEMA_VERSION).ok()?;
            (json!({ "op": "log_records", "document": doc }), json!({ "ok": { "op": "log_records", "next": next } }))
        }
        ("sync", "log_document") => {
            let doc = transit_sync::EditDocument::new(dispatch_fixture_edits());
            let records: std::collections::BTreeMap<String, transit_log::Held> =
                doc.records.iter().map(|r| (r.id.clone(), transit_log::Held { version: transit_sync::SCHEMA_VERSION, value: value(r) })).collect();
            (
                json!({ "op": "log_document", "records": records }),
                json!({ "ok": { "op": "log_document", "document": cap_abi::canonical_bytes(&doc).ok()?, "document_hash": cap_abi::canonical_hash(&doc).ok()? } }),
            )
        }
        ("sync", "log_revert") => {
            // The revert of the write that accepted the diversion: it goes
            // back to submitted, stamped to win the merge.
            let accepted = dispatch_fixture_edits().remove(0);
            let submitted = transit_sync::EditRecord { status: transit_sync::EditStatus::Submitted, rev: 1, ..accepted.clone() };
            let held = |r: &transit_sync::EditRecord| transit_log::Held { version: transit_sync::SCHEMA_VERSION, value: value(r) };
            let states = vec![transit_log::RevertState { kind: "edit_record".into(), id: accepted.id.clone(), now: Some(held(&accepted)), prior: Some(held(&submitted)) }];
            let now_ms = 1_770_000_120_000;
            let back = transit_sync::revert_record(Some(&submitted), Some(&accepted), now_ms)?;
            let written = vec![transit_log::Written { kind: "edit_record".into(), id: back.id.clone(), version: transit_sync::SCHEMA_VERSION, after: Some(value(&back)) }];
            (json!({ "op": "log_revert", "states": states, "now_ms": now_ms }), json!({ "ok": { "op": "log_revert", "written": written } }))
        }
        // The authorization capability's example computes the REAL
        // fail-closed decision through the same library the module
        // dispatches through.
        ("authz", "authorize") => {
            let policy = transit_authz::Policy::current(vec![transit_authz::Rule {
                name: "ops-write-edits".into(),
                roles: vec!["dispatcher".into()],
                pointers: vec!["edits".into()],
                actions: vec!["write".into()],
                extra: Default::default(),
            }]);
            let identity = transit_authz::Identity { sub: "dispatcher-7".into(), roles: vec!["dispatcher".into()] };
            let decision = transit_authz::authorize(&policy, &identity, transit_authz::Action::Write, "edits");
            (
                json!({ "op": "authorize", "identity": identity, "action": "write", "pointer": "edits", "policy": policy }),
                json!({ "ok": { "op": "authorize", "decision": decision } }),
            )
        }
        ("authz", "effective") => {
            let policy = transit_authz::Policy::current(vec![
                transit_authz::Rule {
                    name: "ops-write-edits".into(),
                    roles: vec!["dispatcher".into()],
                    pointers: vec!["edits".into()],
                    actions: vec!["read".into(), "write".into()],
                    extra: Default::default(),
                },
                transit_authz::Rule {
                    name: "admin-all".into(),
                    roles: vec!["admin".into()],
                    pointers: vec!["*".into()],
                    actions: vec!["*".into()],
                    extra: Default::default(),
                },
            ]);
            let identity = transit_authz::Identity { sub: "dispatcher-7".into(), roles: vec!["dispatcher".into()] };
            let effective = transit_authz::effective(&policy, &identity);
            (
                json!({ "op": "effective", "identity": identity, "policy": policy }),
                json!({ "ok": { "op": "effective", "effective": effective } }),
            )
        }
        ("authz", "validate") => {
            let policy = transit_authz::Policy::current(vec![
                transit_authz::Rule {
                    name: "ops-write-edits".into(),
                    roles: vec!["dispatcher".into()],
                    pointers: vec!["edits".into()],
                    actions: vec!["read".into(), "write".into()],
                    extra: Default::default(),
                },
                transit_authz::Rule {
                    name: "admin-all".into(),
                    roles: vec!["admin".into()],
                    pointers: vec!["*".into()],
                    actions: vec!["*".into()],
                    extra: Default::default(),
                },
            ]);
            let document = cap_abi::canonical_bytes(&policy).ok()?;
            (
                json!({ "op": "validate", "policy": policy }),
                json!({ "ok": { "op": "validate", "issues": [], "warnings": [],
                    "document": document, "document_hash": cap_abi::content_hash(document.as_bytes()) } }),
            )
        }
        // The plan-chain capability's examples compute REAL answers
        // through the same shared library the module dispatches
        // through — a two-pick chain over the fixture's dates.
        // Authoring examples run the REAL edit through the shared
        // library: the fixture network stamped as a pick, the answer's
        // hash computed the way every module computes it.
        ("author", "stamp_pick") => {
            // The fixture feed, stamped as the fall pick on its first
            // service date.
            let ctx = net?;
            let edits = transit_network::stamp_as_pick(ctx.derived_network(), "Fall 2026 feed", "fall-2026");
            let next = transit_network::apply_edits(ctx.derived_network(), &edits).ok()?;
            (
                json!({ "op": "stamp_pick", "network_hash": ctx.network_hash, "name": "Fall 2026 feed", "id": "fall-2026" }),
                json!({ "ok": { "op": "stamp_pick",
                    "network_hash": cap_abi::canonical_hash(&next).ok()?,
                    "network": next,
                    "issues": transit_network::validation_issues(&next),
                    "warnings": transit_network::validation_warnings(&next),
                    "stamped": true } }),
            )
        }
        ("author", "apply") => {
            let ctx = net?;
            let edits = vec![transit_network::NetworkEdit::SetPlan {
                plan: transit_network::PlanMeta {
                    id: "fall-2026".into(),
                    name: "Fall 2026".into(),
                    effective_from: chrono::NaiveDate::from_ymd_opt(2026, 10, 19),
                    extra: Default::default(),
                },
                through: Some("2027-02-26".into()),
            }, transit_network::NetworkEdit::AddStop { id: None, lat: Some(45.5), lon: Some(-73.5) }];
            let transit_network::Applied { network: next, created, touched } =
                transit_network::apply_edits_reporting(ctx.derived_network(), &edits).ok()?;
            (
                json!({ "op": "apply", "network_hash": ctx.network_hash, "edits": edits }),
                json!({ "ok": { "op": "apply",
                    "network_hash": cap_abi::canonical_hash(&next).ok()?,
                    "network": next,
                    "issues": transit_network::validation_issues(&next),
                    "warnings": transit_network::validation_warnings(&next),
                    "created": created,
                    "touched": touched } }),
            )
        }
        // Diversion-authoring examples run the REAL draft through the
        // shared libraries over the fixture network: route 10 anchored
        // end to end on its default pattern.
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
                json!({ "op": "list", "network_hash": ctx.network_hash, "edits": transit_sync::EditDocument::new(records.clone()) }),
                json!({ "ok": { "op": "list",
                    "rows": transit_dispatch::diversion_rows(ctx.derived_network(), &records),
                    "cancels": transit_dispatch::cancel_rows(ctx.derived_network(), &records) } }),
            )
        }
        ("divert", "reports") => {
            let ctx = net?;
            let records = dispatch_fixture_edits();
            (
                json!({ "op": "reports", "network_hash": ctx.network_hash, "edits": transit_sync::EditDocument::new(records.clone()) }),
                json!({ "ok": { "op": "reports",
                    "reports": transit_dispatch::report_against_network(ctx.derived_network(), &records) } }),
            )
        }
        ("divert", "propose") => {
            let ctx = net?;
            let draft = divert_example_draft(ctx.derived_network())?;
            let now_ms: u64 = 1_770_000_000_000;
            let diversion = transit_dispatch::diversion_of(ctx.derived_network(), &draft, &format!("div-10-{now_ms}")).ok()?;
            (
                json!({ "op": "propose", "network_hash": ctx.network_hash, "draft": draft, "edits": transit_sync::EditDocument::default(), "record_id": "e2", "now_ms": now_ms }),
                json!({ "ok": { "op": "propose", "edits": transit_sync::EditDocument::new(transit_sync::propose(&[], "e2", diversion, now_ms)) } }),
            )
        }
        ("divert", "cancel") => {
            // The 08:00 trip on the fixture's Friday, appended beside the
            // fixture's diversion.
            let ctx = net?;
            let records = dispatch_fixture_edits();
            let now_ms: u64 = 1_770_000_000_000;
            let cancel = transit_dispatch::cancel_of(ctx.derived_network(), &format!("cancel-t10-{now_ms}"), "t10", None, "2026-10-23").ok()?;
            (
                json!({ "op": "cancel", "network_hash": ctx.network_hash, "edits": transit_sync::EditDocument::new(records.clone()), "record_id": "e2",
                    "trip_id": "t10", "date": "2026-10-23", "now_ms": now_ms }),
                json!({ "ok": { "op": "cancel", "edits": transit_sync::EditDocument::new(transit_sync::propose_cancel(&records, "e2", cancel, now_ms)) } }),
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
                json!({ "op": op, "edits": transit_sync::EditDocument::new(records), "record_id": "e1", "now_ms": now_ms }),
                json!({ "ok": { "op": op, "edits": transit_sync::EditDocument::new(next) } }),
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
        ("plans", "publish") => {
            let net = net?;
            let first = &fixture_plan_set()[0];
            let published =
                transit_plans::publish(&transit_plans::PublishedPlans::empty(), first, &net.network_hash, fixture_today()).ok()?;
            let document = cap_abi::canonical_bytes(&published).ok()?;
            (
                json!({ "op": "publish", "published": null, "plan": first, "network_hash": net.network_hash, "today": "2026-01-01" }),
                json!({ "ok": { "op": "publish", "issues": [], "published": published,
                    "document": document, "document_hash": cap_abi::content_hash(document.as_bytes()) } }),
            )
        }
        ("plans", "in_force") => {
            let net = net?;
            let published = fixture_plan_set().iter().fold(transit_plans::PublishedPlans::empty(), |set, plan| {
                transit_plans::publish(&set, plan, &net.network_hash, fixture_today()).unwrap_or(set)
            });
            let plan = transit_plans::in_force(&published, chrono::NaiveDate::from_ymd_opt(2026, 10, 23)?).ok()?.cloned();
            (
                json!({ "op": "in_force", "published": published, "date": "2026-10-23" }),
                json!({ "ok": { "op": "in_force", "plan": plan } }),
            )
        }
        ("plans", "in_use") => {
            let net = net?;
            let published = fixture_plan_set().iter().fold(transit_plans::PublishedPlans::empty(), |set, plan| {
                transit_plans::publish(&set, plan, &net.network_hash, fixture_today()).unwrap_or(set)
            });
            let plans: Vec<_> =
                transit_plans::in_use(&published, chrono::NaiveDate::from_ymd_opt(2026, 10, 23)?).ok()?.into_iter().cloned().collect();
            (
                json!({ "op": "in_use", "published": published, "today": "2026-10-23" }),
                json!({ "ok": { "op": "in_use", "plans": plans } }),
            )
        }
        ("plans", "retire") => {
            let net = net?;
            let published = fixture_plan_set().iter().fold(transit_plans::PublishedPlans::empty(), |set, plan| {
                transit_plans::publish(&set, plan, &net.network_hash, fixture_today()).unwrap_or(set)
            });
            let retired = transit_plans::retire(&published, "fall", fixture_today()).ok()?;
            let document = cap_abi::canonical_bytes(&retired).ok()?;
            (
                json!({ "op": "retire", "published": published, "id": "fall", "today": "2026-01-01" }),
                json!({ "ok": { "op": "retire", "issues": [], "published": retired,
                    "document": document, "document_hash": cap_abi::content_hash(document.as_bytes()) } }),
            )
        }
        ("plans", "freeze") => {
            let net = net?;
            let first = &fixture_plan_set()[0];
            let purpose = "proposal for public comment";
            let frozen = transit_plans::freeze(&transit_plans::FrozenVersions::empty(), first, &net.network_hash, purpose).ok()?;
            let document = cap_abi::canonical_bytes(&frozen).ok()?;
            let next = transit_log::document_records(&serde_json::to_value(&frozen).ok()?, "versions", "frozen_version", "frozen_versions", 1).ok()?;
            (
                json!({ "op": "freeze", "frozen": null, "plan": first, "network_hash": net.network_hash, "purpose": purpose }),
                json!({ "ok": { "op": "freeze", "issues": [], "frozen": frozen,
                    "document": document, "document_hash": cap_abi::content_hash(document.as_bytes()), "next": next } }),
            )
        }
        ("plans", "adopt") => {
            let net = net?;
            let first = &fixture_plan_set()[0];
            let frozen = transit_plans::freeze(&transit_plans::FrozenVersions::empty(), first, &net.network_hash, "board packet").ok()?;
            let adopted = transit_plans::adopt(&transit_plans::PublishedPlans::empty(), &frozen, &net.network_hash, fixture_today(), Some(3)).ok()?;
            let document = cap_abi::canonical_bytes(&adopted).ok()?;
            let next = transit_log::document_records(&serde_json::to_value(&adopted).ok()?, "plans", "pick", "plans", 1).ok()?;
            (
                json!({ "op": "adopt", "published": null, "frozen": frozen, "network_hash": net.network_hash, "today": "2026-01-01", "stops_seq": 3 }),
                json!({ "ok": { "op": "adopt", "issues": [], "published": adopted,
                    "document": document, "document_hash": cap_abi::content_hash(document.as_bytes()), "next": next } }),
            )
        }
        ("plans", "log_records") => {
            let net = net?;
            let published = transit_plans::publish(&transit_plans::PublishedPlans::empty(), &fixture_plan_set()[0], &net.network_hash, fixture_today()).ok()?;
            let next = transit_log::document_records(&serde_json::to_value(&published).ok()?, "plans", "pick", "plans", 1).ok()?;
            (
                json!({ "op": "log_records", "published": published }),
                json!({ "ok": { "op": "log_records", "next": next } }),
            )
        }
        ("plans", "log_document") => {
            let net = net?;
            let published = transit_plans::publish(&transit_plans::PublishedPlans::empty(), &fixture_plan_set()[0], &net.network_hash, fixture_today()).ok()?;
            let next = transit_log::document_records(&serde_json::to_value(&published).ok()?, "plans", "pick", "plans", 1).ok()?;
            let held = |k: &transit_log::KindRecords| -> serde_json::Map<String, serde_json::Value> {
                k.records.iter().map(|(id, v)| (id.clone(), json!({ "version": k.version, "value": v }))).collect()
            };
            let document = cap_abi::canonical_bytes(&published).ok()?;
            (
                json!({ "op": "log_document", "kind": "plans", "records": held(&next[0]), "document": held(&next[1]).remove("document") }),
                json!({ "ok": { "op": "log_document", "document": document, "document_hash": cap_abi::content_hash(document.as_bytes()) } }),
            )
        }
        ("plans", "timeline") => {
            // A device holding the fall pick and a draft; the winter
            // pick is published and not on the device yet.
            let set: transit_plans::PublishedPlans = serde_json::from_value(json!({
                "kind": "plans", "schema_version": transit_plans::PUBLISHED_SCHEMA_VERSION,
                "plans": [
                    { "id": "fall", "name": "Fall 2026", "effective_from": "2026-10-19", "network_hash": "sha256:…", "status": "published" },
                    { "id": "winter", "name": "Winter 2027", "effective_from": "2027-01-04", "network_hash": "sha256:…", "status": "published" },
                ]
            })).ok()?;
            let device = vec![
                transit_plans::DevicePlan { key: "plan-fall".into(), id: "fall".into(), name: "Fall 2026".into(), effective_from: Some("2026-10-19".into()), served: false },
                transit_plans::DevicePlan { key: "plan-draft".into(), id: "spring".into(), name: "Spring 2027 draft".into(), effective_from: Some("2027-03-01".into()), served: false },
            ];
            let t = transit_plans::timeline(&device, Some(&set), chrono::NaiveDate::from_ymd_opt(2026, 11, 13)?).ok()?;
            (
                json!({ "op": "timeline", "plans": device, "published": set, "today": "2026-11-13" }),
                json!({ "ok": { "op": "timeline", "rows": t.rows, "in_force": t.in_force, "periods": t.periods, "issues": t.issues } }),
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
        ("kernel", "load_feed") => return None,
        ("kernel", "load_layer") => return None,
        ("kernel", "load_rt") => return None,
        _ => return None,
    };
    Some(OpExample { request, response })
}

/// The plan-chain examples' plan set: a two-pick chain around the
/// fixture's dates — the spring pick in force on the fixture Friday,
/// the fall pick taking over after it.
/// The host's date in the plan examples: before every fixture pick, so
/// each publishes as a future one.
fn fixture_today() -> chrono::NaiveDate {
    chrono::NaiveDate::from_ymd_opt(2026, 1, 1).expect("a date")
}

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

/// A record's identity in a log, as cap-log computes it.
fn log_record_hash(value: &serde_json::Value) -> String {
    cap_abi::canonical_hash(value).unwrap_or_else(|e| format!("unhashable: {e}"))
}

/// A fixture log entry's header.
fn fixture_log_header(seq: u64, at_ms: u64, op: &str, key: Option<&str>) -> transit_log::Header {
    transit_log::Header {
        log: "stops".into(),
        seq,
        writer: "planner-3".into(),
        at_ms,
        module: "cap-log@sha256:0000000000000000000000000000000000000000000000000000000000000000".into(),
        intent: transit_log::Intent { op: op.into(), key: key.map(String::from), ..Default::default() },
    }
}

/// The fixture stops document's and requests document's records, each
/// the collection's records then the document's own record.
fn fixture_log_records() -> (Vec<transit_log::KindRecords>, Vec<transit_log::KindRecords>) {
    let stops = serde_json::to_value(fixture_stops().checked().expect("the fixture stops are valid")).unwrap();
    let requests = serde_json::to_value(fixture_requests().checked().expect("the fixture requests are valid")).unwrap();
    (
        transit_log::document_records(&stops, "stops", "stop", "stops", transit_stops::SCHEMA_VERSION).unwrap(),
        transit_log::document_records(&requests, "requests", "stop_request", "stop_requests", transit_requests::SCHEMA_VERSION).unwrap(),
    )
}

/// The fixture stops log: the stops and requests seeded (entry 1), then
/// a request completed and its stop's event added in one entry (entry
/// 2). Answers the fold after them and the two entries.
fn fixture_stops_log() -> (transit_log::Checkpoint, Vec<transit_log::Entry>) {
    let (stops, requests) = fixture_log_records();
    let mut fold = transit_log::Checkpoint::empty("stops");
    let mut entries = Vec::new();
    let mut write = |fold: &mut transit_log::Checkpoint, header, next: &[transit_log::KindRecords]| {
        if let transit_log::Composed::Entry(e) = transit_log::compose(fold, header, next, &log_record_hash) {
            transit_log::apply(fold, &e, &log_record_hash).expect("the fixture entry applies");
            entries.push(e);
        }
    };
    write(&mut fold, fixture_log_header(1, 1_770_000_000_000, "migrate", None), &[stops.clone(), requests.clone()].concat());
    let (mut stops2, mut requests2) = (stops[0].clone(), requests[0].clone());
    let id = stops2.records.keys().next().expect("a fixture stop").clone();
    if let Some(events) = stops2.records.get_mut(&id).and_then(|v| v.as_array_mut()) {
        events.push(serde_json::json!({"id": "e-repair", "on": "2027-02-20", "event": "changed", "note": "pad replaced"}));
    }
    if let Some(r) = requests2.records.values_mut().next() {
        r["status"] = serde_json::json!("completed");
        r["completed_on"] = serde_json::json!("2027-02-20");
    }
    write(&mut fold, fixture_log_header(2, 1_770_000_060_000, "POST /logs/stops", Some("tablet-4/17")), &[stops2, requests2]);
    (fold, entries)
}

fn dispatch_fixture_edits() -> Vec<transit_sync::EditRecord> {
    let date = chrono::NaiveDate::from_ymd_opt(2026, 10, 23).unwrap();
    vec![transit_sync::EditRecord {
        id: "e1".into(),
        diversion: Some(transit_sync::Diversion {
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
            extra: Default::default(),
        }),
        cancel: None,
        status: transit_sync::EditStatus::Accepted,
        rev: 1,
        updated_at: 1,
        extra: Default::default(),
    }]
}
