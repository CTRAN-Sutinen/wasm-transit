//! Native dispatch tests for cap-runcut (`OPS.md`). They pin:
//!
//! 1. the network document's identity is the module's canonical serialization;
//! 2. the blocks list holds only blocks with running trips, with that day's
//!    counts;
//! 3. one block's chain is the authored runcut and ripples under the edit;
//! 4. an invalid edit record is reported at load and not rejected;
//! 5. an unknown hash or garbage input is an error and never a panic.

use cap_runcut::{dispatch, dispatch_bytes, dispatch_json, Request, Response};
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

/// The test reads the schedule id from the model and does not rely on the
/// codec's naming.
fn route10_schedule(net: &Network) -> String {
    let date = transit_date::Date::parse(DATE).unwrap();
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

#[test]
fn the_network_identity_is_the_modules_canonical_serialization() {
    let hash = load_network();
    let canonical = cap_abi::canonical_bytes(&derived_network()).unwrap();
    let expect = format!("sha256:{:x}", sha2::Sha256::digest(canonical.as_bytes()));
    assert_eq!(hash, expect);
    assert_eq!(load_network(), hash);
}

#[test]
fn the_days_blocks_are_the_index_facet_of_the_chain() {
    let network_hash = load_network();

    // B2's only trip is single-stop and the import drops it, so the list does
    // not offer B2.
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
    assert!((summary.platform_hours - 1800.0 / 3600.0).abs() < 1e-9, "platform hours");
    assert!((summary.revenue_hours - 1200.0 / 3600.0).abs() < 1e-9, "revenue hours");
    assert!((summary.layover_hours - 600.0 / 3600.0).abs() < 1e-9, "layover hours");
    assert!(
        (summary.platform_hours - summary.revenue_hours - summary.layover_hours).abs() < 1e-9,
        "platform = revenue + layover"
    );
    let b1 = &summary.spans[0];
    assert_eq!(
        (b1.block_id.as_str(), b1.trips, b1.first_start_secs, b1.span_secs, b1.longest_gap_secs, b1.longest_gap_at_secs),
        ("B1", 2, 28800, 1800, Some(600), Some(29400))
    );

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

    let edits_hash = load_edits(&network_hash, vec![record(EditStatus::Accepted, 300)]);
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
    assert_eq!(ripped[0].pattern_trips, route10_schedule(&net));
}

/// t10 is predicted +370 s at its last stop, so it leaves 230 s of the 600 s
/// layover before t20.
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
    let live = LiveDay { kind: Default::default(), schema_version: Default::default(),
        window: OnTimeWindow::default(),
        trips: vec![LiveTrip { date: None,
            trip: first.trip.clone(),
            start_secs: first.start_secs,
            route: None,
            route_name: None,
            worst_delay_secs: Some(370),
            status: LiveStatus::Late,
            canceled: false,
            stops: vec![LiveStop {
                stop: first.last_stop.clone(),
                scheduled_departure: first.end_secs,
                arrival_delay: Some(370),
                departure_delay: Some(370),
                predicted_departure: first.end_secs.map(|e| e + 370),
                skipped: false,
            }],
        }],
        vehicles: vec![],
        unmatched_trips: 0,
        alerts: vec![],
        unmatched_alerts: 0,
    };
    let live_hash = cap_abi::kernel::kind::<transit_live::LiveDay>().register(live).unwrap().0;
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

/// t10 and t11 share route and stop sequence. The service type tells them
/// apart, and only WK runs on the fixture Friday.
fn route10_schedule_of(net: &Network, seq: &[&str]) -> String {
    let date = transit_date::Date::parse(DATE).unwrap();
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
    let _ = derived_network();
    let network_hash = load_network();

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

    // The via-C trip keeps A→B with the added time on the kept segment (B at
    // 08:55).
    let edits_hash = load_edits(&network_hash, vec![record(EditStatus::Accepted, 300)]);
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
    assert!(err.message.contains("unknown edits"), "{err}");

    let bad = dispatch_json(r#"{"op":"nope"}"#);
    assert!(bad.contains("invalid request"), "{bad}");
    let bad = dispatch_json(r#"{"op":"blocks","network_hash":"sha256:x","date":"2026-13-99"}"#);
    assert!(bad.contains("invalid date"), "{bad}");
    let bad = dispatch_bytes(0, b"whatever");
    assert!(bad.contains("unknown bulk op"), "the module takes JSON documents: {bad}");
}

#[test]
fn the_trip_line_is_the_effective_day_drawn() {
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
    let doc = include_str!("../../../testdata/network.json");
    let via_json: serde_json::Value =
        serde_json::from_str(&cap_runcut::invoke(&format!(r#"{{"op":"load_network","network":{doc}}}"#))).unwrap();
    let via_bytes: serde_json::Value =
        serde_json::from_str(&cap_runcut::invoke_bytes(<transit_network::Network as cap_abi::kernel::Load>::CODE, doc.as_bytes())).unwrap();
    assert!(via_json["ok"]["network_hash"].as_str().is_some_and(|h| h.starts_with("sha256:")), "{via_json}");
    assert_eq!(via_bytes, via_json);
    let bad = cap_runcut::invoke_bytes(<transit_network::Network as cap_abi::kernel::Load>::CODE, b"{not a network");
    assert!(bad.contains("invalid document"), "{bad}");
}

fn load_rules(min_secs: u32) -> String {
    let rules = transit_runs::SchedulingConstraints {
        recovery: vec![transit_runs::RecoveryRule { route: None, service_type: None, min_secs: Some(min_secs), pct_of_run: None, extra: Default::default() }],
        ..Default::default()
    };
    let answer = cap_abi::kernel::invoke(
        &[&cap_runcut::CAPABILITY],
        &serde_json::json!({ "op": "load_scheduling_constraints", "scheduling_constraints": rules }).to_string(),
    );
    let value: serde_json::Value = serde_json::from_str(&answer).unwrap();
    assert_eq!(value["ok"]["version"], 1, "{answer}");
    value["ok"]["scheduling_constraints_hash"].as_str().unwrap().to_string()
}

#[test]
fn the_recovery_view_answers_what_a_wait_requires_and_its_slack() {
    let network_hash = load_network();
    let view = |scheduling_constraints_hash: Option<String>| match dispatch(Request::Recovery {
        network_hash: network_hash.clone(),
        edits_hash: None,
        date: DATE.into(),
        scheduling_constraints_hash,
    })
    .unwrap()
    {
        Response::Recovery { recovery } => recovery,
        _ => panic!("recovery"),
    };
    let floor = view(None);
    let w = &floor.waits[0];
    assert_eq!((w.block_id.as_str(), w.gap_secs, w.required_secs, w.slack_secs), ("B1", 600, Some(600), Some(0)));
    assert_eq!(w.required_by, Some(transit_network::RecoverySource::Terminal));
    let ruled = view(Some(load_rules(900)));
    let w = &ruled.waits[0];
    assert_eq!((w.required_secs, w.slack_secs), (Some(900), Some(-300)));
    assert_eq!(ruled.patterns[0].short, 1);
    let err = dispatch(Request::Recovery {
        network_hash,
        edits_hash: None,
        date: DATE.into(),
        scheduling_constraints_hash: Some("sha256:nope".into()),
    })
    .unwrap_err();
    assert!(err.message.contains("load_scheduling_constraints"), "{err}");
}

/// Adopting the answer's edits makes a document whose fleet peak is the
/// answer's.
#[test]
fn a_suggestion_adopts_into_the_fleet_it_names() {
    let mut cleared = derived_network();
    cleared.blocks.clear();
    let network_hash = cap_abi::kernel::kind::<transit_network::Network>().register(cleared.clone()).unwrap().0;
    let suggestion = match dispatch(Request::SuggestBlocks {
        network_hash,
        edits_hash: None,
        date: DATE.into(),
        scheduling_constraints_hash: None,
    })
    .unwrap()
    {
        Response::SuggestBlocks { suggestion } => suggestion,
        _ => panic!("suggest_blocks"),
    };
    let date = transit_date::Date::parse(DATE).unwrap();
    let day = transit_network::materialize(&cleared, date).unwrap();
    let suggested: usize = suggestion.blocks.iter().map(|b| b.trips.len()).sum();
    assert_eq!(suggested + suggestion.unblocked.len(), day.len());
    assert_eq!(suggestion.authored_peak_vehicles, 0);
    let adopted = transit_network::apply_edits(&cleared, &suggestion.edits).unwrap();
    assert_eq!(transit_network::validation_issues(&adopted), vec![]);
    let adopted_hash = cap_abi::kernel::kind::<transit_network::Network>().register(adopted).unwrap().0;
    let fleet = match dispatch(Request::FleetSummary { network_hash: adopted_hash, edits_hash: None, date: DATE.into() })
        .unwrap()
    {
        Response::FleetSummary { summary } => summary,
        _ => panic!("fleet_summary"),
    };
    assert_eq!(fleet.peak_vehicles, suggestion.peak_vehicles);
    assert_eq!(suggestion.added_vehicles, suggestion.peak_vehicles);
}
