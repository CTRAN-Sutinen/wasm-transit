//! Native dispatch tests for the stop requests capability. These test the
//! seam without wasm. The document's own laws are tested at the library
//! seam (transit-requests). Here the module's contract is pinned. validate
//! and the kernel's load agree on one identity. A refused document is
//! refused at both, with its reasons named. put begins and changes the
//! document. A stop's requests are answered. Garbage is an honest error,
//! never a panic.

use serde_json::{json, Value};

fn run(request: Value) -> Value {
    serde_json::from_str(&cap_requests::invoke(&request.to_string())).unwrap()
}

fn ok(request: Value) -> Value {
    let answer = run(request.clone());
    answer.get("ok").cloned().unwrap_or_else(|| panic!("{request} answered {answer}"))
}

fn sample() -> Value {
    json!({
        "kind": "stop_requests", "schema_version": 2,
        "requests": {
            "R-1": { "stop": "N1", "job": "new", "status": "submitted", "requested_on": "2027-01-04", "routes": ["37"], "direction": "eastbound" },
            "R-2": { "stop": "6124", "job": "repair_improve", "status": "drafted" }
        }
    })
}

#[test]
fn validate_and_the_load_agree_on_one_identity() {
    let v = ok(json!({ "op": "validate", "requests": sample() }));
    let l = ok(json!({ "op": "load_stop_requests", "requests": sample() }));
    assert_eq!(v["document_hash"], l["requests_hash"]);
    assert_eq!(l["requests"], 2);
}

#[test]
fn a_refused_document_names_each_reason_and_never_registers() {
    let mut bad = sample();
    bad["requests"]["R-1"]["status"] = json!("ordered");
    let v = ok(json!({ "op": "validate", "requests": bad }));
    assert_eq!(v["refusals"][0], json!({ "request": "R-1", "problem": { "kind": "no_work_order" } }));
    assert!(v["document"].is_null());
    assert!(run(json!({ "op": "load_stop_requests", "requests": bad }))["error"].as_str().unwrap().contains("R-1"));
}

#[test]
fn put_begins_and_changes_the_document() {
    let first = ok(json!({ "op": "put", "id": "R-1", "request": { "stop": "N1", "job": "new", "status": "drafted" } }));
    assert_eq!(first["refusals"], json!([]));
    let hash = first["requests_hash"].clone();
    let next = ok(json!({ "op": "put", "requests_hash": hash, "id": "R-1",
        "request": { "stop": "N1", "job": "new", "status": "ordered", "work_order": { "system": "Maintenance", "id": "WO-1" } } }));
    assert_eq!(next["requests"]["requests"]["R-1"]["work_order"]["id"], "WO-1");
    let refused = ok(json!({ "op": "put", "requests_hash": next["requests_hash"], "id": "R-1",
        "request": { "stop": "N1", "job": "new", "status": "completed" } }));
    assert_eq!(refused["refusals"][0]["problem"]["kind"], "no_completion");
    assert!(refused["requests_hash"].is_null());
    let listed = ok(json!({ "op": "requests_for", "requests_hash": next["requests_hash"], "stop": "N1" }));
    assert_eq!(listed["requests"][0]["id"], "R-1");
    assert_eq!(ok(json!({ "op": "requests_for", "requests_hash": next["requests_hash"], "stop": "nope" }))["requests"], json!([]));
}

/// The work across every stop: filtered by status and job, every
/// request counted by status whatever the filters ask.
#[test]
fn list_is_the_work_across_every_stop() {
    let a = ok(json!({ "op": "put", "id": "R-1", "request": { "stop": "N1", "job": "new", "status": "drafted" } }));
    let b = ok(json!({ "op": "put", "requests_hash": a["requests_hash"], "id": "R-2", "request": { "stop": "N2", "job": "repair_improve", "status": "submitted" } }));
    let c = ok(json!({ "op": "put", "requests_hash": b["requests_hash"], "id": "R-3", "request": { "stop": "N1", "job": "repair_improve", "status": "drafted" } }));
    let hash = c["requests_hash"].clone();
    let all = ok(json!({ "op": "list", "requests_hash": hash }));
    let ids: Vec<&str> = all["requests"].as_array().unwrap().iter().map(|r| r["id"].as_str().unwrap()).collect();
    assert_eq!(ids, ["R-1", "R-2", "R-3"]);
    assert_eq!(all["counts"], json!([{ "status": "drafted", "requests": 2 }, { "status": "submitted", "requests": 1 }]));
    let some = ok(json!({ "op": "list", "requests_hash": hash, "statuses": ["drafted"], "jobs": ["repair_improve"] }));
    assert_eq!(some["requests"].as_array().unwrap().len(), 1);
    assert_eq!(some["requests"][0]["id"], "R-3");
    assert_eq!(some["counts"], all["counts"]);
    assert!(run(json!({ "op": "list", "requests_hash": "sha256:00" }))["error"].is_string());
}

#[test]
fn the_vocabulary_is_the_librarys() {
    let v = ok(json!({ "op": "vocabulary" }));
    assert_eq!(v["jobs"], json!(transit_requests::JOBS));
    assert_eq!(v["statuses"], json!(transit_requests::STATUSES));
    assert_eq!(v["open_statuses"], json!(["drafted", "submitted", "ordered"]));
}

