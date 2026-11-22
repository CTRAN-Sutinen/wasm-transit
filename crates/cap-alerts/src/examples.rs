//! The alerts capability's worked examples (its `examples` feature): the
//! documents to load and each request. The generator runs them through
//! this crate's `invoke` and records the answers.

use cap_abi::manifest::Example;
use serde_json::json;
use transit_alerts::{AlertTemplate, FillInput, ServiceAlert};

pub fn examples() -> Vec<Example> {
    let net = fixtures::network();
    let alerts = fixtures::fixture_alerts().checked().expect("the fixture alerts pass");
    let templates = fixtures::fixture_templates().checked().expect("the fixture templates pass");
    let alerts_hash = cap_abi::canonical_hash(&alerts).expect("a hash");
    let templates_hash = cap_abi::canonical_hash(&templates).expect("a hash");
    let load_alerts = || fixtures::load("load_service_alerts", "alerts", &alerts);
    let load_templates = || fixtures::load("load_alert_templates", "templates", &templates);
    let network = || fixtures::load_network(&net.derived);
    // Read through the types, so each request is serde's own bytes.
    // Snow: an agency-wide alert, published now.
    let snow: ServiceAlert = serde_json::from_value(json!({
        "status": "published", "cause": "weather", "effect": "significant_delays",
        "header": [{"text": "Snow: expect delays on every route"}], "informed": [{}]
    }))
    .expect("the snow alert reads");
    // A detour template beside the closure.
    let detour: AlertTemplate = serde_json::from_value(json!({
        "name": "Detour", "cause": "construction", "effect": "detour",
        "header": [{"text": "Route {route} is on detour from {start date}"}]
    }))
    .expect("the detour template reads");
    // The closure at stop A, served by routes 10 and 20. The Spanish
    // header's {stop} fills; its dates are not written.
    let input = FillInput {
        stops: vec!["A".into()],
        routes: vec!["10".into(), "20".into()],
        informed: vec![],
        start_date: Some("2026-10-26".into()),
        end_date: Some("2026-11-20".into()),
    };
    // The first alert, put in an empty alerts log.
    let first = alerts.alerts.values().next().expect("a fixture alert").clone();
    let records: std::collections::BTreeMap<String, serde_json::Value> =
        alerts.alerts.iter().map(|(id, a)| (id.clone(), json!({ "version": transit_alerts::ALERTS_VERSION, "value": a }))).collect();
    vec![
        Example { bytes: None, kinds: vec![], op: "validate", loads: vec![], request: json!({ "op": "validate", "alerts": alerts }) },
        Example { bytes: None, kinds: vec![], op: "put", loads: vec![load_alerts()], request: json!({ "op": "put", "alerts_hash": alerts_hash, "id": "snow", "alert": snow }) },
        // The detour lifts now.
        Example {
            bytes: None, kinds: vec![],
            op: "end",
            loads: vec![load_alerts()],
            request: json!({ "op": "end", "alerts_hash": alerts_hash, "id": "detour-10", "at": fixtures::ALERTS_NOW }),
        },
        Example { bytes: None, kinds: vec![], op: "list", loads: vec![load_alerts()], request: json!({ "op": "list", "alerts_hash": alerts_hash, "now": fixtures::ALERTS_NOW }) },
        // The deployment speaks English and Spanish.
        Example {
            bytes: None, kinds: vec![],
            op: "flags",
            loads: vec![load_alerts(), network()],
            request: json!({ "op": "flags", "alerts_hash": alerts_hash, "network_hash": net.network_hash, "languages": ["en", "es"] }),
        },
        Example { bytes: None, kinds: vec![], op: "validate_templates", loads: vec![], request: json!({ "op": "validate_templates", "templates": templates }) },
        Example {
            bytes: None, kinds: vec![],
            op: "put_template",
            loads: vec![load_templates()],
            request: json!({ "op": "put_template", "templates_hash": templates_hash, "id": "detour", "template": detour }),
        },
        Example { bytes: None, kinds: vec![], op: "templates", loads: vec![load_templates()], request: json!({ "op": "templates", "templates_hash": templates_hash }) },
        Example {
            bytes: None, kinds: vec![],
            op: "fill",
            loads: vec![load_templates(), network()],
            request: json!({ "op": "fill", "templates_hash": templates_hash, "template": "stop-closure", "network_hash": net.network_hash, "input": input }),
        },
        Example {
            bytes: None, kinds: vec![],
            op: "receive",
            loads: vec![],
            request: json!({ "op": "receive", "write": fixtures::receive_write("alerts", "dispatch-1", fixtures::ALERTS_NOW * 1000),
                             "ops": [{ "cap": "alerts", "op": "put", "args": { "id": "a1", "alert": first } }] }),
        },
        Example {
            bytes: None, kinds: vec![],
            op: "log_records",
            loads: vec![load_alerts(), load_templates()],
            request: json!({ "op": "log_records", "alerts_hash": alerts_hash, "templates_hash": templates_hash }),
        },
        // The alerts document assembled from the log's records.
        Example { bytes: None, kinds: vec![], op: "log_document", loads: vec![], request: json!({ "op": "log_document", "kind": "service_alerts", "records": records }) },
    ]
}
