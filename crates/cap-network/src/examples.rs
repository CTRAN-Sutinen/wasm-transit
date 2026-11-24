//! The worked examples of the network capability (its `examples` feature):
//! the documents to load and each request, over the fixture's real derived
//! document. The generator runs them through this crate's `invoke` and
//! records the answers.

use cap_abi::manifest::Example;
use serde_json::json;

pub fn examples() -> Vec<Example> {
    let net = fixtures::network();
    let network = || vec![fixtures::load_network(&net.derived)];
    let by_hash = |op: &'static str| Example { bytes: None, kinds: vec![], op, loads: network(), request: json!({ "op": op, "network_hash": net.network_hash }) };
    vec![
        Example { bytes: None, kinds: vec![], op: "import", loads: vec![fixtures::load_feed()], request: json!({ "op": "import", "feed_hash": fixtures::feed_hash() }) },
        by_hash("validate"),
        by_hash("document"),
        by_hash("publishable"),
        // A stored plan as it was written: its own version is read when
        // the page uses it.
        Example {
            bytes: None, kinds: vec![],
            op: "stored_plans",
            loads: vec![],
            request: json!({ "op": "stored_plans", "document": { "kind": "plan_set", "schema_version": 1, "plans": { "p1": { "plan": { "id": "", "name": "Spring draft" } } } } }),
        },
        Example {
            bytes: None, kinds: vec![],
            op: "export",
            loads: vec![fixtures::load_network(&net.derived), fixtures::load("load_boundary", "boundary", &net.boundary)],
            request: json!({ "op": "export", "network_hash": net.network_hash, "boundary_hash": net.boundary_hash }),
        },
    ]
}
