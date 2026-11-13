//! Native dispatch tests for the stops capability — the seam without
//! wasm. The document's own laws (the stop on a date, served, each
//! refusal, canonical bytes, what is not known kept) are tested at the
//! library seam (transit-stops); here the module's contract is pinned:
//! `validate` and the kernel's `load_stops` agree on one identity, a
//! refused document is refused at both with its reasons named, the
//! date questions answer over a registered document, and garbage is an
//! honest error, never a panic.

use cap_abi::kernel::OP_LOAD_STOPS;
use serde_json::{json, Value};

fn run(request: Value) -> Value {
    serde_json::from_str(&cap_stops::invoke(&request.to_string())).unwrap()
}

fn ok(request: Value) -> Value {
    let answer = run(request.clone());
    answer.get("ok").cloned().unwrap_or_else(|| panic!("{request} answered {answer}"))
}

fn err(request: Value) -> String {
    let answer = run(request.clone());
    answer["error"].as_str().unwrap_or_else(|| panic!("{request} answered {answer}")).to_string()
}

fn sample() -> Value {
    json!({
        "kind": "stops", "schema_version": 1,
        "stops": {
            "6124": [
                {"on": "2004-09-01", "event": "in_service", "lat": 45.6934, "lon": -122.6512, "name": "NE 99th St & Hwy 99", "code": "6124", "wheelchair": true},
                {"on": "2025-05-12", "event": "changed", "amenities": {"shelter": true, "bench": true, "bin": true}},
                {"on": "2027-03-03", "event": "suspended", "until": "2027-07-01", "note": "water main"},
                {"on": "2027-09-01", "event": "moved", "lat": 45.6936, "lon": -122.6509}
            ],
            "7001": [
                {"on": "2027-01-10", "event": "planned", "lat": 45.70, "lon": -122.66, "name": "NE 104th St"},
                {"on": "2027-05-03", "event": "permitted"}
            ]
        }
    })
}

