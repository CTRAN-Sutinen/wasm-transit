//! Native dispatch tests for the notes capability: the seam without
//! wasm. The document's own laws are tested at the library seam
//! (transit-notes). Here the module's contract is pinned: validate and
//! the kernel's load agree on one identity, a refused document is
//! refused at both with its reasons named, write begins and changes the
//! document with the stamps the host gives, a target's notes and a
//! span's notes answer over a registered document, the document round
//! trips through the log records, and garbage is an honest error, never
//! a panic.

use serde_json::{json, Value};

fn run(request: Value) -> Value {
    serde_json::from_str(&cap_notes::invoke(&request.to_string())).unwrap()
}

fn ok(request: Value) -> Value {
    let answer = run(request.clone());
    answer.get("ok").cloned().unwrap_or_else(|| panic!("{request} answered {answer}"))
}

fn err(request: Value) -> String {
    let answer = run(request.clone());
    answer.get("error").and_then(Value::as_str).unwrap_or_else(|| panic!("{request} answered {answer}")).to_string()
}

fn trip() -> Value {
    json!({ "kind": "trip", "trip": "t10", "date": "2026-11-19", "start_secs": 28800 })
}

fn sample() -> Value {
    json!({
        "kind": "notes", "schema_version": 1,
        "notes": {
            "n1": { "target": trip(), "text": "No operator.", "written": { "by": "r.alvarez", "at": 1000 } },
            "n2": { "target": { "kind": "alert", "id": "a1" }, "text": "Alert posted.", "written": { "by": "j.tran", "at": 900 },
                    "edited": { "by": "j.tran", "at": 950 } }
        }
    })
}

#[test]
fn validate_and_the_load_agree_on_one_identity() {
    let v = ok(json!({ "op": "validate", "notes": sample() }));
    let l = ok(json!({ "op": "load_notes", "notes": sample() }));
    assert_eq!(v["document_hash"], l["notes_hash"]);
    assert_eq!(l["notes"], 2);
}

#[test]
fn a_refused_document_names_each_reason_and_never_registers() {
    let mut bad = sample();
    bad["notes"]["n1"]["text"] = json!(" ");
    let v = ok(json!({ "op": "validate", "notes": bad }));
    assert_eq!(v["refusals"][0], json!({ "note": "n1", "problem": { "kind": "blank_text" } }));
    assert!(v["document"].is_null());
    assert!(run(json!({ "op": "load_notes", "notes": bad }))["error"].as_str().unwrap().contains("n1"));
}

#[test]
fn write_begins_and_changes_the_document_with_the_hosts_stamps() {
    let first = ok(json!({ "op": "write", "id": "n1", "target": trip(), "text": "No operator.", "by": "r.alvarez", "at": 1000 }));
    assert_eq!(first["notes"]["notes"]["n1"]["written"], json!({ "by": "r.alvarez", "at": 1000 }));
    let edited = ok(json!({ "op": "write", "notes_hash": first["notes_hash"], "id": "n1", "target": trip(),
                            "text": "No operator. Next trip covered.", "by": "k.moore", "at": 1060 }));
    assert_eq!(edited["notes"]["notes"]["n1"]["edited"], json!({ "by": "k.moore", "at": 1060 }));
    let moved = ok(json!({ "op": "write", "notes_hash": edited["notes_hash"], "id": "n1",
                           "target": { "kind": "stop", "stop": "A" }, "text": "x", "by": "k.moore", "at": 1070 }));
    assert_eq!(moved["refusals"][0]["problem"], json!({ "kind": "target_changed" }));
    assert!(moved["notes_hash"].is_null());
}

#[test]
fn a_targets_notes_and_a_spans_notes_answer_over_a_registered_document() {
    let notes_hash = ok(json!({ "op": "load_notes", "notes": sample() }))["notes_hash"].clone();
    let on = ok(json!({ "op": "on", "notes_hash": notes_hash, "target": trip() }));
    assert_eq!(on["notes"].as_array().unwrap().len(), 1);
    assert_eq!(on["notes"][0]["id"], "n1");
    let span = ok(json!({ "op": "between", "notes_hash": notes_hash, "from": 0, "to": 2000 }));
    let ids: Vec<&str> = span["notes"].as_array().unwrap().iter().map(|r| r["id"].as_str().unwrap()).collect();
    assert_eq!(ids, ["n1", "n2"], "newest first");
    assert_eq!(ok(json!({ "op": "between", "from": 0, "to": 2000 }))["notes"], json!([]), "no document is an empty one");
}

/// The document becomes the notes log's records and back (D-197): the
/// same document, under the same hash.
#[test]
fn the_document_round_trips_through_the_log_records() {
    let notes_hash = ok(json!({ "op": "load_notes", "notes": sample() }))["notes_hash"].clone();
    let next = ok(json!({ "op": "log_records", "notes_hash": notes_hash }))["next"].clone();
    let kinds: Vec<&str> = next.as_array().unwrap().iter().map(|k| k["kind"].as_str().unwrap()).collect();
    assert_eq!(kinds, ["note", "notes"]);
    let held = |kind: &Value| -> Value {
        let version = kind["version"].clone();
        Value::Object(kind["records"].as_object().unwrap().iter().map(|(id, v)| (id.clone(), json!({ "version": version, "value": v }))).collect())
    };
    let records = held(&next[0]);
    let document = held(&next[1])["document"].clone();
    let back = ok(json!({ "op": "log_document", "records": records, "document": document }));
    assert_eq!(back["document_hash"], notes_hash);
}

#[test]
fn garbage_is_an_error_not_a_panic() {
    assert!(err(json!({ "op": "on", "notes_hash": "sha256:nope", "target": trip() })).contains("sha256:nope"));
    assert!(err(json!({ "op": "write", "id": "n1" })).contains("missing field"));
    assert!(run(json!({ "op": "load_notes", "notes": [1, 2] }))["error"].is_string());
}
