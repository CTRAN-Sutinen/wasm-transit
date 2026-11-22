//! The diversion capability's worked examples (its `examples` feature):
//! the documents to load and each request. The generator runs them
//! through this crate's `invoke` and records the answers. Each runs over
//! the fixture network: route 10 anchored end to end on its default
//! pattern.

use cap_abi::manifest::Example;
use serde_json::json;

pub fn examples() -> Vec<Example> {
    let net = fixtures::network();
    let loads = || vec![fixtures::load_network(&net.derived)];
    let draft = fixtures::divert_example_draft(&net.network).expect("the example draft");
    let edits = transit_sync::EditDocument::new(fixtures::dispatch_fixture_edits());
    let now_ms: u64 = 1_770_000_000_000;
    vec![
        Example { bytes: None, kinds: vec![], op: "draft", loads: loads(), request: json!({ "op": "draft", "network_hash": net.network_hash, "draft": draft }) },
        Example { bytes: None, kinds: vec![], op: "list", loads: loads(), request: json!({ "op": "list", "network_hash": net.network_hash, "edits": edits }) },
        Example { bytes: None, kinds: vec![], op: "reports", loads: loads(), request: json!({ "op": "reports", "network_hash": net.network_hash, "edits": edits }) },
        Example {
            bytes: None, kinds: vec![],
            op: "propose",
            loads: loads(),
            request: json!({ "op": "propose", "network_hash": net.network_hash, "draft": draft, "edits": transit_sync::EditDocument::default(),
                             "record_id": "e2", "now_ms": now_ms }),
        },
        // The 08:00 trip on the fixture's Friday, appended beside the
        // fixture's diversion.
        Example {
            bytes: None, kinds: vec![],
            op: "cancel",
            loads: loads(),
            request: json!({ "op": "cancel", "network_hash": net.network_hash, "edits": edits, "record_id": "e2",
                             "trip_id": "t10", "date": "2026-10-23", "now_ms": now_ms }),
        },
        // Retire the fixture's diversion, and restore it once retired.
        Example {
            bytes: None, kinds: vec![],
            op: "retire",
            loads: vec![],
            request: json!({ "op": "retire", "edits": edits, "record_id": "e1", "now_ms": 1_770_000_060_000u64 }),
        },
        Example {
            bytes: None, kinds: vec![],
            op: "restore",
            loads: vec![],
            request: json!({ "op": "restore", "edits": transit_sync::EditDocument::new(
                transit_sync::set_retired(&fixtures::dispatch_fixture_edits(), "e1", true, 1_770_000_000_000).expect("e1 retires")),
                "record_id": "e1", "now_ms": 1_770_000_060_000u64 }),
        },
    ]
}
