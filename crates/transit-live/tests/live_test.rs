//! The live document's laws: one classification rule under one
//! window, loop-safe visit lookup, and the summary's two modes
//! (late buses when positions exist, late trips when they do not).

use transit_live::{classify, summarize, LiveDay, LiveStatus, LiveStop, LiveTrip, LiveVehicle, OnTimeWindow};

#[test]
fn classification_follows_the_window_edges() {
    let w = OnTimeWindow::default();
    assert_eq!(classify(None, &w), LiveStatus::NoPrediction);
    assert_eq!(classify(Some(-600), &w), LiveStatus::OnTime, "no early bound: earliness is on time");
    assert_eq!(classify(Some(60), &w), LiveStatus::OnTime, "the edge is inclusive");
    assert_eq!(classify(Some(61), &w), LiveStatus::Late);
    assert_eq!(classify(Some(300), &w), LiveStatus::Late);
    assert_eq!(classify(Some(301), &w), LiveStatus::VeryLate);
    let strict = OnTimeWindow { early_secs: Some(60), ..w };
    assert_eq!(classify(Some(-60), &strict), LiveStatus::OnTime);
    assert_eq!(classify(Some(-61), &strict), LiveStatus::Early);
}

fn stop(stop: &str, scheduled: u32, delay: i32) -> LiveStop {
    LiveStop {
        stop: stop.into(),
        scheduled_departure: Some(scheduled),
        arrival_delay: Some(delay),
        departure_delay: Some(delay),
        predicted_departure: Some((scheduled as i32 + delay) as u32),
    }
}

fn trip(schedule: &str, start: u32, worst: i32, stops: Vec<LiveStop>) -> LiveTrip {
    LiveTrip {
        schedule: schedule.into(),
        start_secs: start,
        agency_trip_id: format!("agency-{schedule}"),
        route: Some("10".into()),
        route_name: Some("10".into()),
        worst_delay_secs: Some(worst),
        status: classify(Some(worst), &OnTimeWindow::default()),
        stops,
    }
}

#[test]
fn a_loop_visit_resolves_by_its_scheduled_time() {
    // A loop: the trip visits A at 08:00 and again at 08:30.
    let day = LiveDay {
        window: OnTimeWindow::default(),
        trips: vec![trip("s1", 28800, 240, vec![stop("A", 28800, 60), stop("B", 29400, 120), stop("A", 30600, 240)])],
        vehicles: vec![],
        unmatched_trips: 0,
    };
    assert_eq!(day.visit("s1", 28800, "A", 30600).unwrap().departure_delay, Some(240));
    assert_eq!(day.visit("s1", 28800, "A", 28800).unwrap().departure_delay, Some(60));
    // A shifted (diverted) visit falls back to the first entry at the stop.
    assert_eq!(day.visit("s1", 28800, "A", 99999).unwrap().departure_delay, Some(60));
    assert!(day.visit("s1", 28800, "Z", 28800).is_none());
    assert!(day.visit("other", 28800, "A", 28800).is_none());
}

#[test]
fn the_summary_ranks_buses_and_falls_back_to_predictions() {
    let w = OnTimeWindow::default();
    let vehicle = |key: &str, delay: Option<i32>| LiveVehicle {
        key: key.into(),
        vehicle_id: Some(key.into()),
        label: None,
        lat: 0.0,
        lon: 0.0,
        bearing: None,
        route: Some("10".into()),
        route_name: Some("10".into()),
        next_stop: None,
        schedule: None,
        start_secs: None,
        agency_trip_id: None,
        delay_secs: delay,
        status: classify(delay, &w),
    };
    let trips = vec![trip("s1", 28800, 400, vec![]), trip("s2", 29000, 30, vec![])];
    let day = LiveDay {
        window: w,
        trips: trips.clone(),
        vehicles: vec![vehicle("v1", Some(120)), vehicle("v2", Some(900)), vehicle("v3", None), vehicle("v4", Some(0))],
        unmatched_trips: 0,
    };
    let s = summarize(&day, 15);
    assert_eq!(s.vehicles, 4);
    assert_eq!((s.counts.on_time, s.counts.late, s.counts.very_late, s.counts.no_prediction), (1, 1, 1, 1));
    assert!(!s.late_are_predictions);
    let keys: Vec<_> = s.late.iter().map(|l| l.key.as_str()).collect();
    assert_eq!(keys, vec!["v2", "v1"], "worst first; the bus number falls back to the vehicle id");
    assert_eq!(s.late[0].label.as_deref(), Some("v2"));

    let no_positions = LiveDay { vehicles: vec![], ..day };
    let s = summarize(&no_positions, 15);
    assert!(s.late_are_predictions);
    assert_eq!(s.late.len(), 1);
    assert_eq!(s.late[0].key, "s1@28800");
    assert_eq!(summarize(&no_positions, 0).late.len(), 0, "the limit holds");
}
