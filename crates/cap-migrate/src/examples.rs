//! The worked examples of the migration capability (its `examples` feature),
//! one for each request. The generator runs them through this crate's
//! `invoke` and records the answers.

use cap_abi::manifest::Example;
use serde_json::json;

pub fn examples() -> Vec<Example> {
    let v1 = json!([{ "id": "e1", "status": "accepted", "rev": 1, "updated_at": 1000,
        "cancel": { "id": "c1", "trip_id": "t1", "start_secs": 28800, "date": "2026-10-23" } }]);
    vec![
        Example { bytes: None, kinds: vec![], op: "migrate", loads: vec![], request: json!({ "op": "migrate", "kind": "edit_records", "document": v1 }) },
        Example { bytes: None, kinds: vec![], op: "version", loads: vec![], request: json!({ "op": "version", "kind": "watchlist", "document": { "version": "v1", "stops": [] } }) },
    ]
}
