//! The calendar capability's worked examples (its `examples` feature):
//! the documents to load and each request. The generator runs them
//! through this crate's `invoke` and records the answers.

use cap_abi::manifest::Example;
use serde_json::json;

pub fn examples() -> Vec<Example> {
    let net = fixtures::network();
    let loads = || vec![fixtures::load_network(&net.derived)];
    vec![
        Example { bytes: None, kinds: vec![], op: "active_services", loads: loads(), request: json!({ "op": "active_services", "network_hash": net.network_hash, "date": "2026-10-23" }) },
        // The reverse read: the window crosses the fixture's calendar
        // exception (WK removed on 2026-10-29).
        Example {
            bytes: None, kinds: vec![],
            op: "service_dates",
            loads: loads(),
            request: json!({ "op": "service_dates", "network_hash": net.network_hash, "service_type": "WK", "from": "2026-10-23", "to": "2026-11-03" }),
        },
    ]
}
