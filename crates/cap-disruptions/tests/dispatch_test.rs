//! Native dispatch tests for the disruptions capability: the seam
//! without wasm. The laws are tested at the library seam
//! (transit-disruptions). Here the module's contract is pinned: both ops
//! read registered documents by hash, the gap list and the day agree
//! with the documents given, a filter answers one stop, and garbage is
//! an honest error, never a panic.

use serde_json::{json, Value};

fn run(request: Value) -> Value {
    serde_json::from_str(&cap_disruptions::invoke(&request.to_string())).unwrap()
}

fn ok(request: Value) -> Value {
    let answer = run(request.clone());
    answer.get("ok").cloned().unwrap_or_else(|| panic!("{request} answered {answer}"))
}

fn err(request: Value) -> String {
    let answer = run(request.clone());
    answer.get("error").and_then(Value::as_str).unwrap_or_else(|| panic!("{request} answered {answer}")).to_string()
}

fn network_hash() -> Value {
    let text = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../web/public/caps/sample-network-load.json")).unwrap();
    ok(serde_json::from_str(&text).unwrap())["network_hash"].clone()
}

/// The documents of one day: two cancels in the edit document, the feed
/// cancelling t10 too, stop B suspended, and an alert explaining t30's
/// cancel.
fn documents() -> Value {
    let edits = json!({ "kind": "edit_records", "schema_version": 2, "records": [
        { "id": "r1", "status": "accepted", "rev": 1, "updated_at": 0,
          "cancel": { "id": "c1", "trip_id": "t10", "start_secs": 28800, "date": "2026-11-19" } },
        { "id": "r2", "status": "accepted", "rev": 1, "updated_at": 0,
          "cancel": { "id": "c2", "trip_id": "t30", "start_secs": 31200, "date": "2026-11-19" } }
    ]});
    let stops = json!({ "kind": "stops", "schema_version": 1, "stops": {
        "B": [{ "event": "in_service", "lat": 45.51, "lon": -73.51 }, { "on": "2026-11-01", "event": "suspended" }]
    }});
    let live = json!({ "kind": "live", "schema_version": 2, "window": { "late_secs": 60, "very_late_secs": 300 },
        "trips": [{ "trip": "t10", "date": "2026-11-19", "start_secs": 28800, "route": "10", "status": "no_prediction", "canceled": true, "stops": [] }],
        "vehicles": [], "unmatched_trips": 0 });
    let alerts = json!({ "kind": "service_alerts", "schema_version": 2, "alerts": {
        "a1": { "status": "published", "header": [{ "text": "The 8:40 trip is cancelled" }], "informed": [{ "route": "10" }],
                "explains": [{ "kind": "edit", "id": "r2" }] }
    }});
    json!({
        "network_hash": network_hash(),
        "edits_hash": ok(json!({ "op": "load_edits", "edits": edits }))["edits_hash"],
        "stops_hash": ok(json!({ "op": "load_stops", "stops": stops }))["stops_hash"],
        "live_hash": ok(json!({ "op": "load_live", "live": live }))["live_hash"],
        "alerts_hash": ok(json!({ "op": "load_service_alerts", "alerts": alerts }))["alerts_hash"],
        "date": "2026-11-19", "now": 2000
    })
}

fn with(op: &str, extra: Value) -> Value {
    let mut request = documents();
    request["op"] = json!(op);
    for (k, v) in extra.as_object().unwrap() {
        request[k] = v.clone();
    }
    request
}

#[test]
fn the_gap_list_reads_every_source() {
    let gaps = ok(with("gaps", json!({})));
    let sources: Vec<Value> = gaps["gaps"].as_array().unwrap().iter().map(|g| g["source"].clone()).collect();
    assert_eq!(
        sources,
        vec![
            json!({ "kind": "edit", "id": "r1" }),
            json!({ "kind": "trip", "trip": "t10", "date": "2026-11-19", "start_secs": 28800 }),
            json!({ "kind": "stop", "stop": "B" }),
        ],
        "r2 is explained; the feed's cancel of t10 is listed beside the edit's"
    );
}

#[test]
fn the_day_lists_routes_with_their_disruptions() {
    let today = ok(with("today", json!({})));
    let routes: Vec<&str> = today["routes"].as_array().unwrap().iter().map(|r| r["route"].as_str().unwrap()).collect();
    assert_eq!(routes, ["10", "20"], "route 20 serves the suspended stop B");
    let r10 = &today["routes"][0]["disruptions"];
    assert_eq!(r10.as_array().unwrap().len(), 4);
    assert_eq!(r10[0]["first_stop"], json!({ "id": "A", "name": "Main & 1st", "lat": 45.5, "lon": -73.5 }));
    assert_eq!(r10[1]["explained_by"], json!(["a1"]));
    assert_eq!(r10[2]["from"], "feed");
    assert!(today["unaffected_routes"].as_u64().is_some());
    let at_c = ok(with("today", json!({ "stop": "C" })));
    let only: Vec<&str> = at_c["routes"][0]["disruptions"].as_array().unwrap().iter().map(|d| d["trip"].as_str().unwrap()).collect();
    assert_eq!(only, ["t30"], "only t30 serves stop C");
}

#[test]
fn garbage_is_an_error_not_a_panic() {
    assert!(err(json!({ "op": "gaps", "network_hash": "sha256:nope", "date": "2026-11-19", "now": 0 })).contains("sha256:nope"));
    assert!(err(with("today", json!({ "date": "11/19" }))).contains("11/19"));
}
