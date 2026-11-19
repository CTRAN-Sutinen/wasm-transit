//! Native dispatch tests for the service alerts capability: the seam
//! without wasm. The documents' own laws are tested at the library seam
//! (transit-alerts). Here the module's contract is pinned: validate and
//! the kernel's load agree on one identity, a refused document is
//! refused at both with its reasons named, put begins and changes the
//! document, end closes an alert, the list, the notices, the gap list,
//! and the template fill answer over registered documents, and garbage
//! is an honest error, never a panic.

use serde_json::{json, Value};

fn run(request: Value) -> Value {
    serde_json::from_str(&cap_alerts::invoke(&request.to_string())).unwrap()
}

fn ok(request: Value) -> Value {
    let answer = run(request.clone());
    answer.get("ok").cloned().unwrap_or_else(|| panic!("{request} answered {answer}"))
}

fn err(request: Value) -> String {
    let answer = run(request.clone());
    answer.get("error").and_then(Value::as_str).unwrap_or_else(|| panic!("{request} answered {answer}")).to_string()
}

fn sample() -> Value {
    json!({
        "kind": "service_alerts", "schema_version": 1,
        "alerts": {
            "a1": { "status": "published", "effect": "detour", "header": [{ "text": "Route 10 detour" }],
                    "informed": [{ "route": "10" }], "explains": [{ "kind": "edit", "id": "r1" }] },
            "a2": { "status": "draft", "header": [{ "text": "Stop B closed" }], "informed": [{ "stop": "B" }],
                    "communication": [{ "start": 1000, "end": null }] }
        }
    })
}

fn network_hash() -> Value {
    let text = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../web/public/caps/sample-network-load.json")).unwrap();
    ok(serde_json::from_str(&text).unwrap())["network_hash"].clone()
}

#[test]
fn validate_and_the_load_agree_on_one_identity() {
    let v = ok(json!({ "op": "validate", "alerts": sample() }));
    let l = ok(json!({ "op": "load_service_alerts", "alerts": sample() }));
    assert_eq!(v["document_hash"], l["alerts_hash"]);
    assert_eq!(l["alerts"], 2);
}

#[test]
fn a_refused_document_names_each_reason_and_never_registers() {
    let mut bad = sample();
    bad["alerts"]["a1"]["informed"] = json!([]);
    let v = ok(json!({ "op": "validate", "alerts": bad }));
    assert_eq!(v["refusals"][0], json!({ "alert": "a1", "problem": { "kind": "no_entity" } }));
    assert!(v["document"].is_null());
    assert!(run(json!({ "op": "load_service_alerts", "alerts": bad }))["error"].as_str().unwrap().contains("a1"));
}

#[test]
fn put_begins_and_changes_the_document_and_end_closes_an_alert() {
    let first = ok(json!({ "op": "put", "id": "a1", "alert": sample()["alerts"]["a1"] }));
    assert_eq!(first["refusals"], json!([]));
    let refused = ok(json!({ "op": "put", "alerts_hash": first["alerts_hash"], "id": "a2",
        "alert": { "status": "published", "header": [], "informed": [{ "route": "10" }] } }));
    assert_eq!(refused["refusals"][0]["problem"]["kind"], "no_header");
    assert!(refused["alerts_hash"].is_null());
    let ended = ok(json!({ "op": "end", "alerts_hash": first["alerts_hash"], "id": "a1", "at": 5000 }));
    assert_eq!(ended["alerts"]["alerts"]["a1"]["communication"], json!([{ "start": null, "end": 5000 }]));
    assert!(err(json!({ "op": "end", "alerts_hash": first["alerts_hash"], "id": "nope", "at": 5000 })).contains("nope"));
}

