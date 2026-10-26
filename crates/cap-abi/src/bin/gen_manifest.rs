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
        ops: &["load_feed", "departures", "station_departures", "stops", "stops_by_ids", "active_services", "blocks", "block_trips", "feed_info", "load_watchlist", "migrate_watchlist", "watchlist_board"],
        consumes: &[],
        speaks: &[("feed", &[1]), ("watchlist", &[1, 2])],
        migrates: &[("watchlist", &[(1, 2)])],
        purpose: "Read a GTFS static zip: parse it into a hash-addressed feed registry and answer schedule questions (departures at a stop, stop lists). Assemble for any user who needs to know when service runs. Also owns the watchlist document (the rider's saved stops) and its v1→v2 migration.",
        op_docs: &[
            ("load_feed", "Register a GTFS zip under its content hash (idempotent). Bulk-bytes op 0; pass the raw zip to transit_invoke_bytes."),
            ("departures", "Upcoming departures at one stop on a service date, schedule only (GTFS-rt is another capability). The stop's own board — a station pin answers nothing here; see station_departures."),
            ("station_departures", "Upcoming departures POOLED over a stop's walking group — the station board. A station (Fisher's Landing, Main Station) answers no departures of its own: the bays carry the trips, and this op unions the whole group (the stop, its parent, siblings sharing that parent, the parent's children), merged by time, each row tagged with its concrete bay. Any member of the group answers the same board."),
            ("stops", "Stop list for UI pickers, first N by id."),
            ("stops_by_ids", "Stop names by id, in the request's order — the join hosts otherwise do by hand when a view (like rt's) carries ids a human reads as names. Unknown ids are omitted."),
            ("active_services", "Service keys running on a date — often several at once (regular patterns plus exception-added services). The dispatcher's first question: what is running today?"),
            ("blocks", "All blocks in the feed; with `date`, only the blocks RUNNING that day, with that day's trip counts — the vehicle picker shows buses in service, not parked buses. A block is one vehicle's trip chain."),
            ("block_trips", "One block's trip chain on a service date: where the vehicle starts, where it ends, the layover between trips. Unknown blocks answer empty, never error."),
            ("feed_info", "The feed's own metadata (version, publisher), when the feed provides feed_info.txt — the identity question a dispatcher asks first: which feed am I dispatching?"),
            ("load_watchlist", "Register a watchlist document (version-tagged JSON) under its content hash."),
            ("migrate_watchlist", "Migrate a version-1 watchlist to version 2: label = note, colors dropped and reported. Pure and deterministic; the caller keeps the old bytes."),
            ("watchlist_board", "Each watched stop's upcoming departures, labelled per the watchlist's version."),
        ],
    },
    ModuleSpec {
        id: "router",
        file: "cap_router.wasm",
        ops: &["load_feed", "plan"],
        consumes: &[("feed", "gtfs")],
        speaks: &[("feed", &[1])],
        migrates: &[],
        purpose: "Earliest-arrival trip planning (RAPTOR) over a feed document the gtfs capability produced. Assemble for users who need directions, not just departure times. Reads documents by content hash; writes nothing.",
        op_docs: &[
            ("load_feed", "Register a GTFS zip under its content hash (the same document identity gtfs returns — assert equality at the host). Bulk-bytes op 0."),
            ("plan", "Earliest-arrival itinerary from stop to stop at a departure time; null itinerary = unreachable within max_legs."),
        ],
    },
    ModuleSpec {
        id: "network",
        file: "cap_network.wasm",
        ops: &["load_feed", "import", "load_document", "validate", "export"],
        consumes: &[],
        speaks: &[("feed", &[1]), ("network", &[1])],
        migrates: &[],
        purpose: "The network codec: infer an authored network document from a GTFS feed (the planner's bootstrap — patterns, service types, blocks, reported resolutions) and export one back to GTFS zip bytes (the publish path). The second capability extracted from the monolith — the codec lives in the shared transit-codec library, and the monolith keeps no copy. Assemble for planners: it is how a planner's session starts and how it publishes.",
        op_docs: &[
            ("load_feed", "Register a GTFS zip under its content hash (idempotent). Bulk-bytes op 0; pass the raw zip to transit_invoke_bytes."),
            ("import", "The planner's bootstrap: infer an authored network document from a loaded feed. Deterministic; every resolution the inference applied is reported in the notes — never silent. The inferred document registers itself, so its hash answers validate/export immediately."),
            ("load_document", "Register an authored network document (the planner's JSON document) under its content hash; idempotent."),
            ("validate", "Every issue of a registered document — the authoring UI's feedback loop, the whole list, never thrown."),
            ("export", "The publish path: materialize a registered document into a GTFS zip (deterministic bytes) and return it base64-encoded under the content hash of those bytes — the same identity any capability returns for the same bytes. Invalid documents are refused."),
        ],
    },
    ModuleSpec {
        id: "rt",
        file: "cap_rt.wasm",
        ops: &["decode", "load_feed", "trip_progress", "late_trips"],
        consumes: &[("feed", "gtfs")],
        // The gtfs-rt payload: an ephemeral document (polled every
        // few seconds), versioned by the protobuf schema it speaks;
        // the static feed it joins against is the SAME document gtfs
        // serves (one zip, one identity).
        speaks: &[("gtfs_rt", &[1]), ("feed", &[1])],
        migrates: &[],
        purpose: "Realtime decoding and the schedule join: turn a GTFS-rt protobuf payload (trip updates, vehicle positions) into the flat view hosts show, and answer the dispatcher's joined questions — one trip's predicted times, the ranked what's-late-now view with the delayed-but-dark flag. The third capability extracted from the monolith; the domain code lives in the shared transit-rt library. Assemble for riders (live boards) and dispatchers (the operation view) alike.",
        op_docs: &[
            ("decode", "Decode a GTFS-rt protobuf payload into the overview (entity counts, trip updates with per-stop delays, vehicle positions), and register the snapshot under its content hash — idempotent, the same bytes are the same snapshot — so trip_progress/late_trips can address it. Bulk-bytes op 0. Garbage bytes are an error outcome, never a trap."),
            ("load_feed", "Register the STATIC feed the schedule joins come from — the same GTFS zip bytes gtfs loads, the same content-hash identity (assert equality at the host). Bulk-bytes op 1."),
            ("trip_progress", "One trip's joined board: every scheduled stop with the live delay and the PREDICTED time (schedule + delay) — the dispatcher's most-asked question in one op. A trip the snapshot does not mention answers from the schedule alone; a trip the feed does not carry answers null."),
            ("late_trips", "The ranked what's-late-now view: every trip whose worst stop delay reaches min_delay_secs, worst first, each carrying the vehicle's label and position when one names it — and the DELAYED-BUT-DARK flag (has_position false) when none does. vp_hash names the VehiclePositions snapshot (agencies serve it as a SEPARATE feed — C-TRAN does); omit it for combined feeds."),
        ],
    },
    ModuleSpec {
        id: "analysis",
        file: "cap_analysis.wasm",
        ops: &["load_network", "load_population_layer", "network_stats", "service_types_overview", "service_type_detail", "type_coverage"],
        // The network document is produced by the network capability —
        // the extraction this module's own docs once called "future".
        consumes: &[("network", "network")],
        speaks: &[("network", &[1])],
        migrates: &[],
        purpose: "Planner analysis over an authored network document: network/route/stop stats, per-service-type overview and detail, population coverage. The first capability extracted from the monolith — the domain code lives in the shared transit-analysis library. Assemble for planners; riders never download it. Documents are hash-addressed (load once, query by hash — never per-op document serialization).",
        op_docs: &[
            ("load_network", "Register an authored network document (the planner's JSON document, produced or derived by the network capability) under its content hash; idempotent."),
            ("load_population_layer", "Register a GeoJSON population layer under its content hash. Bulk-bytes op 0; pop_field optional (common candidates tried)."),
            ("network_stats", "The network's numbers on one service date: trips, spans, revenue hours, transfer points, per-route and per-stop activity, headways. include_routes/include_stops/include_service_types (default true) shape the response: false empties the corresponding lists — the day-shape question answers in under a KiB."),
            ("service_types_overview", "Every service type's numbers, each on its representative day — the design axis, no calendar dates in sight."),
            ("service_type_detail", "One service type expanded on its representative day: routes, stops, schedules, totals."),
            ("type_coverage", "Population coverage per service type against a hash-addressed layer: what share of the population each type serves, and serves frequently."),
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

/// Worked examples — the corpus-as-context idea: an agent reads these
/// to learn the shapes. Requests carry the REAL feed hash computed
/// above; responses carry the fixture's known answers.
fn example_for(
    capability: &str,
    op: &str,
    feed_hash: &str,
    net: Option<&NetworkExampleContext>,
) -> Option<OpExample> {
    use serde_json::json;
    let (request, response) = match (capability, op) {
        ("gtfs", "departures") => (
            json!({
                "op": "departures",
                "feed_hash": feed_hash,
                "stop_id": "A",
                "date": "2026-10-23",
                "from_secs": 25200,
                "limit": 5
            }),
            json!({
                "ok": { "op": "departures", "departures": [
                    { "stop_id": "A", "trip_id": "t10", "route_id": "10", "route_name": "10",
                      "service_id": "WK", "headsign": null, "block_id": "B1",
                      "departure_time": 28800, "departure_time_hhmmss": "08:00:00", "stop_sequence": 1 },
                    { "stop_id": "A", "trip_id": "t20", "route_id": "20", "route_name": "20",
                      "service_id": "WK", "headsign": null, "block_id": "B1",
                      "departure_time": 30000, "departure_time_hhmmss": "08:20:00", "stop_sequence": 1 }
                ] }
            }),
        ),
        ("gtfs", "station_departures") => (
            json!({
                "op": "station_departures",
                "feed_hash": feed_hash,
                "stop_id": "STN",
                "date": "2026-10-23",
                "from_secs": 25200,
                "limit": 5
            }),
            json!({ "ok": { "op": "station_departures", "departures": [
                { "stop_id": "SA", "trip_id": "t40", "route_id": "40", "route_name": "40",
                  "service_id": "WK", "headsign": "Bay B", "block_id": null,
                  "departure_time": 29700, "departure_time_hhmmss": "08:15:00", "stop_sequence": 1 },
                { "stop_id": "SB", "trip_id": "t40", "route_id": "40", "route_name": "40",
                  "service_id": "WK", "headsign": "Bay B", "block_id": null,
                  "departure_time": 30000, "departure_time_hhmmss": "08:20:00", "stop_sequence": 2 },
                { "stop_id": "SB", "trip_id": "t41", "route_id": "40", "route_name": "40",
                  "service_id": "WK", "headsign": "Bay A", "block_id": null,
                  "departure_time": 30600, "departure_time_hhmmss": "08:35:00", "stop_sequence": 1 },
                { "stop_id": "SA", "trip_id": "t41", "route_id": "40", "route_name": "40",
                  "service_id": "WK", "headsign": "Bay A", "block_id": null,
                  "departure_time": 31200, "departure_time_hhmmss": "08:40:00", "stop_sequence": 2 }
            ] } }),
        ),
        // The dispatch reads' examples compute REAL answers over the
        // fixture through the same transit-core library the module
        // dispatches through.
        ("gtfs", "active_services") => {
            let gtfs = fixture_gtfs()?;
            let date = chrono::NaiveDate::from_ymd_opt(2026, 10, 23).unwrap();
            (
                json!({ "op": "active_services", "feed_hash": feed_hash, "date": "2026-10-23" }),
                json!({ "ok": { "op": "active_services", "services": gtfs.active_services(date) } }),
            )
        }
        ("gtfs", "blocks") => {
            let gtfs = fixture_gtfs()?;
            let date = chrono::NaiveDate::from_ymd_opt(2026, 10, 23).unwrap();
            (
                json!({ "op": "blocks", "feed_hash": feed_hash, "date": "2026-10-23" }),
                json!({ "ok": { "op": "blocks", "blocks": gtfs.blocks_on(date) } }),
            )
        }
        ("gtfs", "block_trips") => {
            let gtfs = fixture_gtfs()?;
            let date = chrono::NaiveDate::from_ymd_opt(2026, 10, 23).unwrap();
            (
                json!({ "op": "block_trips", "feed_hash": feed_hash, "block_id": "B1", "date": "2026-10-23" }),
                json!({ "ok": { "op": "block_trips", "trips": gtfs.block_trips("B1", date) } }),
            )
        }
        ("gtfs", "feed_info") => {
            let gtfs = fixture_gtfs()?;
            (
                json!({ "op": "feed_info", "feed_hash": feed_hash }),
                json!({ "ok": { "op": "feed_info", "info": gtfs.feed_info } }),
            )
        }
        ("gtfs", "stops") => (
            json!({ "op": "stops", "feed_hash": feed_hash, "limit": 3 }),
            json!({ "ok": { "op": "stops", "stops": [
                { "stop_id": "A", "name": "Main & 1st" },
                { "stop_id": "B", "name": "Main & 2nd" },
                { "stop_id": "C", "name": "Main & 3rd" }
            ] } }),
        ),
        ("gtfs", "load_watchlist") => (
            json!({ "op": "load_watchlist", "document": { "version": "v2", "stops": [
                { "id": "A", "label": "home" }, { "id": "B", "label": "work" }
            ] } }),
            json!({ "ok": { "op": "load_watchlist", "version": 2, "watchlist_hash": "sha256:<of the document>" } }),
        ),
        ("gtfs", "migrate_watchlist") => (
            json!({ "op": "migrate_watchlist", "document": { "version": "v1", "stops": [
                { "id": "A", "note": "home", "color": "blue" }
            ] } }),
            json!({ "ok": { "op": "migrated_watchlist", "document": { "version": "v2", "stops": [
                { "id": "A", "label": "home" }
            ] }, "notes": [ { "kind": "dropped_field", "field": "color", "count": 1 } ] } }),
        ),
        ("gtfs", "watchlist_board") => (
            json!({ "op": "watchlist_board", "watchlist_hash": "sha256:<of the document>",
                    "feed_hash": feed_hash, "date": "2026-10-23", "from_secs": 25200, "limit": 3 }),
            json!({ "ok": { "op": "watchlist_board", "stops": [
                { "id": "A", "label": "home", "departures": [
                    { "stop_id": "A", "trip_id": "t10", "route_id": "10", "route_name": "10",
                      "service_id": "WK", "headsign": null, "block_id": "B1",
                      "departure_time": 28800, "departure_time_hhmmss": "08:00:00", "stop_sequence": 1 }
                ] },
                { "id": "B", "label": "work", "departures": [
                    { "stop_id": "B", "trip_id": "t12", "route_id": "20", "route_name": "20",
                      "service_id": "HOL", "headsign": "Extra run", "block_id": "B2",
                      "departure_time": 27000, "departure_time_hhmmss": "07:30:00", "stop_sequence": 1 }
                ] }
            ] } }),
        ),
        ("router", "plan") => (
            json!({ "op": "plan", "feed_hash": feed_hash, "from": "A", "to": "B",
                    "date": "2026-10-23", "depart_secs": 25200, "max_legs": 4 }),
            json!({ "ok": { "op": "plan", "itinerary": { "from_stop_id": "A", "to_stop_id": "B",
                "depart_secs": 28800, "arrival_secs": 29400, "arrival_hhmmss": "08:10:00",
                "legs": [] } } }),
        ),
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
        ("network", "load_document") => {
            let ctx = net?;
            (
                json!({ "op": "load_document", "network": ctx.derived }),
                json!({ "ok": { "op": "load_document", "network_hash": ctx.network_hash } }),
            )
        }
        ("network", "validate") => {
            let ctx = net?;
            (
                json!({ "op": "validate", "network_hash": ctx.network_hash }),
                json!({ "ok": { "op": "validate", "issues": [] } }),
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
                    "coverage": transit_analysis::type_coverage(&ctx.derived_network(), &layer, 500.0, 3600, 2) } }),
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
        ("gtfs", "stops_by_ids") => {
            let gtfs = fixture_gtfs()?;
            (
                json!({ "op": "stops_by_ids", "feed_hash": feed_hash, "ids": ["A", "STN"] }),
                json!({ "ok": { "op": "stops_by_ids", "stops": [
                    { "stop_id": "A", "name": gtfs.stops.get("A").map(|s| s.stop_name.clone()) },
                    { "stop_id": "STN", "name": gtfs.stops.get("STN").map(|s| s.stop_name.clone()) },
                ] } }),
            )
        }
        // Bulk-bytes ops have no JSON request example (the payload is
        // the raw zip/protobuf); the description carries the contract.
        (_, "load_feed") => return None,
        (_, "load_population_layer") => return None,
        _ => return None,
    };
    Some(OpExample { request, response })
}
