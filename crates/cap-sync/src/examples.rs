//! The sync capability's worked examples, under its `examples` feature. Each
//! example gives a request, over the fixture edit document. The generator runs
//! them through this crate's `invoke` and records the answers.

use cap_abi::manifest::Example;
use serde_json::json;
use transit_sync::{EditDocument, EditRecord, EditStatus};

pub fn examples() -> Vec<Example> {
    let fixture = fixtures::dispatch_fixture_edits();
    let doc = EditDocument::new(fixture.clone());
    let now_ms: u64 = 1_770_000_000_000;
    // A draft client against an empty server: merged and submitted.
    let mut draft = fixture.clone();
    draft[0].status = EditStatus::Draft;
    // A tombstone older than the 90-day horizon.
    let mut old = fixture.clone();
    old[0].status = EditStatus::Retired;
    old[0].updated_at = 1_000;
    let held = |r: &EditRecord| json!({ "version": transit_sync::SCHEMA_VERSION, "value": r });
    let records: std::collections::BTreeMap<String, serde_json::Value> = doc.records.iter().map(|r| (r.id.clone(), held(r))).collect();
    // A client's document in an empty disruptions log: merged, its drafts
    // submitted at the entry's clock, and recorded.
    let mut write = fixtures::receive_write("disruptions", "dispatch-1", 1_792_752_000_000);
    write.header.intent.op = "POST /edits".into();
    write.header.module = "cap-sync@sha256:…".into();
    // The revert of the write that accepted the diversion: it goes back to
    // submitted, stamped to win the merge.
    let accepted = fixture[0].clone();
    let submitted = EditRecord { status: EditStatus::Submitted, rev: 1, ..accepted.clone() };
    let states = json!([{ "kind": "edit_record", "id": accepted.id, "now": held(&accepted), "prior": held(&submitted) }]);
    vec![
        Example { bytes: None, kinds: vec![], op: "sync_edits", loads: vec![], request: json!({ "op": "sync_edits", "client": draft, "server": [], "now_ms": now_ms }) },
        Example { bytes: None, kinds: vec![], op: "read_edits", loads: vec![], request: json!({ "op": "read_edits", "document": doc }) },
        Example {
            bytes: None, kinds: vec![],
            op: "compact_edits",
            loads: vec![],
            request: json!({ "op": "compact_edits", "records": old, "now_ms": now_ms, "horizon_ms": 90u64 * 24 * 3600 * 1000 }),
        },
        Example { bytes: None, kinds: vec![], op: "log_records", loads: vec![], request: json!({ "op": "log_records", "document": doc }) },
        Example { bytes: None, kinds: vec![], op: "log_document", loads: vec![], request: json!({ "op": "log_document", "records": records }) },
        Example {
            bytes: None, kinds: vec![],
            op: "receive",
            loads: vec![],
            request: json!({ "op": "receive", "write": write, "ops": [{ "cap": "sync", "op": "sync", "args": { "client": doc } }] }),
        },
        Example { bytes: None, kinds: vec![], op: "log_revert", loads: vec![], request: json!({ "op": "log_revert", "states": states, "now_ms": 1_770_000_120_000u64 }) },
    ]
}
