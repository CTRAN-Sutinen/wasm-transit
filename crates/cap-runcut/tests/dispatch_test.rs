//! Native dispatch tests for the runcut capability — the seam
//! without wasm, over the REAL fixture-derived network (the same
//! document cap-network's import produces from the e2e zip). The
//! substrate is model-native: schedules, materialized days, and the
//! authored runcut — never a GTFS expansion. The laws (the library
//! carries the day-level detail in transit-dispatch's own suite;
//! these pin the SEAM):
//!
//! 1. the network document's identity is the module's canonical
//!    serialization (the same identity cap-analysis returns);
//! 2. the day's blocks list is the index facet: only blocks with
//!    running trips, with that day's counts;
//! 3. one block's chain is the authored runcut and ripples under
//!    the edit;
//! 4. invalid edit records are REPORTED at load, never rejected;
//! 5. unknown hashes and garbage are errors, never panics.

use cap_runcut::{dispatch, dispatch_bytes, dispatch_json, Request, Response};
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
fn the_days_blocks_are_the_index_facet_of_the_chain() {
    let network_hash = load_network();

    // The fixture Friday: B1 (t10 + t20, both running) is out.
    // B2's only trip is single-stop — the import drops it (and
    // reports it), so the authored runcut has no trip for B2 and
    // the list does not offer it: the honest hole, never a silent
    // blank. A no-service day answers empty.
    let blocks = match dispatch(Request::Blocks {
        network_hash: network_hash.clone(),
        edits_hash: None,
        date: DATE.into(),
    })
    .unwrap()
    {
        Response::Blocks { blocks } => blocks,
        _ => panic!("blocks"),
    };
    assert_eq!(blocks.len(), 1, "only B1 has authored trips running: {blocks:?}");
    assert_eq!(blocks[0].block_id, "B1");
    assert_eq!(blocks[0].trip_count, 2, "t10 and t20 both run");

    // Saturday: WK is off, the holiday was Friday-only — every bus
    // is parked.
    let parked = match dispatch(Request::Blocks {
        network_hash,
        edits_hash: None,
        date: "2026-10-24".into(),
    })
    .unwrap()
    {
        Response::Blocks { blocks } => blocks,
        _ => panic!("blocks"),
    };
    assert!(parked.is_empty(), "no service, no blocks out");
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
        tight_layover_secs: None,
    })
    .unwrap()
    {
        Response::BlockTrips { block_id, trips } => {
            assert_eq!(block_id, "B1", "the answer names its block (QAF-13)");
            trips
        }
        _ => panic!("block_trips"),
    };
    assert_eq!(base.len(), 2);
    assert_eq!(base[0].layover_after_secs, Some(600), "t10 ends 08:10, t20 starts 08:20");
    assert_eq!(base[0].layover_remaining_secs, Some(600), "no edits: the layover stands");
    assert!(base.iter().all(|t| t.runs));

    // An unknown block answers empty, never an error.
    let ghost = match dispatch(Request::BlockTrips {
        network_hash: network_hash.clone(),
        edits_hash: None,
        block_id: "missing".into(),
        date: DATE.into(),
        tight_layover_secs: None,
    })
    .unwrap()
    {
        Response::BlockTrips { trips, .. } => trips,
        _ => panic!("block_trips"),
    };
    assert!(ghost.is_empty());

    // The ripple: t10 diverted +300 ends 08:16 — the connection
    // tightens from 10 min to 4 (the connection-health read).
    let (edits_hash, _) = load_edits(&network_hash, vec![record(EditStatus::Accepted, 300)]);
    let ripped = match dispatch(Request::BlockTrips {
        network_hash,
        edits_hash: Some(edits_hash),
        block_id: "B1".into(),
        date: DATE.into(),
        tight_layover_secs: None,
    })
    .unwrap()
    {
        Response::BlockTrips { trips, .. } => trips,
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
    let blocks = match dispatch(Request::Blocks {
        network_hash,
        edits_hash: Some(edits_hash),
        date: DATE.into(),
    })
    .unwrap()
    {
        Response::Blocks { blocks } => blocks,
        _ => panic!("blocks"),
    };
    assert!(!blocks.is_empty(), "the list still answers");
}

/// The schedule id of one of route 10's patterns in the
/// fixture-derived network, found by its stop sequence (t10 and t11
/// share route and sequence — the service type disambiguates, and
/// only WK runs on the fixture Friday).
fn route10_schedule_of(net: &Network, seq: &[&str]) -> String {
    let date = chrono::NaiveDate::parse_from_str(DATE, "%Y-%m-%d").unwrap();
    net.schedules
        .iter()
        .find(|(_, s)| {
            net.patterns.get(&s.pattern).is_some_and(|p| {
                p.route == "10"
                    && p.stops.iter().map(|ps| ps.stop.as_str()).collect::<Vec<_>>() == seq
            }) && net
                .service_types
                .get(&s.service_type)
                .is_some_and(|st| st.rule.runs_on(date))
        })
        .map(|(id, _)| id.clone())
        .unwrap_or_else(|| panic!("route 10 has a {:?} pattern running on the fixture Friday", seq))
}

