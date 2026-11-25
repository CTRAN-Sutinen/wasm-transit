use cap_abi::manifest::{Example, KindExample};
use serde_json::json;
use transit_network::NetworkEdit;

pub fn examples() -> Vec<Example> {
    let net = fixtures::network();
    let loads = || vec![fixtures::load_network(&net.derived)];
    // Read through the type, so the request is serde's own bytes.
    let edits: Vec<NetworkEdit> = serde_json::from_value(json!([
        { "edit": "set_plan", "plan": { "id": "fall-2026", "name": "Fall 2026", "effective_from": "2026-10-19" }, "through": "2027-02-26" },
        { "edit": "add_stop", "lat": 45.5, "lon": -73.5 }
    ]))
    .expect("the example edits read");
    vec![
        Example {
            bytes: None, kinds: vec![],
            op: "stamp_pick",
            loads: loads(),
            request: json!({ "op": "stamp_pick", "network_hash": net.network_hash, "name": "Fall 2026 feed", "id": "fall-2026" }),
        },
        Example { bytes: None, kinds: kinds(&net.network), op: "apply", loads: loads(), request: json!({ "op": "apply", "network_hash": net.network_hash, "edits": edits }) },
    ]
}

/// An example that stops applying fails the build. So does an edit kind
/// with no example.
fn kinds(network: &transit_network::Network) -> Vec<KindExample> {
    let mut doc = network.clone();
    let mut kinds = Vec::new();
    for (i, example) in edit_kinds().into_iter().enumerate() {
        let edit: NetworkEdit = serde_json::from_value(example.clone()).unwrap_or_else(|e| panic!("edit example {i} ({example}) is not an edit: {e}"));
        doc = transit_network::apply_edits(&doc, std::slice::from_ref(&edit)).unwrap_or_else(|e| panic!("edit example {i} ({example}) does not apply: {e}"));
        let kind = example["edit"].as_str().expect("a tagged edit").to_string();
        kinds.push(KindExample { kind, example });
    }
    let declared = <NetworkEdit as ts_rs::TS>::decl(&ts_rs::Config::default());
    let tags: std::collections::BTreeSet<&str> = declared.split("\"edit\": \"").skip(1).filter_map(|rest| rest.split('"').next()).collect();
    let covered: std::collections::BTreeSet<&str> = kinds.iter().map(|k| k.kind.as_str()).collect();
    assert!(!tags.is_empty(), "the edit kinds are read off NetworkEdit's declaration");
    assert_eq!(covered, tags, "every edit kind has exactly one worked example (edit_kinds)");
    assert_eq!(covered.len(), kinds.len(), "one example per edit kind");
    kinds
}

