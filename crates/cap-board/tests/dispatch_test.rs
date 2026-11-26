//! Native dispatch tests for cap-board (`OPS.md`). They pin:
//!
//! 1. the network document's identity is the module's canonical serialization;
//! 2. the board is the materialized day, with the in-force edits applied;
//! 3. the station board pools its walking group, and the grouped board holds
//!    the same rows by route and destination;
//! 4. an older watchlist version is refused at the load, and both versions
//!    query the board;
//! 5. an invalid edit record is reported at load and not rejected;
//! 6. an unknown hash or garbage input is an error and never a panic.

use cap_board::{dispatch, dispatch_bytes, dispatch_json, Request, Response, Watchlist};
use proptest::prelude::*;
use sha2::Digest;
use transit_network::Network;
use transit_sync::{EditRecord, EditStatus};

const FIXTURE: &[u8] = include_bytes!("../../../web/e2e/fixtures/sample-gtfs.zip");
const DATE: &str = "2026-10-23"; // the fixture's loaded Friday

fn derived_network() -> Network {
    let feed = transit_codec::Feed::from_zip_bytes(FIXTURE).expect("fixture parses");
    transit_codec::import(&feed).network
}

fn load_network() -> String {
    cap_abi::kernel::kind::<transit_network::Network>().register(derived_network()).unwrap().0
}

fn record(status: EditStatus, added_secs: u32) -> EditRecord {
    EditRecord {
        id: "e1".into(),
        diversion: Some(transit_sync::Diversion {
            id: "d1".into(),
            route_id: "10".into(),
            direction_id: None,
            first_kept_stop: "A".into(),
            last_kept_stop: "B".into(),
            skipped_stops: vec!["C".into()],
            detour: None,
            added_secs,
            start_date: transit_date::Date::from_ymd_opt(2026, 10, 23).unwrap(),
            end_date: transit_date::Date::from_ymd_opt(2026, 10, 23).unwrap(),
            extra: Default::default(),
        }),
        cancel: None,
        status,
        rev: 1,
        updated_at: 1,
        extra: Default::default(),
    }
}

fn load_edits(_network_hash: &str, records: Vec<EditRecord>) -> String {
    cap_abi::kernel::kind::<transit_sync::EditDocument>().register(transit_sync::EditDocument::new(records)).unwrap().0
}

fn stop_board(network_hash: &str, edits_hash: Option<String>, stop_id: &str, from_secs: u32, live_hash: Option<String>) -> transit_dispatch::StopBoard {
    match dispatch(Request::StopBoard { previous: None,
        network_hash: network_hash.into(),
        edits_hash,
        stop_id: stop_id.into(),
        date: DATE.into(),
        from_secs,
        limit: 20,
        live_hash,
    })
    .unwrap()
    {
        Response::StopBoard { board } => board,
        _ => panic!("stop_board"),
    }
}

fn departures(network_hash: &str, edits_hash: Option<String>, stop_id: &str) -> Vec<transit_dispatch::BoardDeparture> {
    stop_board(network_hash, edits_hash, stop_id, 0, None).rows.into_iter().map(|r| r.departure).collect()
}

#[test]
fn the_network_identity_is_the_modules_canonical_serialization() {
    let hash = load_network();
    let canonical = cap_abi::canonical_bytes(&derived_network()).unwrap();
    let expect = format!("sha256:{:x}", sha2::Sha256::digest(canonical.as_bytes()));
    assert_eq!(hash, expect);
    assert_eq!(load_network(), hash);
}

