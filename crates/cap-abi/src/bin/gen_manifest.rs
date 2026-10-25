//! gen-manifest: assemble the capability manifest after the wasm
//! builds (`just caps`). Reads each capability module, pins it by
//! content hash, serves it under that hash, and writes the manifest
//! hosts assemble from. The capability set lives HERE, next to the
//! contract — the manifest is the deployment, and the deployment is
//! code-reviewed.
//!
//! Before writing, the manifest is run through the compose-time
//! validator (cap_abi::validate): a manifest this crate assembles
//! must be provably coherent, and the generation is the first
//! consumer of that proof.

use std::fs;
use std::path::Path;

use cap_abi::manifest::{CapManifest, Capability, MigrationSpec, OpDoc, OpExample};
use cap_abi::validate::validate_assembly;
use sha2::{Digest as _, Sha256};

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
        ops: &["load_feed", "departures", "stops", "load_watchlist", "migrate_watchlist", "watchlist_board"],
        consumes: &[],
        speaks: &[("feed", &[1]), ("watchlist", &[1, 2])],
        migrates: &[("watchlist", &[(1, 2)])],
        purpose: "Read a GTFS static zip: parse it into a hash-addressed feed registry and answer schedule questions (departures at a stop, stop lists). Assemble for any user who needs to know when service runs. Also owns the watchlist document (the rider's saved stops) and its v1→v2 migration.",
        op_docs: &[
            ("load_feed", "Register a GTFS zip under its content hash (idempotent). Bulk-bytes op 0; pass the raw zip to transit_invoke_bytes."),
            ("departures", "Upcoming departures at one stop on a service date, schedule only (GTFS-rt is another capability)."),
            ("stops", "Stop list for UI pickers, first N by id."),
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
                    let example = example_for(module.id, op, &feed_hash);
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

    println!("manifest: {} capabilities, kernel v{KERNEL}, assembly valid", MODULES.len());
}

/// Worked examples — the corpus-as-context idea: an agent reads these
/// to learn the shapes. Requests carry the REAL feed hash computed
/// above; responses carry the fixture's known answers.
fn example_for(capability: &str, op: &str, feed_hash: &str) -> Option<OpExample> {
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
        // Bulk-bytes ops have no JSON request example (the payload is
        // the raw zip); the description carries the contract.
        (_, "load_feed") => return None,
        _ => return None,
    };
    Some(OpExample { request, response })
}
