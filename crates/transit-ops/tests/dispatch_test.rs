//! Native tests of the dispatch seam: no wasm involved — the exact
//! `Request → Response` path every host shares.

use std::collections::BTreeMap;
use std::io::Write;

use transit_ops::{dispatch, network_to_gtfs, Request, Response};
use prost::Message;
use transit_network::Schedule;
use transit_rt::gtfs_rt as rt;
use transit_sync::{EditRecord, EditStatus};

const DATE: &str = "2026-10-23"; // Friday: WK + HOL both run

/// The e2e fixture feed (see www/e2e/make_fixtures.py): stops A, B, C;
/// t10 A 08:00 -> B 08:10 (shape), t20 A 08:20 -> B 08:30, t30
/// A 08:40 -> C 08:45 -> B 08:50; SUN and HOL variants.
fn fixture_zip() -> Vec<u8> {
    let mut w = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let opts = zip::write::SimpleFileOptions::default();
    let add = |w: &mut zip::ZipWriter<std::io::Cursor<Vec<u8>>>, name: &str, body: &str| {
        w.start_file(name, opts).unwrap();
        w.write_all(body.as_bytes()).unwrap();
    };

    add(&mut w, "agency.txt", "agency_id,agency_name,agency_url,agency_timezone\n1,T,https://x,UTC\n");
    add(&mut w, "routes.txt", "route_id,agency_id,route_short_name,route_type\n10,1,10,3\n20,1,20,3\n");
    add(
        &mut w,
        "stops.txt",
        "stop_id,stop_name,stop_lat,stop_lon\nA,Main & 1st,45.500,-73.500\nB,Main & 2nd,45.510,-73.510\nC,Main & 3rd,45.520,-73.520\n",
    );
    add(
        &mut w,
        "trips.txt",
        "route_id,service_id,trip_id,direction_id,block_id,shape_id\n10,WK,t10,0,B1,sh1\n20,WK,t20,0,B1,\n10,WK,t30,0,,\n10,SUN,t11,0,,\n20,HOL,t12,0,B2,\n",
    );
    add(
        &mut w,
        "stop_times.txt",
        "trip_id,arrival_time,departure_time,stop_id,stop_sequence\nt10,08:00:00,08:00:00,A,1\nt10,08:10:00,08:10:00,B,2\nt20,08:20:00,08:20:00,A,1\nt20,08:30:00,08:30:00,B,2\nt30,08:40:00,08:40:00,A,1\nt30,08:45:00,08:45:00,C,2\nt30,08:50:00,08:50:00,B,3\nt11,09:00:00,09:00:00,A,1\nt11,09:10:00,09:10:00,B,2\nt12,07:30:00,07:30:00,B,1\n",
    );
    add(
        &mut w,
        "calendar.txt",
        "service_id,monday,tuesday,wednesday,thursday,friday,saturday,sunday,start_date,end_date\nWK,1,1,1,1,1,0,0,20260101,20261231\nSUN,0,0,0,0,0,0,1,20260101,20261231\n",
    );
    add(&mut w, "calendar_dates.txt", "service_id,date,exception_type\nHOL,20261023,1\n");
    add(
        &mut w,
        "shapes.txt",
        "shape_id,shape_pt_lat,shape_pt_lon,shape_pt_sequence\nsh1,45.400,-73.400,1\nsh1,45.500,-73.500,2\nsh1,45.510,-73.510,3\n",
    );
    add(
        &mut w,
        "feed_info.txt",
        "feed_publisher_name,feed_publisher_url,feed_lang,feed_version\nTest Transit,https://example.com,en,2026.10.1\n",
    );
    w.finish().unwrap().into_inner()
}

fn load() -> u32 {
    match dispatch(Request::LoadFeed { bytes: fixture_zip() }).unwrap() {
        Response::LoadFeed { feed_id, summary } => {
            assert_eq!(summary.stops, 3);
            assert_eq!(summary.trips, 5);
            feed_id
        }
        _ => panic!("expected LoadFeed response"),
    }
}

fn record(status: EditStatus, rev: u64) -> EditRecord {
    EditRecord {
        id: "e1".into(),
        diversion: transit_core::Diversion {
            id: "d1".into(),
            route_id: "10".into(),
            direction_id: None,
            first_kept_stop: "A".into(),
            last_kept_stop: "B".into(),
            skipped_stops: vec!["C".into()],
            detour: None,
            added_secs: 300,
            start_date: chrono::NaiveDate::parse_from_str(DATE, "%Y-%m-%d").unwrap(),
            end_date: chrono::NaiveDate::parse_from_str(DATE, "%Y-%m-%d").unwrap(),
        },
        status,
        rev,
        updated_at: 1,
    }
}

fn departures(feed_id: u32, stop: &str) -> Vec<String> {
    match dispatch(Request::Departures {
        feed_id,
        stop_id: stop.into(),
        date: DATE.into(),
        from_secs: 0,
        limit: 10,
    })
    .unwrap()
    {
        Response::Departures { departures } => {
            departures.into_iter().map(|d| format!("{}@{}", d.trip_id, d.departure_time_hhmmss)).collect()
        }
        _ => panic!("expected Departures response"),
    }
}

#[test]
fn load_registers_and_summarizes() {
    let feed_id = load();
    match dispatch(Request::Summary { feed_id }).unwrap() {
        Response::Summary { summary } => assert_eq!(summary.stop_times, 10),
        _ => panic!(),
    }
    // Unknown feed ids are an error, not a panic.
    assert!(dispatch(Request::Summary { feed_id: feed_id + 100 }).is_err());
}

#[test]
fn edit_document_flow_matches_the_wasm_smoke() {
    let feed_id = load();

    let base_b = departures(feed_id, "B");
    assert_eq!(
        base_b,
        vec!["t12@07:30:00", "t10@08:10:00", "t20@08:30:00", "t30@08:50:00"]
    );

    // Push the document: reports come back per record.
    let reports = match dispatch(Request::SetDiversions { feed_id, records: vec![record(EditStatus::Draft, 1)] })
        .unwrap()
    {
        Response::SetDiversions { reports } => reports,
        _ => panic!(),
    };
    assert_eq!(reports.len(), 1);
    assert!(reports[0].valid);
    assert_eq!(reports[0].affected_trips, 3);

    // The effective board reflects the edit; C loses its only service.
    assert_eq!(
        departures(feed_id, "B"),
        vec!["t12@07:30:00", "t10@08:15:00", "t20@08:30:00", "t30@08:55:00"]
    );
    assert!(departures(feed_id, "C").is_empty());

    // Plan compare: diverted arrival vs baseline.
    match dispatch(Request::PlanTripCompare {
        feed_id,
        from: "A".into(),
        to: "B".into(),
        date: DATE.into(),
        depart_secs: 27000,
        max_legs: 6,
    })
    .unwrap()
    {
        Response::PlanTripCompare { active, baseline } => {
            assert_eq!(active.as_ref().unwrap().arrival_hhmmss, "08:15:00");
            assert_eq!(baseline.as_ref().unwrap().arrival_hhmmss, "08:10:00");
        }
        _ => panic!(),
    }

    // Retire: tombstone, base behavior, no baseline in the compare.
    dispatch(Request::SetDiversions { feed_id, records: vec![record(EditStatus::Retired, 2)] }).unwrap();
    assert_eq!(departures(feed_id, "B"), base_b);
    assert_eq!(departures(feed_id, "C").len(), 1);
    match dispatch(Request::PlanTripCompare {
        feed_id,
        from: "A".into(),
        to: "B".into(),
        date: DATE.into(),
        depart_secs: 27000,
        max_legs: 6,
    })
    .unwrap()
    {
        Response::PlanTripCompare { active, baseline } => {
            assert!(baseline.is_none(), "no baseline when nothing is in force");
            assert_eq!(active.as_ref().unwrap().arrival_hhmmss, "08:10:00");
        }
        _ => panic!(),
    }
}