#[test]
fn the_board_is_the_materialized_day_model_vocabulary() {
    let net = derived_network();
    let network_hash = load_network();
    let date = transit_date::Date::parse(DATE).unwrap();
    let (trip_id, start) = net
        .pattern_trips
        .iter()
        .find(|(_, s)| {
            net.patterns.get(&s.pattern).is_some_and(|p| p.route == "10")
                && net.service_types
                    .get(&s.service_type)
                    .is_some_and(|st| st.rule.runs_on(date))
        })
        .map(|(id, s)| {
            let start = s.trips[0].departs.first_start().expect("a start");
            let _ = id;
            (s.trips[0].id.clone(), start)
        })
        .expect("route 10 has a schedule running on the fixture Friday");

    let rows = departures(&network_hash, None, "A");
    let first = rows.iter().find(|r| r.trip == trip_id).expect("route 10 serves A");
    assert_eq!(first.start_secs, start, "the row carries the trip instance's (trip, start) identity");
    assert_eq!(first.route, "10");
    assert_eq!(first.stop, "A", "the concrete stop the row leaves from");
    assert!(!first.destination.is_empty(), "the row says where the bus goes");

    let edits_hash = load_edits(&network_hash, vec![record(EditStatus::Accepted, 300)]);

    assert!(
        departures(&network_hash, Some(edits_hash), "B").is_empty(),
        "trips terminating at B never appear on B's board"
    );
    assert!(departures(&network_hash, None, "B").is_empty());

    assert!(departures(&network_hash, None, "C").len() >= 1, "schedule-only, C is served");
    let edits_hash = load_edits(&network_hash, vec![record(EditStatus::Accepted, 60)]);
    assert!(
        departures(&network_hash, Some(edits_hash), "C").is_empty(),
        "the diversion skips C: C's board goes empty, never errors"
    );
}

#[test]
fn the_station_board_pools_the_walking_group() {
    let network_hash = load_network();
    let pin = stop_board(&network_hash, None, "STN", 0, None);
    assert!(pin.pooled);
    let bays: Vec<&str> = pin.rows.iter().map(|r| r.departure.stop.as_str()).collect();
    assert_eq!(bays, vec!["SA", "SB"], "rows name their bay, soonest first");

    let bay = stop_board(&network_hash, None, "SA", 0, None);
    assert!(!bay.pooled);
    assert!(bay.rows.iter().all(|r| r.departure.stop == "SA"));
}

#[test]
fn the_grouped_board_is_the_board_by_route_and_destination() {
    let network_hash = load_network();
    let grouped = |stop: &str, per_group: Option<u32>| match dispatch(Request::GroupedBoard { previous: None,
        network_hash: network_hash.clone(),
        edits_hash: None,
        stop_id: stop.into(),
        date: DATE.into(),
        from_secs: 0,
        per_group,
        live_hash: None,
    })
    .unwrap()
    {
        Response::GroupedBoard { board } => board,
        _ => panic!("grouped_board"),
    };
    let pin = grouped("STN", None);
    assert!(pin.pooled);
    assert!(pin.groups.iter().all(|g| g.departures.len() <= 3));
    let mut rows: Vec<_> = grouped("STN", Some(1_000)).groups.into_iter().flat_map(|g| g.departures).collect();
    rows.sort_by_key(|r| (r.departure.departure_secs, r.departure.trip.clone()));
    assert_eq!(rows, stop_board(&network_hash, None, "STN", 0, None).rows);
}

fn late_at_a(network_hash: &str) -> (String, transit_live::LiveDay, transit_dispatch::BoardDeparture) {
    use transit_live::{LiveDay, LiveStatus, LiveStop, LiveTrip, OnTimeWindow};
    let row = departures(network_hash, None, "A").into_iter().find(|d| d.route == "10").unwrap();
    let live = LiveDay { kind: Default::default(), schema_version: Default::default(),
        window: OnTimeWindow::default(),
        trips: vec![LiveTrip { date: None,
            trip: row.trip.clone(),
            start_secs: row.start_secs,
            route: Some("10".into()),
            route_name: Some("10".into()),
            worst_delay_secs: Some(180),
            status: LiveStatus::Late,
            canceled: false,
            stops: vec![LiveStop {
                stop: "A".into(),
                scheduled_departure: Some(row.departure_secs),
                arrival_delay: None,
                departure_delay: Some(120),
                predicted_departure: Some(row.departure_secs + 120),
                skipped: false,
            }],
        }],
        vehicles: vec![],
        unmatched_trips: 0,
        alerts: vec![],
        unmatched_alerts: 0,
    };
    let live_hash = cap_abi::kernel::kind::<transit_live::LiveDay>().register(live.clone()).unwrap().0;
    (live_hash, live, row)
}

