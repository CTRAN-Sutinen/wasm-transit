//! The analysis capability's worked examples (its `examples` feature):
//! each request over the fixture network, and the point and area layers
//! the coverage ops read. The generator runs them through this crate's
//! `invoke` and records the answers.

use cap_abi::manifest::Example;
use serde_json::{json, Value};

/// Two people points: one by the fixture's stops, one far away.
const PEOPLE: &str = r#"{"type":"FeatureCollection","features":[
                {"type":"Feature","properties":{"pop":500},"geometry":{"type":"Point","coordinates":[-73.5,45.5]}},
                {"type":"Feature","properties":{"pop":500},"geometry":{"type":"Point","coordinates":[-74.0,46.0]}}
            ]}"#;

/// Jobs: an area around the fixture's stops, and a point far away.
const JOBS: &str = r#"{"type":"FeatureCollection","features":[
                {"type":"Feature","properties":{"jobs":300},"geometry":{"type":"Polygon","coordinates":[[[-73.501,45.4995],[-73.499,45.4995],[-73.499,45.5005],[-73.501,45.5005],[-73.501,45.4995]]]}},
                {"type":"Feature","properties":{"jobs":100},"geometry":{"type":"Point","coordinates":[-74.0,46.0]}}
            ]}"#;

pub fn examples() -> Vec<Example> {
    let net = fixtures::network();
    let hash = net.network_hash.clone();
    let layer_hash = |layer: &str| cap_abi::content_hash(layer.as_bytes());
    let ask = |op: &'static str, layer: Option<&str>, args: Value| {
        let mut request = json!({ "op": op, "network_hash": hash });
        for (k, v) in args.as_object().expect("an object") {
            request[k] = v.clone();
        }
        let mut loads = vec![fixtures::load_network(&net.derived)];
        if let Some(layer) = layer {
            loads.push(fixtures::load("load_layer", "bytes", layer.as_bytes()));
        }
        Example { bytes: None, kinds: vec![], op, loads, request }
    };
    let window = json!({ "min_secs": 60, "max_secs": 1500 });
    vec![
        ask("network_stats", None, json!({ "date": "2026-01-05", "include_routes": false, "include_stops": false, "include_service_types": false })),
        ask("service_types_overview", None, json!({})),
        ask("service_groups", None, json!({})),
        ask("service_type_detail", None, json!({ "members": ["WK"] })),
        ask("service_type_compare", None, json!({ "base_hash": hash, "members": ["WK"] })),
        ask("stop_gaps", None, json!({ "date": "2026-01-05", "stop_id": "A" })),
        ask("hourly_trips", None, json!({ "date": "2026-01-05" })),
        ask("schedule_texture", None, json!({ "members": ["WK"] })),
        ask("route_overlap", None, json!({ "date": "2026-01-05" })),
        ask("timed_transfers", None, json!({ "date": "2026-01-05", "stop_id": "A", "window": window })),
        ask("hub_connections", None, json!({ "date": "2026-01-05", "stop_id": "A", "window": window })),
        ask("missed_connections", None, json!({ "date": "2026-01-05", "stop_id": "A", "miss_secs": 1500 })),
        ask("longest_waits", None, json!({ "date": "2026-01-05", "window": { "start_secs": 25200, "end_secs": 36000 }, "limit": 5 })),
        ask("day_types", None, json!({ "from": "2026-01-01" })),
        ask("plan_compare", None, json!({ "base_hash": hash, "from": "2026-01-01" })),
        ask("network_findings", None, json!({ "base_hash": hash, "members": ["WK"], "params": { "wait_secs": 900, "limit": 5 } })),
        ask("route_frequency", None, json!({ "members": ["WK"] })),
        ask("type_coverage", Some(PEOPLE), json!({ "layer_hash": layer_hash(PEOPLE), "members": ["WK"], "field": "pop", "label": "people",
            "radius_m": 500, "frequent_secs": 3600, "min_departures": 2 })),
        ask("unserved_points", Some(PEOPLE), json!({ "layer_hash": layer_hash(PEOPLE), "radius_m": 500, "members": ["WK"] })),
        ask("coverage_compare", Some(JOBS), json!({ "base_hash": hash, "layer_hash": layer_hash(JOBS), "field": "jobs", "label": "jobs",
            "base_members": ["WK"], "members": ["WK"], "radius_m": 500, "frequent_secs": 3600, "min_departures": 2 })),
    ]
}