#[test]
fn invalid_records_are_reported_not_rejected() {
    let feed_id = load();
    let mut bad = record(EditStatus::Draft, 1);
    bad.diversion.route_id = "nope".into();
    let reports = match dispatch(Request::SetDiversions {
        feed_id,
        records: vec![record(EditStatus::Draft, 1), bad],
    })
    .unwrap()
    {
        Response::SetDiversions { reports } => reports,
        _ => panic!(),
    };
    assert_eq!(reports.len(), 2);
    assert!(reports[0].valid);
    assert!(!reports[1].valid);
    assert!(reports[1].error.as_deref().unwrap().contains("unknown route"));
    // The valid record still applies.
    assert!(departures(feed_id, "C").is_empty());
}

#[test]
fn geojson_ops_return_typed_features() {
    let feed_id = load();
    match dispatch(Request::StopsGeojson { feed_id }).unwrap() {
        Response::StopsGeojson { collection } => {
            assert_eq!(collection.kind, "FeatureCollection");
            assert_eq!(collection.features.len(), 3);
            assert_eq!(collection.features[0].properties.stop_id, "A");
        }
        _ => panic!(),
    }
    match dispatch(Request::TripPathGeojson { feed_id, trip_id: "t10".into(), date: DATE.into() })
        .unwrap()
    {
        Response::TripPathGeojson { feature } => {
            let f = feature.expect("t10 has a shape");
            assert_eq!(f.geometry.coordinates.len(), 3);
            assert_eq!(f.properties.route_name.as_deref(), Some("10"));
        }
        _ => panic!(),
    }
    // The route ghost: the representative trip's path — t30 is route
    // 10's longest trip (A→C→B, no shape), so the ghost is its chord
    // line; direction filtering narrows within the route.
    match dispatch(Request::RoutePathGeojson {
        feed_id,
        route_id: "10".into(),
        direction_id: None,
        date: DATE.into(),
    })
    .unwrap()
    {
        Response::RoutePathGeojson { feature } => {
            let f = feature.expect("route 10 has a representative");
            assert_eq!(f.properties.trip_id, "t30");
            assert_eq!(f.geometry.coordinates.len(), 3); // A → C → B
        }
        _ => panic!(),
    }
    match dispatch(Request::RoutePathGeojson {
        feed_id,
        route_id: "10".into(),
        direction_id: Some(0),
        date: DATE.into(),
    })
    .unwrap()
    {
        Response::RoutePathGeojson { feature } => {
            assert!(feature.is_some(), "t30 is direction 0 — same trip");
        }
        _ => panic!(),
    }
    match dispatch(Request::RoutePathGeojson {
        feed_id,
        route_id: "no-such-route".into(),
        direction_id: None,
        date: DATE.into(),
    })
    .unwrap()
    {
        Response::RoutePathGeojson { feature } => assert!(feature.is_none()),
        _ => panic!(),
    }
}

#[test]
fn decode_rt_reports_delays_and_vehicles() {
    // A minimal GTFS-rt feed, encoded here with the same generated
    // prost types the decoder uses: t10 delayed +120 s at A, plus one
    // vehicle on t10.
    let msg = rt::FeedMessage {
        header: rt::FeedHeader {
            gtfs_realtime_version: "2.0".into(),
            timestamp: Some(1_796_000_000),
            ..Default::default()
        },
        entity: vec![
            rt::FeedEntity {
                id: "tu1".into(),
                trip_update: Some(rt::TripUpdate {
                    trip: rt::TripDescriptor { trip_id: Some("t10".into()), ..Default::default() },
                    stop_time_update: vec![rt::trip_update::StopTimeUpdate {
                        stop_id: Some("A".into()),
                        departure: Some(rt::trip_update::StopTimeEvent { delay: Some(120), ..Default::default() }),
                        ..Default::default()
                    }],
                    ..Default::default()
                }),
                ..Default::default()
            },
            rt::FeedEntity {
                id: "vp1".into(),
                vehicle: Some(rt::VehiclePosition {
                    trip: Some(rt::TripDescriptor { trip_id: Some("t10".into()), ..Default::default() }),
                    position: Some(rt::Position {
                        latitude: 45.44,
                        longitude: -73.44,
                        bearing: Some(90.0),
                        ..Default::default()
                    }),
                    stop_id: Some("A".into()),
                    // The descriptor carries the bus number: internal
                    // id and the rider-visible label.
                    vehicle: Some(rt::VehicleDescriptor {
                        id: Some("4201".into()),
                        label: Some("4201".into()),
                        ..Default::default()
                    }),
                    ..Default::default()
                }),
                ..Default::default()
            },
        ],
    };
    let bytes = msg.encode_to_vec();

    match dispatch(Request::DecodeRt { bytes }).unwrap() {
        Response::DecodeRt { overview } => {
            assert_eq!(overview.gtfs_realtime_version.as_deref(), Some("2.0"));
            assert_eq!(overview.trip_updates, 1);
            assert_eq!(overview.vehicles, 1);
            assert_eq!(overview.trips[0].trip_id.as_deref(), Some("t10"));
            assert_eq!(overview.trips[0].stop_updates[0].departure_delay, Some(120));
            // Position lat/lon are f32 on the wire (GTFS-rt spec).
            assert_eq!(overview.vehicle_positions[0].lat, 45.44f32 as f64);
            assert_eq!(overview.vehicle_positions[0].stop_id.as_deref(), Some("A"));
            assert_eq!(overview.vehicle_positions[0].vehicle_id.as_deref(), Some("4201"));
            assert_eq!(overview.vehicle_positions[0].label.as_deref(), Some("4201"));
        }
        _ => panic!(),
    }

    // Garbage bytes decode to an error, not a panic.
    assert!(dispatch(Request::DecodeRt { bytes: vec![0xff, 0xff] }).is_err());
}

