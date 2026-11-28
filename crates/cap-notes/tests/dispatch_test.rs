//! Native dispatch tests for cap-notes without wasm (`OPS.md`).
//! `validate` and the kernel's load agree on one identity, and a refused
//! document is refused at both with its reasons named. `write` stamps with
//! what the host gives. The document round-trips through the log records,
//! and garbage is an error, never a panic.

use serde_json::{json, Value};

fn run(request: Value) -> Value {
    serde_json::from_str(&cap_notes::invoke(&request.to_string())).unwrap()
}

fn ok(request: Value) -> Value {
    let answer = run(request.clone());
    answer
        .get("ok")
        .cloned()
        .unwrap_or_else(|| panic!("{request} answered {answer}"))
}

fn err(request: Value) -> String {
    let answer = run(request.clone());
    answer
        .get("error")
        .and_then(|e| e.get("message"))
        .and_then(Value::as_str)
        .unwrap_or_else(|| panic!("{request} answered {answer}"))
        .to_string()
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
    assert_eq!(
        v["refusals"][0],
        json!({ "note": "n1", "problem": { "kind": "blank_text" } })
    );
    assert!(v["document"].is_null());
    assert!(
        run(json!({ "op": "load_notes", "notes": bad }))["error"]["message"]
            .as_str()
            .unwrap()
            .contains("n1")
    );
}

#[test]
fn write_begins_and_changes_the_document_with_the_hosts_stamps() {
    let first = ok(
        json!({ "op": "write", "id": "n1", "target": trip(), "text": "No operator.", "by": "r.alvarez", "at": 1000 }),
    );
    assert_eq!(
        first["notes"]["notes"]["n1"]["written"],
        json!({ "by": "r.alvarez", "at": 1000 })
    );
    let edited = ok(
        json!({ "op": "write", "notes_hash": first["notes_hash"], "id": "n1", "target": trip(),
                            "text": "No operator. Next trip covered.", "by": "k.moore", "at": 1060 }),
    );
    assert_eq!(
        edited["notes"]["notes"]["n1"]["edited"],
        json!({ "by": "k.moore", "at": 1060 })
    );
    let moved = ok(
        json!({ "op": "write", "notes_hash": edited["notes_hash"], "id": "n1",
                           "target": { "kind": "stop", "stop": "A" }, "text": "x", "by": "k.moore", "at": 1070 }),
    );
    assert_eq!(
        moved["refusals"][0]["problem"],
        json!({ "kind": "target_changed" })
    );
    assert!(moved["notes_hash"].is_null());
}

