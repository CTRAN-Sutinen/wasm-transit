//! Native tests of the dispatch seam: no wasm involved — the exact
//! `Request → Response` path every host shares.

use std::io::Write;

use transit_ops::{dispatch, Request, Response};
use prost::Message;
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
        }
        _ => panic!(),
    }

    // Garbage bytes decode to an error, not a panic.
    assert!(dispatch(Request::DecodeRt { bytes: vec![0xff, 0xff] }).is_err());
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
