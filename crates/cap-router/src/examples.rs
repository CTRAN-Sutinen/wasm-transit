//! The router's worked examples (its `examples` feature): the documents
//! to load and each request. The generator runs them through this
//! crate's `invoke` and records the answers.

use cap_abi::manifest::Example;
use serde_json::json;

pub fn examples() -> Vec<Example> {
    let net = fixtures::network();
    let (live, live_hash) = fixtures::live_example().expect("the fixture live document");
    let edits = transit_sync::EditDocument::new(fixtures::dispatch_fixture_edits());
    let network = || fixtures::load_network(&net.derived);
    // The itinerary whose paths the map draws: A to B at 07:00 on the
    // fixture Friday.
    let date = chrono::NaiveDate::from_ymd_opt(2026, 10, 23).expect("a date");
    let day = transit_network::materialize(&net.network, date).expect("the fixture day");
    let graph = transit_router::RouterNetwork::build_from_day(&net.network, &day, date);
    let itinerary = transit_router::earliest_arrival(&graph, "A", "B", 25200, 4).expect("query").expect("A→B on the fixture Friday");
    vec![
        // Over the fixture's live document: the legs carry the predicted
        // times and say how far they moved.
        Example {
            bytes: None, kinds: vec![],
            op: "plan",
            loads: vec![network(), fixtures::load("load_live", "live", &live)],
            request: json!({ "op": "plan", "network_hash": net.network_hash, "from": "A", "to": "B",
                             "date": "2026-10-23", "depart_secs": 25200, "max_legs": 4, "live_hash": live_hash }),
        },
        // No deadline: the last bus home from A to B on the fixture Friday.
        Example {
            bytes: None, kinds: vec![],
            op: "plan_arrive_by",
            loads: vec![network()],
            request: json!({ "op": "plan_arrive_by", "network_hash": net.network_hash, "from": "A", "to": "B", "date": "2026-10-23", "max_legs": 4 }),
        },
        // The diversion costs the rider 5 minutes: the notice says so.
        Example {
            bytes: None, kinds: vec![],
            op: "plan_compare",
            loads: vec![network(), fixtures::load("load_edits", "edits", &edits)],
            request: json!({ "op": "plan_compare", "network_hash": net.network_hash, "edits_hash": cap_abi::canonical_hash(&edits).expect("a hash"),
                             "from": "A", "to": "B", "date": "2026-10-23", "depart_secs": 25200, "max_legs": 4 }),
        },
        Example {
            bytes: None, kinds: vec![],
            op: "itinerary_paths",
            loads: vec![network()],
            request: json!({ "op": "itinerary_paths", "network_hash": net.network_hash, "date": "2026-10-23", "itinerary": itinerary }),
        },
    ]
}
