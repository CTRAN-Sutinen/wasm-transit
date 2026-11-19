//! Native dispatch tests for the log capability (D-190, D-197): the
//! seam without wasm. The log's laws are property-tested in
//! transit-log; here the module's contract is pinned:
//!
//! 1. `compose` answers an entry with each `before` the canonical hash
//!    of the record as stored, and `fold` of the entries holds the
//!    records of the last write, with a checkpoint to start from;
//! 2. a write that changes nothing answers no entry, and a key that
//!    landed answers its entry;
//! 3. one entry changes two kinds;
//! 4. a revert writes back, and a record the entry created leaves the
//!    fold (D-198); a later change refuses it; a kind's written records
//!    take the place of the states before, over the states it answers;
//! 5. a bad entry is quarantined and named; a gap's entry needs a cause;
//! 6. the entry's `seq` must follow the fold.

use cap_log::{dispatch_json};
use serde_json::{json, Value};

fn run(request: Value) -> Value {
    serde_json::from_str(&dispatch_json(&request.to_string())).unwrap()
}

fn ok(request: Value) -> Value {
    let answer = run(request.clone());
    answer.get("ok").cloned().unwrap_or_else(|| panic!("{request} answered {answer}"))
}

fn err(request: Value) -> String {
    let answer = run(request.clone());
    answer["error"].as_str().unwrap_or_else(|| panic!("{request} answered {answer}")).to_string()
}

fn header(seq: u64, key: Option<&str>) -> Value {
    let mut intent = json!({"op": "POST /logs/stops"});
    if let Some(k) = key {
        intent["key"] = json!(k);
    }
    json!({"log": "stops", "seq": seq, "writer": "planner-3", "at_ms": seq * 1000, "module": "cap-log@sha256:test", "intent": intent})
}

/// A small log, built through the seam: each write's entry, composed
/// over the fold of the entries before it.
struct Log {
    checkpoint: Option<Value>,
    entries: Vec<Value>,
}

impl Log {
    fn new() -> Self {
        Log { checkpoint: None, entries: Vec::new() }
    }
    fn seq(&self) -> u64 {
        self.entries.len() as u64
    }
    fn write(&mut self, next: Value, key: Option<&str>) -> Value {
        let mut req = json!({"op": "compose", "header": header(self.seq() + 1, key), "next": next});
        if let Some(c) = &self.checkpoint {
            req["checkpoint"] = c.clone();
        }
        let answer = ok(req);
        if let Some(entry) = answer["entry"].as_str() {
            self.entries.push(json!({"seq": self.seq() + 1, "text": entry}));
            self.checkpoint = Some(self.fold()["checkpoint"].as_str().map(|c| serde_json::from_str(c).unwrap()).unwrap());
        }
        answer
    }
    fn fold(&self) -> Value {
        ok(json!({"op": "fold", "log": "stops", "entries": self.entries}))
    }
}

fn stops(records: Value) -> Value {
    json!({"kind": "stop", "version": 1, "records": records})
}

fn requests(records: Value) -> Value {
    json!({"kind": "stop_request", "version": 1, "records": records})
}

#[test]
fn the_fold_of_the_entries_is_the_last_write() {
    let mut log = Log::new();
    let a1 = json!([{"on": "2027-01-10", "event": "planned"}]);
    log.write(json!([stops(json!({"A": a1}))]), None);
    let a2 = json!([{"on": "2027-01-10", "event": "planned"}, {"on": "2027-05-03", "event": "permitted"}]);
    let second = log.write(json!([stops(json!({"A": a2, "B": [{"event": "in_service"}]}))]), None);
    assert_eq!(second["changes"], 2);
    let entry: Value = serde_json::from_str(log.entries[1]["text"].as_str().unwrap()).unwrap();
    assert_eq!(entry["kind"], "log_entry");
    assert_eq!(entry["changes"][0]["before"], cap_abi::canonical_hash(&a1).unwrap(), "the record as stored before");
    assert!(entry["changes"][1].get("before").is_none(), "B is new");
    let folded = log.fold();
    assert_eq!(folded["seq"], 2);
    assert_eq!(folded["records"]["stop"]["A"], json!({"version": 1, "value": a2}));
    assert_eq!(folded["quarantined"], json!([]));
    // From the checkpoint at 1, the same fold.
    let at1: Value = serde_json::from_str(ok(json!({"op": "fold", "log": "stops", "entries": [log.entries[0]]}))["checkpoint"].as_str().unwrap()).unwrap();
    let again = ok(json!({"op": "fold", "log": "stops", "checkpoint": at1, "entries": [log.entries[1]]}));
    assert_eq!(again["checkpoint"], folded["checkpoint"]);
}

