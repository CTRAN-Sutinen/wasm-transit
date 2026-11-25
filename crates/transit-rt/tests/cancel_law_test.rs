//! A trip that the rt feed cancels, or a stop that it skips, does not run
//! there, and no reader shows or rides it (cap-rt/OPS.md `live`).
//! `LiveDay::runs` is false for a cancelled instance and a skipped visit.

use prost::Message;
use transit_live::{LiveStatus, OnTimeWindow};
use transit_rt::gtfs_rt::trip_descriptor::ScheduleRelationship as TripRel;
use transit_rt::gtfs_rt::trip_update::stop_time_update::ScheduleRelationship as StopRel;
use transit_rt::gtfs_rt::trip_update::{StopTimeEvent, StopTimeUpdate};
use transit_rt::gtfs_rt::{FeedEntity, FeedHeader, FeedMessage, TripDescriptor, TripUpdate};
use transit_rt::{decode_overview, live_day};

const H8: u32 = 8 * 3600;

fn update(trip: &str, rel: TripRel, stops: Vec<StopTimeUpdate>) -> FeedEntity {
    FeedEntity {
        id: trip.into(),
        trip_update: Some(TripUpdate {
            trip: TripDescriptor { trip_id: Some(trip.into()), schedule_relationship: Some(rel as i32), ..Default::default() },
            stop_time_update: stops,
            ..Default::default()
        }),
        ..Default::default()
    }
}

fn stop(stop: &str, seq: u32, rel: StopRel, delay: Option<i32>) -> StopTimeUpdate {
    let event = delay.map(|d| StopTimeEvent { delay: Some(d), ..Default::default() });
    StopTimeUpdate {
        stop_id: Some(stop.into()),
        stop_sequence: Some(seq),
        arrival: event,
        departure: event,
        schedule_relationship: Some(rel as i32),
        ..Default::default()
    }
}

/// Over the e2e fixture (t10 A 08:00 → B 08:10; t20 A 08:20 → B
/// 08:30; t30 A 08:40 → C 08:45 → B 08:50): t20 cancelled, t10
/// deleted, t30 skipping C one minute late.
fn snapshot() -> FeedMessage {
    FeedMessage {
        header: FeedHeader { gtfs_realtime_version: "2.0".into(), timestamp: Some(1_728_000_000), ..Default::default() },
        entity: vec![
            update("t20", TripRel::Canceled, vec![]),
            update("t10", TripRel::Deleted, vec![]),
            update(
                "t30",
                TripRel::Scheduled,
                vec![stop("A", 1, StopRel::Scheduled, Some(60)), stop("C", 2, StopRel::Skipped, None)],
            ),
        ],
    }
}

#[test]
fn the_decode_reads_cancelled_trips_and_skipped_stops() {
    let rt = decode_overview(&snapshot().encode_to_vec()).unwrap();
    let canceled = |id: &str| rt.trips.iter().find(|t| t.trip_id.as_deref() == Some(id)).unwrap().canceled;
    assert!(canceled("t20"), "CANCELED");
    assert!(canceled("t10"), "DELETED is a cancellation riders never see");
    assert!(!canceled("t30"), "SCHEDULED runs");
    let t30 = rt.trips.iter().find(|t| t.trip_id.as_deref() == Some("t30")).unwrap();
    let skipped: Vec<bool> = t30.stop_updates.iter().map(|s| s.skipped).collect();
    assert_eq!(skipped, vec![false, true]);
}

#[test]
fn the_live_document_says_what_does_not_run() {
    let feed = transit_codec::Feed::from_zip_bytes(include_bytes!("../../../web/e2e/fixtures/sample-gtfs.zip")).unwrap();
    let imported = transit_codec::import(&feed);
    let (net, boundary) = (&imported.network, &imported.boundary);
    let rt = decode_overview(&snapshot().encode_to_vec()).unwrap();
    let day = live_day(net, boundary, Some(&rt), None, None, OnTimeWindow::default());
    assert_eq!(day.unmatched_trips, 0);

    for (trip, start) in [("t20", H8 + 1200), ("t10", H8)] {
        let t = day.trips.iter().find(|t| t.trip == trip).unwrap();
        assert!(t.canceled, "{trip} is cancelled");
        assert_eq!(t.start_secs, start);
        assert_eq!(t.status, LiveStatus::NoPrediction, "a cancelled trip is not late or on time");
        assert!(!day.runs(trip, start, "A", start), "{trip} does not leave A");
        assert!(!day.runs(trip, start, "B", start + 600), "{trip} does not reach B");
    }

    let t30 = day.trips.iter().find(|t| t.trip == "t30").unwrap();
    assert!(!t30.canceled);
    let c = t30.stops.iter().find(|s| s.stop == "C").expect("a skipped stop is in the document");
    assert!(c.skipped);
    assert_eq!((c.arrival_delay, c.departure_delay), (None, None));
    let b = day.visit("t30", H8 + 2400, "B", H8 + 3000).unwrap();
    assert!(!b.skipped);
    assert_eq!(b.departure_delay, Some(60), "the delay carries past the skipped stop");
    assert!(day.runs("t30", H8 + 2400, "A", H8 + 2400));
    assert!(!day.runs("t30", H8 + 2400, "C", H8 + 2700), "the skipped visit does not happen");
    assert!(day.runs("t30", H8 + 2400, "B", H8 + 3000));

    assert!(day.runs("t40", H8 + 900, "SA", H8 + 900));
}

#[test]
fn nothing_cancelled_changes_no_bytes() {
    // A document with no cancellation serializes as before, so its hashes
    // stay put.
    let feed = transit_codec::Feed::from_zip_bytes(include_bytes!("../../../web/e2e/fixtures/sample-gtfs.zip")).unwrap();
    let imported = transit_codec::import(&feed);
    let rt = decode_overview(include_bytes!("../../../web/e2e/fixtures/sample-rt.pb")).unwrap();
    let day = live_day(&imported.network, &imported.boundary, Some(&rt), None, None, OnTimeWindow::default());
    let json = serde_json::to_string(&day).unwrap();
    assert!(!json.contains("canceled") && !json.contains("skipped"), "{json}");
}
