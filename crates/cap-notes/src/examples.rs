use cap_abi::manifest::Example;
use serde_json::json;
use transit_notes::NoteTarget;

fn trip() -> NoteTarget {
    NoteTarget::Trip { trip: "t10".into(), date: "2026-10-23".into(), start_secs: 28800 }
}

pub fn examples() -> Vec<Example> {
    let notes = fixtures::fixture_notes().checked().expect("the fixture notes pass");
    let notes_hash = cap_abi::canonical_hash(&notes).expect("the fixture notes hash");
    let load = || vec![json!({ "op": "load_notes", "notes": notes })];
    let records: std::collections::BTreeMap<String, serde_json::Value> =
        notes.notes.iter().map(|(id, n)| (id.clone(), json!({ "version": transit_notes::SCHEMA_VERSION, "value": n }))).collect();
    vec![
        Example { bytes: None, kinds: vec![], op: "validate", loads: vec![], request: json!({ "op": "validate", "notes": notes }) },
        Example {
            bytes: None, kinds: vec![],
            op: "write",
            loads: load(),
            request: json!({ "op": "write", "notes_hash": notes_hash, "id": "n3", "target": trip(),
                             "text": "A rider called. Told them the next trip runs.", "by": "service-1", "at": 1_792_758_000u64 }),
        },
        Example { bytes: None, kinds: vec![], op: "on", loads: load(), request: json!({ "op": "on", "notes_hash": notes_hash, "target": trip() }) },
        Example {
            bytes: None, kinds: vec![],
            op: "between",
            loads: load(),
            request: json!({ "op": "between", "notes_hash": notes_hash, "from": fixtures::NOTES_DAY.0, "to": fixtures::NOTES_DAY.1 }),
        },
        Example {
            bytes: None, kinds: vec![],
            op: "receive",
            loads: vec![],
            request: json!({ "op": "receive", "write": fixtures::receive_write("notes", "dispatch-1", 1_792_752_000_000),
                             "ops": [{ "cap": "notes", "op": "write", "args": { "id": "n1", "target": trip(), "text": "No operator. Trip cancelled." } }] }),
        },
        Example { bytes: None, kinds: vec![], op: "log_records", loads: load(), request: json!({ "op": "log_records", "notes_hash": notes_hash }) },
        Example { bytes: None, kinds: vec![], op: "log_document", loads: vec![], request: json!({ "op": "log_document", "records": records }) },
    ]
}