#[test]
fn blocks_op_lists_all_blocks_or_only_the_days() {
    let feed_id = load();

    // Without a date: every block in the feed, full trip counts.
    let Response::Blocks { blocks } = dispatch(Request::Blocks { feed_id, date: None }).unwrap() else {
        panic!("expected Blocks response");
    };
    let ids: Vec<(&str, usize)> = blocks
        .iter()
        .map(|b| (b.block_id.as_str(), b.trip_count))
        .collect();
    assert_eq!(ids, vec![("B1", 2), ("B2", 1)]);

    // With Friday's date: both run (B1's WK trips, B2's holiday trip).
    let Response::Blocks { blocks } =
        dispatch(Request::Blocks { feed_id, date: Some(DATE.into()) }).unwrap() else {
            panic!("expected Blocks response");
        };
    let ids: Vec<(&str, usize)> = blocks
        .iter()
        .map(|b| (b.block_id.as_str(), b.trip_count))
        .collect();
    assert_eq!(ids, vec![("B1", 2), ("B2", 1)]);

    // Sunday: WK is off and the holiday is Friday-only — every bus is
    // parked, so the picker has nothing to offer.
    let Response::Blocks { blocks } = dispatch(Request::Blocks {
        feed_id,
        date: Some("2026-10-25".into()),
    })
    .unwrap() else {
        panic!("expected Blocks response");
    };
    assert!(blocks.is_empty());

    // A bad date is an error, not a panic.
    assert!(dispatch(Request::Blocks { feed_id, date: Some("2026-13-99".into()) }).is_err());
}

#[test]
fn request_serializes_as_tagged_json() {
    // The protocol has a textual form for hosts that want one —
    // serde-tagged enums make the "op" naming rule explicit.
    let json = serde_json::to_value(
        Request::Departures { feed_id: 0, stop_id: "A".into(), date: "2026-10-23".into(), from_secs: 0, limit: 10 },
    )
    .unwrap();
    assert_eq!(json["op"], "departures");
    assert_eq!(json["stop_id"], "A");
    let response = serde_json::to_value(Response::LoadFeed {
        feed_id: 0,
        summary: transit_core::FeedSummary {
            agencies: 1,
            routes: 2,
            stops: 3,
            trips: 5,
            stop_times: 10,
        },
    })
    .unwrap();
    assert_eq!(response["op"], "load_feed");
}

#[test]
fn dispatch_json_round_trips_the_seam() {
    let feed_id = load();

    // A request in, an outcome out — the path hosts without a native
    // Rust ABI (browser, Go server) actually take.
    let outcome = transit_ops::dispatch_json(
        &serde_json::json!({
            "op": "departures",
            "feed_id": feed_id,
            "stop_id": "B",
            "date": DATE,
            "from_secs": 0,
            "limit": 10,
        })
        .to_string(),
    );
    let parsed: serde_json::Value = serde_json::from_str(&outcome).unwrap();
    assert!(parsed.get("ok").is_some(), "successful outcome wraps ok: {outcome}");
    let deps = &parsed["ok"]["departures"];
    assert_eq!(deps.as_array().unwrap().len(), 4);
    assert_eq!(deps[1]["trip_id"], "t10");

    // The feed_info op surfaces artifact metadata for the manifest.
    let outcome = transit_ops::dispatch_json(
        &serde_json::json!({ "op": "feed_info", "feed_id": feed_id }).to_string(),
    );
    let parsed: serde_json::Value = serde_json::from_str(&outcome).unwrap();
    assert_eq!(parsed["ok"]["info"]["feed_version"], "2026.10.1");

    // Errors come back as {"error": ...}, not panics.
    let outcome = transit_ops::dispatch_json(
        &serde_json::json!({ "op": "departures", "feed_id": 99, "stop_id": "B", "date": DATE, "from_secs": 0, "limit": 10 }).to_string(),
    );
    let parsed: serde_json::Value = serde_json::from_str(&outcome).unwrap();
    assert!(parsed["error"].as_str().unwrap().contains("unknown feed_id"));

    // Malformed requests report, not trap.
    let outcome = transit_ops::dispatch_json("not json at all");
    let parsed: serde_json::Value = serde_json::from_str(&outcome).unwrap();
    assert!(parsed["error"].as_str().unwrap().contains("invalid request"));

    // Bytes ops: the code-driven form for bulk payloads.
    let outcome = transit_ops::dispatch_bytes(transit_ops::OP_LOAD_FEED, &fixture_zip());
    let parsed: serde_json::Value = serde_json::from_str(&outcome).unwrap();
    assert!(parsed["ok"].is_object());
    let outcome = transit_ops::dispatch_bytes(77, &[]);
    let parsed: serde_json::Value = serde_json::from_str(&outcome).unwrap();
    assert!(parsed["error"].as_str().unwrap().contains("unknown bytes op"));
}

#[test]
fn sync_edits_merges_submits_and_reports() {
    let feed_id = load();

    // Client arrives with a draft; the server already holds a newer
    // tombstone for one record and nothing for the other.
    let mut draft = record(EditStatus::Draft, 1);
    draft.id = "e-new".into();
    let mut retired = record(EditStatus::Accepted, 3);
    retired.id = "e-old".into();
    retired.status = EditStatus::Retired;
    retired.updated_at = 2;

    let mut stale_local = retired.clone();
    stale_local.status = EditStatus::Accepted;
    stale_local.updated_at = 1;

    match dispatch(Request::SyncEdits {
        feed_id: Some(feed_id),
        client: vec![draft, stale_local],
        server: vec![retired],
        now_ms: 1_000,
    })
    .unwrap()
    {
        Response::SyncEdits { records, reports } => {
            // Union of ids, sorted; the tombstone (newer timestamp)
            // won over the stale accepted version.
            let ids: Vec<&str> = records.iter().map(|r| r.id.as_str()).collect();
            assert_eq!(ids, vec!["e-new", "e-old"]);
            assert_eq!(records[0].status, EditStatus::Submitted, "drafts submit");
            assert_eq!(records[0].rev, 2, "submit bumps rev");
            assert_eq!(records[0].updated_at, 1_000, "submit stamps the host clock");
            assert_eq!(records[1].status, EditStatus::Retired, "tombstone propagates");

            // The valid draft reports against the feed.
            assert_eq!(reports.len(), 2);
            assert!(reports.iter().all(|r| r.valid), "fixture-valid records: {reports:?}");
        }
        _ => panic!("expected SyncEdits response"),
    }

    // No feed: no reports, records still merge and submit.
    match dispatch(Request::SyncEdits {
        feed_id: None,
        client: vec![],
        server: vec![record(EditStatus::Draft, 1)],
        now_ms: 5,
    })
    .unwrap()
    {
        Response::SyncEdits { records, reports } => {
            assert_eq!(records.len(), 1);
            assert_eq!(records[0].status, EditStatus::Submitted);
            assert!(reports.is_empty(), "no feed, no reports");
        }
        _ => panic!(),
    }
}

// ---------------------------------------------------------------------
// Authored network documents (transit-network) through the same seam
// ---------------------------------------------------------------------

/// The shared network fixture (testdata/network.json — the corpus
/// generator and the ABI smoke read the same file): A→B→C pattern,
/// weekday calendar with one removed date, three exact trips, one
/// two-trip block.
const NETWORK_DOC: &str = include_str!("../../../testdata/network.json");

fn network_load() -> u32 {
    let outcome = transit_ops::dispatch_json(&format!(
        r#"{{"op":"network_load","network":{NETWORK_DOC}}}"#
    ));
    let parsed: serde_json::Value = serde_json::from_str(&outcome).unwrap();
    assert!(parsed.get("ok").is_some(), "fixture network must load: {outcome}");
    parsed["ok"]["feed_id"].as_u64().unwrap() as u32
}