#[test]
fn the_board_reads_a_live_document() {
    use transit_live::LiveStatus;
    let network_hash = load_network();
    let (live_hash, live, row) = late_at_a(&network_hash);
    let canonical = cap_abi::canonical_bytes(&live).unwrap();
    assert_eq!(live_hash, format!("sha256:{:x}", sha2::Sha256::digest(canonical.as_bytes())));

    let board = stop_board(&network_hash, None, "A", row.departure_secs + 60, Some(live_hash));
    assert!(board.live);
    let late = board.rows.iter().find(|r| r.departure.trip == row.trip).expect("the late bus is still to come");
    assert_eq!(late.expected_secs, Some(row.departure_secs + 120));
    assert_eq!(late.delay_secs, Some(120));
    assert_eq!(late.status, LiveStatus::Late);

    let err = dispatch(Request::StopBoard { previous: None,
        network_hash,
        edits_hash: None,
        stop_id: "A".into(),
        date: DATE.into(),
        from_secs: 0,
        limit: 5,
        live_hash: Some("sha256:missing".into()),
    })
    .unwrap_err();
    assert!(err.message.contains("unknown live document"), "{err}");
}

#[test]
fn an_invalid_edit_record_moves_nothing() {
    let network_hash = load_network();
    // The host owns the edit document, so a record with no anchors in the
    // network still registers. The day patches nothing.
    let mut broken = record(EditStatus::Accepted, 60);
    broken.diversion.as_mut().unwrap().first_kept_stop = "ghost".into();
    let edits_hash = load_edits(&network_hash, vec![broken]);
    assert_eq!(
        departures(&network_hash, Some(edits_hash), "A"),
        departures(&network_hash, None, "A"),
        "the board answers as if no edit were in force"
    );
}


fn watchlist() -> Watchlist {
    let stop = |id: &str, label: &str| cap_board::WatchStop { id: id.into(), label: label.into(), extra: Default::default() };
    Watchlist::new(vec![stop("A", "home"), stop("B", "work")])
}

fn load_watchlist_json(document: &serde_json::Value) -> serde_json::Value {
    let request = serde_json::json!({ "op": "load_watchlist", "document": document }).to_string();
    serde_json::from_str(&cap_board::invoke(&request)).unwrap()
}

fn load_watchlist(document: &Watchlist) -> (String, u32) {
    let outcome = load_watchlist_json(&serde_json::to_value(document).unwrap());
    let ok = &outcome["ok"];
    (ok["watchlist_hash"].as_str().expect("watchlist_hash").to_string(), ok["version"].as_u64().expect("version") as u32)
}

fn watchlist_board(watchlist_hash: &str, network_hash: &str) -> Vec<cap_board::WatchBoardStop> {
    match dispatch(Request::WatchlistBoard { previous: None,
        watchlist_hash: watchlist_hash.into(),
        network_hash: network_hash.into(),
        edits_hash: None,
        date: DATE.into(),
        from_secs: 0,
        limit: 5,
        live_hash: None,
    })
    .unwrap()
    {
        Response::WatchlistBoard { stops } => stops,
        _ => panic!("watchlist_board"),
    }
}

#[test]
fn the_watchlist_board_answers_each_saved_stop_in_order() {
    let network_hash = load_network();
    let (hash, version) = load_watchlist(&watchlist());
    assert_eq!(version, 2);
    let board = watchlist_board(&hash, &network_hash);
    assert_eq!(board.len(), 2, "every watched stop answers, in saved order");
    assert_eq!((board[0].id.as_str(), board[0].label.as_str()), ("A", "home"));
    assert_eq!(board[1].label, "work");
    assert!(board.iter().all(|s| s.board.rows.iter().all(|r| !r.departure.trip.is_empty())), "model-native rows");

    assert_eq!(load_watchlist(&watchlist()).0, hash);

    let edits_hash = load_edits(&network_hash, vec![record(EditStatus::Accepted, 60)]);
    let patched = match dispatch(Request::WatchlistBoard { previous: None,
        watchlist_hash: hash,
        network_hash,
        edits_hash: Some(edits_hash),
        date: DATE.into(),
        from_secs: 0,
        limit: 5,
        live_hash: None,
    })
    .unwrap()
    {
        Response::WatchlistBoard { stops } => stops,
        _ => panic!("watchlist_board"),
    };
    assert_eq!(patched.len(), 2, "the personal board patches with the edits in force");
}

