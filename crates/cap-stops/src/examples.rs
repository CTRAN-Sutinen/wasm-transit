use cap_abi::manifest::Example;
use serde_json::{json, Value};
use transit_stops::stop_record::seed;
use transit_stops::{Event, Stops};

fn event(v: Value) -> Event {
    serde_json::from_value(v).expect("the example event reads")
}

fn hash(v: &impl serde::Serialize) -> String {
    cap_abi::canonical_hash(v).expect("a fixture document hashes")
}

fn load_stops(stops: &Stops) -> Value {
    fixtures::load("load_stops", "stops", stops)
}

fn ex(op: &'static str, loads: Vec<Value>, request: Value) -> Example {
    Example { bytes: None, kinds: vec![], op, loads, request }
}

pub fn examples() -> Vec<Example> {
    let net = fixtures::network();
    let network = || fixtures::load_network(&net.derived);
    let stops = fixtures::fixture_stops().checked().expect("the fixture stops pass");
    let held = || vec![load_stops(&stops)];
    let at = stops.stop_on("6124", transit_date::Date::from_ymd_opt(2027, 4, 1).expect("a date")).expect("6124 on the date");
    let retired = stops.add_event("6124", event(json!({"on": "2027-10-01", "event": "retired"}))).checked().expect("6124 retires");
    let requests = fixtures::fixture_requests().checked().expect("the fixture requests pass");
    // 7001's permit has a wrong date, so the event is corrected by its id.
    let permit = stops.stops["7001"][1].id.clone();
    let renamed = seed(&net.network).add_event("A", event(json!({"on": "2026-10-01", "event": "changed", "name": "A Street Station"}))).checked().expect("A renames");
    // A retired pick in the published set between the fall pick and the next
    let (pick, published) = fixtures::pick_example(&net).expect("the pick example");
    let b_retires = seed(&pick).add_event("B", event(json!({"on": "2026-11-02", "event": "retired"}))).checked().expect("B retires");
    let b = pick.stops.get("B").expect("stop B");
    let b_moves = seed(&pick).add_event("B", event(json!({"on": "2026-11-02", "event": "moved", "lat": b.lat + 0.001, "lon": b.lon}))).checked().expect("B moves");
    let planned = Stops::empty()
        .add_event("9906", event(json!({"on": "2026-10-20", "event": "planned", "lat": 45.58, "lon": -122.348, "name": "Washougal Transit Center", "code": "9906"})))
        .checked()
        .expect("9906 is planned");
    let plan_held = net.network.stops.keys().next().expect("a stop").clone();
    let net_b = net.network.stops.get("B").expect("stop B");
    let seeded_retired = seed(&net.network).add_event("B", event(json!({"on": "2026-11-02", "event": "retired"}))).checked().expect("B retires");
    let seeded_moved = seed(&net.network).add_event("B", event(json!({"on": "2026-10-15", "event": "moved", "lat": net_b.lat + 0.001, "lon": net_b.lon}))).checked().expect("B moves");
    let mut lacks_c = seed(&net.network);
    lacks_c.stops.remove("C");
    let lacks_c = lacks_c.checked().expect("the document without C passes");
    let recorded = transit_stops::stop_record::import_feed(&lacks_c, &net.network, transit_date::Date::from_ymd_opt(2026, 10, 1).expect("a date"), "sha256:feed").stops;
    let surveyed = stops.add_event("7001", event(json!({"on": "2027-02-01", "event": "changed",
        "surveyed_by": "", "site": {"position": "far_side", "surface": "concrete"}, "refs": {"sign": "S-1180"}})));
    let (mut log, _) = fixtures::fixture_log_records();
    let own = log.pop().and_then(|mut k| k.records.remove(transit_log::DOCUMENT_ID)).expect("the document's own record");
    let records: std::collections::BTreeMap<String, Value> =
        log.pop().expect("the stop records").records.into_iter().map(|(k, v)| (k, json!({ "version": 1, "value": v }))).collect();
    let request: transit_requests::StopRequest = serde_json::from_value(json!({ "stop": "6124", "job": "repair_improve", "status": "drafted" })).expect("the request reads");
    vec![
        ex("validate", vec![], json!({ "op": "validate", "stops": stops })),
        ex("stops_on", held(), json!({ "op": "stops_on", "stops_hash": hash(&stops), "date": "2027-04-01" })),
        ex("stops_near", held(), json!({ "op": "stops_near", "stops_hash": hash(&stops), "date": "2027-04-01", "lat": at.lat + 0.0002, "lon": at.lon, "limit": 2 })),
        ex("stop", held(), json!({ "op": "stop", "stops_hash": hash(&stops), "stop_id": "6124", "date": "2027-07-01" })),
        ex(
            "request_warnings",
            vec![load_stops(&retired), fixtures::load("load_stop_requests", "requests", &requests)],
            json!({ "op": "request_warnings", "stops_hash": hash(&retired), "requests_hash": hash(&requests), "date": "2027-11-01" }),
        ),
        ex("seed", vec![network()], json!({ "op": "seed", "network_hash": net.network_hash })),
        ex("add_event", held(), json!({ "op": "add_event", "stops_hash": hash(&stops), "stop_id": "6124",
            "event": event(json!({"on": "2026-01-05", "event": "suspended", "until": "2026-01-12", "note": "paving"})) })),
        ex("correct_event", held(), json!({ "op": "correct_event", "stops_hash": hash(&stops), "stop_id": "7001", "event_id": permit,
            "event": event(json!({"on": "2027-04-19", "event": "permitted"})) })),
        ex("read_records", vec![network(), load_stops(&renamed)], json!({ "op": "read_records", "network_hash": net.network_hash, "stops_hash": hash(&renamed), "on": "2026-10-05" })),
        ex("read_pick", vec![fixtures::load_network(&pick), load_stops(&b_retires)],
            json!({ "op": "read_pick", "network_hash": hash(&pick), "stops_hash": hash(&b_retires), "published": published })),
        ex("export_pick", vec![fixtures::load_network(&pick), load_stops(&b_moves)],
            json!({ "op": "export_pick", "network_hash": hash(&pick), "stops_hash": hash(&b_moves), "published": published })),
        ex("records", vec![load_stops(&planned)], json!({ "op": "records", "stops_hash": hash(&planned), "stop_ids": ["9906", "9999"], "on": "2026-11-13" })),
        ex("take_stops", vec![network(), load_stops(&planned)],
            json!({ "op": "take_stops", "network_hash": net.network_hash, "stops_hash": hash(&planned), "stop_ids": ["9906", plan_held], "on": "2026-11-13" })),
        ex("visit_warnings", vec![network(), load_stops(&seeded_retired)],
            json!({ "op": "visit_warnings", "network_hash": net.network_hash, "stops_hash": hash(&seeded_retired), "from": "2026-10-01", "until": "2026-12-01" })),
        ex("export_versions", vec![network(), load_stops(&seeded_moved)],
            json!({ "op": "export_versions", "network_hash": net.network_hash, "stops_hash": hash(&seeded_moved), "from": "2026-10-01", "until": "2026-12-01" })),
        ex("import_events", vec![network(), load_stops(&lacks_c)],
            json!({ "op": "import_events", "stops_hash": hash(&lacks_c), "network_hash": net.network_hash, "on": "2026-10-01" })),
        ex("feed_events", vec![load_stops(&recorded)], json!({ "op": "feed_events", "stops_hash": hash(&recorded), "feed": "sha256:feed" })),
        ex("questions", vec![], json!({ "op": "questions" })),
        ex("attribute_surveys", held(), json!({ "op": "attribute_surveys", "stops_hash": hash(&stops), "stops": surveyed, "by": "Rosa Ortiz" })),
        ex("vocabulary", vec![], json!({ "op": "vocabulary" })),
        ex("log_records", held(), json!({ "op": "log_records", "stops_hash": hash(&stops) })),
        ex("receive", vec![], json!({ "op": "receive", "write": fixtures::receive_write("stops", "planner-1", 1_792_752_000_000), "by": "Rosa Ortiz",
            "ops": [{ "cap": "requests", "op": "put", "args": { "id": "R-1", "request": request } }] })),
        ex("log_document", vec![], json!({ "op": "log_document", "records": records, "document": { "version": 1, "value": own } })),
    ]
}