#[test]
fn network_validate_reports_issues_without_loading() {
    // The valid fixture: no issues.
    let outcome = transit_ops::dispatch_json(&format!(
        r#"{{"op":"network_validate","network":{NETWORK_DOC}}}"#
    ));
    let parsed: serde_json::Value = serde_json::from_str(&outcome).unwrap();
    assert_eq!(parsed["ok"]["issues"].as_array().unwrap().len(), 0);

    // Broken: a service points at a calendar that doesn't exist, and
    // the headway is zero. Every issue comes back, loadable elsewhere.
    let broken = NETWORK_DOC
        .replace("\"service_type\": \"weekday\"", "\"service_type\": \"ghost\"")
        + ""; // one defect is enough; see prop_test for the battery
    let outcome = transit_ops::dispatch_json(&format!(
        r#"{{"op":"network_validate","network":{broken}}}"#
    ));
    let parsed: serde_json::Value = serde_json::from_str(&outcome).unwrap();
    let issues = parsed["ok"]["issues"].as_array().unwrap();
    assert_eq!(issues.len(), 1, "one referential defect: {issues:?}");
    assert!(
        issues[0].get("unknown_service_type").is_some(),
        "snake_case NetworkError: {issues:?}"
    );
}

#[test]
fn network_load_rejects_invalid_documents() {
    let invalid = NETWORK_DOC.replace("\"stop\": \"B\"", "\"stop\": \"ghost\"");
    let outcome = transit_ops::dispatch_json(&format!(
        r#"{{"op":"network_load","network":{invalid}}}"#
    ));
    let parsed: serde_json::Value = serde_json::from_str(&outcome).unwrap();
    let error = parsed["error"].as_str().unwrap();
    assert!(error.contains("invalid network"), "{error}");
}

#[test]
fn network_document_drives_the_whole_op_surface() {
    let feed_id = network_load();

    // Summary: 3 stops, 1 route, 3 trips, 9 stop_times.
    let summary = match dispatch(Request::Summary { feed_id }).unwrap() {
        Response::Summary { summary } => summary,
        _ => panic!("summary"),
    };
    assert_eq!((summary.stops, summary.routes, summary.trips, summary.stop_times), (3, 1, 3, 9));

    // The calendar rule (weekday, minus 2026-11-26) becomes explicit
    // running dates: Friday 2026-10-23 runs, the removed day doesn't.
    let services = match dispatch(Request::ActiveServices { feed_id, date: "2026-10-23".into() }).unwrap() {
        Response::ActiveServices { services } => services,
        _ => panic!("active_services"),
    };
    assert_eq!(services, vec!["10-weekday".to_string()]);
    let none = match dispatch(Request::ActiveServices { feed_id, date: "2026-11-26".into() }).unwrap() {
        Response::ActiveServices { services } => services,
        _ => panic!("active_services"),
    };
    assert!(none.is_empty(), "the Removed exception date has no service");

    // Departures at A: the derived trip ids are "<service>@<start>".
    let deps = match dispatch(Request::Departures {
        feed_id,
        stop_id: "A".into(),
        date: "2026-10-23".into(),
        from_secs: 0,
        limit: 10,
    })
    .unwrap()
    {
        Response::Departures { departures } => departures,
        _ => panic!("departures"),
    };
    assert_eq!(deps.len(), 3);
    assert_eq!(deps[0].trip_id, "10-weekday@28800");
    assert_eq!(deps[0].service_id, "10-weekday");
    // Interpolation: B (midpoint of a 600s run) is 5 minutes in.
    let deps_b = match dispatch(Request::Departures {
        feed_id,
        stop_id: "B".into(),
        date: "2026-10-23".into(),
        from_secs: 0,
        limit: 10,
    })
    .unwrap()
    {
        Response::Departures { departures } => departures,
        _ => panic!("departures"),
    };
    assert_eq!(deps_b[0].departure_time, Some(28800 + 300));

    // The runcut: block 400 chains both trips. The derived view shows
    // the realized gap (20 min: 15 min authored layover + 5 min run
    // slack) — the authored layover minimum is a constraint, not a
    // time, and GTFS-shaped views can only carry the gap.
    let block = match dispatch(Request::BlockTrips {
        feed_id,
        block_id: "400".into(),
        date: "2026-10-23".into(),
    })
    .unwrap()
    {
        Response::BlockTrips { trips } => trips,
        _ => panic!("block_trips"),
    };
    assert_eq!(block.len(), 2);
    assert_eq!(block[1].layover_secs, Some(1200));

    // Planning: A→C on the planner's own document, 1 leg.
    let plan = match dispatch(Request::PlanTrip {
        feed_id,
        from: "A".into(),
        to: "C".into(),
        date: "2026-10-23".into(),
        depart_secs: 28800,
        max_legs: 3,
    })
    .unwrap()
    {
        Response::PlanTrip { itinerary } => itinerary,
        _ => panic!("plan_trip"),
    };
    let itinerary = plan.expect("A→C is plannable");
    assert_eq!(itinerary.legs.len(), 1);
    match &itinerary.legs[0] {
        transit_router::Leg::Transit { trip_id, alight_secs, .. } => {
            assert_eq!(trip_id, "10-weekday@28800");
            // C is a timepoint: the full 600s run.
            assert_eq!(*alight_secs, 28800 + 600);
        }
        leg => panic!("expected a transit leg, got {leg:?}"),
    }
}

#[test]
fn network_import_infers_a_valid_document_from_a_feed() {
    let feed_id = load();
    let network = match dispatch(Request::NetworkImport { feed_id }).unwrap() {
        Response::NetworkImport { network, .. } => network,
        _ => panic!("network_import"),
    };
    // The inference must produce a document the model accepts — that
    // is the contract that lets the planner edit it immediately.
    assert!(
        network.validate().is_ok(),
        "inferred document must validate: {:?}",
        transit_network::validation_issues(&network)
    );

    // Fixture shape (see fixture_zip): 3 stops, 2 routes, three
    // two-plus-stop patterns (t12 is a single-stop trip — dropped by
    // design), two calendars (HOL served only t12), four services.
    assert_eq!(network.stops.len(), 3);
    assert_eq!(network.routes.len(), 2);
    assert_eq!(network.patterns.len(), 3);
    assert_eq!(network.service_types.len(), 2);
    assert!(network.service_types.contains_key("WK"));
    assert!(network.service_types.contains_key("SUN"));
    assert_eq!(network.schedules.len(), 4);

    // Route 10's A→B pattern: exact starts 08:00 (WK, t10) and
    // 09:00 (SUN, t11) — one service per (service key, pattern).
    let svc = network.schedules.get("WK:pat-10-0-0").expect("service id scheme");
    assert_eq!(svc.schedule, Schedule::ExactTimes { starts: vec![8 * 3600] });
    let pattern = &network.patterns[&svc.pattern];
    assert_eq!(pattern.stops.len(), 2);
    assert!(pattern.stops.iter().all(|s| s.timepoint));

    // The alignment survives inference: t10's sh1 (3 points) is the
    // pattern's path; patterns whose trips carry no shapes have none.
    let path = pattern.path.as_ref().expect("sh1 becomes the pattern path");
    assert_eq!(path.len(), 3);
    assert_eq!((path[0].lat, path[0].lon), (45.400, -73.400));
    let unshaped: Vec<&String> = network
        .patterns
        .iter()
        .filter(|(_, p)| p.path.is_none())
        .map(|(id, _)| id)
        .collect();
    assert_eq!(unshaped.len(), 2, "only pat-10-0-0 has a path: {unshaped:?}");

    // The runcut survives: B1 chains t10 and t20 chronologically.
    let block = network.blocks.get("B1").expect("block B1 kept");
    assert_eq!(block.trips.len(), 2);
    assert_eq!(block.trips[0].start_secs, 8 * 3600);
    assert_eq!(block.trips[0].schedule, "WK:pat-10-0-0");
    assert!(block.layovers.is_empty(), "GTFS carries no layover data");
}

