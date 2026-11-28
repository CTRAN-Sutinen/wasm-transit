use cap_abi::manifest::Example;
use serde_json::json;

pub fn examples() -> Vec<Example> {
    let net = fixtures::network();
    let (live, live_hash) = fixtures::live_example().expect("the fixture live document");
    let watchlist = fixtures::fixture_watchlist();
    let network = || fixtures::load_network(&net.derived);
    let with_live = || vec![network(), fixtures::load("load_live", "live", &live)];
    let alert: transit_alerts::ServiceAlert = serde_json::from_value(json!({
        "status": "published", "header": [{ "text": "Main & 1st: route 10 boards across the street" }],
        "informed": [{ "route": "10", "stop": "A" }, { "stop": "STN" }]
    }))
    .expect("the example alert reads");
    vec![
        Example {
            bytes: None,
            kinds: vec![],
            op: "stop_board",
            loads: with_live(),
            request: json!({ "op": "stop_board", "network_hash": net.network_hash, "stop_id": "A", "date": "2026-10-23",
                             "from_secs": 28860, "limit": 5, "live_hash": live_hash }),
        },
        Example {
            bytes: None,
            kinds: vec![],
            op: "alert_reach",
            loads: vec![network()],
            request: json!({ "op": "alert_reach", "network_hash": net.network_hash, "date": "2026-10-23", "alert": alert }),
        },
        Example {
            bytes: None,
            kinds: vec![],
            op: "grouped_board",
            loads: with_live(),
            request: json!({ "op": "grouped_board", "network_hash": net.network_hash, "stop_id": "A", "date": "2026-10-23",
                             "from_secs": 28860, "per_group": 3, "live_hash": live_hash }),
        },
        Example {
            bytes: None,
            kinds: vec![],
            op: "watchlist_board",
            loads: vec![
                network(),
                fixtures::load("load_watchlist", "document", &watchlist),
            ],
            request: json!({ "op": "watchlist_board", "watchlist_hash": cap_abi::canonical_hash(&watchlist).expect("a hash"),
                             "network_hash": net.network_hash, "date": "2026-10-23", "from_secs": 25200, "limit": 3 }),
        },
    ]
}
