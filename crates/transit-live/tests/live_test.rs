//! The live document's laws: one classification rule under one
//! window, loop-safe visit lookup, and the summary's two modes
//! (late buses when positions exist, late trips when they do not).

use transit_live::{classify, summarize, LiveDay, LiveStatus, LiveStop, LiveTrip, LiveVehicle, OnTimeWindow};

#[test]
fn classification_follows_the_window_edges() {
    let w = OnTimeWindow::default();
    assert_eq!(classify(None, &w), LiveStatus::NoPrediction);
    assert_eq!(classify(Some(0), &w), LiveStatus::OnTime);
    assert_eq!(classify(Some(60), &w), LiveStatus::OnTime, "the edge is inclusive");
    assert_eq!(classify(Some(61), &w), LiveStatus::Late);
    assert_eq!(classify(Some(300), &w), LiveStatus::Late);
    assert_eq!(classify(Some(301), &w), LiveStatus::VeryLate);
    let grace = OnTimeWindow { early_secs: 60, ..w };
    assert_eq!(classify(Some(-60), &grace), LiveStatus::OnTime);
    assert_eq!(classify(Some(-61), &grace), LiveStatus::Early);
}

/// Early is not on time (the user's decision: 0 s): under the default
/// window ANY earliness is early — a bus that leaves before its time
/// is a missed bus. QAF-71: the 09:25 034 at -91 s read on time.
#[test]
fn under_the_default_window_any_earliness_is_early() {
    let w = OnTimeWindow::default();
    assert_eq!(w.early_secs, 0);
    for d in [-1, -91, -600] {
        assert_eq!(classify(Some(d), &w), LiveStatus::Early, "{d} s early");
    }
}

/// A document minted without an early bound reads the default — the
/// strict one, never a permissive fallback.
#[test]
fn a_window_without_an_early_bound_is_strict() {
    let w: OnTimeWindow = serde_json::from_str(r#"{"late_secs":60,"very_late_secs":300}"#).unwrap();
    assert_eq!(classify(Some(-1), &w), LiveStatus::Early);
}

fn stop(stop: &str, scheduled: u32, delay: i32) -> LiveStop {
    LiveStop {
        stop: stop.into(),
        scheduled_departure: Some(scheduled),
        arrival_delay: Some(delay),
        departure_delay: Some(delay),
        predicted_departure: Some((scheduled as i32 + delay) as u32),
        skipped: false,
    }
}

fn trip(id: &str, start: u32, worst: i32, stops: Vec<LiveStop>) -> LiveTrip {
    LiveTrip { date: None,
        trip: id.into(),
        start_secs: start,
        route: Some("10".into()),
        route_name: Some("10".into()),
        worst_delay_secs: Some(worst),
        status: classify(Some(worst), &OnTimeWindow::default()),
        canceled: false,
        stops,
    }
}

#[test]
fn a_loop_visit_resolves_by_its_scheduled_time() {
    // A loop: the trip visits A at 08:00 and again at 08:30.
    let day = LiveDay { kind: Default::default(), schema_version: Default::default(),
        window: OnTimeWindow::default(),
        trips: vec![trip("s1", 28800, 240, vec![stop("A", 28800, 60), stop("B", 29400, 120), stop("A", 30600, 240)])],
        vehicles: vec![],
        unmatched_trips: 0,
        alerts: vec![],
        unmatched_alerts: 0,
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
    let vehicle = |key: &str, delay: Option<i32>| LiveVehicle { date: None,
        key: key.into(),
        vehicle_id: Some(key.into()),
        label: None,
        lat: 0.0,
        lon: 0.0,
        bearing: None,
        route: Some("10".into()),
        route_name: Some("10".into()),
        next_stop: None,
        trip: None,
        start_secs: None,
        delay_secs: delay,
        status: classify(delay, &w),
    };
    let trips = vec![trip("s1", 28800, 400, vec![]), trip("s2", 29000, 30, vec![])];
    let day = LiveDay { kind: Default::default(), schema_version: Default::default(),
        window: w,
        trips: trips.clone(),
        vehicles: vec![vehicle("v1", Some(120)), vehicle("v2", Some(900)), vehicle("v3", None), vehicle("v4", Some(0))],
        unmatched_trips: 0,
        alerts: vec![],
        unmatched_alerts: 0,
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

/// A trip instance is (trip, service date, start): the live document
/// carries the date a feed states (GTFS-rt `start_date`), and a reader
/// asks for one date's view. An instance stating no date is the
/// current service day's (the GTFS-rt spec's reading) — in that day's
/// view only. Vehicles and trip-scoped alerts follow the same rule;
/// an alert scoped to no trip speaks on every date.
#[test]
fn a_dates_view_holds_that_dates_instances() {
    use transit_live::{AlertScope, LiveAlert};
    let dated = |date: Option<&str>| LiveTrip { date: date.map(Into::into), ..trip("t", 88_500, 60, vec![stop("A", 88_500, 60)]) };
    let vehicle = |key: &str, date: Option<&str>| LiveVehicle {
        key: key.into(),
        vehicle_id: None,
        label: None,
        lat: 0.0,
        lon: 0.0,
        bearing: None,
        route: None,
        route_name: None,
        next_stop: None,
        trip: Some("t".into()),
        date: date.map(Into::into),
        start_secs: Some(88_500),
        delay_secs: None,
        status: LiveStatus::NoPrediction,
    };
    let alert = |id: &str, scope: AlertScope| LiveAlert { id: id.into(), scopes: vec![scope], ..Default::default() };
    let day = LiveDay {
        kind: Default::default(),
        schema_version: Default::default(),
        window: OnTimeWindow::default(),
        trips: vec![dated(Some("2026-10-23")), dated(None)],
        vehicles: vec![vehicle("fri", Some("2026-10-23")), vehicle("now", None)],
        unmatched_trips: 0,
        alerts: vec![
            alert("fri-trip", AlertScope { trip: Some("t".into()), date: Some("2026-10-23".into()), ..Default::default() }),
            alert("now-trip", AlertScope { trip: Some("t".into()), ..Default::default() }),
            alert("stop", AlertScope { stop: Some("A".into()), ..Default::default() }),
        ],
        unmatched_alerts: 0,
    };
    let fri = day.on("2026-10-23", "2026-10-24");
    assert_eq!(fri.trips.iter().map(|t| t.date.as_deref()).collect::<Vec<_>>(), [Some("2026-10-23")]);
    assert_eq!(fri.vehicles.iter().map(|v| v.key.as_str()).collect::<Vec<_>>(), ["fri"]);
    assert_eq!(fri.alerts.iter().map(|a| a.id.as_str()).collect::<Vec<_>>(), ["fri-trip", "stop"]);
    let sat = day.on("2026-10-24", "2026-10-24");
    assert_eq!(sat.trips.iter().map(|t| t.date.as_deref()).collect::<Vec<_>>(), [None]);
    assert_eq!(sat.vehicles.iter().map(|v| v.key.as_str()).collect::<Vec<_>>(), ["now"]);
    assert_eq!(sat.alerts.iter().map(|a| a.id.as_str()).collect::<Vec<_>>(), ["now-trip", "stop"]);
    // The live document is v2: the date is part of the instance.
    assert_eq!(serde_json::to_value(&day).unwrap()["schema_version"], 2);
}
