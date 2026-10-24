//! Round-trip test: build a FeedMessage, encode it as protobuf, decode it
//! back. This validates our generated types against prost itself without
//! needing a live feed.
//!
//! Note: gtfs-realtime.proto is proto2. Two prost consequences matter here:
//! * `required` fields (FeedMessage.header, TripUpdate.trip, FeedEntity.id,
//!   FeedHeader.gtfs_realtime_version) become plain, non-Option Rust fields.
//! * `optional` fields become Option<T>.
//! Nested messages like StopTimeUpdate live in generated modules
//! (`gtfs_rt::trip_update`) matching the proto's scoping.

use prost::Message;
use transit_rt::decode;
use transit_rt::gtfs_rt::trip_update::{StopTimeEvent, StopTimeUpdate};
use transit_rt::gtfs_rt::{FeedEntity, FeedHeader, FeedMessage, TripDescriptor, TripUpdate};

fn sample_feed() -> FeedMessage {
    FeedMessage {
        header: FeedHeader {
            gtfs_realtime_version: "2.0".into(),
            timestamp: Some(1_728_000_000),
            ..Default::default()
        },
        entity: vec![FeedEntity {
            id: "entity-1".into(),
            trip_update: Some(TripUpdate {
                trip: TripDescriptor {
                    trip_id: Some("t10".into()),
                    route_id: Some("10".into()),
                    ..Default::default()
                },
                stop_time_update: vec![StopTimeUpdate {
                    stop_id: Some("A".into()),
                    stop_sequence: Some(1),
                    departure: Some(StopTimeEvent {
                        delay: Some(300),
                        ..Default::default()
                    }),
                    ..Default::default()
                }],
                ..Default::default()
            }),
            ..Default::default()
        }],
    }
}

#[test]
fn encodes_and_decodes_roundtrip() {
    let original = sample_feed();
    let bytes = original.encode_to_vec();

    let decoded = decode(&bytes).expect("decode");
    assert_eq!(decoded.header.gtfs_realtime_version, "2.0");
    assert_eq!(decoded.header.timestamp, Some(1_728_000_000));

    let entity = &decoded.entity[0];
    assert_eq!(entity.id, "entity-1");

    let tu = entity.trip_update.as_ref().unwrap();
    assert_eq!(tu.trip.trip_id.as_deref(), Some("t10"));
    assert_eq!(tu.trip.route_id.as_deref(), Some("10"));
    let stu = &tu.stop_time_update[0];
    assert_eq!(stu.stop_id.as_deref(), Some("A"));
    assert_eq!(stu.stop_sequence, Some(1));
    assert_eq!(stu.departure.as_ref().unwrap().delay, Some(300));
}

#[test]
fn rejects_garbage_bytes() {
    assert!(decode(b"this is not a protobuf payload").is_err());
}
