//! The boundary resolution over the REAL e2e fixture pair
//! (sample-gtfs.zip + sample-rt.pb: t10 +120 s at A, +180 s at B,
//! one vehicle on t10): agency ids resolve into derived trips once,
//! and the document classifies under the window it records.

use transit_core::Gtfs;
use transit_live::{LiveStatus, OnTimeWindow};
use transit_rt::{decode_overview, live_day};

#[test]
fn a_snapshot_resolves_into_derived_trips() {
    let gtfs = Gtfs::from_zip_bytes(include_bytes!("../../../web/e2e/fixtures/sample-gtfs.zip")).unwrap();
    let import = transit_codec::gtfs_to_network(&gtfs);
    let rt = decode_overview(include_bytes!("../../../web/e2e/fixtures/sample-rt.pb")).unwrap();
    let t10 = import
        .trip_map
        .entries
        .iter()
        .find(|e| e.agency_trip_ids.iter().any(|id| id == "t10"))
        .expect("t10 is in the trip map");

    let day = live_day(&gtfs, Some(&rt), None, &import.trip_map, OnTimeWindow::default());
    assert_eq!(day.unmatched_trips, 0);
    assert_eq!(day.window, OnTimeWindow::default());
    let trip = day.trips.iter().find(|t| t.agency_trip_id == "t10").unwrap();
    assert_eq!((trip.schedule.as_str(), trip.start_secs), (t10.schedule.as_str(), t10.start_secs));
    assert_eq!(trip.worst_delay_secs, Some(180));
    assert_eq!(trip.status, LiveStatus::Late);
    let a = day.visit(&t10.schedule, t10.start_secs, "A", 8 * 3600).unwrap();
    assert_eq!(a.predicted_departure, Some(8 * 3600 + 120), "08:02");

    // The vehicle rides its derived trip, with the trip's headline.
    let bus = &day.vehicles[0];
    assert_eq!(bus.schedule.as_deref(), Some(t10.schedule.as_str()));
    assert_eq!(bus.delay_secs, Some(180));
    assert_eq!(bus.status, LiveStatus::Late);

    // A tighter window reclassifies — the policy is an input.
    let strict = OnTimeWindow { early_secs: None, late_secs: 60, very_late_secs: 120 };
    assert_eq!(live_day(&gtfs, Some(&rt), None, &import.trip_map, strict).vehicles[0].status, LiveStatus::VeryLate);

    // Positions alone (no trip updates): vehicles, no predictions.
    let positions_only = live_day(&gtfs, None, Some(&rt), &import.trip_map, OnTimeWindow::default());
    assert!(positions_only.trips.is_empty());
    assert_eq!(positions_only.vehicles[0].status, LiveStatus::NoPrediction);

    // An rt trip the import never saw is reported, not dropped silently.
    let empty_map = transit_codec::TripIdMap { entries: vec![] };
    assert_eq!(live_day(&gtfs, Some(&rt), None, &empty_map, OnTimeWindow::default()).unmatched_trips, rt.trips.len() as u32);
}