/// A production-shaped feed (see the regression tests below): trip
/// ids that do not sort by start time, a duplicated start, and
/// per-trip run-time variation inside one pattern. These shapes are
/// routine in real agency feeds (found importing C-TRAN) and the
/// inference must still produce a valid document — "import is valid
/// by construction" is the contract that lets the planner edit a
/// derived network immediately.
fn production_zip() -> Vec<u8> {
    let mut w = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let opts = zip::write::SimpleFileOptions::default();
    let add = |w: &mut zip::ZipWriter<std::io::Cursor<Vec<u8>>>, name: &str, body: &str| {
        w.start_file(name, opts).unwrap();
        w.write_all(body.as_bytes()).unwrap();
    };

    add(&mut w, "agency.txt", "agency_id,agency_name,agency_url,agency_timezone\n1,T,https://x,UTC\n");
    add(&mut w, "routes.txt", "route_id,agency_id,route_short_name,route_type\n10,1,10,3\n");
    add(
        &mut w,
        "stops.txt",
        "stop_id,stop_name,stop_lat,stop_lon\nA,Main & 1st,45.500,-73.500\nB,Main & 2nd,45.510,-73.510\nC,Main & 3rd,45.520,-73.520\n",
    );
    // Lexical trip-id order is NOT start-time order (t1 < t3 < t5 <
    // t9), t1 and t3 duplicate the 08:00 start, and the pattern's
    // earliest trip (t9) runs 10 minutes slower than the rest.
    add(
        &mut w,
        "trips.txt",
        "route_id,service_id,trip_id,direction_id,block_id\n10,WK,t1,0,B1\n10,WK,t3,0,\n10,WK,t5,0,B1\n10,WK,t9,0,B1\n10,WK,t7,0,\n",
    );
    add(
        &mut w,
        "stop_times.txt",
        // t9 (the pattern's earliest trip) runs A→C in 30 minutes;
        // t1/t3/t5 run it in 20. The block chain is feasible against
        // every trip's own times (t1 arrives 08:20, t5 departs 08:22).
        "trip_id,arrival_time,departure_time,stop_id,stop_sequence\n\
         t9,06:00:00,06:00:00,A,1\nt9,06:10:00,06:10:00,B,2\nt9,06:30:00,06:30:00,C,3\n\
         t1,08:00:00,08:00:00,A,1\nt1,08:08:00,08:08:00,B,2\nt1,08:20:00,08:20:00,C,3\n\
         t3,08:00:00,08:00:00,A,1\nt3,08:08:00,08:08:00,B,2\nt3,08:20:00,08:20:00,C,3\n\
         t5,08:22:00,08:22:00,A,1\nt5,08:30:00,08:30:00,B,2\nt5,08:42:00,08:42:00,C,3\n\
         t7,09:00:00,09:00:00,A,1\nt7,09:10:00,09:10:00,B,2\n",
    );
    add(
        &mut w,
        "calendar.txt",
        "service_id,monday,tuesday,wednesday,thursday,friday,saturday,sunday,start_date,end_date\nWK,1,1,1,1,1,0,0,20260101,20261231\n",
    );
    w.finish().unwrap().into_inner()
}

fn import_production() -> transit_network::Network {
    let feed_id = match dispatch(Request::LoadFeed { bytes: production_zip() }).unwrap() {
        Response::LoadFeed { feed_id, .. } => feed_id,
        _ => panic!("expected LoadFeed response"),
    };
    match dispatch(Request::NetworkImport { feed_id }).unwrap() {
        Response::NetworkImport { network, .. } => network,
        _ => panic!("expected NetworkImport response"),
    }
}

#[test]
fn network_import_sorts_and_dedupes_exact_starts() {
    // Regression (C-TRAN): members arrive in trip-id order and real
    // feeds carry duplicate starts; the model requires sorted,
    // unique starts. t1/t3 duplicate 08:00 and t9's 06:00 sorts
    // after them in trip-id order.
    let network = import_production();
    let svc = network.schedules.get("WK:pat-10-0-1").expect("service");
    assert_eq!(
        svc.schedule,
        Schedule::ExactTimes { starts: vec![6 * 3600, 8 * 3600, 8 * 3600 + 22 * 60] }
    );
}

#[test]
fn network_import_keeps_feasible_blocks_despite_runtime_variation() {
    // Regression (C-TRAN): the timing profile used to come from the
    // pattern's earliest trip; applying that trip's 30-minute
    // runtime to the later 20-minute trips pushed modeled arrivals
    // past the next block trip's start (187 issues on the real
    // feed, every derived network unloadable). The profile must
    // come from the shortest-running member, so connections that
    // hold in the source feed hold in the model.
    let network = import_production();
    assert!(
        network.validate().is_ok(),
        "inferred document must validate: {:?}",
        transit_network::validation_issues(&network)
    );
    let block = network.blocks.get("B1").expect("block B1 kept");
    assert_eq!(block.trips.len(), 3);
    // The model's arrival for t1 is start + the 20-minute runtime.
    let t1_arrival = transit_network::trip_arrival_end_secs(&network, "WK:pat-10-0-1", 8 * 3600)
        .expect("t1 arrival");
    assert_eq!(t1_arrival, 8 * 3600 + 20 * 60);
    assert!(t1_arrival <= block.trips[2].start_secs);
    // The whole point: the derived document loads back into the app.
    assert!(network_to_gtfs(&network).is_ok());
}