#[test]
fn the_trip_detail_answers_the_effective_day() {
    // The map's click-through question: one derived trip's stops,
    // EFFECTIVE for the date (a diverted trip answers its patched
    // sequence). The answer carries no geometry — the network
    // document holds the coordinates; the map is the host's view.
    let net = derived_network();
    let network_hash = load_network();
    let direct = route10_schedule_of(&net, &["A", "B"]);
    let via_c = route10_schedule_of(&net, &["A", "C", "B"]);

    // The authored day: t10's direct run A→B, times model-native.
    let stops = match dispatch(Request::TripStops {
        network_hash: network_hash.clone(),
        edits_hash: None,
        schedule: direct,
        start_secs: 8 * 3600,
        date: DATE.into(),
    })
    .unwrap()
    {
        Response::TripStops { stops } => stops,
        _ => panic!("trip_stops"),
    };
    assert_eq!(
        stops.iter().map(|s| s.stop_id.as_str()).collect::<Vec<_>>(),
        ["A", "B"]
    );
    assert_eq!(stops[0].departure_secs, 8 * 3600, "A departs 08:00");
    assert_eq!(stops[1].arrival_secs, 8 * 3600 + 600, "B arrives 08:10");

    // A trip not in the day (this start does not exist) answers
    // empty, never an error.
    let absent = match dispatch(Request::TripStops {
        network_hash: network_hash.clone(),
        edits_hash: None,
        schedule: route10_schedule(&net),
        start_secs: 123,
        date: DATE.into(),
    })
    .unwrap()
    {
        Response::TripStops { stops } => stops,
        _ => panic!("trip_stops"),
    };
    assert!(absent.is_empty(), "no such trip in the day");

    // The patched day: the diverted via-C trip keeps A→B, C skipped,
    // the added time on the kept segment (B at 08:55).
    let (edits_hash, _) = load_edits(&network_hash, vec![record(EditStatus::Accepted, 300)]);
    let patched = match dispatch(Request::TripStops {
        network_hash: network_hash.clone(),
        edits_hash: Some(edits_hash),
        schedule: via_c,
        start_secs: 8 * 3600 + 40 * 60,
        date: DATE.into(),
    })
    .unwrap()
    {
        Response::TripStops { stops } => stops,
        _ => panic!("trip_stops"),
    };
    assert_eq!(
        patched.iter().map(|s| s.stop_id.as_str()).collect::<Vec<_>>(),
        ["A", "B"],
        "the effective trip answers its patched sequence"
    );
    assert_eq!(patched[1].arrival_secs, 8 * 3600 + 55 * 60, "B arrives 08:55 (08:50 + 300 s)");
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
    let err = dispatch(Request::BlockTrips {
        network_hash,
        edits_hash: Some("sha256:missing".into()),
        block_id: "B1".into(),
        date: DATE.into(),
        tight_layover_secs: None,
    })
    .unwrap_err();
    assert!(err.contains("unknown edits"), "{err}");

    let bad = dispatch_json(r#"{"op":"nope"}"#);
    assert!(bad.contains("invalid request"), "{bad}");
    let bad = dispatch_json(r#"{"op":"blocks","network_hash":"sha256:x","date":"2026-13-99"}"#);
    assert!(bad.contains("invalid date"), "{bad}");
    let bad = dispatch_bytes(0, b"whatever");
    assert!(bad.contains("unknown bulk op"), "the module takes JSON documents: {bad}");
}

#[test]
fn the_trip_line_is_the_effective_day_drawn() {
    // The fixture authors no paths: a trip draws its stops joined,
    // and a trip that does not run draws nothing.
    let net = derived_network();
    let network_hash = load_network();
    let via_c = route10_schedule_of(&net, &["A", "C", "B"]);
    let path = |start_secs: u32| match dispatch(Request::TripPath {
        network_hash: network_hash.clone(),
        edits_hash: None,
        schedule: via_c.clone(),
        start_secs,
        date: DATE.into(),
    })
    .unwrap()
    {
        Response::TripPath { path } => path,
        _ => panic!("trip_path"),
    };
    let first_start = match &net.schedules[&via_c].schedule {
        transit_network::Schedule::ExactTimes { starts } => starts[0],
        _ => panic!("fixture uses exact times"),
    };
    assert_eq!(path(first_start).len(), 3, "A, C, B joined");
    assert!(path(1).is_empty());
}
