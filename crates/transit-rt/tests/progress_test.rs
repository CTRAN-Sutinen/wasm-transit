//! The progress join's propagation law, over the e2e fixture feed:
//! a stated delay carries to the downstream stops the update does
//! not mention, and never to the stops before it.

use transit_rt::{trip_progress, RtOverview, StopUpdateView, TripUpdateView};

fn overview(trip: &str, updates: Vec<StopUpdateView>) -> RtOverview {
    RtOverview {
        gtfs_realtime_version: None,
        timestamp: None,
        entities: 1,
        trip_updates: 1,
        vehicles: 0,
        alerts: 0,
        trips: vec![TripUpdateView {
            trip_id: Some(trip.into()),
            route_id: None,
            start_time: None,
            stop_updates: updates,
        }],
        vehicle_positions: vec![],
    }
}

fn update(stop: &str, seq: u32, delay: i32) -> StopUpdateView {
    StopUpdateView {
        stop_id: Some(stop.into()),
        stop_sequence: Some(seq),
        arrival_delay: Some(delay),
        departure_delay: Some(delay),
    }
}

#[test]
fn a_stated_delay_carries_downstream_not_upstream() {
    let feed = transit_codec::Feed::from_zip_bytes(include_bytes!("../../../web/e2e/fixtures/sample-gtfs.zip")).unwrap();
    let imported = transit_codec::import(&feed);
    let (net, boundary) = (&imported.network, &imported.boundary);
    // t30 runs A(1) → C(2) → B(3); the feed only speaks about C.
    let p = trip_progress(net, boundary, &overview("t30", vec![update("C", 2, 240)]), "t30").unwrap();
    let by_stop: Vec<_> = p.stops.iter().map(|s| (s.stop_id.as_str(), s.departure_delay)).collect();
    assert_eq!(by_stop, vec![("A", None), ("C", Some(240)), ("B", Some(240))]);
    assert_eq!(p.stops[2].predicted_arrival, Some(8 * 3600 + 50 * 60 + 240), "08:54");

    // A later update overrides the carried one.
    let p = trip_progress(net, boundary, &overview("t30", vec![update("A", 1, 300), update("B", 3, 60)]), "t30").unwrap();
    let by_stop: Vec<_> = p.stops.iter().map(|s| s.departure_delay).collect();
    assert_eq!(by_stop, vec![Some(300), Some(300), Some(60)]);
}
