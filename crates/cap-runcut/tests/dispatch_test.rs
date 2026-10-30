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
    let feed = transit_codec::Feed::from_zip_bytes(FIXTURE).expect("fixture parses");
    transit_codec::import(&feed).network
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
    net.pattern_trips
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

/// QAF-3's fold, pinned at the seam: the day's fleet in one answer
/// — running blocks, the peak and when, and the hour bases. The
/// deep oracle law lives in transit-dispatch's prop suite; this
/// pins the numbers the fixture pins: B1 is one vehicle out
/// 08:00–08:30 (t10 + t20), platform = revenue + layover.
#[test]
fn the_fleet_summary_folds_the_day_into_one_answer() {
    let network_hash = load_network();

    let summary = match dispatch(Request::FleetSummary {
        network_hash: network_hash.clone(),
        edits_hash: None,
        date: DATE.into(),
    })
    .unwrap()
    {
        Response::FleetSummary { summary } => summary,
        _ => panic!("fleet_summary"),
    };
    assert_eq!(summary.blocks, 1, "only B1 runs");
    assert_eq!(summary.peak_vehicles, 1, "one vehicle is out at a time");
    assert_eq!(summary.peak_at_secs, Some(28800), "out from t10's 08:00");
    // Platform 08:00–08:30 = 1800 s; the trips run 600 s each
    // (revenue 1200 s); the rest is the layover (600 s).
    assert!((summary.platform_hours - 1800.0 / 3600.0).abs() < 1e-9, "platform hours");
    assert!((summary.revenue_hours - 1200.0 / 3600.0).abs() < 1e-9, "revenue hours");
    assert!((summary.layover_hours - 600.0 / 3600.0).abs() < 1e-9, "layover hours");
    assert!(
        (summary.platform_hours - summary.revenue_hours - summary.layover_hours).abs() < 1e-9,
        "platform = revenue + layover"
    );
    // B1 in outline (QAF-15): out 08:00–08:30, two trips, the
    // 10-minute layover its longest gap, from t10's 08:10 end.
    let b1 = &summary.spans[0];
    assert_eq!(
        (b1.block_id.as_str(), b1.trips, b1.first_start_secs, b1.span_secs, b1.longest_gap_secs, b1.longest_gap_at_secs),
        ("B1", 2, 28800, 1800, Some(600), Some(29400))
    );

    // A no-service day answers a zero fleet, never an error.
    let parked = match dispatch(Request::FleetSummary {
        network_hash,
        edits_hash: None,
        date: "2026-10-24".into(),
    })
    .unwrap()
    {
        Response::FleetSummary { summary } => summary,
        _ => panic!("fleet_summary"),
    };
    assert_eq!((parked.blocks, parked.peak_vehicles), (0, 0));
    assert_eq!(parked.peak_at_secs, None);
    assert_eq!(parked.platform_hours, 0.0);
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
        live_hash: None,
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
        live_hash: None,
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
        live_hash: None,
    })
    .unwrap()
    {
        Response::BlockTrips { trips, .. } => trips,
        _ => panic!("block_trips"),
    };
    assert_eq!(ripped[0].layover_remaining_secs, Some(300), "the diverted end eats the layover (600 - 300)");
    // The chain rows carry the authored trip identity, model-native.
    assert_eq!(ripped[0].pattern_trips, route10_schedule(&net));
}

