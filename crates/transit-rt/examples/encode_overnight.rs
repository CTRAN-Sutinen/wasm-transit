//! Encode the overnight e2e realtime fixture (web/e2e/fixtures/
//! overnight-rt.pb): owl trip n1's Friday 2026-10-23 instance — its
//! A departure is 24:40, Saturday 00:40 on the wall — +120 s at A,
//! naming its service date (`start_date`), as a producer must for a
//! trip past midnight.
//!
//! Usage: `cargo run -p transit-rt --example encode_overnight [out.pb]`

use prost::Message;
use transit_rt::gtfs_rt::trip_update::{StopTimeEvent, StopTimeUpdate};
use transit_rt::gtfs_rt::{FeedEntity, FeedHeader, FeedMessage, TripDescriptor, TripUpdate};

fn main() -> std::io::Result<()> {
    let out_path = std::env::args().nth(1).unwrap_or_else(|| "overnight-rt.pb".into());
    let feed = FeedMessage {
        header: FeedHeader { gtfs_realtime_version: "2.0".into(), timestamp: Some(1_792_816_200), ..Default::default() },
        entity: vec![FeedEntity {
            id: "tu-n1".into(),
            trip_update: Some(TripUpdate {
                trip: TripDescriptor {
                    trip_id: Some("n1".into()),
                    route_id: Some("90".into()),
                    start_date: Some("20261023".into()),
                    ..Default::default()
                },
                stop_time_update: vec![StopTimeUpdate {
                    stop_id: Some("A".into()),
                    stop_sequence: Some(1),
                    departure: Some(StopTimeEvent { delay: Some(120), ..Default::default() }),
                    ..Default::default()
                }],
                ..Default::default()
            }),
            ..Default::default()
        }],
    };
    std::fs::write(out_path, feed.encode_to_vec())
}