#[test]
fn a_watchlist_from_before_the_tags_loads_as_its_version_or_is_refused() {
    let (tagged, _) = load_watchlist(&watchlist());
    let legacy = load_watchlist_json(&serde_json::json!({"version": "v2", "stops": [{"id": "A", "label": "home"}, {"id": "B", "label": "work"}]}));
    assert_eq!(legacy["ok"]["watchlist_hash"].as_str(), Some(tagged.as_str()), "{legacy}");
    assert_eq!(legacy["ok"]["version"], 2);

    let v1 = load_watchlist_json(&serde_json::json!({"version": "v1", "stops": [{"id": "A", "note": "home", "color": "blue"}]}));
    let refused = v1["error"]["message"].as_str().unwrap_or_else(|| panic!("{v1}"));
    assert!(refused.contains("watchlist v1") && refused.contains("migrate"), "{refused}");

    let v3 = load_watchlist_json(&serde_json::json!({"kind": "watchlist", "schema_version": 3, "stops": []}));
    let refused = v3["error"]["message"].as_str().unwrap_or_else(|| panic!("{v3}"));
    assert!(refused.contains("v3") && refused.contains("reads v2"), "{refused}");
}

#[test]
fn the_watchlist_board_is_the_stop_board() {
    let network_hash = load_network();
    let (live_hash, _, row) = late_at_a(&network_hash);
    let edits_hash = load_edits(&network_hash, vec![record(EditStatus::Accepted, 60)]);
    let watched = Watchlist::new(
        ["A", "STN", "C"]
            .into_iter()
            .map(|id| cap_board::WatchStop { id: id.into(), label: id.to_lowercase(), extra: Default::default() })
            .collect(),
    );
    let (watchlist_hash, _) = load_watchlist(&watched);
    let from_secs = row.departure_secs + 60;
    let stops = match dispatch(Request::WatchlistBoard { previous: None,
        watchlist_hash,
        network_hash: network_hash.clone(),
        edits_hash: Some(edits_hash.clone()),
        date: DATE.into(),
        from_secs,
        limit: 20,
        live_hash: Some(live_hash.clone()),
    })
    .unwrap()
    {
        Response::WatchlistBoard { stops } => stops,
        _ => panic!("watchlist_board"),
    };
    assert_eq!(stops.iter().map(|s| s.id.as_str()).collect::<Vec<_>>(), ["A", "STN", "C"]);
    for s in &stops {
        let expected = stop_board(&network_hash, Some(edits_hash.clone()), &s.id, from_secs, Some(live_hash.clone()));
        assert_eq!(s.board, expected, "{}: the stop board", s.id);
    }
    assert!(stops[0].board.live && stops[0].board.rows.iter().any(|r| r.departure.trip == row.trip && r.delay_secs == Some(120)));
    assert!(stops[1].board.pooled && !stops[1].board.rows.is_empty(), "a watched station pools its bays");
}

