//! The directory's worked examples, under its `examples` feature. Each
//! request runs over the fixture network: the stop list with its walking
//! group, the route list, and route 10's patterns (the direct run and the
//! via-C run). The generator runs them through this crate's `invoke` and
//! records the answers.

use cap_abi::manifest::Example;
use serde_json::{json, Value};

pub fn examples() -> Vec<Example> {
    let net = fixtures::network();
    let ask = |op: &'static str, args: Value| {
        let mut request = json!({ "op": op, "network_hash": net.network_hash });
        for (k, v) in args.as_object().expect("an object") {
            request[k] = v.clone();
        }
        Example { bytes: None, kinds: vec![], op, loads: vec![fixtures::load_network(&net.derived)], request }
    };
    vec![
        ask("about", json!({})),
        ask("stops", json!({ "limit": 3 })),
        // Standing at the station's Bay A. The station answers once (0 m, at
        // its nearest member). Then the plain stops follow by distance.
        ask("stops_near", json!({ "lat": 45.515, "lon": -73.5155, "limit": 3 })),
        ask("stops_by_ids", json!({ "ids": ["A", "ghost", "STN"] })),
        ask("routes", json!({})),
        ask("focus", json!({ "kind": "route", "id": "10" })),
        ask("service_keys", json!({})),
        ask("route_lines", json!({})),
        ask("entities", json!({ "kind": "pattern_trips", "query": "wk" })),
        ask("route_detail", json!({ "route_id": "10" })),
        ask("timetable", json!({ "route_id": "10", "direction": "0", "members": ["WK"] })),
        ask("stop_detail", json!({ "stop_id": "B", "members": ["WK"] })),
        ask("route_stop_sequence", json!({ "route_id": "10" })),
    ]
}
