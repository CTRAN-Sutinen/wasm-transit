//! THE SCHEMAS DESCRIBE THE REAL BYTES (docs/FORMATS.md, "The schema
//! lock"): every capability's worked examples run through the
//! capability's own dispatch (`cap_<id>::invoke`, the code its module
//! runs) over the e2e fixture feed, and every document in every answer
//! matches the locked schema of its tag and version. This is where
//! `schemars` and serde can disagree (`skip_serializing_if`, `untagged`,
//! `flatten`). The examples are the manifest's (`just caps` writes
//! web/public/caps/manifest.json), over the fixtures they name. A kind
//! a worked example's recorded answer holds must be checked from a real
//! answer too. The check is `common/doc_check.rs`,
//! and the real-data soak (`real_data_soak.rs`) uses it too.

#[path = "common/doc_check.rs"]
mod doc_check;

use std::path::Path;

use serde_json::{json, Value};

const OP_LOAD_FEED: u32 = 266;
const OP_LOAD_RT: u32 = 267;

/// One capability's two lanes, by id.
fn lanes(id: &str) -> (fn(&str) -> String, fn(u32, &[u8]) -> String) {
    match id {
        "alerts" => (cap_alerts::invoke, cap_alerts::invoke_bytes),
        "notes" => (cap_notes::invoke, cap_notes::invoke_bytes),
        "analysis" => (cap_analysis::invoke, cap_analysis::invoke_bytes),
        "author" => (cap_author::invoke, cap_author::invoke_bytes),
        "authz" => (cap_authz::invoke, cap_authz::invoke_bytes),
        "board" => (cap_board::invoke, cap_board::invoke_bytes),
        "calendar" => (cap_calendar::invoke, cap_calendar::invoke_bytes),
        "directory" => (cap_directory::invoke, cap_directory::invoke_bytes),
        "divert" => (cap_divert::invoke, cap_divert::invoke_bytes),
        "gtfs" => (cap_gtfs::invoke, cap_gtfs::invoke_bytes),
        "log" => (cap_log::invoke, cap_log::invoke_bytes),
        "migrate" => (cap_migrate::invoke, cap_migrate::invoke_bytes),
        "network" => (cap_network::invoke, cap_network::invoke_bytes),
        "plans" => (cap_plans::invoke, cap_plans::invoke_bytes),
        "requests" => (cap_requests::invoke, cap_requests::invoke_bytes),
        "router" => (cap_router::invoke, cap_router::invoke_bytes),
        "rt" => (cap_rt::invoke, cap_rt::invoke_bytes),
        "runcut" => (cap_runcut::invoke, cap_runcut::invoke_bytes),
        "stops" => (cap_stops::invoke, cap_stops::invoke_bytes),
        "streets" => (cap_streets::invoke, cap_streets::invoke_bytes),
        "sync" => (cap_sync::invoke, cap_sync::invoke_bytes),
        other => panic!("capability {other} is not in the real-output gate: add it to `lanes` and to cap-abi's dev-dependencies"),
    }
}

/// The kernel's worked load examples, by the document kind a capability
/// says it reads (independence.mjs registers the same).
const LOADS: &[(&str, &str)] = &[
    ("load_network", "network"),
    ("load_edits", "edits"),
    ("load_live", "live"),
    ("load_boundary", "boundary"),
    ("load_walks", "walks"),
    ("load_streets", "streets"),
    ("load_watchlist", "watchlist"),
    ("load_work_rules", "work_rules"),
    ("load_stops", "stops"),
    ("load_stop_requests", "stop_requests"),
    ("load_service_alerts", "service_alerts"),
    ("load_alert_templates", "alert_templates"),
    ("load_notes", "notes"),
];

