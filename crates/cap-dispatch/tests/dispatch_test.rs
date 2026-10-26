//! Native dispatch tests for the dispatch capability — the seam
//! without wasm, over the REAL fixture-derived network (the same
//! document cap-network's import produces from the e2e zip). The
//! substrate is model-native: schedules, materialized days, and the
//! authored runcut — never a GTFS expansion. The laws (the library
//! carries the day-level detail in transit-dispatch's own suite;
//! these pin the SEAM):
//!
//! 1. the network document's identity is the module's canonical
//!    serialization (the same identity cap-analysis returns);
//! 2. the board is the materialized day, model vocabulary in the
//!    rows, edits applied in force;
//! 3. the station board pools the walking group;
//! 4. one block's chain is the authored runcut and ripples under the
//!    edit;
//! 5. invalid edit records are REPORTED at load, never rejected;
//! 6. unknown hashes and garbage are errors, never panics.

use cap_dispatch::{dispatch, dispatch_json, Request, Response};
use sha2::Digest;
use transit_network::Network;
use transit_sync::{EditRecord, EditStatus};

const FIXTURE: &[u8] = include_bytes!("../../../web/e2e/fixtures/sample-gtfs.zip");
const DATE: &str = "2026-10-23"; // the fixture's loaded Friday

/// The fixture-derived network document (import is the codec — the
/// boundary; this module composes over the document it produces).
fn derived_network() -> Network {
    let gtfs = transit_core::Gtfs::from_zip_bytes(FIXTURE).expect("fixture parses");
    transit_codec::gtfs_to_network(&gtfs).network
}

fn load_network() -> String {
    match dispatch(Request::LoadNetwork { network: derived_network() }).unwrap() {
        Response::LoadNetwork { network_hash } => network_hash,
        _ => panic!("load_network"),
    }
}

/// The schedule id of route 10's pattern in the fixture-derived
/// document that RUNS on the fixture Friday, discovered from the
/// document (the test reads the model, not the codec's naming).
fn route10_schedule(net: &Network) -> String {
    let date = chrono::NaiveDate::parse_from_str(DATE, "%Y-%m-%d").unwrap();
    net.schedules
        .iter()
        .find(|(_, s)| {
            net.patterns.get(&s.pattern).is_some_and(|p| p.route == "10")
                && net.service_types
                    .get(&s.service_type)
                    .is_some_and(|st| st.rule.runs_on(date))
        })
        .map(|(id, _)| id.clone())
        .expect("route 10 has a schedule running on the fixture Friday")
}

fn route10_start(net: &Network) -> u32 {
    let id = route10_schedule(net);
    let schedule = &net.schedules[&id];
    match &schedule.schedule {
        transit_network::Schedule::ExactTimes { starts } => starts[0],
        transit_network::Schedule::HeadwayBands { .. } => panic!("fixture uses exact times"),
    }
}

/// The fixture's diversion: route 10 keeps A→B, skips C, +N s.
fn record(status: EditStatus, added_secs: u32) -> EditRecord {
    EditRecord {
        id: "e1".into(),
        diversion: transit_sync::Diversion {
            id: "d1".into(),
            route_id: "10".into(),
            direction_id: None,
            first_kept_stop: "A".into(),
            last_kept_stop: "B".into(),
            skipped_stops: vec!["C".into()],
            detour: None,
            added_secs,
            start_date: chrono::NaiveDate::from_ymd_opt(2026, 10, 23).unwrap(),
            end_date: chrono::NaiveDate::from_ymd_opt(2026, 10, 23).unwrap(),
        },
        status,
        rev: 1,
        updated_at: 1,
    }
}

fn load_edits(network_hash: &str, records: Vec<EditRecord>) -> (String, Vec<transit_sync::EditReport>) {
    match dispatch(Request::LoadEdits { network_hash: network_hash.into(), records }).unwrap() {
        Response::LoadEdits { edits_hash, reports } => (edits_hash, reports),
        _ => panic!("load_edits"),
    }
}

#[test]
fn the_network_identity_is_the_modules_canonical_serialization() {
    let hash = load_network();
    // The same formula every network-consuming module uses: the
    // canonical serde bytes hashed. One document, one truth.
    let canonical = serde_json::to_string(&derived_network()).unwrap();
    let expect = format!("sha256:{:x}", sha2::Sha256::digest(canonical.as_bytes()));
    assert_eq!(hash, expect);
    // Idempotent: the same document loads to the same hash.
    assert_eq!(load_network(), hash);
}

#[test]
fn the_board_is_the_materialized_day_model_vocabulary() {
    let net = derived_network();
    let network_hash = load_network();
    let schedule_id = route10_schedule(&net);
    let start = route10_start(&net);

    let rows = match dispatch(Request::Departures {
        network_hash,
        edits_hash: None,
        stop_id: "A".into(),
        date: DATE.into(),
        from_secs: 0,
        limit: 10,
    })
    .unwrap()
    {
        Response::Departures { departures } => departures,
        _ => panic!("departures"),
    };
    let first = rows.iter().find(|r| r.schedule == schedule_id).expect("route 10 serves A");
    assert_eq!(first.start_secs, start, "the row carries the trip's (schedule, start) identity");
    assert_eq!(first.route, "10");
    assert_eq!(first.route_name, "10");
    assert_eq!(first.stop, "A", "the concrete stop the row leaves from");
    assert!(!first.destination.is_empty(), "the row says where the bus goes");

    // The edit in force: route 10's rows at B shift by +300; the
    // rest of the board does not.
    let (edits_hash, reports) = load_edits(&load_network(), vec![record(EditStatus::Accepted, 300)]);
    assert_eq!(reports.len(), 1);
    assert!(reports[0].valid, "the fixture diversion is valid against the network: {reports:?}");
    assert!(reports[0].affected_trips >= 1, "the report counts the touched trips");

    let rows = match dispatch(Request::Departures {
        network_hash: load_network(),
        edits_hash: Some(edits_hash),
        stop_id: "B".into(),
        date: DATE.into(),
        from_secs: 0,
        limit: 20,
    })
    .unwrap()
    {
        Response::Departures { departures } => departures,
        _ => panic!("departures"),
    };
    let diverted = rows.iter().find(|r| r.schedule == schedule_id).expect("the diverted trip still serves B");
    assert_eq!(diverted.departure_secs, 8 * 3600 + 600 + 300, "t10 at B: 08:15 (+300)");
    let other = rows.iter().find(|r| r.schedule != schedule_id && r.route == "20").expect("t20 at B");
    assert_eq!(other.departure_secs, 8 * 3600 + 1800, "route 20 untouched");
}