#[test]
fn a_targets_notes_and_a_spans_notes_answer_over_a_registered_document() {
    let notes_hash = ok(json!({ "op": "load_notes", "notes": sample() }))["notes_hash"].clone();
    let on = ok(json!({ "op": "on", "notes_hash": notes_hash, "target": trip() }));
    assert_eq!(on["notes"].as_array().unwrap().len(), 1);
    assert_eq!(on["notes"][0]["id"], "n1");
    let span = ok(json!({ "op": "between", "notes_hash": notes_hash, "from": 0, "to": 2000 }));
    let ids: Vec<&str> = span["notes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, ["n1", "n2"], "newest first");
    assert_eq!(
        ok(json!({ "op": "between", "from": 0, "to": 2000 }))["notes"],
        json!([]),
        "no document is an empty one"
    );
}

/// The same document comes back under the same hash.
#[test]
fn the_document_round_trips_through_the_log_records() {
    let notes_hash = ok(json!({ "op": "load_notes", "notes": sample() }))["notes_hash"].clone();
    let next = ok(json!({ "op": "log_records", "notes_hash": notes_hash }))["next"].clone();
    let kinds: Vec<&str> = next
        .as_array()
        .unwrap()
        .iter()
        .map(|k| k["kind"].as_str().unwrap())
        .collect();
    assert_eq!(kinds, ["note", "notes"]);
    let held = |kind: &Value| -> Value {
        let version = kind["version"].clone();
        Value::Object(
            kind["records"]
                .as_object()
                .unwrap()
                .iter()
                .map(|(id, v)| (id.clone(), json!({ "version": version, "value": v })))
                .collect(),
        )
    };
    let records = held(&next[0]);
    let document = held(&next[1])["document"].clone();
    let back = ok(json!({ "op": "log_document", "records": records, "document": document }));
    assert_eq!(back["document_hash"], notes_hash);
}

#[test]
fn garbage_is_an_error_not_a_panic() {
    assert!(
        err(json!({ "op": "on", "notes_hash": "sha256:nope", "target": trip() }))
            .contains("sha256:nope")
    );
    assert!(err(json!({ "op": "write", "id": "n1" })).contains("missing field"));
    assert!(run(json!({ "op": "load_notes", "notes": [1, 2] }))["error"]["message"].is_string());
}

fn header(seq: u64, key: Option<&str>) -> Value {
    let mut intent = json!({ "op": "POST /logs/notes" });
    if let Some(k) = key {
        intent["key"] = json!(k);
    }
    json!({ "log": "notes", "seq": seq, "writer": "r.alvarez", "at_ms": 2_000_000, "module": "cap-notes@test", "intent": intent })
}

fn write_op(id: &str, text: &str) -> Value {
    json!({ "cap": "notes", "op": "write", "args": { "id": id, "target": trip(), "text": text } })
}

fn folded(entry: &str) -> Value {
    let stored = [transit_log::Stored {
        seq: 1,
        text: entry.into(),
    }];
    let fold = transit_log::fold(
        transit_log::Checkpoint::empty("notes"),
        &stored,
        &cap_abi::record_hash,
    )
    .unwrap();
    serde_json::to_value(fold.fold).unwrap()
}

#[test]
fn receive_writes_the_note_stamped_by_the_header_and_answers_its_entry() {
    let got = ok(
        json!({ "op": "receive", "write": { "header": header(1, Some("device-1/1")) }, "ops": [write_op("n1", "No operator.")] }),
    );
    let entry: Value = serde_json::from_str(got["entry"].as_str().expect("an entry")).unwrap();
    assert_eq!(entry["seq"], 1);
    assert_eq!(
        got["changes"], 2,
        "the note and the document's own record: {got}"
    );
    let note = entry["changes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["kind"] == "note")
        .expect("the note changes");
    assert_eq!(
        note["after"]["written"],
        json!({ "by": "r.alvarez", "at": 2000 }),
        "the header's writer and clock, never the page's"
    );
    let head = got["heads"]["notes"]
        .as_str()
        .expect("the head after the write");
    let again = ok(json!({ "op": "receive", "heads": { "notes": head },
        "write": { "checkpoint": folded(got["entry"].as_str().unwrap()), "header": header(2, Some("device-1/1")) },
        "ops": [write_op("n1", "No operator.")] }));
    assert_eq!(again["landed"], 1, "{again}");
    assert!(again.get("entry").is_none());
    let refused_now = ok(json!({ "op": "receive", "heads": { "notes": head },
        "write": { "checkpoint": folded(got["entry"].as_str().unwrap()), "header": header(2, Some("device-1/1")) },
        "ops": [write_op("n1", "  ")] }));
    assert_eq!(refused_now["landed"], 1, "{refused_now}");
}

#[test]
fn receive_answers_a_refused_op_with_no_entry() {
    let got = ok(
        json!({ "op": "receive", "write": { "header": header(1, None) }, "ops": [write_op("n1", "  ")] }),
    );
    assert_eq!(got["refused"]["op"], 0, "{got}");
    assert!(!got["refused"]["refusals"].as_array().unwrap().is_empty());
    assert!(got.get("entry").is_none() && got.get("heads").is_none());
}

#[test]
fn receive_refuses_another_log_s_op_and_a_page_that_names_the_writer_is_not_believed() {
    let other = ok(
        json!({ "op": "receive", "write": { "header": header(1, None) }, "ops": [{ "cap": "alerts", "op": "put", "args": {} }] }),
    );
    assert_eq!(other["failed"]["op"], 0, "{other}");
    assert!(
        other["failed"]["error"]["message"]
            .as_str()
            .unwrap()
            .contains("not a write to the notes log"),
        "{other}"
    );
    let unread = ok(
        json!({ "op": "receive", "write": { "header": header(1, None) }, "ops": [{ "cap": "notes", "op": "write", "args": { "id": "n1" } }] }),
    );
    assert!(
        unread["failed"]["error"]["message"]
            .as_str()
            .unwrap()
            .contains("notes write"),
        "arguments it cannot read: {unread}"
    );
    let mut named = write_op("n1", "Called the rider.");
    named["args"]["by"] = json!("someone-else");
    let got =
        ok(json!({ "op": "receive", "write": { "header": header(1, None) }, "ops": [named] }));
    let entry: Value = serde_json::from_str(got["entry"].as_str().unwrap()).unwrap();
    let note = entry["changes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["kind"] == "note")
        .unwrap();
    assert_eq!(note["after"]["written"]["by"], "r.alvarez");
}

#[test]
fn receive_over_a_seq_the_fold_is_not_at_is_an_error() {
    let e = err(
        json!({ "op": "receive", "write": { "header": header(3, None) }, "ops": [write_op("n1", "x")] }),
    );
    assert!(e.contains("seq is 3, and the fold is at 0"), "{e}");
}