/// The network has routes 10/20/40, stops A B C and the station STN with bays
/// SA SB, service types WK SUN HOL, trips t10 t11 t20 t30 t40 t41, and block
/// B1 = t10 then t20. Each edit may name what an earlier one created.
fn edit_kinds() -> Vec<serde_json::Value> {
    vec![
        json!({ "edit": "set_plan", "plan": { "id": "fall-2026", "name": "Fall 2026", "effective_from": "2026-10-19" } }),
        json!({ "edit": "update_plan", "name": "Fall 2026 service" }),
        json!({ "edit": "set_name", "name": "Sample transit" }),
        json!({ "edit": "add_stop", "id": "D", "lat": 45.53, "lon": -73.53 }),
        json!({ "edit": "add_route", "id": "30" }),
        json!({ "edit": "add_pattern", "id": "pat-30", "route": "30" }),
        json!({ "edit": "add_pattern_through", "id": "pat-10-0-3", "route": "10", "direction": "0", "stops": ["A", "B", "C"], "run_secs": 900 }),
        json!({ "edit": "add_service_type", "id": "SAT", "from": "2026-10-19" }),
        json!({ "edit": "add_pattern_trips", "id": "SAT:pat-10-0-1", "pattern": "pat-10-0-1", "service_type": "SAT" }),
        json!({ "edit": "update_stop", "id": "D", "name": "Main & 4th", "transfer": true }),
        json!({ "edit": "update_route", "id": "30", "name": "30", "mode": "bus" }),
        json!({ "edit": "set_direction", "route": "30", "id": "0", "name": "Uptown" }),
        json!({ "edit": "update_pattern", "id": "pat-30", "name": "30 via Main", "direction": "0" }),
        json!({ "edit": "update_service_type", "id": "SAT", "name": "Saturday" }),
        json!({ "edit": "set_pattern_trips_refs", "id": "SAT:pat-10-0-1", "service_type": "SAT" }),
        json!({ "edit": "insert_pattern_stop", "pattern": "pat-10-0-2", "stop": "D", "index": 1 }),
        json!({ "edit": "set_pattern_stop", "pattern": "pat-10-0-2", "index": 1, "distance_m": 400.0 }),
        json!({ "edit": "toggle_timepoint", "pattern": "pat-10-0-2", "index": 1 }),
        json!({ "edit": "add_timing_profile", "pattern": "pat-10-0-2" }),
        json!({ "edit": "set_timing_profile", "pattern": "pat-10-0-2", "index": 1, "start_secs": 25200, "end_secs": 32400 }),
        json!({ "edit": "set_timepoint_time", "pattern": "pat-10-0-2", "profile": 1, "timepoint": 1, "secs": 300 }),
        json!({ "edit": "remove_timing_profile", "pattern": "pat-10-0-2", "index": 1 }),
        json!({ "edit": "remove_pattern_stop", "pattern": "pat-10-0-2", "index": 1 }),
        json!({ "edit": "draw_alignment", "pattern": "pat-10-0-2", "redraw": "one", "id": "line-10-0-2",
            "points": [ { "lat": 45.5, "lon": -73.5 }, { "lat": 45.505, "lon": -73.505 }, { "lat": 45.51, "lon": -73.51 } ] }),
        json!({ "edit": "toggle_service_day", "id": "SAT", "day": "sat" }),
        json!({ "edit": "add_service_range", "id": "SAT" }),
        json!({ "edit": "set_service_range", "id": "SAT", "index": 1, "start": "2027-01-01", "end": "2027-06-30" }),
        json!({ "edit": "remove_service_range", "id": "SAT", "index": 1 }),
        json!({ "edit": "set_service_exception", "id": "SAT", "date": "2026-12-26", "state": "removed" }),
        json!({ "edit": "remove_service_exception", "id": "SAT", "date": "2026-12-26" }),
        json!({ "edit": "retime_trip", "trip": "t30", "to_secs": 31500 }),
        json!({ "edit": "add_trip", "pattern_trips": "WK:pat-10-0-1", "id": "t12", "start_secs": 34200 }),
        json!({ "edit": "remove_trip", "trip": "t12" }),
        json!({ "edit": "upsert_pattern_trips", "id": "WK:pat-20-0-0", "pattern_trips": {
            "pattern": "pat-20-0-0", "service_type": "WK", "trips": [
                { "id": "t20", "departs": { "kind": "at", "start_secs": 30000 }, "times": [
                    { "arrival_secs": 0, "departure_secs": 0 }, { "arrival_secs": 600, "departure_secs": 600 } ] },
                { "id": "t21", "departs": { "kind": "every", "exact": false, "bands": [
                    { "start_secs": 36000, "end_secs": 43200, "headway_secs": 1200 } ] }, "times": [
                    { "arrival_secs": 0, "departure_secs": 0 }, { "arrival_secs": 600, "departure_secs": 600 } ] } ] } }),
        json!({ "edit": "add_band", "trip": "t21" }),
        json!({ "edit": "set_band", "trip": "t21", "index": 1, "start_secs": 43200, "end_secs": 50400, "headway_secs": 1800 }),
        json!({ "edit": "remove_band", "trip": "t21", "index": 1 }),
        json!({ "edit": "set_headway", "route": "10", "direction": "0", "service_type": "WK",
            "from_secs": 28800, "to_secs": 32400, "headway_secs": 1200 }),
        json!({ "edit": "set_span", "route": "10", "direction": "0", "service_type": "WK", "last_secs": 36000, "headway_secs": 1800 }),
        json!({ "edit": "adjust_running_time", "route": "10", "direction": "0", "service_type": "WK",
            "from_secs": 28800, "to_secs": 32400, "delta_secs": 120 }),
        json!({ "edit": "insert_direction_stop", "route": "10", "direction": "0", "stop": "D", "after": "A" }),
        json!({ "edit": "remove_direction_stop", "route": "10", "direction": "0", "stop": "D" }),
        json!({ "edit": "add_block", "id": "B2" }),
        json!({ "edit": "add_block_trip", "block": "B2", "trip": { "trip": "t40" } }),
        json!({ "edit": "set_layover", "block": "B1", "after_trip": 0, "secs": 600 }),
        json!({ "edit": "remove_block_trip", "block": "B2", "index": 0 }),
        json!({ "edit": "upsert_stop", "id": "E", "stop": { "name": "Main & 5th", "lat": 45.54, "lon": -73.54 } }),
        json!({ "edit": "upsert_route", "id": "50", "route": { "name": "50", "mode": "bus" } }),
        json!({ "edit": "upsert_pattern", "id": "pat-50", "pattern": { "route": "50", "name": "50 Main", "stops": [
            { "stop": "A", "distance_m": 0.0, "timepoint": true }, { "stop": "B", "distance_m": 800.0, "timepoint": true } ],
            "timing": [ { "start_secs": 0, "end_secs": 86400, "at_timepoints": [0, 240] } ] } }),
        json!({ "edit": "upsert_service_type", "id": "XMAS", "service_type": { "name": "Christmas", "rule": {
            "days": { "mon": false, "tue": false, "wed": false, "thu": false, "fri": false, "sat": false, "sun": false },
            "ranges": [], "exceptions": [ { "date": "2026-12-25", "state": "added" } ] } } }),
        json!({ "edit": "upsert_block", "id": "B3", "block": { "trips": [ { "trip": "t11" } ], "layovers": [] } }),
        // Removals refuse while referenced. These name what nothing references.
        json!({ "edit": "remove_block", "id": "B3" }),
        json!({ "edit": "remove_service_type", "id": "XMAS" }),
        json!({ "edit": "remove_stop", "id": "E" }),
        json!({ "edit": "remove_pattern_trips", "id": "SAT:pat-10-0-1" }),
        json!({ "edit": "remove_pattern", "id": "pat-30" }),
        json!({ "edit": "remove_direction", "route": "30", "id": "0" }),
        json!({ "edit": "remove_route", "id": "30" }),
    ]
}
