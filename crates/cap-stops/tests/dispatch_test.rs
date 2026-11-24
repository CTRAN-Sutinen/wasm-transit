//! Native dispatch tests for the stops capability. They test the seam
//! without wasm. The document's own laws (the stop on a date, served, each
//! refusal, canonical bytes, what is not known kept) are tested at the
//! library seam (transit-stops). Here the module's contract is pinned:
//! `validate` and the kernel's `load_stops` agree on one identity. A
//! refused document is refused at both, with its reasons named. The date
//! questions answer over a registered document. Garbage is an honest
//! error, never a panic.

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

    // A write stores the document that validate answers, with ids given
    // (D-166). The load registers it under the same identity.
    let answered: Value = serde_json::from_str(document).unwrap();
    let loaded = ok(json!({"op": "load_stops", "stops": answered}));
    assert_eq!(loaded, json!({"op": "load_stops", "stops_hash": hash, "stops": 2}));
    // The bulk lane takes the document's bytes, with no envelope. It gives
    // the same answer.
    let bulk: Value = serde_json::from_str(&cap_stops::invoke_bytes(<transit_stops::Stops as cap_abi::kernel::Load>::CODE, document.as_bytes())).unwrap();
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

fn sample_network_value() -> Value {
    let text = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../web/public/caps/sample-network-load.json")).unwrap();
    serde_json::from_str::<Value>(&text).unwrap()["network"].clone()
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
    // The seed is registered under its hash. Loading it again gives the
    // same identity.
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

/// The field's "which stop am I at?": nearest first with its distance,
/// a planned stop included, filtered by status and radius; a point off
/// the earth is an error.
#[test]
fn stops_near_answers_the_nearest_stops_on_the_date() {
    let hash = ok(json!({"op": "load_stops", "stops": sample()}))["stops_hash"].clone();
    let near = ok(json!({"op": "stops_near", "stops_hash": hash, "date": "2027-04-01", "lat": 45.6935, "lon": -122.6512}));
    let ids: Vec<&str> = near["stops"].as_array().unwrap().iter().map(|s| s["stop_id"].as_str().unwrap()).collect();
    assert_eq!(ids, ["6124", "7001"], "the planned stop is a stop too");
    assert!(near["stops"][0]["distance_m"].as_f64().unwrap() < 20.0);
    assert_eq!(near["stops"][0]["status"], "suspended");
    let one = ok(json!({"op": "stops_near", "stops_hash": hash, "date": "2027-04-01", "lat": 45.6935, "lon": -122.6512, "limit": 1}));
    assert_eq!(one["stops"].as_array().unwrap().len(), 1);
    let planned = ok(json!({"op": "stops_near", "stops_hash": hash, "date": "2027-04-01", "lat": 45.6935, "lon": -122.6512, "statuses": ["planned"]}));
    assert_eq!(planned["stops"][0]["stop_id"], "7001");
    let close = ok(json!({"op": "stops_near", "stops_hash": hash, "date": "2027-04-01", "lat": 45.6935, "lon": -122.6512, "radius_m": 100}));
    assert_eq!(close["stops"].as_array().unwrap().len(), 1);
    assert!(err(json!({"op": "stops_near", "stops_hash": hash, "date": "2027-04-01", "lat": 95.0, "lon": 0.0})).contains("not on the earth"));
    assert!(err(json!({"op": "stops_near", "stops_hash": hash, "date": "2027-04-01", "lat": 45.0, "lon": 0.0, "radius_m": -1})).contains("negative"));
}

#[test]
fn correct_event_edits_the_event_that_was_wrong() {
    // a write gives every event an id (D-166); a correction names it
    let valid = ok(json!({"op": "validate", "stops": sample()}));
    let stops: serde_json::Value = serde_json::from_str(valid["document"].as_str().unwrap()).unwrap();
    let ids: Vec<String> = stops["stops"]["6124"].as_array().unwrap().iter().map(|e| e["id"].as_str().expect("every event has an id").to_string()).collect();
    let hash = ok(json!({"op": "load_stops", "stops": stops}))["stops_hash"].clone();
    let fixed = ok(json!({"op": "correct_event", "stops_hash": hash, "stop_id": "6124", "event_id": ids[2],
        "event": {"on": "2027-03-10", "event": "suspended", "until": "2027-07-01", "note": "water main"}}));
    assert_eq!(fixed["refusals"], json!([]));
    assert_eq!(fixed["stops"]["stops"]["6124"][2]["on"], "2027-03-10");
    assert_eq!(fixed["stops"]["stops"]["6124"][2]["id"], ids[2].as_str(), "the corrected event keeps its id");
    let removed = ok(json!({"op": "correct_event", "stops_hash": hash, "stop_id": "6124", "event_id": ids[3]}));
    assert_eq!(removed["stops"]["stops"]["6124"].as_array().unwrap().len(), 3);
    let refused = ok(json!({"op": "correct_event", "stops_hash": hash, "stop_id": "6124", "event_id": ids[0]}));
    assert_eq!(refused["refusals"][0]["problem"]["kind"], "first_event", "taking out the first event leaves no beginning");
    assert!(err(json!({"op": "correct_event", "stops_hash": hash, "stop_id": "6124", "event_id": "e-none"})).contains("no event \"e-none\""));
}

#[test]
fn a_write_gives_each_event_an_id_and_refuses_one_used_twice() {
    let added = ok(json!({"op": "add_event", "stops_hash": ok(json!({"op": "load_stops", "stops": sample()}))["stops_hash"], "stop_id": "7001",
        "event": {"on": "2027-08-01", "event": "changed", "name": "Main & 2nd"}}));
    for e in added["stops"]["stops"]["7001"].as_array().unwrap() {
        assert!(e["id"].as_str().is_some_and(|id| !id.is_empty()), "{e}");
    }
    let mut twice = sample();
    twice["stops"]["6124"][1]["id"] = json!("same");
    twice["stops"]["6124"][2]["id"] = json!("same");
    let valid = ok(json!({"op": "validate", "stops": twice}));
    assert_eq!(valid["refusals"][0]["problem"], json!({"kind": "duplicate_event_id", "id": "same"}));
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
    // A plan takes a stop that the document holds (D-173). The record is in
    // the network's terms, as read_records gives it. A stop not there yet,
    // or not held, is not answered.
    let planned = ok(json!({"op": "add_event", "stops_hash": seeded["stops_hash"], "stop_id": "N1",
        "event": {"on": "2026-10-20", "event": "planned", "lat": 45.58, "lon": -122.348, "name": "Washougal TC", "code": "9906"}}));
    let taken = ok(json!({"op": "records", "stops_hash": planned["stops_hash"], "stop_ids": ["N1", "A", "Z"], "on": "2026-10-20"}));
    assert_eq!(taken["records"]["N1"]["name"], "Washougal TC");
    assert_eq!(taken["records"]["N1"]["info"]["code"], "9906");
    let pick = ok(json!({"op": "read_records", "network_hash": network_hash, "stops_hash": planned["stops_hash"], "on": "2026-10-20"}));
    assert_eq!(taken["records"]["A"], pick["network"]["stops"]["A"], "the record a pick reads");
    assert!(taken["records"].get("Z").is_none());
    let early = ok(json!({"op": "records", "stops_hash": planned["stops_hash"], "stop_ids": ["N1"], "on": "2026-10-19"}));
    assert_eq!(early["records"], json!({}));
    // A new route takes its stops. The plan's own stops are left. The rest
    // become upsert_stop edits with the record, once each. A stop not
    // answered refuses the take, and the error names it.
    let take = ok(json!({"op": "take_stops", "network_hash": network_hash, "stops_hash": planned["stops_hash"], "stop_ids": ["N1", "A", "N1"], "on": "2026-10-20"}));
    assert_eq!(take["edits"].as_array().unwrap().len(), 1);
    assert_eq!(take["edits"][0]["edit"], "upsert_stop");
    assert_eq!(take["edits"][0]["id"], "N1");
    assert_eq!(take["edits"][0]["stop"], taken["records"]["N1"]);
    let refused = err(json!({"op": "take_stops", "network_hash": network_hash, "stops_hash": planned["stops_hash"], "stop_ids": ["N1", "Z"], "on": "2026-10-20"}));
    assert!(refused.contains("Z") && !refused.contains("N1"), "{refused}");
    let retired = ok(json!({"op": "add_event", "stops_hash": seeded["stops_hash"], "stop_id": "B", "event": {"on": "2026-11-02", "event": "retired"}}));
    let warned = ok(json!({"op": "visit_warnings", "network_hash": network_hash, "stops_hash": retired["stops_hash"], "from": "2026-10-01", "until": "2026-12-01"}));
    assert_eq!(warned["warnings"], json!([{"stop_not_in_service": {"stop": "B", "status": "retired", "from": "2026-11-02", "until": "2026-12-01"}}]));
    assert!(err(json!({"op": "visit_warnings", "network_hash": network_hash, "stops_hash": retired["stops_hash"], "from": "2026-12-01", "until": "2026-12-01"})).contains("until"));
}

/// GTFS out and in with the stops' history. A stop moved inside the
/// export's dates is two stops sharing its code. The export read back
/// adds no event. (The round trip through a GTFS zip is transit-codec's
/// law: the codec stays in its homes.) A feed's rename is an event on its
/// first date.
#[test]
fn the_export_carries_the_history_and_the_import_records_a_feeds_differences() {
    let network_hash = sample_network()["network_hash"].clone();
    let seeded = ok(json!({"op": "seed", "network_hash": network_hash}));
    let b = seeded["stops"]["stops"]["B"][0].clone();
    let moved = ok(json!({"op": "add_event", "stops_hash": seeded["stops_hash"], "stop_id": "B",
        "event": {"on": "2026-10-15", "event": "moved", "lat": b["lat"].as_f64().unwrap() + 0.001, "lon": b["lon"]}}));
    let exported = ok(json!({"op": "export_versions", "network_hash": network_hash, "stops_hash": moved["stops_hash"], "from": "2026-10-01", "until": "2026-12-01"}));
    let stops = exported["network"]["stops"].as_object().unwrap();
    assert!(stops.contains_key("B@2026-10-01") && stops.contains_key("B"));
    assert_eq!(stops["B@2026-10-01"]["info"]["code"], stops["B"]["info"]["code"]);
    let again = ok(json!({"op": "import_events", "stops_hash": moved["stops_hash"], "network_hash": exported["network_hash"], "on": "2026-10-01"}));
    assert_eq!(again["added"], json!([]), "{again}");
    assert_eq!(again["stops_hash"], moved["stops_hash"]);

    // A feed with no history, whose stop A is renamed, records the change
    // on the feed's first date.
    let mut renamed: transit_network::Network = serde_json::from_value(sample_network_value()).unwrap();
    renamed.stops.get_mut("A").unwrap().name = "A Street Station".into();
    let renamed_hash = ok(json!({"op": "load_network", "network": renamed}))["network_hash"].clone();
    let changed = ok(json!({"op": "import_events", "stops_hash": moved["stops_hash"], "network_hash": renamed_hash, "on": "2026-10-01"}));
    assert_eq!(changed["added"], json!([{"stop": "A", "event": {"on": "2026-10-01", "event": "changed", "name": "A Street Station"}}]));
    assert_eq!(changed["on"], "2026-10-01");

    // With no date, the feed's own first service date is used. The feed
    // states the date it starts on (cap-network import answers it too).
    let first = transit_network::first_service_date(&renamed).unwrap().to_string();
    let dated = ok(json!({"op": "import_events", "stops_hash": moved["stops_hash"], "network_hash": renamed_hash}));
    assert_eq!(dated["on"], first);
    assert_eq!(dated["added"][0]["event"]["on"], first);

    // A feed named on the import is named on every event it records. The
    // events are found again by its id (D-150: the planner sees what the
    // feed did).
    let named = ok(json!({"op": "import_events", "stops_hash": moved["stops_hash"], "network_hash": renamed_hash, "feed": "sha256:feed"}));
    assert_eq!(named["added"][0]["event"]["from_feed"], "sha256:feed");
    let shown = ok(json!({"op": "feed_events", "stops_hash": named["stops_hash"], "feed": "sha256:feed"}));
    assert_eq!(shown["events"], named["added"]);
    assert_eq!(ok(json!({"op": "feed_events", "stops_hash": named["stops_hash"], "feed": "sha256:other"}))["events"], json!([]));
}

/// What a survey asks (D-158) is the library's. It lists every core question
/// in its order, with its kind of answer, and the core amenity names. An
/// answer outside its kind is refused at add_event, and named.
#[test]
fn questions_are_the_librarys_and_a_bad_answer_is_refused() {
    let q = ok(json!({ "op": "questions" }));
    let keys: Vec<&str> = q["site"].as_array().unwrap().iter().map(|x| x["key"].as_str().unwrap()).collect();
    let core: Vec<&str> = transit_stops::SITE.iter().map(|(k, _)| *k).collect();
    assert_eq!(keys, core);
    let position = q["site"].as_array().unwrap().iter().find(|x| x["key"] == "position").unwrap();
    assert_eq!(position["answer"], json!({ "kind": "one_of", "answers": ["near_side", "far_side", "opposite", "mid_block"] }));
    let lanes = q["site"].as_array().unwrap().iter().find(|x| x["key"] == "lanes").unwrap();
    assert_eq!(lanes["answer"], json!({ "kind": "number" }));
    assert_eq!(q["amenities"].as_array().unwrap().len(), transit_stops::AMENITIES.len());

    let loaded = ok(json!({ "op": "load_stops", "stops": sample() }));
    let hash = loaded["stops_hash"].clone();
    let survey = |position: &str| {
        ok(json!({ "op": "add_event", "stops_hash": hash, "stop_id": "6124",
            "event": { "on": "2026-02-02", "event": "changed", "surveyed_by": "R. Ortiz",
                "site": { "position": position, "x_bike_lane": "painted" }, "refs": { "sign": "S-1180" } } }))
    };
    let good = survey("far_side");
    assert_eq!(good["refusals"], json!([]));
    let bad = survey("behind");
    assert_eq!(bad["refusals"][0]["problem"], json!({ "kind": "bad_answer", "key": "position", "value": "behind" }));
    assert!(bad["stops_hash"].is_null());
}

/// The surveyor is the writer (D-158). A new survey names `by`. A held one
/// keeps its own. This is the receive path's step before validate.
#[test]
fn attribute_surveys_names_the_writer() {
    let held = ok(json!({ "op": "load_stops", "stops": sample() }));
    let mut next = sample();
    next["stops"]["6124"].as_array_mut().unwrap().push(json!(
        { "on": "2027-10-01", "event": "changed", "surveyed_by": "Typed By Hand", "site": { "surface": "concrete" } }));
    let out = ok(json!({ "op": "attribute_surveys", "stops_hash": held["stops_hash"], "stops": next, "by": "Rosa Ortiz" }));
    assert_eq!(out["stops"]["stops"]["6124"][4]["surveyed_by"], "Rosa Ortiz");
    assert_eq!(out["stops"]["stops"]["6124"][1].get("surveyed_by"), None);
    // a first write: every survey is new
    let first = ok(json!({ "op": "attribute_surveys", "stops": next, "by": "Rosa Ortiz" }));
    assert_eq!(first["stops"]["stops"]["6124"][4]["surveyed_by"], "Rosa Ortiz");
    // a held document the module does not hold is an error
    assert!(err(json!({ "op": "attribute_surveys", "stops_hash": "sha256:00", "stops": next, "by": "x" })).len() > 0);
}

/// A field this build does not know is kept and flagged, never refused
/// (D-167): validate answers its name, the document keeps it, and the
/// kernel's load names it too.
#[test]
fn validate_flags_an_unknown_field_and_keeps_it() {
    let mut newer = sample();
    newer["stops"]["6124"][0]["shelter_model"] = json!("Tolar 2027");
    newer["survey_round"] = json!(3);
    let valid = ok(json!({"op": "validate", "stops": newer}));
    assert_eq!(valid["refusals"], json!([]));
    assert_eq!(valid["unknown_fields"], json!(["shelter_model", "survey_round"]));
    let document: Value = serde_json::from_str(valid["document"].as_str().unwrap()).unwrap();
    assert_eq!(document["stops"]["6124"][0]["shelter_model"], "Tolar 2027");
    assert_eq!(document["survey_round"], 3);
    let loaded = ok(json!({"op": "load_stops", "stops": document}));
    assert_eq!(loaded["unknown_fields"], json!(["shelter_model", "survey_round"]));
    // every field known: no flag at all
    assert!(ok(json!({"op": "validate", "stops": sample()})).get("unknown_fields").is_none());
}

/// The stops log (D-197): the document's records are its stops, each
/// whole, and the document assembled from them is the same document,
/// under the same hash. A record in another version is refused.
#[test]
fn the_document_and_its_log_records_round_trip() {
    let valid = ok(json!({"op": "validate", "stops": sample()}));
    let hash = valid["document_hash"].clone();
    let stops: Value = serde_json::from_str(valid["document"].as_str().unwrap()).unwrap();
    ok(json!({"op": "load_stops", "stops": stops}));
    let next = ok(json!({"op": "log_records", "stops_hash": hash}))["next"].clone();
    let records = &next[0];
    assert_eq!((records["kind"].as_str(), records["version"].as_u64()), (Some("stop"), Some(1)));
    assert_eq!(records["records"]["6124"], stops["stops"]["6124"], "one record per stop, its events whole");
    assert_eq!(next[1]["kind"], "stops");
    assert_eq!(next[1]["records"]["document"], json!({"kind": "stops", "schema_version": 1, "stops": {}}), "the document's own record: the document with no stops");
    let held: serde_json::Map<String, Value> =
        records["records"].as_object().unwrap().iter().map(|(id, v)| (id.clone(), json!({"version": 1, "value": v}))).collect();
    let document = json!({"version": 1, "value": next[1]["records"]["document"]});
    let back = ok(json!({"op": "log_document", "records": held, "document": document}));
    assert_eq!(back["document_hash"], hash, "the same document, the same identity");
    assert_eq!(back["document"], valid["document"]);
    let mut newer = held.clone();
    newer["7001"]["version"] = json!(2);
    assert!(err(json!({"op": "log_document", "records": newer})).contains("record 7001"));
}

/// A correction names the event as the writer read it (D-166): changed
/// since, the answer is a conflict naming it, and nothing is corrected.
#[test]
fn a_correction_of_an_event_changed_since_is_a_conflict() {
    let valid = ok(json!({"op": "validate", "stops": sample()}));
    let stops: Value = serde_json::from_str(valid["document"].as_str().unwrap()).unwrap();
    let event = stops["stops"]["7001"][1].clone();
    let id = event["id"].as_str().unwrap().to_string();
    let hash = ok(json!({"op": "load_stops", "stops": stops}))["stops_hash"].clone();
    let saw = cap_abi::canonical_hash(&event).unwrap();
    let fix = json!({"on": "2027-05-04", "event": "permitted"});
    let fixed = ok(json!({"op": "correct_event", "stops_hash": hash, "stop_id": "7001", "event_id": id, "event": fix, "saw": saw}));
    assert!(fixed.get("conflict").is_none() && fixed["stops"].is_object(), "{fixed}");
    // Another writer's read: the event as it stands before the fix.
    let stale = ok(json!({"op": "correct_event", "stops_hash": fixed["stops_hash"], "stop_id": "7001", "event_id": id, "event": fix, "saw": saw}));
    assert_eq!(stale["conflict"], json!(id));
    assert!(stale["stops"].is_null());
}

/// A pick reads the stops over the period the published set gives it
/// (D-228): `read_pick` and `export_pick` agree with the explicit-date
/// ops, a retired pick bounds nothing, the next published pick does, and
/// a network with no plan id is no pick.
#[test]
fn a_pick_reads_the_stops_over_the_period_the_published_set_gives() {
    let mut pick = sample_network_value();
    pick["plan"] = json!({"id": "fall-2026", "name": "Fall 2026", "effective_from": "2026-10-01"});
    let pick_hash = ok(json!({"op": "load_network", "network": pick}))["network_hash"].clone();
    let seeded = ok(json!({"op": "seed", "network_hash": pick_hash}));
    let b = seeded["stops"]["stops"]["B"][0].clone();
    let renamed = ok(json!({"op": "add_event", "stops_hash": seeded["stops_hash"], "stop_id": "A", "event": {"on": "2026-09-01", "event": "changed", "name": "A Street Station"}}));
    let retired = ok(json!({"op": "add_event", "stops_hash": renamed["stops_hash"], "stop_id": "B", "event": {"on": "2026-11-02", "event": "retired"}}));
    let moved = ok(json!({"op": "add_event", "stops_hash": retired["stops_hash"], "stop_id": "C",
        "event": {"on": "2026-11-10", "event": "moved", "lat": b["lat"].as_f64().unwrap() + 0.001, "lon": b["lon"]}}));
    let stops_hash = moved["stops_hash"].clone();
    let entry = |id: &str, from: &str, status: &str| json!({"id": id, "name": id, "effective_from": from, "network_hash": "sha256:x", "status": status});
    let published = json!({"kind": "plans", "schema_version": 1, "plans": [
        entry("fall-2026", "2026-10-01", "published"), entry("holiday", "2026-11-01", "retired"), entry("winter", "2026-12-01", "published")]});

    let read = ok(json!({"op": "read_pick", "network_hash": pick_hash, "stops_hash": stops_hash, "published": published}));
    assert_eq!(read["period"], json!({"from": "2026-10-01", "until": "2026-12-01"}), "the retired pick bounds nothing");
    assert_eq!(read["network"]["stops"]["A"]["name"], "A Street Station");
    let direct = ok(json!({"op": "read_records", "network_hash": pick_hash, "stops_hash": stops_hash, "on": "2026-10-01"}));
    assert_eq!(read["network_hash"], direct["network_hash"]);
    let warned = ok(json!({"op": "visit_warnings", "network_hash": pick_hash, "stops_hash": stops_hash, "from": "2026-10-01", "until": "2026-12-01"}));
    assert_eq!(read["warnings"], warned["warnings"]);
    assert_eq!(read["warnings"], json!([{"stop_not_in_service": {"stop": "B", "status": "retired", "from": "2026-11-02", "until": "2026-12-01"}}]));

    let exported = ok(json!({"op": "export_pick", "network_hash": pick_hash, "stops_hash": stops_hash, "published": published}));
    let versions = ok(json!({"op": "export_versions", "network_hash": pick_hash, "stops_hash": stops_hash, "from": "2026-10-01", "until": "2026-12-01"}));
    assert_eq!(exported["network_hash"], versions["network_hash"]);
    assert!(exported["network"]["stops"].as_object().unwrap().contains_key("C@2026-10-01"), "C moved inside the period");

    // before the first publish the pick is open-ended
    let first = ok(json!({"op": "read_pick", "network_hash": pick_hash, "stops_hash": stops_hash}));
    assert_eq!(first["period"], json!({"from": "2026-10-01"}));

    // a network with no plan id is no pick: nothing is read
    let plain = sample_network()["network_hash"].clone();
    let none = ok(json!({"op": "read_pick", "network_hash": plain, "stops_hash": stops_hash, "published": published}));
    assert!(none.get("period").is_none() && none["warnings"] == json!([]) && none["network_hash"] == plain);
}

/// A write that says the stop is new is refused a held id; an event added
/// to a held stop without that is an ordinary write (D-228). The
/// vocabulary lists the statuses in order.
#[test]
fn a_new_stop_is_refused_a_held_id_and_the_vocabulary_lists_the_statuses() {
    let network_hash = sample_network()["network_hash"].clone();
    let seeded = ok(json!({"op": "seed", "network_hash": network_hash}));
    let event = json!({"on": "2026-10-01", "event": "changed", "name": "A Street Station"});
    let refused = ok(json!({"op": "add_event", "stops_hash": seeded["stops_hash"], "stop_id": "A", "event": event, "new": true}));
    assert_eq!(refused["refusals"], json!([{"stop": "A", "problem": {"kind": "stop_exists"}}]));
    assert!(refused["stops_hash"].is_null());
    let ordinary = ok(json!({"op": "add_event", "stops_hash": seeded["stops_hash"], "stop_id": "A", "event": event}));
    assert_eq!(ordinary["refusals"], json!([]));
    let began = ok(json!({"op": "add_event", "stops_hash": seeded["stops_hash"], "stop_id": "N9", "new": true,
        "event": {"on": "2026-10-01", "event": "planned", "lat": 45.5, "lon": -122.3}}));
    assert_eq!(began["refusals"], json!([]));
    assert_eq!(ok(json!({"op": "vocabulary"}))["statuses"], json!(["planned", "permitted", "built", "in_service", "suspended", "retired"]));
}

// ---------------------------------------------------------------------
// receive: the stops log's write, whole (D-228)
// ---------------------------------------------------------------------

fn header(seq: u64, key: Option<&str>) -> Value {
    let mut intent = json!({ "op": "POST /logs/stops" });
    if let Some(k) = key {
        intent["key"] = json!(k);
    }
    json!({ "log": "stops", "seq": seq, "writer": "sub-7", "at_ms": 2_000_000, "module": "cap-stops@test", "intent": intent })
}

fn requests_sample() -> Value {
    json!({ "kind": "stop_requests", "schema_version": 2, "requests": {
        "R-1": { "stop": "6124", "job": "repair_improve", "status": "submitted" }
    } })
}

/// The fold after the host stores `entry` at seq 1.
fn folded(entry: &str) -> Value {
    let stored = [transit_log::Stored { seq: 1, text: entry.into() }];
    let fold = transit_log::fold(transit_log::Checkpoint::empty("stops"), &stored, &cap_abi::record_hash).unwrap();
    serde_json::to_value(fold.fold).unwrap()
}

fn kinds_changed(entry: &Value) -> Vec<String> {
    let mut kinds: Vec<String> = entry["changes"].as_array().unwrap().iter().map(|c| c["kind"].as_str().unwrap().to_string()).collect();
    kinds.sort();
    kinds.dedup();
    kinds
}

#[test]
fn receive_writes_a_stop_and_the_request_it_carries_out_as_one_entry() {
    let stops = ok(json!({"op": "load_stops", "stops": sample()}))["stops_hash"].clone();
    let requests = ok(json!({"op": "load_stop_requests", "requests": requests_sample()}))["requests_hash"].clone();
    let event = json!({"on": "2027-10-01", "event": "changed", "amenities": {"shelter": true}, "request": "R-1"});
    let ordered = json!({"system": "manual", "id": "WO-9", "status": "open"});
    let got = ok(json!({ "op": "receive", "heads": {"stops": stops, "stop_requests": requests},
        "write": { "header": header(1, Some("dev/1")) },
        "ops": [
            {"cap": "stops", "op": "add_event", "args": {"stop_id": "6124", "event": event}},
            {"cap": "requests", "op": "record_work_order", "args": {"id": "R-1", "work_order": ordered}}
        ] }));
    let entry: Value = serde_json::from_str(got["entry"].as_str().unwrap_or_else(|| panic!("an entry: {got}"))).unwrap();
    assert!(kinds_changed(&entry).contains(&"stop".to_string()) && kinds_changed(&entry).contains(&"stop_request".to_string()), "{entry}");
    assert!(got["heads"]["stops"].is_string() && got["heads"]["stop_requests"].is_string(), "both documents written: {got}");
    // The same key again, over the fold that holds it, lands once.
    let again = ok(json!({ "op": "receive", "heads": got["heads"].clone(),
        "write": { "checkpoint": folded(got["entry"].as_str().unwrap()), "header": header(2, Some("dev/1")) },
        "ops": [{"cap": "stops", "op": "add_event", "args": {"stop_id": "6124", "event": event}}] }));
    assert_eq!(again["landed"], 1, "{again}");
}

#[test]
fn receive_records_only_the_documents_its_ops_wrote() {
    let stops = ok(json!({"op": "load_stops", "stops": sample()}))["stops_hash"].clone();
    let requests = ok(json!({"op": "load_stop_requests", "requests": requests_sample()}))["requests_hash"].clone();
    let got = ok(json!({ "op": "receive", "heads": {"stops": stops, "stop_requests": requests},
        "write": { "header": header(1, None) },
        "ops": [{"cap": "requests", "op": "put", "args": {"id": "R-2", "request": {"stop": "7001", "job": "new", "status": "drafted"}, "new": true}}] }));
    let entry: Value = serde_json::from_str(got["entry"].as_str().unwrap()).unwrap();
    assert_eq!(kinds_changed(&entry), vec!["stop_request", "stop_requests"], "the requests and their document's own record: {entry}");
    assert!(got["heads"].get("stops").is_none(), "{got}");
    let taken = ok(json!({ "op": "receive", "heads": {"stop_requests": requests}, "write": { "header": header(1, None) },
        "ops": [{"cap": "requests", "op": "put", "args": {"id": "R-1", "request": {"stop": "7001", "job": "new", "status": "drafted"}, "new": true}}] }));
    assert_eq!(taken["refused"]["op"], 0, "a new request under a held id: {taken}");
}

#[test]
fn receive_names_the_signed_in_writer_on_each_new_survey() {
    let stops = ok(json!({"op": "load_stops", "stops": sample()}))["stops_hash"].clone();
    let mut whole = sample();
    whole["stops"]["7001"].as_array_mut().unwrap().push(json!({"on": "2027-06-01", "event": "changed", "surveyed_by": "someone-else", "amenities": {"bench": true}}));
    let got = ok(json!({ "op": "receive", "heads": {"stops": stops.clone()}, "by": "Rae Alvarez",
        "write": { "header": header(1, None) },
        "ops": [{"cap": "stops", "op": "validate", "args": {"stops": whole, "from": stops}}] }));
    let entry: Value = serde_json::from_str(got["entry"].as_str().unwrap_or_else(|| panic!("an entry: {got}"))).unwrap();
    let stop = entry["changes"].as_array().unwrap().iter().find(|c| c["kind"] == "stop" && c["id"] == "7001").expect("stop 7001 changes");
    let surveyed: Vec<&Value> = stop["after"].as_array().unwrap().iter().filter_map(|e| e.get("surveyed_by")).collect();
    assert_eq!(surveyed, vec![&json!("Rae Alvarez")], "the host's name, never the page's: {stop}");
}

#[test]
fn receive_answers_a_moved_head_a_conflict_and_another_log_s_op_with_no_entry() {
    // validate gives each event its id (D-166)
    let valid: Value = serde_json::from_str(ok(json!({"op": "validate", "stops": sample()}))["document"].as_str().unwrap()).unwrap();
    let stops = ok(json!({"op": "load_stops", "stops": valid}))["stops_hash"].clone();
    let moved = ok(json!({ "op": "receive", "heads": {"stops": stops.clone()}, "write": { "header": header(1, None) },
        "ops": [{"cap": "stops", "op": "validate", "args": {"stops": sample(), "from": "sha256:old"}}] }));
    assert_eq!(moved["moved"]["current"], stops, "{moved}");
    let card = ok(json!({"op": "stop", "stops_hash": stops, "stop_id": "6124", "date": "2026-01-06"}));
    let event_id = card["events"][0]["id"].clone();
    let conflict = ok(json!({ "op": "receive", "heads": {"stops": stops}, "write": { "header": header(1, None) },
        "ops": [{"cap": "stops", "op": "correct_event", "args": {"stop_id": "6124", "event_id": event_id, "saw": "sha256:stale"}}] }));
    assert_eq!(conflict["conflict"], json!({"op": 0, "id": event_id}), "{conflict}");
    let other = ok(json!({ "op": "receive", "write": { "header": header(1, None) }, "ops": [{"cap": "notes", "op": "write", "args": {}}] }));
    assert!(other["failed"]["error"].as_str().unwrap().contains("not a write to the stops log"), "{other}");
    let none = ok(json!({ "op": "receive", "write": { "header": header(1, None) }, "ops": [{"cap": "stops", "op": "add_event", "args": {"stop_id": "1", "event": {"on": "2027-01-01", "event": "planned", "lat": 45.0, "lon": -122.0}}}] }));
    assert_eq!(none["failed"]["error"], "no stops document yet", "{none}");
    for answer in [moved, conflict, other, none] {
        assert!(answer.get("entry").is_none(), "{answer}");
    }
}

#[test]
fn receive_migrate_records_the_heads_as_they_are_and_a_feed_needs_a_stops_document() {
    let stops = ok(json!({"op": "load_stops", "stops": sample()}))["stops_hash"].clone();
    let requests = ok(json!({"op": "load_stop_requests", "requests": requests_sample()}))["requests_hash"].clone();
    let got = ok(json!({ "op": "receive", "heads": {"stops": stops, "stop_requests": requests},
        "write": { "header": header(1, None) }, "ops": [{"cap": "stops", "op": "migrate"}] }));
    assert_eq!(got["heads"], json!({"stops": stops, "stop_requests": requests}), "the same documents: {got}");
    let entry: Value = serde_json::from_str(got["entry"].as_str().unwrap()).unwrap();
    assert_eq!(kinds_changed(&entry), vec!["stop", "stop_request", "stop_requests", "stops"], "{entry}");
    let net = sample_network()["network_hash"].clone();
    let nothing = ok(json!({ "op": "receive", "write": { "header": header(1, None) },
        "ops": [{"cap": "stops", "op": "import_events", "args": {"network_hash": net, "feed": "sha256:feed"}}] }));
    assert!(nothing.get("entry").is_none() && nothing.get("failed").is_none(), "{nothing}");
}

// ---------------------------------------------------------------------
// the stop rule of a request (D-233, D-238)
// ---------------------------------------------------------------------

/// A stops document with stop B, in service, and retired on `retired`
/// when given.
fn stops_with_b(retired: Option<&str>) -> Value {
    let mut b = vec![json!({"on": "2020-01-01", "event": "in_service", "lat": 45.6, "lon": -122.6, "name": "B"})];
    if let Some(on) = retired {
        b.push(json!({"on": on, "event": "retired"}));
    }
    let mut doc = sample();
    doc["stops"]["B"] = Value::Array(b);
    doc
}

/// The stops log's records of a stops document, as a fold holds them.
fn stop_records(stops: &Value) -> Value {
    let hash = ok(json!({"op": "load_stops", "stops": stops}))["stops_hash"].clone();
    let next = ok(json!({"op": "log_records", "stops_hash": hash}))["next"].clone();
    let mut out = serde_json::Map::new();
    for k in next.as_array().unwrap() {
        let by_id: serde_json::Map<String, Value> =
            k["records"].as_object().unwrap().iter().map(|(id, v)| (id.clone(), json!({"version": k["version"], "value": v}))).collect();
        out.insert(k["kind"].as_str().unwrap().into(), Value::Object(by_id));
    }
    Value::Object(out)
}

fn put(id: &str, stop: &str, status: &str) -> Value {
    json!({"cap": "requests", "op": "put", "args": {"id": id, "request": {"stop": stop, "job": "repair_improve", "status": status}}})
}

/// A write as of the stops its writer read, on 2026-11-21, over the heads
/// `stops` and `requests`.
fn rule_write(read: &Value, stops: &Value, requests: &Value, ops: Value) -> Value {
    let stops = ok(json!({"op": "load_stops", "stops": stops}))["stops_hash"].clone();
    let requests = ok(json!({"op": "load_stop_requests", "requests": requests}))["requests_hash"].clone();
    let mut h = header(1, None);
    h["at_ms"] = json!(1_795_000_000_000u64);
    ok(json!({ "op": "receive", "heads": {"stops": stops, "stop_requests": requests},
        "write": { "header": h, "read": {"stops": stop_records(read)}, "today": "2026-11-21" }, "ops": ops }))
}

fn rules(list: &Value) -> Vec<(String, String)> {
    list.as_array().map(|l| l.iter().filter(|b| b["rule"].is_string()).map(|b| (b["id"].as_str().unwrap().into(), b["names"].as_str().unwrap().into())).collect()).unwrap_or_default()
}

#[test]
fn an_open_request_on_a_stop_retired_when_read_is_refused() {
    let retired = stops_with_b(Some("2026-10-01"));
    let got = rule_write(&retired, &retired, &requests_sample(), json!([put("R-9", "B", "submitted")]));
    assert!(got["entry"].is_null(), "refused: {got}");
    assert_eq!(rules(&got["broken"]), vec![("R-9".to_string(), "B".to_string())], "{got}");
    assert!(got["broken"][0]["rule"].as_str().unwrap().contains("was retired on 2026-10-01"), "{got}");
    // Completed work on a retired stop is history.
    let done = json!({"cap": "requests", "op": "put", "args": {"id": "R-9", "request": {"stop": "B", "job": "repair_improve", "status": "cancelled"}}});
    assert!(rule_write(&retired, &retired, &requests_sample(), json!([done]))["entry"].is_string());
    // A retirement dated after the write's date breaks nothing yet.
    let later = stops_with_b(Some("2027-01-01"));
    let got = rule_write(&later, &later, &requests_sample(), json!([put("R-9", "B", "submitted")]));
    assert!(got["entry"].is_string() && rules(&got["references"]).is_empty(), "{got}");
}

#[test]
fn a_stop_retired_after_the_writer_read_it_lands_with_a_warning() {
    let got = rule_write(&stops_with_b(None), &stops_with_b(Some("2026-10-01")), &requests_sample(), json!([put("R-9", "B", "submitted")]));
    assert!(got["entry"].is_string(), "the writer saw B in service: {got}");
    assert_eq!(rules(&got["references"]), vec![("R-9".to_string(), "B".to_string())], "{got}");
    assert_eq!(got["references"][0]["entry"]["seq"], 1, "{got}");
}

#[test]
fn retiring_a_stop_that_an_open_request_names_warns_the_planner() {
    // R-1 (requests_sample) is a submitted request on 6124.
    let retire = json!({"cap": "stops", "op": "add_event", "args": {"stop_id": "6124", "event": {"on": "2026-11-01", "event": "retired"}}});
    let mut stops = sample();
    stops["stops"]["6124"] = json!([{"on": "2004-09-01", "event": "in_service", "lat": 45.6934, "lon": -122.6512, "name": "NE 99th St & Hwy 99"}]);
    let got = rule_write(&stops, &stops, &requests_sample(), json!([retire]));
    assert!(got["entry"].is_string(), "a retirement is never refused for a request: {got}");
    assert_eq!(rules(&got["references"]), vec![("R-1".to_string(), "6124".to_string())], "{got}");
    // The request the retirement names is its outcome: no warning.
    let by_r1 = json!({"cap": "stops", "op": "add_event", "args": {"stop_id": "6124", "event": {"on": "2026-11-01", "event": "retired", "request": "R-1"}}});
    let got = rule_write(&stops, &stops, &requests_sample(), json!([by_r1]));
    assert!(got["entry"].is_string() && rules(&got["references"]).is_empty(), "{got}");
}

#[test]
fn a_request_already_broken_can_still_be_changed_or_cancelled() {
    let retired = stops_with_b(Some("2026-10-01"));
    let mut held = requests_sample();
    held["requests"]["R-OLD"] = json!({"stop": "B", "job": "repair_improve", "status": "submitted"});
    let mut noted = put("R-OLD", "B", "submitted");
    noted["args"]["request"]["note"] = json!("still waiting");
    let got = rule_write(&retired, &retired, &held, json!([noted]));
    assert!(got["entry"].is_string(), "R-OLD was broken before the write: {got}");
    assert_eq!(rules(&got["references"]), vec![("R-OLD".to_string(), "B".to_string())], "still warned: {got}");
    let got = rule_write(&retired, &retired, &held, json!([put("R-OLD", "B", "cancelled")]));
    assert!(got["entry"].is_string() && rules(&got["references"]).is_empty(), "cancelled: {got}");
}

/// What a request's page shows before any write (D-238): each open
/// request whose stop is retired on the date, with why; a completed one
/// and a stop's own retiring request are not named; `stop` narrows.
#[test]
fn request_warnings_names_each_open_request_on_a_stop_retired_that_day() {
    let stops = ok(json!({"op": "load_stops", "stops": stops_with_b(Some("2026-10-01"))}))["stops_hash"].clone();
    let mut requests = requests_sample();
    requests["requests"]["R-B"] = json!({"stop": "B", "job": "repair_improve", "status": "ordered", "work_order": {"id": "WO-1"}});
    requests["requests"]["R-DONE"] = json!({"stop": "B", "job": "repair_improve", "status": "cancelled"});
    let requests = ok(json!({"op": "load_stop_requests", "requests": requests}))["requests_hash"].clone();
    let got = ok(json!({"op": "request_warnings", "stops_hash": stops, "requests_hash": requests, "date": "2026-11-21"}));
    assert_eq!(got["warnings"], json!([{"request": "R-B", "stop": "B", "reason": "was retired on 2026-10-01"}]));
    let before = ok(json!({"op": "request_warnings", "stops_hash": stops, "requests_hash": requests, "date": "2026-09-30"}));
    assert_eq!(before["warnings"], json!([]), "B is in service before its retirement");
    let other = ok(json!({"op": "request_warnings", "stops_hash": stops, "requests_hash": requests, "date": "2026-11-21", "stop": "6124"}));
    assert_eq!(other["warnings"], json!([]), "only the requests of 6124");
}
