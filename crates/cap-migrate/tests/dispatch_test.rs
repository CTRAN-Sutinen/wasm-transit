//! cap-migrate's seam laws (docs/units/document-versioning.md):
//!
//! * a v1 network, edit document, and watchlist each come forward to
//!   the version this build reads, and the answer's hash is the one
//!   the kernel registers the migrated bytes under (one identity on
//!   every host);
//! * the chain's losses come back as notes, each naming its step;
//! * a document already current is refused (nothing to migrate); a
//!   newer one is refused naming both versions; an unknown kind names
//!   the kinds it knows;
//! * `version` answers a document's version and the version read.

use cap_migrate::{dispatch_json, KINDS};
use serde_json::{json, Value};

fn ok(request: Value) -> Value {
    let out: Value = serde_json::from_str(&dispatch_json(&request.to_string())).unwrap();
    assert!(out.get("error").is_none(), "unexpected error: {out}");
    out["ok"].clone()
}

fn err(request: Value) -> String {
    let out: Value = serde_json::from_str(&dispatch_json(&request.to_string())).unwrap();
    out["error"].as_str().unwrap_or_else(|| panic!("expected an error: {out}")).to_string()
}

fn v1_network() -> Value {
    json!({
        "name": "Sample",
        "stops": {"s1": {"name": "First", "lat": 45.0, "lon": -122.0}},
        "routes": {"r1": {"name": "1", "mode": "bus"}},
        "patterns": {}, "service_types": {}, "pattern_trips": {}, "blocks": {}
    })
}

#[test]
fn an_untagged_network_comes_forward_under_the_kernels_identity() {
    let answer = ok(json!({"op": "migrate", "kind": "network", "document": v1_network()}));
    assert_eq!((answer["from"].as_u64(), answer["to"].as_u64()), (Some(1), Some(transit_network::SCHEMA_VERSION as u64)));
    assert_eq!(answer["notes"], json!([]));
    let canonical = answer["canonical"].as_str().unwrap();
    assert!(canonical.starts_with("{\"kind\":\"network\",\"schema_version\":2,"), "{canonical}");
    let loaded = cap_abi::parse_network(canonical.as_bytes()).expect("the kernel reads what migrate answers");
    assert_eq!(answer["document_hash"].as_str().unwrap(), cap_abi::canonical_hash(&loaded).unwrap());
    assert_eq!(loaded.stops.keys().map(|k| k.as_str()).collect::<Vec<_>>(), ["s1"], "ids survive");
}

#[test]
fn a_bare_edit_array_comes_forward_as_the_tagged_document() {
    let records = json!([{"id": "e1", "status": "accepted", "rev": 1, "updated_at": 1000,
        "cancel": {"id": "c1", "trip_id": "t1", "start_secs": 28800, "date": "2026-10-23"}}]);
    let answer = ok(json!({"op": "migrate", "kind": "edit_records", "document": records}));
    assert_eq!(answer["document"]["records"], records, "every record, unchanged");
    let doc: transit_sync::EditDocument = serde_json::from_str(answer["canonical"].as_str().unwrap()).unwrap();
    assert_eq!(answer["document_hash"].as_str().unwrap(), cap_abi::canonical_hash(&doc).unwrap());
}

#[test]
fn a_v1_watchlist_comes_forward_reporting_its_colors() {
    let v1 = json!({"version": "v1", "stops": [{"id": "A", "note": "home", "color": "red"}, {"id": "B", "note": "work"}]});
    let answer = ok(json!({"op": "migrate", "kind": "watchlist", "document": v1}));
    assert_eq!(answer["document"], json!({"version": "v2", "stops": [{"id": "A", "label": "home"}, {"id": "B", "label": "work"}]}));
    assert_eq!(answer["notes"], json!([{"from": 1, "kind": "dropped_field", "field": "color", "count": 1}]));
}

#[test]
fn current_newer_and_unknown_documents_are_refused() {
    let current = serde_json::to_value(transit_network::Network::default()).unwrap();
    assert!(err(json!({"op": "migrate", "kind": "network", "document": current})).contains("nothing to migrate"));

    let mut newer = serde_json::to_value(transit_network::Network::default()).unwrap();
    newer["schema_version"] = json!(transit_network::SCHEMA_VERSION + 1);
    let refused = err(json!({"op": "migrate", "kind": "network", "document": newer}));
    assert!(refused.contains("network v3") && refused.contains("reads v2"), "{refused}");

    let unknown = err(json!({"op": "migrate", "kind": "timetable", "document": {}}));
    for (kind, _) in KINDS {
        assert!(unknown.contains(kind), "{unknown}");
    }
}

#[test]
fn version_answers_without_migrating() {
    assert_eq!(ok(json!({"op": "version", "kind": "edit_records", "document": []})), json!({"op": "version", "version": 1, "reads": 2}));
    assert_eq!(
        ok(json!({"op": "version", "kind": "watchlist", "document": {"version": "v2", "stops": []}})),
        json!({"op": "version", "version": 2, "reads": 2})
    );
}