#[test]
fn network_stats_reports_route_and_stop_activity() {
    // The planner's read view: activity on one service date, from the
    // document (no materialization needed). On the production-shaped
    // fixture (DATE = Friday, WK runs): 4 trips survive the
    // duplicate-start dedup (t1/t3 share 08:00), two patterns, three
    // stops, every stop on route 10.
    let network = import_production();
    let stats = match dispatch(Request::NetworkStats { network, date: DATE.into() }).unwrap() {
        Response::NetworkStats { stats } => stats,
        _ => panic!("network_stats"),
    };
    assert_eq!(stats.date, DATE);
    assert_eq!(stats.trips, 4); // t9, t1, t5 (dedup), t7
    assert_eq!(stats.active_routes, 1);
    assert_eq!(stats.active_stops, 3);
    assert_eq!(stats.first_start_secs, Some(6 * 3600));
    assert_eq!(stats.last_arrival_secs, Some(9 * 3600 + 10 * 60));
    // Run times as the MODEL materializes them: every A→C trip gets
    // the shortest member's 1200 s profile (the lossy collapse), t7
    // runs 600 s — 4200 s total. t9's own 1800 s runtime is the
    // documented loss.
    assert!((stats.revenue_hours - 4200.0 / 3600.0).abs() < 1e-9);
    let route = &stats.routes[0];
    assert_eq!(route.route_id, "10");
    assert_eq!(route.trips, 4);
    assert_eq!(route.patterns, 2);
    assert_eq!(route.stops, 3);
    assert_eq!(route.first_start_secs, Some(6 * 3600));
    assert_eq!(route.last_arrival_secs, Some(9 * 3600 + 10 * 60));
    // A→C starts 06:00/08:00/08:22 → (30120-21600)/2 = 4260 s; the
    // A→B pattern has one trip, no headway signal.
    assert_eq!(route.typical_headway_secs, Some(4260));

    let by_id = |id: &str| stats.stops.iter().find(|s| s.stop_id == id).expect(id);
    let a = by_id("A");
    assert_eq!((a.departures, a.routes), (4, 1));
    assert_eq!(a.first_arrival_secs, Some(6 * 3600));
    assert_eq!(a.last_arrival_secs, Some(9 * 3600));
    assert_eq!(a.typical_headway_secs, Some((9 * 3600 - 6 * 3600) / 3));
    let c = by_id("C");
    assert_eq!(c.departures, 3);
    // C arrivals as materialized: 06:20/08:20/08:42 (the profile's
    // 1200 s offset — t9's original 06:30 is the documented loss).
    assert_eq!(c.typical_headway_secs, Some(((8 * 3600 + 42 * 60) - (6 * 3600 + 20 * 60)) / 2));
}

/// Three population points: on stop A, on stop C, and far from the
/// network. Property spelled "POP" (auto-detected on load).
fn pop_layer_geojson() -> Vec<u8> {
    br#"{"type":"FeatureCollection","features":[
        {"type":"Feature","geometry":{"type":"Point","coordinates":[-73.500,45.500]},
         "properties":{"POP":100}},
        {"type":"Feature","geometry":{"type":"Point","coordinates":[-73.520,45.520]},
         "properties":{"POP":50}},
        {"type":"Feature","geometry":{"type":"Point","coordinates":[-73.400,45.500]},
         "properties":{"POP":30}}
    ]}"#
    .to_vec()
}

#[test]
fn coverage_loads_and_measures_service_quality() {
    // The planner's opening questions, answered by the artifact:
    // what share of the population is near service, and what share
    // is near frequent service. Stop A runs 4 visits hourly (that
    // IS the frequent bar here: frequent_secs 3600, ≥4 visits);
    // stop C runs 3 — served, not frequent; the far point neither.
    let layer_id = match dispatch(Request::CoverageLoad { bytes: pop_layer_geojson(), pop_field: None })
        .unwrap()
    {
        Response::CoverageLoad { layer_id, points, pop_field, total_pop } => {
            assert_eq!(points, 3);
            assert_eq!(pop_field, "POP");
            assert!((total_pop - 180.0).abs() < 1e-9);
            layer_id
        }
        _ => panic!("coverage_load"),
    };
    let network = import_production();
    let stats = match dispatch(Request::CoverageStats {
        network,
        date: DATE.into(),
        layer_id,
        radius_m: 400.0,
        frequent_secs: 3600,
        min_departures: Some(4),
    })
    .unwrap()
    {
        Response::CoverageStats { stats } => stats,
        _ => panic!("coverage_stats"),
    };
    assert_eq!(stats.served_stops, 3);
    // Frequent bar: typical headway ≤ 3600 s on ≥ 4 visits. Stop A:
    // starts 06:00/08:00/08:22/09:00 → 3600 s exactly, qualifies.
    // Stop B: the materialized 480 s timepoint offset puts its
    // spread at 3640 s — served, not frequent. Stop C: 3 visits.
    assert_eq!(stats.frequent_stops, 1);
    // 100 (near A) + 50 (near C) of 180 — the far point is uncovered.
    assert!((stats.served_pct - 150.0 / 180.0 * 100.0).abs() < 1e-6);
    // Only the point near A (frequent); C is served but not frequent.
    assert!((stats.frequent_pct - 100.0 / 180.0 * 100.0).abs() < 1e-6);

    // Unknown layer ids are an error, not a panic.
    let network = import_production();
    assert!(dispatch(Request::CoverageStats {
        network,
        date: DATE.into(),
        layer_id: layer_id + 100,
        radius_m: 400.0,
        frequent_secs: 3600,
        min_departures: Some(4),
    })
    .is_err());
}

/// A feed carrying all four resolution shapes: an infeasible block
/// chain (departs before the vehicle arrives), two trips of one
/// block at the same second, the same trip claimed by two blocks,
/// and a single-stop trip. The import must resolve every one — AND
/// report it. Silently dropping is what the notes exist to prevent.
#[test]
fn network_import_reports_its_resolutions() {
    let mut w = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let opts = zip::write::SimpleFileOptions::default();
    let add = |w: &mut zip::ZipWriter<std::io::Cursor<Vec<u8>>>, name: &str, body: &str| {
        w.start_file(name, opts).unwrap();
        w.write_all(body.as_bytes()).unwrap();
    };
    add(&mut w, "agency.txt", "agency_id,agency_name,agency_url,agency_timezone\n1,T,https://x,UTC\n");
    add(&mut w, "routes.txt", "route_id,agency_id,route_short_name,route_type\n10,1,10,3\n");
    add(&mut w, "stops.txt", "stop_id,stop_name,stop_lat,stop_lon\nA,Main & 1st,45.500,-73.500\nB,Main & 2nd,45.510,-73.510\n");
    // t1/t2: block B1, t2 departs 08:05 while t1 arrives 08:10 (the
    // pattern's shortest runtime) — genuinely infeasible source
    // data. t3/t4: block B2, both at 09:00. t5: block B3 claims the
    // same derived trip as t3. t6: a single-stop trip.
    add(&mut w, "trips.txt",
        "route_id,service_id,trip_id,block_id\n10,WK,t1,B1\n10,WK,t2,B1\n10,WK,t3,B2\n10,WK,t4,B2\n10,WK,t5,B3\n10,WK,t6,\n");
    add(&mut w, "stop_times.txt",
        "trip_id,arrival_time,departure_time,stop_id,stop_sequence\n\
         t1,08:00:00,08:00:00,A,1\nt1,08:30:00,08:30:00,B,2\n\
         t2,08:05:00,08:05:00,A,1\nt2,08:15:00,08:15:00,B,2\n\
         t3,09:00:00,09:00:00,A,1\nt3,09:10:00,09:10:00,B,2\n\
         t4,09:00:00,09:00:00,B,1\nt4,09:10:00,09:10:00,A,2\n\
         t5,09:00:00,09:00:00,A,1\nt5,09:10:00,09:10:00,B,2\n\
         t6,07:00:00,07:00:00,B,1\n");
    add(&mut w, "calendar.txt",
        "service_id,monday,tuesday,wednesday,thursday,friday,saturday,sunday,start_date,end_date\nWK,1,1,1,1,1,0,0,20260101,20261231\n");
    let bytes = w.finish().unwrap().into_inner();

    let feed_id = match dispatch(Request::LoadFeed { bytes }).unwrap() {
        Response::LoadFeed { feed_id, .. } => feed_id,
        _ => panic!("expected LoadFeed response"),
    };
    let (network, notes) = match dispatch(Request::NetworkImport { feed_id }).unwrap() {
        Response::NetworkImport { network, notes } => (network, notes),
        _ => panic!("expected NetworkImport response"),
    };
    // The whole point: the resolved document is valid.
    assert!(
        network.validate().is_ok(),
        "resolved document must validate: {:?}",
        transit_network::validation_issues(&network)
    );
    // B1 keeps t1 (t2 was an impossible continuation); B2 keeps t3
    // (t4 was the same second); B3 is gone (its only trip already
    // belonged to B2).
    assert_eq!(network.blocks.get("B1").unwrap().trips.len(), 1);
    assert_eq!(network.blocks.get("B2").unwrap().trips.len(), 1);
    assert!(network.blocks.get("B3").is_none());

    // And every resolution is reported — one note per shape.
    assert_eq!(notes.len(), 4, "one note per resolution: {notes:?}");
    let kinds: Vec<String> = notes
        .iter()
        .map(|n| serde_json::to_value(n).unwrap()["kind"].as_str().unwrap().to_string())
        .collect();
    assert!(kinds.contains(&"infeasible_block_chain".to_string()), "{kinds:?}");
    assert!(kinds.contains(&"same_second_block_trips".to_string()), "{kinds:?}");
    assert!(kinds.contains(&"duplicate_block_trip".to_string()), "{kinds:?}");
    assert!(kinds.contains(&"dropped_trips".to_string()), "{kinds:?}");
}

