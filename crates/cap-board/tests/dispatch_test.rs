//! Native dispatch tests for the board capability — the seam
//! without wasm, over the REAL fixture-derived network (the same
//! document cap-network's import produces from the e2e zip). The
//! laws (the library carries the day-level detail in
//! transit-dispatch's own suite; these pin the SEAM):
//!
//! 1. the network document's identity is the module's canonical
//!    serialization (the same identity cap-analysis returns);
//! 2. the board is the materialized day, model vocabulary in the
//!    rows, edits applied in force;
//! 3. the station board pools the walking group;
//! 4. the watchlist document migrates with loss reported, and both
//!    versions query the board;
//! 5. invalid edit records are REPORTED at load, never rejected;
//! 6. unknown hashes and garbage are errors, never panics.

use cap_board::{dispatch, dispatch_bytes, dispatch_json, migrate_watchlist, Request, Response, Watchlist};
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

fn stop_board(network_hash: &str, edits_hash: Option<String>, stop_id: &str, from_secs: u32, live_hash: Option<String>) -> transit_dispatch::StopBoard {
    match dispatch(Request::StopBoard {
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
    // Route 10's schedule that runs on the fixture Friday,
    // discovered from the document (the test reads the model).
    let date = chrono::NaiveDate::parse_from_str(DATE, "%Y-%m-%d").unwrap();
    let (schedule_id, start) = net
        .schedules
        .iter()
        .find(|(_, s)| {
            net.patterns.get(&s.pattern).is_some_and(|p| p.route == "10")
                && net.service_types
                    .get(&s.service_type)
                    .is_some_and(|st| st.rule.runs_on(date))
        })
        .map(|(id, s)| {
            let start = match &s.schedule {
                transit_network::Schedule::ExactTimes { starts } => starts[0],
                _ => panic!("fixture uses exact times"),
            };
            (id.clone(), start)
        })
        .expect("route 10 has a schedule running on the fixture Friday");

    let rows = departures(&network_hash, None, "A");
    let first = rows.iter().find(|r| r.schedule == schedule_id).expect("route 10 serves A");
    assert_eq!(first.start_secs, start, "the row carries the trip's (schedule, start) identity");
    assert_eq!(first.route, "10");
    assert_eq!(first.stop, "A", "the concrete stop the row leaves from");
    assert!(!first.destination.is_empty(), "the row says where the bus goes");

    // The edit in force: the diverted trip still serves B, +300 s
    // after its anchor; the rest of the board does not move.
    let (edits_hash, reports) = load_edits(&network_hash, vec![record(EditStatus::Accepted, 300)]);
    assert_eq!(reports.len(), 1);
    assert!(reports[0].valid, "the fixture diversion is valid against the network: {reports:?}");
    assert!(reports[0].affected_trips >= 1, "the report counts the touched trips");

    // Every fixture trip through B ENDS there: an arrival is not a
    // departure, so B's board is empty with or without the edit.
    assert!(
        departures(&network_hash, Some(edits_hash), "B").is_empty(),
        "trips terminating at B never appear on B's board"
    );
    assert!(departures(&network_hash, None, "B").is_empty());

    // The skipped stop loses its service — the honest empty board.
    assert!(departures(&network_hash, None, "C").len() >= 1, "schedule-only, C is served");
    let (edits_hash, _) = load_edits(&network_hash, vec![record(EditStatus::Accepted, 60)]);
    assert!(
        departures(&network_hash, Some(edits_hash), "C").is_empty(),
        "the diversion skips C: C's board goes empty, never errors"
    );
}

#[test]
fn the_station_board_pools_the_walking_group() {
    let network_hash = load_network();
    // The station pin has nothing of its own: it answers its bays'
    // board (the fixture's shuttle), pooled, rows tagged with the bay.
    let pin = stop_board(&network_hash, None, "STN", 0, None);
    assert!(pin.pooled);
    // t40 departs SA (ends SB), t41 departs SB (ends SA): one
    // departure per bay — the terminating visits are not rows.
    let bays: Vec<&str> = pin.rows.iter().map(|r| r.departure.stop.as_str()).collect();
    assert_eq!(bays, vec!["SA", "SB"], "rows name their bay, soonest first");

    // A bay with departures of its own answers itself, unpooled.
    let bay = stop_board(&network_hash, None, "SA", 0, None);
    assert!(!bay.pooled);
    assert!(bay.rows.iter().all(|r| r.departure.stop == "SA"));
}

/// The realtime-first board over a LIVE document: the late bus
/// scheduled before `from_secs` is still to come, with its expected
/// time, delay, and status — the module classifies, no host does.
#[test]
fn the_board_reads_a_live_document() {
    use transit_live::{LiveDay, LiveStatus, LiveStop, LiveTrip, OnTimeWindow};
    let network_hash = load_network();
    let row = departures(&network_hash, None, "A").into_iter().find(|d| d.route == "10").unwrap();
    let live = LiveDay {
        window: OnTimeWindow::default(),
        trips: vec![LiveTrip {
            schedule: row.schedule.clone(),
            start_secs: row.start_secs,
            agency_trip_id: "t10".into(),
            route: Some("10".into()),
            route_name: Some("10".into()),
            worst_delay_secs: Some(180),
            status: LiveStatus::Late,
            stops: vec![LiveStop {
                stop: "A".into(),
                scheduled_departure: Some(row.departure_secs),
                arrival_delay: None,
                departure_delay: Some(120),
                predicted_departure: Some(row.departure_secs + 120),
            }],
        }],
        vehicles: vec![],
        unmatched_trips: 0,
    };
    let live_hash = match dispatch(Request::LoadLive { live: live.clone() }).unwrap() {
        Response::LoadLive { live_hash } => live_hash,
        _ => panic!("load_live"),
    };
    let canonical = serde_json::to_string(&live).unwrap();
    assert_eq!(live_hash, format!("sha256:{:x}", sha2::Sha256::digest(canonical.as_bytes())));

    let board = stop_board(&network_hash, None, "A", row.departure_secs + 60, Some(live_hash));
    assert!(board.live);
    let late = board.rows.iter().find(|r| r.departure.schedule == row.schedule).expect("the late bus is still to come");
    assert_eq!(late.expected_secs, Some(row.departure_secs + 120));
    assert_eq!(late.delay_secs, Some(120));
    assert_eq!(late.status, LiveStatus::Late);

    let err = dispatch(Request::StopBoard {
        network_hash,
        edits_hash: None,
        stop_id: "A".into(),
        date: DATE.into(),
        from_secs: 0,
        limit: 5,
        live_hash: Some("sha256:missing".into()),
    })
    .unwrap_err();
    assert!(err.contains("unknown live document"), "{err}");
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
    assert!(!departures(&network_hash, Some(edits_hash), "A").is_empty(), "the board still answers");
}

// ---------------------------------------------------------------------
// The watchlist document — the personal parameter of the same
// question. Both versions query; the migration reports its loss.
// ---------------------------------------------------------------------

fn v1() -> Watchlist {
    Watchlist::V1 {
        stops: vec![
            cap_board::WatchStopV1 { id: "A".into(), note: "home".into(), color: Some("blue".into()) },
            cap_board::WatchStopV1 { id: "B".into(), note: "work".into(), color: None },
        ],
    }
}

fn load_watchlist(document: &Watchlist) -> (String, u32) {
    match dispatch(Request::LoadWatchlist { document: document.clone() }).unwrap() {
        Response::LoadWatchlist { watchlist_hash, version } => (watchlist_hash, version),
        _ => panic!("load_watchlist"),
    }
}

fn watchlist_board(watchlist_hash: &str, network_hash: &str) -> Vec<cap_board::WatchBoardStop> {
    match dispatch(Request::WatchlistBoard {
        watchlist_hash: watchlist_hash.into(),
        network_hash: network_hash.into(),
        edits_hash: None,
        date: DATE.into(),
        from_secs: 0,
        limit: 5,
    })
    .unwrap()
    {
        Response::WatchlistBoard { stops } => stops,
        _ => panic!("watchlist_board"),
    }
}

#[test]
fn the_watchlist_migrates_with_loss_reported_and_both_versions_query() {
    let network_hash = load_network();

    // v1 queries: labels are the notes.
    let (v1_hash, v1_version) = load_watchlist(&v1());
    assert_eq!(v1_version, 1);
    let board = watchlist_board(&v1_hash, &network_hash);
    assert_eq!(board.len(), 2, "every watched stop answers, in authored order");
    assert_eq!(board[0].id, "A");
    assert_eq!(board[0].label, "home", "v1 labels are the notes");
    assert_eq!(board[1].label, "work");
    assert!(board.iter().all(|s| s.departures.iter().all(|d| !d.schedule.is_empty())), "model-native rows");

    // The migration: label = note, the color dropped AND reported.
    let (migrated, notes) = migrate_watchlist(&v1()).expect("v1 migrates");
    assert_eq!(migrated.version(), 2);
    assert_eq!(notes.len(), 1);
    assert_eq!(notes[0].kind, "dropped_field");
    assert_eq!(notes[0].field, "color");
    assert_eq!(notes[0].count, 1, "one authored color, one report");
    assert!(
        migrate_watchlist(&migrated).is_err(),
        "re-migrating v2 is an error, never a panic"
    );

    // v2 queries: labels are the labels.
    let (v2_hash, v2_version) = load_watchlist(&migrated);
    assert_eq!(v2_version, 2);
    let board = watchlist_board(&v2_hash, &network_hash);
    assert_eq!(board[0].label, "home", "the migration maps note → label");
    assert_eq!(board[1].label, "work");

    // Loading is idempotent by content hash: same document, same
    // hash, one registry entry.
    let (again, _) = load_watchlist(&v1());
    assert_eq!(again, v1_hash);

    // The board composes with the edits document like every board.
    let (edits_hash, _) = load_edits(&network_hash, vec![record(EditStatus::Accepted, 60)]);
    let patched = match dispatch(Request::WatchlistBoard {
        watchlist_hash: v1_hash,
        network_hash,
        edits_hash: Some(edits_hash),
        date: DATE.into(),
        from_secs: 0,
        limit: 5,
    })
    .unwrap()
    {
        Response::WatchlistBoard { stops } => stops,
        _ => panic!("watchlist_board"),
    };
    assert_eq!(patched.len(), 2, "the personal board patches with the edits in force");
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
    let err = dispatch(Request::StopBoard {
        network_hash,
        edits_hash: Some("sha256:missing".into()),
        stop_id: "A".into(),
        date: DATE.into(),
        from_secs: 0,
        limit: 5,
        live_hash: None,
    })
    .unwrap_err();
    assert!(err.contains("unknown edits"), "{err}");

    let err = dispatch(Request::WatchlistBoard {
        watchlist_hash: "sha256:missing".into(),
        network_hash: load_network(),
        edits_hash: None,
        date: DATE.into(),
        from_secs: 0,
        limit: 5,
    })
    .unwrap_err();
    assert!(err.contains("unknown watchlist"), "{err}");

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
    // The bare document on the kernel's bulk op — parsed straight into
    // the model, no request envelope — answers exactly what the JSON
    // op answers; garbage bytes are an error outcome, never a trap.
    let doc = include_str!("../../../testdata/network.json");
    let via_json: serde_json::Value =
        serde_json::from_str(&cap_board::dispatch_json(&format!(r#"{{"op":"load_network","network":{doc}}}"#))).unwrap();
    let via_bytes: serde_json::Value =
        serde_json::from_str(&cap_board::dispatch_bytes(cap_abi::OP_LOAD_NETWORK, doc.as_bytes())).unwrap();
    assert!(via_json["ok"]["network_hash"].as_str().is_some_and(|h| h.starts_with("sha256:")), "{via_json}");
    assert_eq!(via_bytes, via_json);
    let bad = cap_board::dispatch_bytes(cap_abi::OP_LOAD_NETWORK, b"{not a network");
    assert!(bad.contains("invalid document"), "{bad}");
}
