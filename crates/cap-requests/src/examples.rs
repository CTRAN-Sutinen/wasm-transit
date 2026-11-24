//! The worked examples of the stop requests capability (its `examples`
//! feature): the documents to load and each request. The generator runs
//! them through this crate's `invoke` and records the answers.

use cap_abi::manifest::Example;
use serde_json::json;
use transit_requests::{StopRequest, WorkOrder};

pub fn examples() -> Vec<Example> {
    let requests = fixtures::fixture_requests().checked().expect("the fixture requests pass");
    let hash = |r: &transit_requests::StopRequests| cap_abi::canonical_hash(r).expect("a hash");
    let load = |r: &transit_requests::StopRequests| vec![fixtures::load("load_stop_requests", "requests", r)];
    let read = |v: serde_json::Value| -> StopRequest { serde_json::from_value(v).expect("the example request reads") };
    // The repair at 6124, submitted.
    let submitted = read(json!({"stop": "6124", "job": "repair_improve", "status": "submitted", "requested_on": "2027-02-01", "note": "pad cracked at the rear door"}));
    // The repair at 6124, ordered: a refresh reads WO-90001.
    let ordered = requests
        .put("R-2027-020", read(json!({"stop": "6124", "job": "repair_improve", "status": "ordered", "requested_on": "2027-02-01",
            "work_order": {"system": "Maintenance", "id": "WO-90001", "status": "open", "read_on": "2027-02-02"}})))
        .checked()
        .expect("the ordered request passes");
    let opened = WorkOrder { system: Some("Maintenance".into()), id: "WO-90001".into(), status: Some("open".into()), read_on: Some("2027-02-02".into()), ..Default::default() };
    // The requests document as the stops log holds it: its records and its
    // own record, in the version this build writes.
    let (_, mut next) = fixtures::fixture_log_records();
    let own = next.pop().and_then(|mut k| k.records.remove(transit_log::DOCUMENT_ID)).expect("the document's own record");
    let held: std::collections::BTreeMap<String, serde_json::Value> = next
        .pop()
        .expect("the request records")
        .records
        .into_iter()
        .map(|(k, v)| (k, json!({ "version": transit_requests::SCHEMA_VERSION, "value": v })))
        .collect();
    vec![
        Example { bytes: None, kinds: vec![], op: "validate", loads: vec![], request: json!({ "op": "validate", "requests": requests }) },
        Example { bytes: None, kinds: vec![], op: "requests_for", loads: load(&requests), request: json!({ "op": "requests_for", "requests_hash": hash(&requests), "stop": "7001" }) },
        // The work in flight across every stop: drafted and submitted.
        Example {
            bytes: None, kinds: vec![],
            op: "list",
            loads: load(&requests),
            request: json!({ "op": "list", "requests_hash": hash(&requests), "statuses": ["drafted", "submitted"] }),
        },
        Example {
            bytes: None, kinds: vec![],
            op: "put",
            loads: load(&requests),
            request: json!({ "op": "put", "requests_hash": hash(&requests), "id": "R-2027-020", "request": submitted }),
        },
        Example {
            bytes: None, kinds: vec![],
            op: "work_order_call",
            loads: load(&ordered),
            request: json!({ "op": "work_order_call", "requests_hash": hash(&ordered), "id": "R-2027-020", "action": "refresh" }),
        },
        // The repair at 6124: the work order system opened WO-90001.
        Example {
            bytes: None, kinds: vec![],
            op: "record_work_order",
            loads: load(&requests),
            request: json!({ "op": "record_work_order", "requests_hash": hash(&requests), "id": "R-2027-020", "work_order": opened }),
        },
        Example { bytes: None, kinds: vec![], op: "vocabulary", loads: vec![], request: json!({ "op": "vocabulary" }) },
        Example { bytes: None, kinds: vec![], op: "log_records", loads: load(&requests), request: json!({ "op": "log_records", "requests_hash": hash(&requests) }) },
        Example {
            bytes: None, kinds: vec![],
            op: "log_document",
            loads: vec![],
            request: json!({ "op": "log_document", "records": held, "document": { "version": transit_requests::SCHEMA_VERSION, "value": own } }),
        },
    ]
}