/// Transfer centers across a street: routes 10 and 20 meet at
/// sibling stops N1/N2 (parent station STN), with transfers.txt
/// declaring the connection. Plus a transfers.txt row on a
/// single-route pair (A, C) whose intent cannot compose.
fn transfer_zip() -> Vec<u8> {
    let mut w = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let opts = zip::write::SimpleFileOptions::default();
    let add = |w: &mut zip::ZipWriter<std::io::Cursor<Vec<u8>>>, name: &str, body: &str| {
        w.start_file(name, opts).unwrap();
        w.write_all(body.as_bytes()).unwrap();
    };
    add(&mut w, "agency.txt", "agency_id,agency_name,agency_url,agency_timezone\n1,T,https://x,UTC\n");
    add(&mut w, "routes.txt", "route_id,agency_id,route_short_name,route_type\n10,1,10,3\n20,1,20,3\n");
    add(&mut w, "stops.txt",
        "stop_id,stop_name,stop_lat,stop_lon,location_type,parent_station\n\
         STN,Transfer Center,45.510,-73.510,1,\n\
         N1,Main & 1st,45.510,-73.5105,0,STN\n\
         N2,Main & 1st across,45.510,-73.5095,0,STN\n\
         A,Elm & 1st,45.500,-73.500,0,\n\
         C,Oak & 3rd,45.520,-73.520,0,\n");
    add(&mut w, "trips.txt", "route_id,service_id,trip_id,direction_id,block_id\n10,WK,t10,0,\n20,WK,t20,0,\n");
    add(&mut w, "stop_times.txt",
        "trip_id,arrival_time,departure_time,stop_id,stop_sequence\n\
         t10,08:00:00,08:00:00,A,1\nt10,08:10:00,08:10:00,N1,2\n\
         t20,08:20:00,08:20:00,N2,1\nt20,08:30:00,08:30:00,C,2\n");
    add(&mut w, "calendar.txt",
        "service_id,monday,tuesday,wednesday,thursday,friday,saturday,sunday,start_date,end_date\nWK,1,1,1,1,1,0,0,20260101,20261231\n");
    // N1↔N2 is a real transfer point (two routes across the street);
    // A↔C is the feed's intent, but nothing composes there.
    add(&mut w, "transfers.txt", "from_stop_id,to_stop_id,transfer_type\nN1,N2,0\nA,C,0\n");
    w.finish().unwrap().into_inner()
}