#[test]
fn the_list_and_the_notices_answer_over_a_registered_document() {
    let alerts_hash = ok(json!({ "op": "load_service_alerts", "alerts": sample() }))["alerts_hash"].clone();
    let list = ok(json!({ "op": "list", "alerts_hash": alerts_hash, "now": 2000 }));
    assert_eq!(list["in_force"][0]["id"], "a1");
    assert_eq!(list["drafts"][0]["id"], "a2");
    let flags = ok(json!({ "op": "flags", "alerts_hash": alerts_hash, "network_hash": network_hash(), "languages": ["en", "es"] }));
    assert_eq!(flags["flags"][0], json!({ "alert": "a1", "notice": { "kind": "missing_translation", "field": "header", "lang": "en" } }));
    // No document yet: the empty list.
    assert_eq!(ok(json!({ "op": "list", "now": 0 }))["in_force"], json!([]));
}

#[test]
fn the_gap_list_reads_the_edits_and_the_stops() {
    let alerts_hash = ok(json!({ "op": "load_service_alerts", "alerts": sample() }))["alerts_hash"].clone();
    let edits = json!({ "kind": "edit_records", "schema_version": 2, "records": [
        { "id": "r1", "status": "accepted", "rev": 1, "updated_at": 0,
          "cancel": { "id": "c1", "trip_id": "t10", "date": "2026-11-19" } },
        { "id": "r2", "status": "accepted", "rev": 1, "updated_at": 0,
          "cancel": { "id": "c2", "trip_id": "t30", "date": "2026-11-19" } }
    ]});
    let edits_hash = ok(json!({ "op": "load_edits", "edits": edits }))["edits_hash"].clone();
    let stops = json!({ "kind": "stops", "schema_version": 1, "stops": {
        "B": [{ "event": "in_service", "lat": 45.51, "lon": -73.51 }, { "on": "2026-11-01", "event": "suspended" }]
    }});
    let stops_hash = ok(json!({ "op": "load_stops", "stops": stops }))["stops_hash"].clone();
    let gaps = ok(json!({ "op": "gaps", "alerts_hash": alerts_hash, "edits_hash": edits_hash, "stops_hash": stops_hash,
        "date": "2026-11-19", "now": 2000 }));
    let sources: Vec<Value> = gaps["gaps"].as_array().unwrap().iter().map(|g| g["source"].clone()).collect();
    assert_eq!(sources, vec![json!({ "kind": "edit", "id": "r2" }), json!({ "kind": "stop", "stop": "B" })]);
    assert!(err(json!({ "op": "gaps", "date": "11/19", "now": 0 })).contains("11/19"));
}

#[test]
fn a_template_is_checked_and_filled() {
    let templates = json!({ "kind": "alert_templates", "schema_version": 1, "templates": {
        "closure": { "name": "Stop closure", "effect": "no_service", "header": [{ "text": "The stop at {stop} is closed starting {start date}" }] }
    }});
    let v = ok(json!({ "op": "validate_templates", "templates": templates }));
    assert_eq!(v["refusals"], json!([]));
    let templates_hash = ok(json!({ "op": "load_alert_templates", "templates": templates }))["templates_hash"].clone();
    assert_eq!(templates_hash, v["document_hash"]);
    let filled = ok(json!({ "op": "fill", "templates_hash": templates_hash, "template": "closure", "network_hash": network_hash(),
        "input": { "stops": ["A"], "start_date": "2026-08-17" } }));
    assert_eq!(filled["filled"]["header"][0]["text"], "The stop at Main & 1st is closed starting Monday, August 17");
    let bad = ok(json!({ "op": "put_template", "templates_hash": templates_hash, "id": "x", "template": { "name": "X", "header": [{ "text": "{oops}" }] } }));
    assert_eq!(bad["refusals"][0]["problem"], json!({ "kind": "unknown_blank", "blank": "oops" }));
    assert!(err(json!({ "op": "fill", "templates_hash": templates_hash, "template": "nope", "network_hash": network_hash(), "input": {} })).contains("nope"));
}

#[test]
fn garbage_is_an_error_not_a_panic() {
    assert!(run(json!({ "op": "no_such_op" }))["error"].is_string());
    assert!(err(json!({ "op": "list", "alerts_hash": "sha256:none", "now": 0 })).contains("load_service_alerts"));
}
