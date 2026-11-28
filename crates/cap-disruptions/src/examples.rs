use cap_abi::manifest::Example;
use serde_json::json;

pub fn examples() -> Vec<Example> {
    // 2026-10-23 on the fixture network: the edit document's detour, the
    // live feed over the fixture snapshot, the alerts (one explains trip
    // t10's 08:00 instance), and the notes (one on that instance).
    let net = fixtures::network();
    let edits = transit_sync::EditDocument::new(fixtures::dispatch_fixture_edits());
    let (live, live_hash) = fixtures::live_example().expect("the fixture live document");
    let stops = fixtures::fixture_stops()
        .checked()
        .expect("the fixture stops pass");
    let alerts = fixtures::fixture_alerts()
        .checked()
        .expect("the fixture alerts pass");
    let notes = fixtures::fixture_notes()
        .checked()
        .expect("the fixture notes pass");
    let hash = |d: &dyn erased::Hash| d.hash();
    let loads = vec![
        fixtures::load_network(&net.derived),
        fixtures::load("load_edits", "edits", &edits),
        fixtures::load("load_live", "live", &live),
        fixtures::load("load_stops", "stops", &stops),
        fixtures::load("load_service_alerts", "alerts", &alerts),
        fixtures::load("load_notes", "notes", &notes),
    ];
    let request = |op: &str| {
        json!({ "op": op, "network_hash": net.network_hash, "edits_hash": hash(&edits), "live_hash": live_hash,
            "stops_hash": hash(&stops), "alerts_hash": hash(&alerts), "notes_hash": hash(&notes), "date": "2026-10-23", "now": fixtures::ALERTS_NOW })
    };
    // The same day before the detour's alert was written: the detour
    // (edit record e1) is a gap, and a new alert for it starts from its
    // effect, riders, and what it explains.
    let mut unexplained = alerts.clone();
    unexplained.alerts.remove("detour-10");
    let gap_loads = {
        let mut l = loads.clone();
        l[4] = fixtures::load("load_service_alerts", "alerts", &unexplained);
        l
    };
    let gap_request = |op: &str| {
        let mut r = request(op);
        r["alerts_hash"] = json!(hash(&unexplained));
        r
    };
    let mut start = gap_request("start_alert");
    start["source"] = json!({ "kind": "edit", "id": "e1" });
    vec![
        Example {
            bytes: None,
            kinds: vec![],
            op: "today",
            loads: loads.clone(),
            request: request("today"),
        },
        Example {
            bytes: None,
            kinds: vec![],
            op: "gaps",
            loads: gap_loads.clone(),
            request: gap_request("gaps"),
        },
        Example {
            bytes: None,
            kinds: vec![],
            op: "start_alert",
            loads: gap_loads,
            request: start,
        },
    ]
}

mod erased {
    pub trait Hash {
        fn hash(&self) -> String;
    }
    impl<T: serde::Serialize> Hash for T {
        fn hash(&self) -> String {
            cap_abi::canonical_hash(self).expect("a fixture document hashes")
        }
    }
}