#[test]
fn unknown_hashes_and_garbage_are_errors_not_panics() {
    let network_hash = load_network();
    let err = dispatch(Request::StopBoard { previous: None,
        network_hash,
        edits_hash: Some("sha256:missing".into()),
        stop_id: "A".into(),
        date: DATE.into(),
        from_secs: 0,
        limit: 5,
        live_hash: None,
    })
    .unwrap_err();
    assert!(err.message.contains("unknown edits"), "{err}");

    let err = dispatch(Request::WatchlistBoard { previous: None,
        watchlist_hash: "sha256:missing".into(),
        network_hash: load_network(),
        edits_hash: None,
        date: DATE.into(),
        from_secs: 0,
        limit: 5,
        live_hash: None,
    })
    .unwrap_err();
    assert!(err.message.contains("unknown watchlist"), "{err}");

    let bad = dispatch_json(r#"{"op":"nope"}"#);
    assert!(bad.contains("invalid request"), "{bad}");
    let bad = dispatch_json(r#"{"op":"stop_board","network_hash":"sha256:x","stop_id":"A","date":"2026-13-99","from_secs":0,"limit":5}"#);
    assert!(bad.contains("invalid date"), "{bad}");
    let bad = dispatch_json(&serde_json::json!({"op": "load_watchlist", "document": {"version": "v3", "stops": []}}).to_string());
    assert!(bad.contains("invalid request"), "an unknown document version is a request error: {bad}");
    let bad = dispatch_bytes(0, b"whatever");
    assert!(bad.contains("unknown bulk op"), "the module takes JSON documents: {bad}");
}

#[test]
fn the_kernel_bulk_op_registers_the_same_identity_as_load_network() {
    let doc = include_str!("../../../testdata/network.json");
    let via_json: serde_json::Value =
        serde_json::from_str(&cap_board::invoke(&format!(r#"{{"op":"load_network","network":{doc}}}"#))).unwrap();
    let via_bytes: serde_json::Value =
        serde_json::from_str(&cap_board::invoke_bytes(<transit_network::Network as cap_abi::kernel::Load>::CODE, doc.as_bytes())).unwrap();
    assert!(via_json["ok"]["network_hash"].as_str().is_some_and(|h| h.starts_with("sha256:")), "{via_json}");
    assert_eq!(via_bytes, via_json);
    let bad = cap_board::invoke_bytes(<transit_network::Network as cap_abi::kernel::Load>::CODE, b"{not a network");
    assert!(bad.contains("invalid document"), "{bad}");
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// Errors are for hash addresses. A lowercase id cannot collide with the
    /// fixture's stops (A, B, C).
    #[test]
    fn an_unknown_stop_answers_an_empty_board(id in "[a-z][a-z0-9_]{0,7}") {
        let hash = load_network();
        match dispatch(Request::StopBoard { previous: None,
            network_hash: hash,
            edits_hash: None,
            stop_id: id,
            date: DATE.into(),
            from_secs: 0,
            limit: 10,
            live_hash: None,
        })
        .expect("a lookup answers, never errors")
        {
            Response::StopBoard { board } => {
                prop_assert!(board.rows.is_empty(), "an unknown stop has no departures");
                prop_assert!(!board.pooled, "an unknown stop pools nothing");
            }
            _ => panic!("stop_board"),
        }
    }
}

#[test]
fn a_skipped_stop_answers_its_diversion() {
    let network_hash = load_network();
    let edits_hash = load_edits(&network_hash, vec![record(EditStatus::Accepted, 120)]);
    let board = stop_board(&network_hash, Some(edits_hash), "C", 0, None);
    assert!(board.rows.iter().all(|r| r.departure.route != "10"), "route 10 skips C");
    assert!(!board.skipped.is_empty(), "the board says what was taken away");
    assert!(board.skipped.iter().all(|s| s.departure.route == "10" && s.diversion.as_deref() == Some("d1")));
    assert!(stop_board(&network_hash, None, "C", 0, None).skipped.is_empty());
}

#[test]
fn an_alert_reaches_each_entitys_departures_as_published() {
    let network_hash = load_network();
    let req = serde_json::json!({ "op": "alert_reach", "network_hash": network_hash, "date": DATE, "alert": {
        "status": "published", "header": [{ "text": "x" }],
        "informed": [{ "route": "10", "stop": "A" }, { "stop": "STN" }, { "route": "10", "direction": "nope" }]
    }});
    let out: serde_json::Value = serde_json::from_str(&cap_board::dispatch_json(&req.to_string())).unwrap();
    let e = &out["ok"]["entities"];
    assert_eq!(e[0]["published"]["route_id"], "10");
    assert_eq!(e[0]["published"]["stop_id"], "A");
    assert_eq!(e[0]["reach"]["departures"], 2);
    assert_eq!(e[0]["reach"]["routes"], serde_json::json!(["10"]));
    assert_eq!(e[1]["names"]["stop"]["bays"], 2);
    assert_eq!(e[1]["reach"]["departures"], 0);
    assert!(e[2]["not_published"].is_string(), "{}", e[2]);
    assert_eq!(out["ok"]["reach"]["departures"], 2);
}
