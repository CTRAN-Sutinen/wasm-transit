//! The boundary resolution over the REAL e2e fixture pair
//! (sample-gtfs.zip + sample-rt.pb: t10 +120 s at A, +180 s at B,
//! one vehicle on t10): rt trip ids are the network's trip ids,
//! resolved once into trip instances, and the document classifies
//! under the window it records.

use transit_codec::Boundary;
use transit_live::{LiveStatus, OnTimeWindow};
use transit_rt::{decode_overview, live_day};

#[test]
fn a_snapshot_resolves_into_trip_instances() {
    let feed = transit_codec::Feed::from_zip_bytes(include_bytes!("../../../web/e2e/fixtures/sample-gtfs.zip")).unwrap();
    let imported = transit_codec::import(&feed);
    let (net, boundary) = (&imported.network, &imported.boundary);
    let rt = decode_overview(include_bytes!("../../../web/e2e/fixtures/sample-rt.pb")).unwrap();

    let day = live_day(net, boundary, Some(&rt), None, None, OnTimeWindow::default());
    assert_eq!(day.unmatched_trips, 0);
    assert_eq!(day.window, OnTimeWindow::default());
    let trip = day.trips.iter().find(|t| t.trip == "t10").unwrap();
    assert_eq!(trip.start_secs, 8 * 3600);
    assert_eq!(trip.worst_delay_secs, Some(180));
    assert_eq!(trip.status, LiveStatus::Late);
    let a = day.visit("t10", 8 * 3600, "A", 8 * 3600).unwrap();
    assert_eq!(a.predicted_departure, Some(8 * 3600 + 120), "08:02");

    // The vehicle rides its trip instance, with the trip's headline.
    let bus = &day.vehicles[0];
    assert_eq!((bus.trip.as_deref(), bus.start_secs), (Some("t10"), Some(8 * 3600)));
    assert_eq!(bus.delay_secs, Some(180));
    assert_eq!(bus.status, LiveStatus::Late);
    assert_eq!(bus.route_name.as_deref(), Some("10"), "named as the model names the route");

    // A tighter window reclassifies — the policy is an input.
    let strict = OnTimeWindow { early_secs: 0, late_secs: 60, very_late_secs: 120 };
    assert_eq!(live_day(net, boundary, Some(&rt), None, None, strict).vehicles[0].status, LiveStatus::VeryLate);

    // Positions alone (no trip updates): vehicles, no predictions.
    let positions_only = live_day(net, boundary, None, Some(&rt), None, OnTimeWindow::default());
    assert!(positions_only.trips.is_empty());
    assert_eq!(positions_only.vehicles[0].status, LiveStatus::NoPrediction);

    // An rt trip the network does not have is reported, not dropped.
    let empty = transit_network::Network::default();
    assert_eq!(live_day(&empty, &Boundary::default(), Some(&rt), None, None, OnTimeWindow::default()).unmatched_trips, rt.trips.len() as u32);
}