#[test]
fn nothing_changed_is_no_entry_and_a_key_lands_once() {
    let mut log = Log::new();
    log.write(json!([stops(json!({"A": [1]}))]), Some("tablet-4/17"));
    let same = log.write(json!([stops(json!({"A": [1]}))]), None);
    assert!(same.get("entry").is_none() && same["changes"] == 0, "{same}");
    let resent = log.write(json!([stops(json!({"A": [2]}))]), Some("tablet-4/17"));
    assert_eq!(resent["landed"], 1, "the outbox's resend lands once");
    assert_eq!(log.seq(), 1);
}

#[test]
fn one_entry_changes_two_kinds() {
    let mut log = Log::new();
    log.write(json!([stops(json!({"B": [{"event": "in_service"}]})), requests(json!({"B-1": {"status": "ordered"}}))]), None);
    let done = log.write(
        json!([stops(json!({"B": [{"event": "in_service"}, {"event": "moved", "request": "B-1"}]})), requests(json!({"B-1": {"status": "completed"}}))]),
        Some("complete-B-1"),
    );
    assert_eq!(done["changes"], 2);
    let entry: Value = serde_json::from_str(log.entries[1]["text"].as_str().unwrap()).unwrap();
    let kinds: Vec<&str> = entry["changes"].as_array().unwrap().iter().map(|c| c["kind"].as_str().unwrap()).collect();
    assert_eq!(kinds, vec!["stop", "stop_request"]);
}

#[test]
fn a_revert_writes_back_and_a_later_change_refuses_it() {
    let mut log = Log::new();
    log.write(json!([stops(json!({"A": [1]}))]), None);
    log.write(json!([stops(json!({"A": [2], "N": [9]}))]), None);
    let back = ok(json!({"op": "revert", "log": "stops", "entries": log.entries, "target": 2, "header": header(3, None)}));
    assert_eq!(back["changes"], 2);
    log.entries.push(json!({"seq": 3, "text": back["entry"]}));
    let folded = log.fold();
    assert_eq!(folded["records"]["stop"], json!({"A": {"version": 1, "value": [1]}}), "A back, N out of the fold (D-198)");
    let entry: Value = serde_json::from_str(back["entry"].as_str().unwrap()).unwrap();
    assert_eq!(entry["reverts"], 2);
    let refused = err(json!({"op": "revert", "log": "stops", "entries": log.entries, "target": 2, "header": header(4, None)}));
    assert!(refused.contains("stop A") && refused.contains("a person decides"), "{refused}");
}

#[test]
fn a_revert_writes_what_the_kind_gives_in_place_of_the_states_before() {
    let mut log = Log::new();
    log.write(json!([stops(json!({"A": [1]}))]), None);
    log.write(json!([stops(json!({"A": [2], "N": [9]}))]), None);
    let plain = ok(json!({"op": "revert", "log": "stops", "entries": log.entries, "target": 2, "header": header(3, None)}));
    assert_eq!(
        plain["states"],
        json!([{"kind": "stop", "id": "A", "now": {"version": 1, "value": [2]}, "prior": {"version": 1, "value": [1]}},
               {"kind": "stop", "id": "N", "now": {"version": 1, "value": [9]}}])
    );
    let written = json!([{"kind": "stop", "id": "N", "version": 1, "after": ["retired"]}]);
    let back = ok(json!({"op": "revert", "log": "stops", "entries": log.entries, "target": 2, "header": header(3, None), "written": written}));
    log.entries.push(json!({"seq": 3, "text": back["entry"]}));
    assert_eq!(
        log.fold()["records"]["stop"],
        json!({"A": {"version": 1, "value": [1]}, "N": {"version": 1, "value": ["retired"]}}),
        "A back to its state before, N as the kind wrote it"
    );
}

#[test]
fn a_bad_entry_is_quarantined_and_a_gap_needs_a_cause() {
    let mut log = Log::new();
    log.write(json!([stops(json!({"A": [1]}))]), None);
    log.entries.push(json!({"seq": 2, "text": "{\"kind\":\"log_entry\""}));
    let folded = log.fold();
    assert_eq!(folded["quarantined"][0]["seq"], 2);
    assert_eq!(folded["seq"], 2);
    assert!(err(json!({"op": "close_gap", "header": header(5, None)})).contains("cause"));
    let mut h = header(5, None);
    h["intent"]["note"] = json!("deleted by hand");
    let closed: Value = serde_json::from_str(ok(json!({"op": "close_gap", "header": h}))["entry"].as_str().unwrap()).unwrap();
    assert_eq!(closed["changes"], json!([]));
}

#[test]
fn the_entry_follows_the_fold() {
    assert!(err(json!({"op": "compose", "header": header(3, None), "next": [stops(json!({"A": [1]}))]})).contains("fold is at 0"));
    assert!(err(json!({"op": "fold", "log": "plans", "checkpoint": {"kind": "log_checkpoint", "schema_version": 1, "log": "stops", "seq": 0, "records": {}, "keys": {}}, "entries": []})).contains("stops log"));
}
