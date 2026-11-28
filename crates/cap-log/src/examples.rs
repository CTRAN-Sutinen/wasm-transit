use cap_abi::manifest::Example;
use serde_json::json;

pub fn examples() -> Vec<Example> {
    let (fold, entries) = fixtures::fixture_stops_log();
    let stored: Vec<transit_log::Stored> = entries
        .iter()
        .map(|e| transit_log::Stored {
            seq: e.seq,
            text: cap_abi::canonical_bytes(e).expect("an entry's bytes"),
        })
        .collect();
    let (stops, _) = fixtures::fixture_log_records();
    let mut next = stops[0].clone();
    if let Some(events) = next
        .records
        .values_mut()
        .next()
        .and_then(|v| v.as_array_mut())
    {
        events.push(json!({"id": "e-shelter", "on": "2027-03-01", "event": "changed", "amenities": {"shelter": true}}));
    }
    let mut revert =
        fixtures::fixture_log_header(3, 1_770_000_120_000, "POST /logs/stops/revert", None);
    revert.writer = "auditor-2".into();
    let mut gap =
        fixtures::fixture_log_header(3, 1_770_000_120_000, "POST /logs/stops/close-gap", None);
    gap.writer = "auditor-2".into();
    gap.intent.note = Some("entry 3 was deleted from the bucket by hand".into());
    let network_hash = fixtures::network().network_hash;
    vec![
        Example {
            bytes: None,
            kinds: vec![],
            op: "compose",
            loads: vec![],
            request: json!({ "op": "compose", "checkpoint": fold, "header": fixtures::fixture_log_header(3, 1_770_000_120_000, "POST /logs/stops", Some("tablet-4/18")), "next": [next] }),
        },
        Example {
            bytes: None,
            kinds: vec![],
            op: "fold",
            loads: vec![],
            request: json!({ "op": "fold", "log": "stops", "entries": stored }),
        },
        Example {
            bytes: None,
            kinds: vec![],
            op: "revert",
            loads: vec![],
            request: json!({ "op": "revert", "log": "stops", "entries": stored, "target": 2, "header": revert }),
        },
        Example {
            bytes: None,
            kinds: vec![],
            op: "close_gap",
            loads: vec![],
            request: json!({ "op": "close_gap", "header": gap }),
        },
        // A stop request names a stop that the stops log does not hold.
        Example {
            bytes: None,
            kinds: vec![],
            op: "references",
            loads: vec![],
            request: json!({ "op": "references",
                "kinds": [
                    {"store_kind": "stops", "log": "stops", "records": [{"kind": "stop", "collection": "stops"}], "references": []},
                    {"store_kind": "stop_requests", "log": "stops", "records": [{"kind": "stop_request", "collection": "requests"}],
                     "references": [{"path": "/requests/*/stop", "target": "stop"}]}
                ],
                "folds": {"stops": {
                    "stop": {"6124": {"version": 1, "value": [{"on": "2004-09-01", "event": "in_service", "lat": 45.6934, "lon": -122.6512}]}},
                    "stop_request": {"R-1": {"version": 1, "value": {"stop": "6124"}}, "R-2": {"version": 1, "value": {"stop": "7001"}}}
                }} }),
        },
        // A frozen version names a network that a revert dropped.
        Example {
            bytes: None,
            kinds: vec![],
            op: "naming",
            loads: vec![],
            request: json!({ "op": "naming",
                "kinds": [{"store_kind": "frozen_versions", "log": "plans", "records": [{"kind": "frozen_version", "collection": "versions"}],
                           "references": [{"path": "/versions/*/network_hash", "target": "network"}]}],
                "folds": {"plans": {"frozen_version": {"fall-2026@1": {"version": 1, "value": {"network_hash": network_hash, "purpose": "the published pick"}}}}},
                "hashes": [network_hash] }),
        },
    ]
}