/// QAF-31 at the seam: a registered live document ripples the chain —
/// t10 predicted +370 s at its last stop leaves 230 s of the 600 s
/// layover before t20 (the dispatcher's "will it make its next
/// trip?"), and the trip says how late it ends.
#[test]
fn the_block_chain_ripples_under_a_live_prediction() {
    use transit_live::{LiveDay, LiveStatus, LiveStop, LiveTrip, OnTimeWindow};
    let network_hash = load_network();
    let chain = |live_hash: Option<String>| match dispatch(Request::BlockTrips {
        network_hash: network_hash.clone(),
        edits_hash: None,
        block_id: "B1".into(),
        date: DATE.into(),
        tight_layover_secs: None,
        live_hash,
    })
    .unwrap()
    {
        Response::BlockTrips { trips, .. } => trips,
        _ => panic!("block_trips"),
    };
    let first = chain(None).remove(0);
    let live = LiveDay {
        window: OnTimeWindow::default(),
        trips: vec![LiveTrip {
            trip: first.trip.clone(),
            start_secs: first.start_secs,
            route: None,
            route_name: None,
            worst_delay_secs: Some(370),
            status: LiveStatus::Late,
            stops: vec![LiveStop {
                stop: first.last_stop.clone(),
                scheduled_departure: first.end_secs,
                arrival_delay: Some(370),
                departure_delay: Some(370),
                predicted_departure: first.end_secs.map(|e| e + 370),
            }],
        }],
        vehicles: vec![],
        unmatched_trips: 0,
        alerts: vec![],
        unmatched_alerts: 0,
    };
    let live_hash = match dispatch(Request::LoadLive { live }).unwrap() {
        Response::LoadLive { live_hash } => live_hash,
        _ => panic!("load_live"),
    };
    let rippled = chain(Some(live_hash));
    assert_eq!(rippled[0].live_end_delay_secs, Some(370));
    assert_eq!(rippled[0].layover_remaining_secs, Some(230));
    assert_eq!(rippled[0].layover_health, Some(transit_dispatch::LayoverHealth::Tight));

    let err = dispatch(Request::BlockTrips {
        network_hash: network_hash.clone(),
        edits_hash: None,
        block_id: "B1".into(),
        date: DATE.into(),
        tight_layover_secs: None,
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
    net.pattern_trips
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
    // The map's click-through question: one trip instance's stops,
    // EFFECTIVE for the date (a diverted trip answers its patched
    // sequence). The answer carries no geometry — the network
    // document holds the coordinates; the map is the host's view.
    let _ = derived_network();
    let network_hash = load_network();

    // The authored day: t10's direct run A→B, times model-native.
    let stops = match dispatch(Request::TripStops {
        network_hash: network_hash.clone(),
        edits_hash: None,
        trip: "t10".into(),
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
        trip: "t10".into(),
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
        trip: "t30".into(),
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
        live_hash: None,
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
        trip: "t30".into(),
        start_secs,
        date: DATE.into(),
    })
    .unwrap()
    {
        Response::TripPath { path } => path,
        _ => panic!("trip_path"),
    };
    let first_start = net.pattern_trips[&via_c].trips[0].departs.first_start().expect("a start");
    assert_eq!(path(first_start).len(), 3, "A, C, B joined");
    assert!(path(1).is_empty());
}

#[test]
fn the_kernel_bulk_op_registers_the_same_identity_as_load_network() {
    // The bare document on the kernel's bulk op — parsed straight into
    // the model, no request envelope — answers exactly what the JSON
    // op answers; garbage bytes are an error outcome, never a trap.
    let doc = include_str!("../../../testdata/network.json");
    let via_json: serde_json::Value =
        serde_json::from_str(&cap_runcut::dispatch_json(&format!(r#"{{"op":"load_network","network":{doc}}}"#))).unwrap();
    let via_bytes: serde_json::Value =
        serde_json::from_str(&cap_runcut::dispatch_bytes(cap_abi::OP_LOAD_NETWORK, doc.as_bytes())).unwrap();
    assert!(via_json["ok"]["network_hash"].as_str().is_some_and(|h| h.starts_with("sha256:")), "{via_json}");
    assert_eq!(via_bytes, via_json);
    let bad = cap_runcut::dispatch_bytes(cap_abi::OP_LOAD_NETWORK, b"{not a network");
    assert!(bad.contains("invalid document"), "{bad}");
}