#[test]
fn the_station_board_pools_the_walking_group() {
    let network_hash = load_network();
    // The station pin answers its bays' board (the fixture's
    // shuttle), rows tagged with the concrete bay.
    let pooled = match dispatch(Request::StationDepartures {
        network_hash: network_hash.clone(),
        edits_hash: None,
        stop_id: "STN".into(),
        date: DATE.into(),
        from_secs: 0,
        limit: 10,
    })
    .unwrap()
    {
        Response::StationDepartures { departures } => departures,
        _ => panic!("station_departures"),
    };
    assert_eq!(pooled.len(), 4, "t40/t41 across both bays");
    assert!(pooled.iter().all(|d| d.stop == "SA" || d.stop == "SB"), "rows name their bay");

    // Any member answers the same board.
    let from_bay = match dispatch(Request::StationDepartures {
        network_hash,
        edits_hash: None,
        stop_id: "SA".into(),
        date: DATE.into(),
        from_secs: 0,
        limit: 10,
    })
    .unwrap()
    {
        Response::StationDepartures { departures } => departures,
        _ => panic!("station_departures"),
    };
    assert_eq!(from_bay, pooled);
}

#[test]
fn the_block_chain_ripples_under_the_edit() {
    let net = derived_network();
    let network_hash = load_network();

    // The authored runcut: B1 chains t10 into t20, the import having
    // computed the observed gap as the layover (10 minutes).
    let base = match dispatch(Request::BlockTrips {
        network_hash: network_hash.clone(),
        edits_hash: None,
        block_id: "B1".into(),
        date: DATE.into(),
    })
    .unwrap()
    {
        Response::BlockTrips { trips } => trips,
        _ => panic!("block_trips"),
    };
    assert_eq!(base.len(), 2);
    assert_eq!(base[0].layover_after_secs, Some(600), "t10 ends 08:10, t20 starts 08:20");
    assert_eq!(base[0].layover_remaining_secs, Some(600), "no edits: the layover stands");
    assert!(base.iter().all(|t| t.runs));

    // The ripple: t10 diverted +300 ends 08:16 — the connection
    // tightens from 10 min to 4 (the dispatcher's health read).
    let (edits_hash, _) = load_edits(&network_hash, vec![record(EditStatus::Accepted, 300)]);
    let ripped = match dispatch(Request::BlockTrips {
        network_hash,
        edits_hash: Some(edits_hash),
        block_id: "B1".into(),
        date: DATE.into(),
    })
    .unwrap()
    {
        Response::BlockTrips { trips } => trips,
        _ => panic!("block_trips"),
    };
    assert_eq!(ripped[0].layover_remaining_secs, Some(300), "the diverted end eats the layover (600 - 300)");
    // The chain rows carry the authored trip identity, model-native.
    assert_eq!(ripped[0].schedule, route10_schedule(&net));
}

#[test]
fn invalid_edit_records_are_reported_not_rejected() {
    let network_hash = load_network();
    // A record whose anchors do not exist in the network: reported
    // invalid, still registered (the sync contract — the host owns
    // the document, the module reports).
    let mut broken = record(EditStatus::Accepted, 60);
    broken.diversion.first_kept_stop = "ghost".into();
    let (edits_hash, reports) = load_edits(&network_hash, vec![broken]);
    assert_eq!(reports.len(), 1);
    assert!(!reports[0].valid, "the broken record is reported: {reports:?}");
    // Registered under its hash: the day views address it (and the
    // invalid record simply matches nothing).
    let rows = match dispatch(Request::Departures {
        network_hash,
        edits_hash: Some(edits_hash),
        stop_id: "A".into(),
        date: DATE.into(),
        from_secs: 0,
        limit: 10,
    })
    .unwrap()
    {
        Response::Departures { departures } => departures,
        _ => panic!("departures"),
    };
    assert!(!rows.is_empty(), "the board still answers");
}

#[test]
fn unknown_hashes_and_garbage_are_errors_not_panics() {
    let err = dispatch(Request::LoadEdits {
        network_hash: "sha256:missing".into(),
        records: vec![],
    })
    .unwrap_err();
    assert!(err.contains("unknown network"), "{err}");

    let network_hash = load_network();
    let err = dispatch(Request::Departures {
        network_hash,
        edits_hash: Some("sha256:missing".into()),
        stop_id: "A".into(),
        date: DATE.into(),
        from_secs: 0,
        limit: 5,
    })
    .unwrap_err();
    assert!(err.contains("unknown edits"), "{err}");

    let bad = dispatch_json(r#"{"op":"nope"}"#);
    assert!(bad.contains("invalid request"), "{bad}");
    let bad = dispatch_json(r#"{"op":"departures","network_hash":"sha256:x","stop_id":"A","date":"2026-13-99","from_secs":0,"limit":5}"#);
    assert!(bad.contains("invalid date"), "{bad}");
}