/// A write that says the request is new is refused a held id; an
/// ordinary put replaces it (D-228).
#[test]
fn a_new_request_is_refused_a_held_id() {
    let held = ok(json!({ "op": "put", "id": "R-1", "request": { "stop": "N1", "job": "new", "status": "drafted" } }));
    let refused = ok(json!({ "op": "put", "requests_hash": held["requests_hash"], "id": "R-1", "new": true,
        "request": { "stop": "N2", "job": "new", "status": "drafted" } }));
    assert_eq!(refused["refusals"], json!([{ "request": "R-1", "problem": { "kind": "id_taken" } }]));
    assert!(refused["requests_hash"].is_null());
    let free = ok(json!({ "op": "put", "requests_hash": held["requests_hash"], "id": "R-2", "new": true,
        "request": { "stop": "N2", "job": "new", "status": "drafted" } }));
    assert_eq!(free["refusals"], json!([]));
    let replaced = ok(json!({ "op": "put", "requests_hash": held["requests_hash"], "id": "R-1",
        "request": { "stop": "N2", "job": "new", "status": "drafted" } }));
    assert_eq!(replaced["requests"]["requests"]["R-1"]["stop"], "N2");
}

#[test]
fn garbage_is_an_error_not_a_panic() {
    assert!(run(json!({ "op": "nope" }))["error"].is_string());
    assert!(run(json!({ "op": "requests_for", "requests_hash": "sha256:00", "stop": "a" }))["error"].is_string());
    assert!(run(json!({ "op": "validate", "requests": { "kind": "stops" } }))["error"].is_string());
}

/// What the work order system answered lands on its request (D-165),
/// checked; an unknown request is an error.
#[test]
fn a_work_order_is_recorded_on_its_request() {
    let held = ok(json!({ "op": "load_stop_requests", "requests": sample() }));
    let out = ok(json!({ "op": "record_work_order", "requests_hash": held["requests_hash"], "id": "R-1",
        "work_order": { "system": "stub", "id": "STUB-R-1", "status": "open", "read_on": "2027-01-07" } }));
    assert_eq!(out["refusals"], json!([]));
    assert_eq!(out["requests"]["requests"]["R-1"]["status"], "ordered");
    assert_eq!(out["requests"]["requests"]["R-1"]["work_order"]["id"], "STUB-R-1");
    assert!(run(json!({ "op": "record_work_order", "requests_hash": held["requests_hash"], "id": "nope", "work_order": { "id": "x" } }))["error"].is_string());
}

/// A field this build does not know is kept and flagged, never refused
/// (D-167).
#[test]
fn validate_flags_an_unknown_field_and_keeps_it() {
    let mut newer = sample();
    let first = newer["requests"].as_object().unwrap().keys().next().unwrap().clone();
    newer["requests"][&first]["permit_fee"] = json!(125);
    let valid = ok(json!({"op": "validate", "requests": newer}));
    assert_eq!(valid["refusals"], json!([]));
    assert_eq!(valid["unknown_fields"], json!(["permit_fee"]));
    let document: Value = serde_json::from_str(valid["document"].as_str().unwrap()).unwrap();
    assert_eq!(document["requests"][&first]["permit_fee"], 125);
}

/// The stops log (D-197): the document's records are its requests, and
/// the document assembled from them is the same document.
#[test]
fn the_document_and_its_log_records_round_trip() {
    let v = ok(json!({ "op": "validate", "requests": sample() }));
    let hash = ok(json!({ "op": "load_stop_requests", "requests": sample() }))["requests_hash"].clone();
    let next = ok(json!({ "op": "log_records", "requests_hash": hash }))["next"].clone();
    let records = &next[0];
    assert_eq!(records["kind"], "stop_request");
    assert_eq!(records["records"].as_object().unwrap().len(), 2);
    assert_eq!(next[1]["kind"], "stop_requests");
    let held = |version: u64| -> serde_json::Map<String, Value> {
        records["records"].as_object().unwrap().iter().map(|(id, v)| (id.clone(), json!({"version": version, "value": v}))).collect()
    };
    let own = next[1]["records"]["document"].clone();
    let back = ok(json!({ "op": "log_document", "records": held(2), "document": {"version": 2, "value": own} }));
    assert_eq!(back["document_hash"], v["document_hash"]);
    assert_eq!(back["document"], v["document"]);
    // A log written before v2 keeps its records at v1: they come forward
    // to the same document.
    let mut own_v1 = own.clone();
    own_v1["schema_version"] = json!(1);
    let old = ok(json!({ "op": "log_document", "records": held(1), "document": {"version": 1, "value": own_v1} }));
    assert_eq!(old["document_hash"], v["document_hash"]);
}

/// The call to the work order system is an op's answer (D-228): the
/// request for an open, the held work order for a refresh.
#[test]
fn the_work_order_call_answers_what_to_send() {
    let held = ok(json!({ "op": "put", "id": "R-1", "request": { "stop": "N1", "job": "new", "status": "submitted" } }));
    let hash = held["requests_hash"].clone();
    let open = ok(json!({ "op": "work_order_call", "requests_hash": hash, "id": "R-1", "action": "open" }));
    assert_eq!(open["call"], json!({ "action": "open", "request": { "stop": "N1", "job": "new", "status": "submitted" } }));
    let refresh = ok(json!({ "op": "work_order_call", "requests_hash": hash, "id": "R-1", "action": "refresh" }));
    assert_eq!(refresh["call"], json!({ "action": "refresh" }));
    assert!(run(json!({ "op": "work_order_call", "requests_hash": hash, "id": "R-9", "action": "open" }))["error"].is_string());
    assert!(run(json!({ "op": "work_order_call", "requests_hash": hash, "id": "R-1", "action": "close" }))["error"].is_string());
}
