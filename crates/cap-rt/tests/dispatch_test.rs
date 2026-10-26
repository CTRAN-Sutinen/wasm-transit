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
        Response::Decode { overview } => {
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