#[test]
fn import_flags_transfer_points_and_reports_unusable_ones() {
    let feed_id = match dispatch(Request::LoadFeed { bytes: transfer_zip() }).unwrap() {
        Response::LoadFeed { feed_id, .. } => feed_id,
        _ => panic!("expected LoadFeed response"),
    };
    let (network, notes) = match dispatch(Request::NetworkImport { feed_id }).unwrap() {
        Response::NetworkImport { network, notes } => (network, notes),
        _ => panic!("expected NetworkImport response"),
    };
    // The composed flags land; the aspirational ones do not — and are
    // reported instead of silently ignored.
    assert!(network.stops["N1"].transfer, "two routes meet across the street");
    assert!(network.stops["N2"].transfer);
    assert!(!network.stops["A"].transfer);
    let unusable: Vec<String> = notes
        .iter()
        .filter_map(|n| match n {
            transit_ops::ImportNote::UnusableTransferPoint { stop } => Some(stop.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(unusable, vec!["A".to_string(), "C".to_string()], "{notes:?}");
    // And the flagged document still validates (import law).
    assert!(network.validate().is_ok(), "{:?}", transit_network::validation_issues(&network));

    // The analysis answers the composed question: N1's walking group
    // (N1, N2, STN) pools both routes, and the day's schedule gives
    // route 10 → 20 a usable timed connection (arrive 08:10,
    // depart 08:20 — 10 minutes, inside the 1–15 min window).
    let stats = match dispatch(Request::NetworkStats { network, date: DATE.into() }).unwrap() {
        Response::NetworkStats { stats } => stats,
        _ => panic!("network_stats"),
    };
    assert_eq!(stats.transfer_stops, 2);
    let n1 = stats.stops.iter().find(|s| s.stop_id == "N1").expect("N1");
    assert!(n1.transfer);
    assert_eq!(n1.transfer_group, Some(3));
    assert_eq!(n1.transfer_pairs, Some(1), "only 10 → 20 fits the window");
    let a = stats.stops.iter().find(|s| s.stop_id == "A").expect("A");
    assert!(!a.transfer);
    assert_eq!(a.transfer_group, None);
}

#[test]
fn service_type_ops_expose_the_design_axis() {
    // The planner designs SERVICE TYPES, not calendar dates: the
    // overview lists every type on its representative day, the
    // detail expands one type into everything that runs under it.
    let network = import_production();

    let types = match dispatch(Request::ServiceTypesOverview { network: network.clone() }).unwrap() {
        Response::ServiceTypesOverview { service_types } => service_types,
        _ => panic!("service_types_overview"),
    };
    assert_eq!(types.len(), 1, "one service key (WK) in the fixture");
    assert_eq!(types[0].service_type, "WK");
    assert_eq!(types[0].date, "2026-01-01", "first date the rule runs (a Thursday)");
    assert_eq!(types[0].trips, 4);
    assert_eq!(types[0].active_routes, 1);
    assert_eq!(types[0].active_stops, 3);
    assert!((types[0].revenue_hours - 4200.0 / 3600.0).abs() < 1e-9);

    let detail = match dispatch(Request::ServiceTypeDetail {
        network: network.clone(),
        service_type: "WK".into(),
    })
    .unwrap()
    {
        Response::ServiceTypeDetail { detail } => detail,
        _ => panic!("service_type_detail"),
    };
    assert_eq!(detail.trips, 4);
    assert_eq!(detail.routes.len(), 1, "route 10 runs; zero-trip routes filtered out");
    assert_eq!(detail.routes[0].typical_headway_secs, Some(4260));
    assert_eq!(detail.stops.len(), 3, "A, B and C are all served");
    let schedule_trips: BTreeMap<&str, u32> = detail
        .schedules
        .iter()
        .map(|s| (s.schedule.as_str(), s.trips))
        .collect();
    assert_eq!(schedule_trips["WK:pat-10-0-1"], 3);
    assert_eq!(schedule_trips["WK:pat-10-0-0"], 1);

    // Coverage per service type: same layer, WK on its day.
    let layer_id = match dispatch(Request::CoverageLoad { bytes: pop_layer_geojson(), pop_field: None })
        .unwrap()
    {
        Response::CoverageLoad { layer_id, .. } => layer_id,
        _ => panic!("coverage_load"),
    };
    let coverage = match dispatch(Request::TypeCoverage {
        network,
        layer_id,
        radius_m: 400.0,
        frequent_secs: 3600,
        min_departures: Some(4),
    })
    .unwrap()
    {
        Response::TypeCoverage { coverage } => coverage,
        _ => panic!("type_coverage"),
    };
    assert_eq!(coverage.len(), 1);
    assert_eq!(coverage[0].service_type, "WK");
    assert!((coverage[0].served_pct - 150.0 / 180.0 * 100.0).abs() < 1e-6);
    assert!((coverage[0].frequent_pct - 100.0 / 180.0 * 100.0).abs() < 1e-6);
}

#[test]
fn imported_network_preserves_the_service_view() {
    // Law: what riders see is preserved through the codec round trip
    // (feed → inferred document → materialized service view), except
    // for the documented drop (single-stop trips).
    let feed_id = load();
    let date = DATE;
    let original = match dispatch(Request::Departures {
        feed_id,
        stop_id: "A".into(),
        date: date.into(),
        from_secs: 0,
        limit: 10,
    })
    .unwrap()
    {
        Response::Departures { departures } => departures,
        _ => panic!("departures"),
    };
    let network = match dispatch(Request::NetworkImport { feed_id }).unwrap() {
        Response::NetworkImport { network, .. } => network,
        _ => panic!("network_import"),
    };
    let derived = network_to_gtfs(&network).unwrap();
    let at_a: Vec<(u32, String)> = derived
        .next_departures(
            "A",
            chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d").unwrap(),
            0,
            10,
        )
        .iter()
        .map(|d| (d.departure_time.unwrap(), d.route_id.clone()))
        .collect();
    let expected: Vec<(u32, String)> = original
        .iter()
        .map(|d| (d.departure_time.unwrap(), d.route_id.clone()))
        .collect();
    assert_eq!(at_a, expected, "departures at A survive the round trip");
}

#[test]
fn imported_alignment_survives_materialization() {
    // Law: a pattern's authored path is the geometry the derived feed
    // draws — trips reference it as their shape, so rider views after
    // a planner publish follow the real alignment, not stop chords.
    let feed_id = load();
    let network = match dispatch(Request::NetworkImport { feed_id }).unwrap() {
        Response::NetworkImport { network, .. } => network,
        _ => panic!("network_import"),
    };
    let derived = network_to_gtfs(&network).unwrap();

    let points = derived.shapes.get("pat-10-0-0").expect("pattern path exported");
    assert_eq!(points.len(), 3);
    assert_eq!(points[0].shape_pt_sequence, 1);

    let trip = derived.trips.get("WK:pat-10-0-0@28800").expect("derived trip id scheme");
    assert_eq!(trip.shape_id.as_deref(), Some("pat-10-0-0"));

    // And the geometry riders actually draw is the polyline, not the
    // stop-to-stop fallback.
    let path = derived.trip_path("WK:pat-10-0-0@28800");
    assert_eq!(path.len(), 3, "trip_path follows the shape: {path:?}");
    assert_eq!(path[0], (45.400, -73.400));

    // Unauthored patterns keep the fallback: no shape reference.
    let bare = derived.trips.get("WK:pat-20-0-0@30000").expect("t20 derived trip");
    assert_eq!(bare.shape_id, None);
}

#[test]
fn street_ops_snap_and_route_through_the_seam() {
    // The detour-authoring street ops: bytes in once, then pure
    // queries against the artifact's streets registry. The Elbow
    // route must follow street geometry, not the diagonal.
    let elbow = br#"{"type":"FeatureCollection","features":[{"type":"Feature","geometry":
      {"type":"LineString","coordinates":[[-73.5030,45.5040],[-73.5080,45.5040],[-73.5080,45.5090]]}}]}"#;
    let (streets_id, segments, nodes) =
        match dispatch(Request::StreetsLoad { bytes: elbow.to_vec() }).unwrap() {
            Response::StreetsLoad { streets_id, segments, nodes } => (streets_id, segments, nodes),
            _ => panic!("streets_load"),
        };
    assert_eq!(segments, 2);
    assert_eq!(nodes, 3);

    let a = match dispatch(Request::StreetSnap { streets_id, lat: 45.5040, lon: -73.5030 }).unwrap() {
        Response::StreetSnap { position: Some(p) } => p,
        _ => panic!("street_snap a"),
    };
    assert_eq!(a.seg, 0);
    assert!((a.t - 0.0).abs() < 1e-9, "the start vertex: {a:?}");

    let b = match dispatch(Request::StreetSnap { streets_id, lat: 45.5080, lon: -73.5080 }).unwrap() {
        Response::StreetSnap { position: Some(p) } => p,
        _ => panic!("street_snap b"),
    };
    assert_eq!(b.seg, 1);

    match dispatch(Request::StreetRoute { streets_id, from: a, to: b }).unwrap() {
        Response::StreetRoute { points } => {
            assert_eq!(points.len(), 2, "corner + endpoint: {points:?}");
            assert!((points[0].lat - 45.5040).abs() < 1e-9
                && (points[0].lon + 73.5080).abs() < 1e-9,
                "the corner is committed, not the diagonal: {points:?}");
        }
        _ => panic!("street_route"),
    }

    // Unknown registry ids are errors; disconnected parts do not
    // route; the JSON form round-trips the seam.
    assert!(dispatch(Request::StreetSnap { streets_id: 999, lat: 0.0, lon: 0.0 }).is_err());
    let json = format!(
        r#"{{"op":"street_snap","streets_id":{streets_id},"lat":45.5040,"lon":-73.5030}}"#
    );
    let outcome = transit_ops::dispatch_json(&json);
    assert!(outcome.contains(r#""ok":{"op":"street_snap""#), "{outcome}");
}