#[test]
fn every_document_a_capability_answers_matches_its_locked_schema() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let manifest: Value = serde_json::from_str(
        &std::fs::read_to_string(root.join("web/public/caps/manifest.json")).expect("the manifest (run `just caps`)"),
    )
    .unwrap();
    let feed = std::fs::read(root.join("web/e2e/fixtures/sample-gtfs.zip")).unwrap();
    let rt = std::fs::read(root.join("web/e2e/fixtures/sample-rt.pb")).unwrap();
    // The street fixtures the streets examples name (gen-manifest
    // imports the same files).
    let load_streets: Vec<String> = ["streets.geojson", "walk-streets.geojson"]
        .iter()
        .map(|file| {
            let layer = std::fs::read(root.join("web/e2e/fixtures").join(file)).unwrap();
            let streets = transit_streets::geojson::import(&layer).unwrap();
            json!({"op": "load_streets", "streets": streets}).to_string()
        })
        .collect();
    let locks = doc_check::Locks::read();

    let mut failures = Vec::new();
    let mut checked = std::collections::BTreeMap::<String, usize>::new();
    // Each kind a worked example's recorded answer holds, and the first
    // op that holds it: the real answers must check it too, so a setup
    // that leaves an op unanswered cannot pass by checking nothing.
    let mut promised = std::collections::BTreeMap::<String, String>::new();
    for cap in manifest["capabilities"].as_array().unwrap() {
        let id = cap["id"].as_str().unwrap();
        let (invoke, invoke_bytes) = lanes(id);
        let reads: Vec<&str> = cap["documents"].as_array().map(|d| d.iter().filter_map(Value::as_str).collect()).unwrap_or_default();
        for (name, kind) in LOADS {
            let load = manifest["kernel_ops"].as_array().unwrap().iter().find(|op| op["name"] == *name);
            if let (true, Some(request)) = (reads.contains(kind), load.and_then(|op| op["example"].get("request"))) {
                invoke(&request.to_string());
            }
        }
        if reads.contains(&"feed") {
            invoke_bytes(OP_LOAD_FEED, &feed);
        }
        if reads.contains(&"rt") {
            invoke_bytes(OP_LOAD_RT, &rt);
        }
        if reads.contains(&"streets") {
            for load in &load_streets {
                invoke(load);
            }
        }
        for (op, doc) in cap["op_docs"].as_object().into_iter().flatten() {
            let Some(request) = doc["example"].get("request") else { continue };
            let answer: Value = match serde_json::from_str(&invoke(&request.to_string())) {
                Ok(answer) => answer,
                Err(e) => {
                    failures.push(format!("{id}.{op}: the answer is not JSON ({e})"));
                    continue;
                }
            };
            for (key, _) in locks.check(&doc["example"]["response"]) {
                promised.entry(key).or_insert_with(|| format!("{id}.{op}"));
            }
            for (key, problems) in locks.check(&answer) {
                *checked.entry(key.clone()).or_default() += 1;
                if !problems.is_empty() {
                    failures.push(format!("{id}.{op}: its {key} document does not match its schema: {}", problems.join("; ")));
                }
            }
        }
    }
    for (key, op) in &promised {
        if !checked.contains_key(key) {
            failures.push(format!(
                "{op}'s worked example answers a {key} document, and no real answer here held one: the setup left it unanswered, \
                 or the manifest is older than the code (run `just caps`)"
            ));
        }
    }
    assert!(failures.is_empty(), "documents that do not match their schemas:\n  {}", failures.join("\n  "));
    assert!(!checked.is_empty(), "no answer held a document: the gate checked nothing");
    println!("real output: {}", json!(checked));
}

// The check itself, broken on deliberate fixtures: a gate that cannot
// fail proves nothing.

#[test]
fn a_document_that_does_not_match_its_lock_is_named() {
    let locks = doc_check::Locks::read();
    let good = json!({"ok": {"rules": {"kind": "work_rules", "schema_version": 1}}});
    assert_eq!(locks.check(&good), vec![("work_rules v1".to_string(), vec![])]);
    let bad = json!({"ok": {"rules": {"kind": "work_rules", "schema_version": 1, "recovery": "lots"}}});
    let found = locks.check(&bad);
    assert_eq!(found.len(), 1);
    assert!(!found[0].1.is_empty(), "a wrong field type is a problem: {found:?}");
}

#[test]
fn a_version_with_no_lock_is_named_and_a_lookalike_is_no_document() {
    let locks = doc_check::Locks::read();
    let found = locks.check(&json!({"ok": {"doc": {"kind": "work_rules", "schema_version": 9}}}));
    assert_eq!(found[0].1, vec!["the locks have no schema for that version".to_string()]);
    assert!(locks.check(&json!({"notes": [{"kind": "dropped_trips", "count": 1}]})).is_empty());
}