#[test]
fn validate_and_the_load_agree_on_one_identity() {
    let checked = ok(json!({"op": "validate", "stops": sample()}));
    assert_eq!(checked["refusals"], json!([]));
    let document = checked["document"].as_str().unwrap();
    let hash = checked["document_hash"].as_str().unwrap();
    assert_eq!(cap_abi::content_hash(document.as_bytes()), hash, "the hash is the document's bytes'");

    let loaded = ok(json!({"op": "load_stops", "stops": sample()}));
    assert_eq!(loaded, json!({"op": "load_stops", "stops_hash": hash, "stops": 2}));
    // the bulk lane: the document's bytes, no envelope — the same answer
    let bulk: Value = serde_json::from_str(&cap_stops::invoke_bytes(OP_LOAD_STOPS, document.as_bytes())).unwrap();
    assert_eq!(bulk["ok"], loaded);
    // a writer's key order is not part of the identity
    let reordered: Value = serde_json::from_str(r#"{"stops":{"7001":[{"name":"NE 104th St","lon":-122.66,"lat":45.70,"event":"planned","on":"2027-01-10"},{"event":"permitted","on":"2027-05-03"}],"6124":[{"on":"2004-09-01","event":"in_service","lat":45.6934,"lon":-122.6512,"name":"NE 99th St & Hwy 99","code":"6124","wheelchair":true},{"on":"2025-05-12","event":"changed","amenities":{"bin":true,"bench":true,"shelter":true}},{"note":"water main","until":"2027-07-01","event":"suspended","on":"2027-03-03"},{"on":"2027-09-01","event":"moved","lat":45.6936,"lon":-122.6509}]},"schema_version":1,"kind":"stops"}"#).unwrap();
    assert_eq!(ok(json!({"op": "validate", "stops": reordered}))["document_hash"], hash);
}

#[test]
fn a_refused_document_names_each_reason_and_never_registers() {
    let mut bad = sample();
    bad["stops"]["6124"][2]["until"] = json!("2027-03-01");
    bad["stops"]["7001"][1]["event"] = json!("planned");
    let checked = ok(json!({"op": "validate", "stops": bad}));
    assert_eq!(
        checked["refusals"],
        json!([
            {"stop": "6124", "event": 2, "problem": {"kind": "until_not_after"}},
            {"stop": "7001", "event": 1, "problem": {"kind": "stage_back"}}
        ])
    );
    assert!(checked["document"].is_null() && checked["document_hash"].is_null());
    let e = err(json!({"op": "load_stops", "stops": bad}));
    assert!(e.contains("stops document") && e.contains(r#"stop "7001" event 1 is a stage the stop is already at or past"#), "{e}");
}

#[test]
fn the_date_questions_answer_over_a_registered_document() {
    let hash = ok(json!({"op": "load_stops", "stops": sample()}))["stops_hash"].clone();
    let on = |date: &str| ok(json!({"op": "stops_on", "stops_hash": hash, "date": date}))["stops"].clone();
    assert_eq!(on("2026-01-01").as_array().unwrap().len(), 1, "the planned stop is not there yet");
    let spring = on("2027-04-01");
    assert_eq!(spring[0]["status"], "suspended");
    assert_eq!(spring[0]["amenities"], json!({"bench": true, "bin": true, "shelter": true}));
    assert_eq!(spring[1], json!({"stop_id": "7001", "status": "planned", "since": "2027-01-10", "lat": 45.70, "lon": -122.66, "name": "NE 104th St"}));
    let only = ok(json!({"op": "stops_on", "stops_hash": hash, "date": "2027-10-01", "stop_ids": ["6124", "nope"]}))["stops"].clone();
    assert_eq!((only.as_array().unwrap().len(), only[0]["lat"].clone(), only[0]["status"].clone()), (1, json!(45.6936), json!("in_service")));

    let card = ok(json!({"op": "stop", "stops_hash": hash, "stop_id": "6124", "date": "2027-07-01"}));
    assert_eq!(card["stop"]["status"], "in_service", "until is the first day back");
    assert_eq!(card["events"].as_array().unwrap().len(), 4);
    assert_eq!(card["events"][2]["note"], "water main");
    assert_eq!(card["next"], json!(["suspended"]), "in service again, and a move recorded later: it may not be retired before it");
    let later = ok(json!({"op": "stop", "stops_hash": hash, "stop_id": "6124", "date": "2027-10-01"}));
    assert_eq!(later["next"], json!(["suspended", "retired"]), "after its last event: suspended or retired");
    let away = ok(json!({"op": "stop", "stops_hash": hash, "stop_id": "6124", "date": "2027-04-01"}));
    assert_eq!(away["next"], json!(["in_service"]), "suspended: back in service (retiring waits on the move recorded later)");
    let before = ok(json!({"op": "stop", "stops_hash": hash, "stop_id": "6124", "date": "2000-01-01"}));
    assert!(before["stop"].is_null() && before["events"].as_array().unwrap().len() == 4);
    assert!(err(json!({"op": "stop", "stops_hash": hash, "stop_id": "nope", "date": "2027-01-01"})).contains("no stop \"nope\""));
}

#[test]
fn garbage_is_an_error_not_a_panic() {
    assert!(err(json!({"op": "stops_on", "stops_hash": "sha256:none", "date": "2027-01-01"})).contains("load_stops"));
    let hash = ok(json!({"op": "load_stops", "stops": sample()}))["stops_hash"].clone();
    assert!(err(json!({"op": "stops_on", "stops_hash": hash, "date": "2027-13-01"})).contains("invalid date"));
    let mut newer = sample();
    newer["schema_version"] = json!(2);
    assert!(err(json!({"op": "validate", "stops": newer})).contains("stops v2"));
    assert!(err(json!({"op": "load_stops", "stops": {"kind": "streets", "stops": {}}})).contains("stops document"));
    assert!(err(json!({"op": "nope"})).contains("invalid request"));
    let bulk: Value = serde_json::from_str(&cap_stops::invoke_bytes(0, b"whatever")).unwrap();
    assert!(bulk["error"].as_str().unwrap().contains("unknown bulk op"), "{bulk}");
}

fn sample_network() -> Value {
    let text = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../web/public/caps/sample-network-load.json")).unwrap();
    ok(serde_json::from_str(&text).unwrap())
}

#[test]
fn seed_begins_a_document_from_a_network() {
    let network_hash = sample_network()["network_hash"].clone();
    let seeded = ok(json!({"op": "seed", "network_hash": network_hash}));
    let stops = &seeded["stops"]["stops"];
    assert_eq!(stops["A"].as_array().unwrap().len(), 1, "one in_service event per stop");
    assert!(stops["A"][0].get("on").is_none() && stops["A"][0]["event"] == "in_service");
    // the seed is registered under its hash: load it again, same identity
    assert_eq!(ok(json!({"op": "load_stops", "stops": seeded["stops"]}))["stops_hash"], seeded["stops_hash"]);
    assert!(err(json!({"op": "seed", "network_hash": "sha256:none"})).contains("load_network"));
}

#[test]
fn add_event_places_an_edit_and_checks_it() {
    let hash = ok(json!({"op": "load_stops", "stops": sample()}))["stops_hash"].clone();
    let closed = ok(json!({"op": "add_event", "stops_hash": hash, "stop_id": "6124",
        "event": {"on": "2026-01-05", "event": "suspended", "until": "2026-01-10", "note": "paving"}}));
    assert_eq!(closed["refusals"], json!([]));
    assert_eq!(closed["stops"]["stops"]["6124"][2]["note"], "paving", "in its place by date");
    let card = ok(json!({"op": "stop", "stops_hash": closed["stops_hash"], "stop_id": "6124", "date": "2026-01-06"}));
    assert_eq!(card["stop"]["status"], "suspended");
    let refused = ok(json!({"op": "add_event", "stops_hash": hash, "stop_id": "6124", "event": {"on": "2026-01-05", "event": "in_service"}}));
    assert_eq!(refused["refusals"][0]["problem"]["kind"], "already_in_service");
    assert!(refused["stops"].is_null() && refused["stops_hash"].is_null());
}

#[test]
fn stops_on_filters_by_status_and_counts_them() {
    let hash = ok(json!({"op": "load_stops", "stops": sample()}))["stops_hash"].clone();
    let spring = ok(json!({"op": "stops_on", "stops_hash": hash, "date": "2027-04-01", "statuses": ["suspended"]}));
    assert_eq!(spring["stops"].as_array().unwrap().len(), 1);
    assert_eq!(spring["stops"][0]["since"], "2027-03-03");
    assert_eq!(spring["stops"][0]["until"], "2027-07-01");
    assert_eq!(spring["stops"][0]["note"], "water main");
    assert_eq!(spring["counts"], json!([{"status": "planned", "stops": 1}, {"status": "suspended", "stops": 1}]), "every stop counted, whatever the filter");
}

#[test]
fn correct_event_edits_the_event_that_was_wrong() {
    let hash = ok(json!({"op": "load_stops", "stops": sample()}))["stops_hash"].clone();
    let fixed = ok(json!({"op": "correct_event", "stops_hash": hash, "stop_id": "6124", "index": 2,
        "event": {"on": "2027-03-10", "event": "suspended", "until": "2027-07-01", "note": "water main"}}));
    assert_eq!(fixed["refusals"], json!([]));
    assert_eq!(fixed["stops"]["stops"]["6124"][2]["on"], "2027-03-10");
    let removed = ok(json!({"op": "correct_event", "stops_hash": hash, "stop_id": "6124", "index": 3}));
    assert_eq!(removed["stops"]["stops"]["6124"].as_array().unwrap().len(), 3);
    let refused = ok(json!({"op": "correct_event", "stops_hash": hash, "stop_id": "6124", "index": 0}));
    assert_eq!(refused["refusals"][0]["problem"]["kind"], "first_event", "taking out the first event leaves no beginning");
    assert!(err(json!({"op": "correct_event", "stops_hash": hash, "stop_id": "6124", "index": 9})).contains("no event 9"));
}

#[test]
fn a_pick_reads_the_records_and_is_warned_of_stops_not_in_service() {
    let network_hash = sample_network()["network_hash"].clone();
    let seeded = ok(json!({"op": "seed", "network_hash": network_hash}));
    let same = ok(json!({"op": "read_records", "network_hash": network_hash, "stops_hash": seeded["stops_hash"], "on": "2026-10-01"}));
    assert_eq!(same["network_hash"], network_hash, "a pick read from its own seed keeps its hash");
    let renamed = ok(json!({"op": "add_event", "stops_hash": seeded["stops_hash"], "stop_id": "A",
        "event": {"on": "2026-10-01", "event": "changed", "name": "A Street Station"}}));
    let read = ok(json!({"op": "read_records", "network_hash": network_hash, "stops_hash": renamed["stops_hash"], "on": "2026-10-05"}));
    assert_eq!(read["network"]["stops"]["A"]["name"], "A Street Station");
    let retired = ok(json!({"op": "add_event", "stops_hash": seeded["stops_hash"], "stop_id": "B", "event": {"on": "2026-11-02", "event": "retired"}}));
    let warned = ok(json!({"op": "visit_warnings", "network_hash": network_hash, "stops_hash": retired["stops_hash"], "from": "2026-10-01", "until": "2026-12-01"}));
    assert_eq!(warned["warnings"], json!([{"stop_not_in_service": {"stop": "B", "status": "retired", "from": "2026-11-02", "until": "2026-12-01"}}]));
    assert!(err(json!({"op": "visit_warnings", "network_hash": network_hash, "stops_hash": retired["stops_hash"], "from": "2026-12-01", "until": "2026-12-01"})).contains("until"));
}
