//! Native dispatch tests for the realtime capability — the seam
//! without wasm, over bytes built with the same generated prost types
//! the decoder uses (and, for the fixture leg, the e2e protobuf the
//! browser tests replay).

use cap_rt::{dispatch, dispatch_bytes, dispatch_json, Request, Response};

#[test]
fn decode_reports_delays_and_vehicles() {
    // A minimal GTFS-rt feed, encoded here with the same generated
    // prost types the decoder uses: t10 delayed +120 s at A, plus one
    // vehicle on t10.
    use prost::Message;
    use transit_rt::gtfs_rt as rt;
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

    match dispatch(Request::Decode { bytes }).unwrap() {
        Response::Decode { overview, .. } => {
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
        _ => panic!("decode response"),
    }

    // Garbage bytes decode to an error, not a panic.
    assert!(dispatch(Request::Decode { bytes: vec![0xff, 0xff] }).is_err());
}

#[test]
fn the_e2e_fixture_decodes_on_the_bulk_seam() {
    // The fixture the browser e2e replays (t10 delayed +120/+180, one
    // vehicle) — through the bulk-bytes op the runner also uses.
    let bytes = include_bytes!("../../../web/e2e/fixtures/sample-rt.pb");
    let outcome = dispatch_bytes(cap_rt::OP_DECODE, bytes);
    let parsed: serde_json::Value = serde_json::from_str(&outcome).unwrap();
    assert!(parsed.get("ok").is_some(), "{outcome}");
    assert_eq!(parsed["ok"]["overview"]["trip_updates"], 1);
    assert_eq!(parsed["ok"]["overview"]["vehicles"], 1);

    // And through the JSON seam, byte-compatible with every module.
    let json = dispatch_json(&format!(
        r#"{{"op":"decode","bytes":{}}}"#,
        serde_json::to_string(bytes.as_ref()).unwrap()
    ));
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert!(parsed["ok"]["overview"]["trips"].is_array(), "{json}");

    // Unknown ops error, never panic.
    let bad = dispatch_json(r#"{"op":"nope"}"#);
    assert!(bad.contains("invalid request"), "{bad}");
}

/// The join the founding dispatcher sessions did by hand, now an op:
/// one trip's schedule + delays + predictions, and the ranked late
/// view with the DELAYED-BUT-DARK signal. All over the REAL e2e
/// fixture pair (sample-gtfs.zip + sample-rt.pb: t10 +120 s at A,
/// +180 s at B, one vehicle on t10).
#[test]
fn trip_progress_joins_schedule_and_delays() {
    let feed = include_bytes!("../../../web/e2e/fixtures/sample-gtfs.zip");
    let loaded = cap_rt::dispatch_bytes(cap_rt::OP_LOAD_FEED, feed);
    let parsed: serde_json::Value = serde_json::from_str(&loaded).unwrap();
    assert!(parsed["ok"].get("feed_hash").is_some(), "{loaded}");
    let feed_hash = parsed["ok"]["feed_hash"].as_str().unwrap().to_string();
    assert_eq!(parsed["ok"]["summary"]["trips"], 7);

    let payload = include_bytes!("../../../web/e2e/fixtures/sample-rt.pb");
    let decoded = cap_rt::dispatch_bytes(cap_rt::OP_DECODE, payload);
    let parsed: serde_json::Value = serde_json::from_str(&decoded).unwrap();
    let rt_hash = parsed["ok"]["rt_hash"].as_str().unwrap().to_string();

    // t10: A 08:00 dep +120 → 08:02; B 08:10 arr +180 → 08:13.
    let progress = cap_rt::dispatch(cap_rt::Request::TripProgress {
        feed_hash: feed_hash.clone(),
        rt_hash: rt_hash.clone(),
        trip_id: "t10".into(),
    })
    .expect("trip_progress");
    let cap_rt::Response::TripProgress { progress: Some(p) } = progress else { panic!("progress") };
    assert_eq!(p.trip_id, "t10");
    assert_eq!(p.route_id.as_deref(), Some("10"));
    assert_eq!(p.block_id.as_deref(), Some("B1"));
    assert_eq!(p.stops.len(), 2);
    assert_eq!(p.stops[0].stop_id, "A");
    assert_eq!(p.stops[0].scheduled_departure, Some(8 * 3600));
    assert_eq!(p.stops[0].departure_delay, Some(120));
    assert_eq!(p.stops[0].predicted_departure, Some(8 * 3600 + 120), "08:02");
    assert_eq!(p.stops[1].stop_id, "B");
    assert_eq!(p.stops[1].arrival_delay, Some(180));
    assert_eq!(p.stops[1].predicted_arrival, Some(8 * 3600 + 600 + 180), "08:13");
    assert_eq!(p.worst_delay_secs, 180);

    // A trip the snapshot does not mention answers from the schedule
    // alone (delay-free) — the plan is still an answer.
    let plain = cap_rt::dispatch(cap_rt::Request::TripProgress {
        feed_hash: feed_hash.clone(),
        rt_hash: rt_hash.clone(),
        trip_id: "t30".into(),
    })
    .expect("trip_progress");
    let cap_rt::Response::TripProgress { progress: Some(p) } = plain else { panic!("progress") };
    assert_eq!(p.stops.len(), 3);
    assert!(p.stops.iter().all(|s| s.arrival_delay.is_none()));
    assert_eq!(p.worst_delay_secs, 0);

    // A trip the FEED does not carry answers null (the rt feed named
    // a trip the schedule never had).
    let missing = cap_rt::dispatch(cap_rt::Request::TripProgress {
        feed_hash,
        rt_hash,
        trip_id: "ghost".into(),
    })
    .expect("trip_progress");
    let cap_rt::Response::TripProgress { progress: None } = missing else { panic!("progress") };
}

/// The ranked view: worst first, and the dark-vehicle signal — a
/// trip with a delay but no position naming it.
#[test]
fn late_trips_rank_and_flag_the_dark_ones() {
    let feed = include_bytes!("../../../web/e2e/fixtures/sample-gtfs.zip").to_vec();
    let cap_rt::Response::LoadFeed { feed_hash, .. } =
        cap_rt::dispatch(cap_rt::Request::LoadFeed { bytes: feed }).unwrap()
    else { panic!("load_feed") };

    // The fixture payload: t10 is late and HAS a vehicle (4201).
    let payload = include_bytes!("../../../web/e2e/fixtures/sample-rt.pb").to_vec();
    let cap_rt::Response::Decode { rt_hash, .. } =
        cap_rt::dispatch(cap_rt::Request::Decode { bytes: payload.clone() }).unwrap()
    else { panic!("decode") };

    let late = cap_rt::dispatch(cap_rt::Request::LateTrips {
        feed_hash: feed_hash.clone(),
        rt_hash: rt_hash.clone(),
        min_delay_secs: 60,
        vp_hash: None,
    })
    .expect("late_trips");
    let cap_rt::Response::LateTrips { trips } = late else { panic!("late_trips") };
    assert_eq!(trips.len(), 1);
    assert_eq!(trips[0].trip_id.as_deref(), Some("t10"));
    assert_eq!(trips[0].worst_delay_secs, 180);
    assert_eq!(trips[0].vehicle_label.as_deref(), Some("4201"));
    assert_eq!(trips[0].vehicle_id.as_deref(), Some("4201"), "the internal id rides along with the label");
    assert!(trips[0].has_position, "the fixture's vehicle names t10");
    assert_eq!(trips[0].position_trip_id.as_deref(), Some("t10"), "located on its own trip");

    // The DARK case: a synthetic payload whose trip update carries a
    // delay but no vehicle names it — the dispatcher's "delayed but
    // dark" fact, surfaced by the op instead of a host-side
    // set-difference.
    use prost::Message;
    use transit_rt::gtfs_rt as rt;
    let dark = rt::FeedMessage {
        header: rt::FeedHeader {
            gtfs_realtime_version: "2.0".into(),
            ..Default::default()
        },
        entity: vec![rt::FeedEntity {
            id: "tu1".into(),
            trip_update: Some(rt::TripUpdate {
                trip: rt::TripDescriptor { trip_id: Some("t20".into()), ..Default::default() },
                stop_time_update: vec![rt::trip_update::StopTimeUpdate {
                    stop_id: Some("A".into()),
                    departure: Some(rt::trip_update::StopTimeEvent { delay: Some(300), ..Default::default() }),
                    ..Default::default()
                }],
                ..Default::default()
            }),
            ..Default::default()
        }],
    }
    .encode_to_vec();
    let cap_rt::Response::Decode { rt_hash: dark_hash, .. } =
        cap_rt::dispatch(cap_rt::Request::Decode { bytes: dark }).unwrap()
    else { panic!("decode") };
    let late = cap_rt::dispatch(cap_rt::Request::LateTrips {
        feed_hash: feed_hash.clone(),
        rt_hash: dark_hash.clone(),
        min_delay_secs: 60,
        vp_hash: None,
    })
    .expect("late_trips");
    let cap_rt::Response::LateTrips { trips } = late else { panic!("late_trips") };
    assert_eq!(trips.len(), 1);
    assert_eq!(trips[0].trip_id.as_deref(), Some("t20"));
    assert!(!trips[0].has_position, "a delay with no position is the dark signal");
    assert_eq!(trips[0].vehicle_label, None);
    assert_eq!(trips[0].vehicle_id, None);

    // The BLOCK law (found dogfooding C-TRAN live, QAF-30): t20's
    // delay is propagated from t10, the trip before it on block B1 —
    // and the positions snapshot shows bus 4201 running t10. The bus
    // is out, finishing the previous trip: late, not dark.
    let late = cap_rt::dispatch(cap_rt::Request::LateTrips {
        feed_hash: feed_hash.clone(),
        rt_hash: dark_hash.clone(),
        min_delay_secs: 60,
        vp_hash: Some(rt_hash.clone()),
    })
    .expect("late_trips");
    let cap_rt::Response::LateTrips { trips } = late else { panic!("late_trips") };
    assert_eq!(trips[0].trip_id.as_deref(), Some("t20"));
    assert_eq!(trips[0].block_id.as_deref(), Some("B1"));
    assert!(trips[0].has_position, "the block's bus is visible on the trip before: not dark");
    assert_eq!(trips[0].position_trip_id.as_deref(), Some("t10"), "the trip the bus is actually on");
    assert_eq!(trips[0].vehicle_id.as_deref(), Some("4201"));

    // The SPLIT-FEED law (found dogfooding C-TRAN live: TripUpdates
    // and VehiclePositions are SEPARATE feeds — positions looked up
    // in the delays snapshot alone marked every late trip DARK).
    // A TU-only delays snapshot + the positions snapshot named by
    // vp_hash: the bus is found, not dark.
    let tu_only = rt::FeedMessage {
        header: rt::FeedHeader { gtfs_realtime_version: "2.0".into(), ..Default::default() },
        entity: vec![rt::FeedEntity {
            id: "tu2".into(),
            trip_update: Some(rt::TripUpdate {
                trip: rt::TripDescriptor { trip_id: Some("t10".into()), ..Default::default() },
                stop_time_update: vec![rt::trip_update::StopTimeUpdate {
                    stop_id: Some("A".into()),
                    departure: Some(rt::trip_update::StopTimeEvent { delay: Some(240), ..Default::default() }),
                    ..Default::default()
                }],
                ..Default::default()
            }),
            ..Default::default()
        }],
    }
    .encode_to_vec();
    let cap_rt::Response::Decode { rt_hash: tu_hash, .. } =
        cap_rt::dispatch(cap_rt::Request::Decode { bytes: tu_only }).unwrap()
    else { panic!("decode") };
    // Same vp_hash as the delays would give (the fixture snapshot
    // carries the vehicle) — but the DELAYS are the split snapshot.
    let late = cap_rt::dispatch(cap_rt::Request::LateTrips {
        feed_hash: feed_hash,
        rt_hash: tu_hash,
        vp_hash: Some(rt_hash),
        min_delay_secs: 60,
    })
    .expect("late_trips");
    let cap_rt::Response::LateTrips { trips } = late else { panic!("late_trips") };
    assert_eq!(trips.len(), 1);
    assert_eq!(trips[0].trip_id.as_deref(), Some("t10"));
    assert_eq!(trips[0].worst_delay_secs, 240, "the split snapshot's delay");
    assert!(trips[0].has_position, "the position comes from the vp_hash snapshot");
    assert_eq!(trips[0].vehicle_label.as_deref(), Some("4201"));
}

#[test]
fn unknown_hashes_are_errors_not_panics() {
    let err = cap_rt::dispatch(cap_rt::Request::TripProgress {
        feed_hash: "sha256:missing".into(),
        rt_hash: "sha256:missing".into(),
        trip_id: "t10".into(),
    })
    .unwrap_err();
    assert!(err.contains("unknown feed"), "{err}");

    let feed = include_bytes!("../../../web/e2e/fixtures/sample-gtfs.zip").to_vec();
    let cap_rt::Response::LoadFeed { feed_hash, .. } =
        cap_rt::dispatch(cap_rt::Request::LoadFeed { bytes: feed }).unwrap()
    else { panic!("load_feed") };
    let err = cap_rt::dispatch(cap_rt::Request::LateTrips {
        feed_hash,
        rt_hash: "sha256:missing".into(),
        min_delay_secs: 0,
        vp_hash: None,
    })
    .unwrap_err();
    assert!(err.contains("unknown rt snapshot"), "{err}");
}

/// THE live document through the seam, over the real fixture pair:
/// the snapshot resolves through the network's trip map into derived
/// trips, statuses classify under the window the request names, and
/// the summary is the status panel's whole answer.
#[test]
fn live_resolves_the_snapshot_into_model_vocabulary() {
    let feed = include_bytes!("../../../web/e2e/fixtures/sample-gtfs.zip");
    let trip_map = transit_codec::trip_map(&transit_codec::import(&transit_codec::Feed::from_zip_bytes(feed).unwrap()).network);
    let cap_rt::Response::LoadFeed { feed_hash, .. } =
        cap_rt::dispatch(cap_rt::Request::LoadFeed { bytes: feed.to_vec() }).unwrap()
    else {
        panic!("load_feed")
    };
    let cap_rt::Response::Decode { rt_hash, .. } = cap_rt::dispatch(cap_rt::Request::Decode {
        bytes: include_bytes!("../../../web/e2e/fixtures/sample-rt.pb").to_vec(),
    })
    .unwrap() else {
        panic!("decode")
    };
    let cap_rt::Response::LoadTripMap { trip_map_hash } =
        cap_rt::dispatch(cap_rt::Request::LoadTripMap { trip_map: trip_map.clone() }).unwrap()
    else {
        panic!("load_trip_map")
    };

    let live = |window| {
        cap_rt::dispatch(cap_rt::Request::Live {
            feed_hash: feed_hash.clone(),
            rt_hash: Some(rt_hash.clone()),
            vp_hash: None,
            trip_map_hash: trip_map_hash.clone(),
            window,
            late_limit: None,
        })
    };
    let cap_rt::Response::Live { live_hash, live: day, summary } = live(None).unwrap() else { panic!("live") };
    let canonical = serde_json::to_string(&day).unwrap();
    assert_eq!(live_hash, format!("sha256:{:x}", <sha2::Sha256 as sha2::Digest>::digest(canonical.as_bytes())), "the module is the hash authority");
    let t10 = trip_map.entries.iter().find(|e| e.agency_trip_ids.iter().any(|id| id == "t10")).unwrap();
    assert!(day.trips.iter().any(|t| t.pattern_trips == t10.pattern_trips && t.start_secs == t10.start_secs));
    assert_eq!(summary.vehicles, 1);
    assert_eq!(summary.counts.late, 1, "+180 s is late under the default window");
    assert_eq!(summary.late[0].delay_secs, 180);
    assert!(!summary.late_are_predictions);

    // The window is the caller's policy: five seconds late is very late.
    let strict = transit_live::OnTimeWindow { early_secs: None, late_secs: 0, very_late_secs: 5 };
    let cap_rt::Response::Live { summary, .. } = live(Some(strict)).unwrap() else { panic!("live") };
    assert_eq!(summary.counts.very_late, 1);

    // No snapshot at all is a request error; an unknown map is a
    // missing-document error.
    let err = cap_rt::dispatch(cap_rt::Request::Live {
        feed_hash: feed_hash.clone(),
        rt_hash: None,
        vp_hash: None,
        trip_map_hash: trip_map_hash.clone(),
        window: None,
        late_limit: None,
    })
    .unwrap_err();
    assert!(err.contains("needs a snapshot"), "{err}");
    let err = cap_rt::dispatch(cap_rt::Request::Live {
        feed_hash,
        rt_hash: Some(rt_hash),
        vp_hash: None,
        trip_map_hash: "sha256:missing".into(),
        window: None,
        late_limit: None,
    })
    .unwrap_err();
    assert!(err.contains("unknown trip map"), "{err}");
}
