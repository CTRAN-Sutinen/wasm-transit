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
        ops: &["import", "validate", "export", "document", "publishable"],
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
        ops: &["decode", "trip_progress", "late_trips", "live"],
        documents: &["network", "boundary", "rt", "live"],
        consumes: &[("network", "network")],
        // The gtfs-rt payload: an ephemeral document (polled every
        // few seconds), versioned by the protobuf schema it speaks;
        // it joins against the NETWORK (an rt trip id is a model trip
        // id), with the import's boundary for the published
        // stop_sequence numbering when there is one.
        // THE LIVE DOCUMENT is minted here and read by the board —
        // optional edges both: neither module needs the other to
        // answer its own questions, so they are spoken, not consumed.
        speaks: &[("gtfs_rt", &[1]), ("network", &[transit_network::SCHEMA_VERSION]), ("boundary", &[1]), ("live", &[1])],
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
        ops: &["stop_board", "grouped_board", "watchlist_board"],
        documents: &["network", "edits", "live", "watchlist"],
        // The network document is produced by the network capability;
        // the operator edit document is host-owned (wholesale-replaced
        // — the sync contract); the watchlist is the PERSONAL document
        // it reads in either version (cap-migrate migrates it).
        consumes: &[("network", "network")],
        speaks: &[("network", &[transit_network::SCHEMA_VERSION]), ("operator_edits", &[transit_sync::SCHEMA_VERSION]), ("watchlist", &[1, 2]), ("live", &[1])],
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
        ops: &["sync_edits", "read_edits", "compact_edits"],
        documents: &[],
        consumes: &[],
        speaks: &[("operator_edits", &[transit_sync::SCHEMA_VERSION])],
        migrates: &[],
        // Its purpose and per-op agent context: crates/cap-sync/OPS.md
        // (a `#` heading, the purpose, then one `##` section per op).
        docs: include_str!("../../../cap-sync/OPS.md"),
    },
    ModuleSpec {
        id: "authz",
        file: "module_authz.wasm",
        ops: &["authorize", "effective"],
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
        ops: &["apply"],
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
        ops: &["resolve", "validate", "periods", "publish", "in_force", "retire"],
        documents: &[],
        consumes: &[],
        speaks: &[("plan_set", &[1])],
        migrates: &[],
        // Its purpose and per-op agent context: crates/cap-plans/OPS.md
        // (a `#` heading, the purpose, then one `##` section per op).
        docs: include_str!("../../../cap-plans/OPS.md"),
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
        ],
        migrates: &[
            ("network", &[(1, 2)]),
            ("operator_edits", &[(1, 2)]),
            ("watchlist", &[(1, 2)]),
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

    let load = |code: u32, name: &str, description: &str| KernelOp {
        code,
        name: name.into(),
        description: description.into(),
        example: example_for("kernel", name, feed_hash, net),
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
        load(kernel::OP_LOAD_RT, "load_rt", "Register a GTFS-rt protobuf payload (a snapshot) under the content hash of its bytes, answering the hash only (the rt capability's decode answers the overview too). Ephemeral: the last 8 stay. Bulk: the raw payload."),
        KernelOp {
            code: OP_EVICT,
            name: "evict".into(),
            description: "EVERY module. Drop everything the keep-set does not name, answering what dropped and what is held after. The bytes are {\"keep\": [hash, …]}: a document stays when its hash is kept; a derived entry (a held day, the router's graph, a street graph) stays only while every document it was computed from is kept. The host names what survives — it owns the documents; the module enforces — it is the hash authority. Asking for an evicted document is the unknown-hash error (`unknown <kind> <hash>: <load op> first`), never a wrong answer: register it again. Ephemeral kinds also bound themselves with no host steering — rt snapshots keep their last 8, live documents their last 4 — and so does each derivation (held days 4, router graphs 8, street and walking graphs 4).".into(),
            example: Some(OpExample { request: keep, response: evicted }),
        },
        KernelOp {
            code: OP_HELD,
            name: "held".into(),
            description: "EVERY module. What the module holds, per registry (each document kind its capabilities read, then the derived entries), oldest first (the bytes are ignored) — the inspection a memory gate reads.".into(),
            example: Some(OpExample { request: serde_json::json!(null), response: held }),
        },
        KernelOp {
            code: kernel::OP_ABOUT,
            name: "about".into(),
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
    let documents = document_kinds(net_ctx.as_ref(), &feed_hash);
    publish_registry(&root, &manifest, &documents, &profile);

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
fn publish_registry(root: &Path, manifest: &CapManifest, documents: &[DocumentKind], profile: &str) {
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

    publish_documents(&registry, documents, &evidence);

    println!(
        "registry: {} capabilities published (blobs + records + pointer)",
        manifest.capabilities.len()
    );
}

/// One document kind the registry describes: its bus name (the
/// manifest's `speaks`), its store kind when the sync store persists
/// it (the envelope's `kind`, docs/FORMATS.md), the schema version this
/// build speaks, the JSON Schema generated from its serde type, and a
/// worked example computed from the fixtures through the owning
/// library — so the example is exactly a document the modules make.
struct DocumentKind {
    kind: &'static str,
    store_kind: Option<&'static str>,
    schema_version: u32,
    schema: schemars::Schema,
    example: Option<serde_json::Value>,
}

fn kind<T: schemars::JsonSchema>(
    kind: &'static str,
    store_kind: Option<&'static str>,
    schema_version: u32,
    example: Option<serde_json::Value>,
) -> DocumentKind {
    DocumentKind { kind, store_kind, schema_version, schema: schemars::schema_for!(T), example }
}

/// Every JSON document kind on the bus and in the store. Not here: the
/// feed (a GTFS zip) and the rt snapshot (protobuf) are bytes with
/// their own specs; the manifest is the Go host's own document
/// (docs/FORMATS.md pins it); the watchlist is device-only and its
/// type lives in cap-board, a module this generator cannot depend on.
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
        kind::<transit_network::Network>("network", Some("network_document"), transit_network::SCHEMA_VERSION, net.map(|n| n.derived.clone())),
        kind::<transit_codec::Boundary>("boundary", None, 1, net.map(|n| n.boundary.clone())),
        kind::<transit_sync::EditDocument>(
            "operator_edits",
            Some("edit_records"),
            transit_sync::SCHEMA_VERSION,
            Some(value(&transit_sync::EditDocument::new(dispatch_fixture_edits()))),
        ),
        kind::<Vec<transit_plans::PlanInfo>>("plan_set", None, 1, Some(value(&fixture_plan_set()))),
        kind::<transit_plans::PublishedPlans>("plans", Some("plans"), transit_plans::PUBLISHED_SCHEMA_VERSION, plans_example),
        kind::<transit_authz::Policy>(
            "policy",
            Some("policy"),
            transit_authz::SCHEMA_VERSION,
            Some(value(&transit_authz::Policy::current(vec![transit_authz::Rule {
                name: "ops-write-edits".into(),
                roles: vec!["dispatcher".into()],
                pointers: vec!["edits".into()],
                actions: vec!["write".into()],
            }]))),
        ),
        kind::<transit_streets::Streets>(
            "streets",
            None,
            1,
            streets_example_context().map(|c| value(&c.streets)),
        ),
        kind::<transit_streets::walking::WalkTable>("walks", None, 1, walks_example),
        kind::<transit_runs::WorkRules>("work_rules", None, transit_runs::SCHEMA_VERSION, Some(value(&fixture_work_rules()))),
        kind::<transit_live::LiveDay>("live", None, 2, live_example().map(|(live, _)| value(&live))),
    ]
}

/// A worked example as a JSON value.
fn value<T: serde::Serialize>(example: &T) -> serde_json::Value {
    serde_json::to_value(example).expect("example serializes")
}

/// Publish the documents: `schemas/<kind>.v<N>.schema.json`, the
/// worked example `examples/<kind>.v<N>.json`, and the index
/// `documents.json` (each entry names its files and their hashes, with
/// the registry's evidence). `go test ./server/...` validates every
/// example against its schema with no Rust (docs/FORMATS.md).
fn publish_documents(registry: &Path, documents: &[DocumentKind], evidence: &serde_json::Value) {
    let schemas = registry.join("schemas");
    let examples = registry.join("examples");
    fs::create_dir_all(&schemas).expect("create registry schemas");
    fs::create_dir_all(&examples).expect("create registry examples");
    let mut index = Vec::new();
    for doc in documents {
        let stem = format!("{}.v{}", doc.kind, doc.schema_version);
        let schema = serde_json::to_string_pretty(&doc.schema).expect("schema serializes") + "\n";
        let schema_path = format!("schemas/{stem}.schema.json");
        fs::write(registry.join(&schema_path), &schema).expect("write schema");
        let mut entry = serde_json::json!({
            "kind": doc.kind,
            "store_kind": doc.store_kind,
            "schema_version": doc.schema_version,
            "schema": schema_path,
            "schema_hash": cap_abi::content_hash(schema.as_bytes()),
        });
        if let Some(example) = &doc.example {
            let text = serde_json::to_string(example).expect("example serializes");
            let example_path = format!("examples/{stem}.json");
            fs::write(registry.join(&example_path), &text).expect("write example");
            entry["example"] = example_path.into();
            entry["example_hash"] = cap_abi::content_hash(text.as_bytes()).into();
        }
        index.push(entry);
    }
    let json = serde_json::to_string_pretty(&serde_json::json!({ "documents": index, "evidence": evidence }))
        .expect("documents index serializes");
    fs::write(registry.join("documents.json"), json + "\n").expect("write documents index");
    println!("registry: {} document kinds published (schemas + worked examples)", documents.len());
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

fn network_example_context(feed_bytes: &[u8]) -> Option<NetworkExampleContext> {
    let outcome = transit_codec::import(&transit_codec::Feed::from_zip_bytes(feed_bytes).ok()?);
    let canonical = serde_json::to_string(&outcome.network).ok()?;
    let network_hash = format!("sha256:{:x}", Sha256::digest(canonical.as_bytes()));
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
    let hash = |v: String| format!("sha256:{:x}", Sha256::digest(v.as_bytes()));
    Some(RtExampleContext {
        network_hash: hash(serde_json::to_string(&outcome.network).ok()?),
        boundary_hash: hash(serde_json::to_string(&outcome.boundary).ok()?),
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
    let live_hash = format!("sha256:{:x}", Sha256::digest(serde_json::to_string(&live).ok()?.as_bytes()));
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
/// every trip, route 10's own rule, routes 10 and 20 interlined, and an
/// eight-hour run under report and clear times.
fn fixture_work_rules() -> transit_runs::WorkRules {
    transit_runs::WorkRules { kind: Default::default(),
        version: transit_runs::SCHEMA_VERSION,
        name: "Fixture rules".into(),
        recovery: vec![
            transit_runs::RecoveryRule { route: None, service_type: None, min_secs: Some(300), pct_of_run: Some(10) },
            transit_runs::RecoveryRule { route: Some("10".into()), service_type: None, min_secs: Some(240), pct_of_run: None },
        ],
        interline: vec![transit_runs::RoutePair { a: "10".into(), b: "20".into() }],
        runs: transit_runs::RunRules {
            max_platform_secs: Some(8 * 3600),
            report_secs: Some(900),
            clear_secs: Some(900),
            ..Default::default()
        },
        pay: None,
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
            let document = serde_json::to_string(&ctx.network).ok()?;
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
        ("kernel", "load_work_rules") => {
            let rules = fixture_work_rules().checked().ok()?;
            let work_rules_hash = cap_abi::canonical_hash(&rules).ok()?;
            (
                json!({ "op": "load_work_rules", "work_rules": fixture_work_rules() }),
                json!({ "ok": { "op": "load_work_rules", "work_rules_hash": work_rules_hash, "version": rules.version } }),
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
            let document = json!({ "version": "v2", "stops": [
                { "id": "A", "label": "home" }, { "id": "B", "label": "work" }
            ] });
            let watchlist: transit_dispatch::watchlist::Watchlist = serde_json::from_value(document.clone()).ok()?;
            let watchlist_hash = cap_abi::canonical_hash(&watchlist).ok()?;
            (
                json!({ "op": "load_watchlist", "document": document }),
                json!({ "ok": { "op": "load_watchlist", "watchlist_hash": watchlist_hash, "version": 2 } }),
            )
        }
        ("migrate", "migrate") => {
            let v1 = json!([{ "id": "e1", "status": "accepted", "rev": 1, "updated_at": 1000,
                "cancel": { "id": "c1", "trip_id": "t1", "start_secs": 28800, "date": "2026-10-23" } }]);
            let (document, notes) = transit_sync::version::migrate_to(v1.clone(), transit_sync::SCHEMA_VERSION).ok()?;
            let typed: transit_sync::EditDocument = serde_json::from_value(document.clone()).ok()?;
            let canonical = serde_json::to_string(&typed).ok()?;
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
                json!({ "op": "watchlist_board", "watchlist_hash": "sha256:<of the document>",
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
            let document = serde_json::to_string(&transit_sync::EditDocument::new(records.clone())).unwrap();
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
            let document = serde_json::to_string(&transit_sync::EditDocument::new(records.clone())).unwrap();
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
            let document = serde_json::to_string(&transit_sync::EditDocument::new(records.clone())).unwrap();
            let document_hash = cap_abi::canonical_hash(&transit_sync::EditDocument::new(records.clone())).unwrap();
            (
                json!({ "op": "compact_edits", "records": old, "now_ms": now_ms, "horizon_ms": horizon_ms }),
                json!({ "ok": { "op": "compact_edits", "records": records, "document": document, "document_hash": document_hash, "dropped": dropped } }),
            )
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
                },
                transit_authz::Rule {
                    name: "admin-all".into(),
                    roles: vec!["admin".into()],
                    pointers: vec!["*".into()],
                    actions: vec!["*".into()],
                },
            ]);
            let identity = transit_authz::Identity { sub: "dispatcher-7".into(), roles: vec!["dispatcher".into()] };
            let effective = transit_authz::effective(&policy, &identity);
            (
                json!({ "op": "effective", "identity": identity, "policy": policy }),
                json!({ "ok": { "op": "effective", "effective": effective } }),
            )
        }
        // The plan-chain capability's examples compute REAL answers
        // through the same shared library the module dispatches
        // through — a two-pick chain over the fixture's dates.
        // Authoring examples run the REAL edit through the shared
        // library: the fixture network stamped as a pick, the answer's
        // hash computed the way every module computes it.
        ("author", "apply") => {
            let ctx = net?;
            let edits = vec![transit_network::NetworkEdit::SetPlan {
                plan: transit_network::PlanMeta {
                    id: "fall-2026".into(),
                    name: "Fall 2026".into(),
                    effective_from: chrono::NaiveDate::from_ymd_opt(2026, 10, 19),
                },
                through: Some("2027-02-26".into()),
            }, transit_network::NetworkEdit::AddStop { id: None, lat: Some(45.5), lon: Some(-73.5) }];
            let transit_network::Applied { network: next, created, touched } =
                transit_network::apply_edits_reporting(ctx.derived_network(), &edits).ok()?;
            let canonical = serde_json::to_string(&next).ok()?;
            (
                json!({ "op": "apply", "network_hash": ctx.network_hash, "edits": edits }),
                json!({ "ok": { "op": "apply",
                    "network_hash": format!("sha256:{:x}", Sha256::digest(canonical.as_bytes())),
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
            let document = serde_json::to_string(&published).ok()?;
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
        ("plans", "retire") => {
            let net = net?;
            let published = fixture_plan_set().iter().fold(transit_plans::PublishedPlans::empty(), |set, plan| {
                transit_plans::publish(&set, plan, &net.network_hash, fixture_today()).unwrap_or(set)
            });
            let retired = transit_plans::retire(&published, "fall", fixture_today()).ok()?;
            let document = serde_json::to_string(&retired).ok()?;
            (
                json!({ "op": "retire", "published": published, "id": "fall", "today": "2026-01-01" }),
                json!({ "ok": { "op": "retire", "issues": [], "published": retired,
                    "document": document, "document_hash": cap_abi::content_hash(document.as_bytes()) } }),
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
        }),
        cancel: None,
        status: transit_sync::EditStatus::Accepted,
        rev: 1,
        updated_at: 1,
    }]
}
