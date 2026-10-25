//! Encode a small sample GTFS-rt feed to a file, so the demo page (and
//! the node smoke test) have a `.pb` to work with. The vehicle's
//! position is parameterized: the e2e suite re-encodes the feed with
//! the bus slightly moved, to prove the bus popup follows its vehicle.
//!
//! Usage: `cargo run -p transit-rt --example encode_sample [out.pb] [lat] [lon]`

use prost::Message;
use transit_rt::gtfs_rt::trip_update::{StopTimeEvent, StopTimeUpdate};
use transit_rt::gtfs_rt::{FeedEntity, FeedHeader, FeedMessage, TripDescriptor, TripUpdate};

fn main() -> std::io::Result<()> {
    let mut args = std::env::args().skip(1);
    let out_path = args.next().unwrap_or_else(|| "sample-rt.pb".into());
    let lat: f32 = args.next().and_then(|s| s.parse().ok()).unwrap_or(45.44);
    let lon: f32 = args.next().and_then(|s| s.parse().ok()).unwrap_or(-73.44);

    let feed = FeedMessage {
        header: FeedHeader {
            gtfs_realtime_version: "2.0".into(),
            timestamp: Some(1_728_000_000),
            ..Default::default()
        },
        entity: vec![
            FeedEntity {
                id: "tu-1".into(),
                trip_update: Some(TripUpdate {
                    trip: TripDescriptor {
                        trip_id: Some("t10".into()),
                        route_id: Some("10".into()),
                        ..Default::default()
                    },
                    stop_time_update: vec![
                        StopTimeUpdate {
                            stop_id: Some("A".into()),
                            stop_sequence: Some(1),
                            departure: Some(StopTimeEvent {
                                delay: Some(120),
                                ..Default::default()
                            }),
                            ..Default::default()
                        },
                        StopTimeUpdate {
                            stop_id: Some("B".into()),
                            stop_sequence: Some(2),
                            arrival: Some(StopTimeEvent {
                                delay: Some(180),
                                ..Default::default()
                            }),
                            ..Default::default()
                        },
                    ],
                    ..Default::default()
                }),
                ..Default::default()
            },
            FeedEntity {
                id: "vp-1".into(),
                vehicle: Some(transit_rt::gtfs_rt::VehiclePosition {
                    trip: Some(TripDescriptor {
                        trip_id: Some("t10".into()),
                        ..Default::default()
                    }),
                    position: Some(transit_rt::gtfs_rt::Position {
                        latitude: lat,
                        longitude: lon,
                        bearing: Some(90.0),
                        ..Default::default()
                    }),
                    stop_id: Some("A".into()),
                    // The bus number, as an agency would serve it.
                    vehicle: Some(transit_rt::gtfs_rt::VehicleDescriptor {
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

    std::fs::write(out_path, feed.encode_to_vec())
}
