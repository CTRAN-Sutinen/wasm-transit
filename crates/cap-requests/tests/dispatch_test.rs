//! Native dispatch tests for the stop requests capability — the seam
//! without wasm. The document's own laws are tested at the library seam
//! (transit-requests); here the module's contract is pinned: validate
//! and the kernel's load agree on one identity, a refused document is
//! refused at both with its reasons named, put begins and changes the
//! document, a stop's requests are answered, and garbage is an honest
//! error, never a panic.

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
        "kind": "stop_requests", "schema_version": 1,
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
