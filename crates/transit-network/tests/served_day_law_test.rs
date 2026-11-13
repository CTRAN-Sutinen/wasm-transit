//! A dated day serves what is open (D-155) — the laws, from the user's intent, before the
//! implementation:
//!
//! * a trip on a date stops only where the stop is open on that date:
//!   a closed, proposed, retired, or not-yet-opened stop loses its stop
//!   time, every other stop keeps its own, unmoved — the day the
//!   network would have materialized without the stop's events, with
//!   those visits taken out;
//! * a pattern whose first or last stop is closed runs to or from its
//!   nearest open stop;
//! * a trip left with fewer than two served stops does not run that
//!   date (it carries no one, and GTFS needs two stop times);
//! * a network whose stops carry no events materializes as before;
//! * the planner is warned, never refused: an endpoint not served and
//!   a pattern left with under two served stops, each with its dates.

use chrono::NaiveDate;
use proptest::prelude::*;
use transit_network::stop_record::stop_on;
use transit_network::{materialize, validation_warnings, Network, NetworkWarning};
use transit_stops::{Event, EventKind, Status};

fn fixture() -> Network {
    let text = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../web/public/caps/sample-network-load.json")).unwrap();
    let load: serde_json::Value = serde_json::from_str(&text).unwrap();
    serde_json::from_value(load["network"].clone()).unwrap()
}

fn day(n: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 10, 19).unwrap() + chrono::Days::new(n as u64)
}

fn date(n: u32) -> String {
    day(n).format("%Y-%m-%d").to_string()
}

/// A stop's events: open from before any record, then a closure or a
/// retirement, maybe temporary — or a stop opening only on a date.
fn with_events(net: &Network, id: &str, kind: u8, on: u32, len: Option<u32>) -> Vec<Event> {
    let stop = &net.stops[id];
    let opened = Event { event: EventKind::Opened, lat: Some(stop.lat), lon: Some(stop.lon), ..Event::default() };
    match kind {
        0 => vec![opened, Event { event: EventKind::Closed, on: Some(date(on)), until: len.map(|l| date(on + l)), ..Event::default() }],
        1 => vec![opened, Event { event: EventKind::Retired, on: Some(date(on)), ..Event::default() }],
        _ => vec![Event { on: Some(date(on)), ..opened }],
    }
}

fn arb_closures() -> impl Strategy<Value = Vec<(usize, u8, u32, Option<u32>)>> {
    proptest::collection::vec((0usize..6, 0u8..3, 0u32..14, proptest::option::of(1u32..6)), 0..4)
}

fn closed(net: &Network, closures: &[(usize, u8, u32, Option<u32>)]) -> Network {
    let mut net = net.clone();
    let ids: Vec<String> = net.stops.keys().cloned().collect();
    for &(i, kind, on, len) in closures {
        let id = &ids[i % ids.len()];
        let events = with_events(&net, id, kind, on, len);
        net.stops.get_mut(id).unwrap().events = events;
    }
    net
}

proptest! {
    /// The day with the stops' events is the day without them, each
    /// trip's visits to a stop not open that date taken out, and a trip
    /// left with fewer than two dropped.
    #[test]
    fn a_dated_day_serves_only_open_stops(closures in arb_closures(), d in 0u32..16) {
        let base = fixture();
        let net = closed(&base, &closures);
        let open = |stop: &str| stop_on(&net, stop, day(d)).is_some_and(|s| s.status == Status::Open);
        let want: Vec<_> = materialize(&base, day(d)).unwrap().into_iter().filter_map(|mut t| {
            t.stop_times.retain(|st| open(&st.stop));
            (t.stop_times.len() >= 2).then_some(t)
        }).collect();
        prop_assert_eq!(materialize(&net, day(d)).unwrap(), want);
    }
}

#[test]
fn stops_with_no_events_materialize_as_before() {
    let net = fixture();
    let a: Vec<_> = (0..16).map(|d| materialize(&net, day(d)).unwrap()).collect();
    assert!(a.iter().any(|t| !t.is_empty()), "the fixture runs on some of these days");
    assert!(net.stops.values().all(|s| s.events.is_empty()));
}

#[test]
fn a_closed_last_stop_ends_the_trip_one_stop_early() {
    let base = fixture();
    // pattern pat-10-0-2 runs A, C, B: B closed on 2026-10-23 only
    let net = closed(&base, &[(base.stops.keys().position(|k| k == "B").unwrap(), 0, 4, Some(1))]);
    let trips = materialize(&net, day(4)).unwrap();
    let via_c: Vec<_> = trips.iter().filter(|t| t.pattern == "pat-10-0-2").collect();
    assert!(!via_c.is_empty(), "the A–C–B pattern runs on 2026-10-23");
    for t in via_c {
        let stops: Vec<&str> = t.stop_times.iter().map(|s| s.stop.as_str()).collect();
        assert_eq!(stops, vec!["A", "C"], "it runs to its nearest open stop");
    }
    assert!(materialize(&base, day(4)).unwrap().iter().any(|t| t.pattern == "pat-10-0-1"), "the weekday A–B pattern runs on 2026-10-23");
    assert!(trips.iter().all(|t| t.pattern != "pat-10-0-1"), "an A–B trip left with one stop does not run");
    // after the closure (until 2026-10-24), the next weekday runs as it
    // would with no events at all
    assert_eq!(materialize(&net, day(7)).unwrap(), materialize(&base, day(7)).unwrap(), "B is back on Monday");
    assert!(materialize(&net, day(7)).unwrap().iter().any(|t| t.pattern == "pat-10-0-1"));
}

#[test]
fn the_planner_is_warned_with_the_dates() {
    let base = fixture();
    let b = base.stops.keys().position(|k| k == "B").unwrap();
    let net = closed(&base, &[(b, 0, 4, Some(3))]);
    let warnings = validation_warnings(&net);
    let span = (Some(date(4)), Some(date(7)));
    assert!(
        warnings.contains(&NetworkWarning::EndpointNotServed { pattern: "pat-10-0-2".into(), stop: "B".into(), from: span.0.clone(), until: span.1.clone() }),
        "{warnings:?}"
    );
    assert!(
        warnings.contains(&NetworkWarning::UnderTwoServed { pattern: "pat-10-0-0".into(), from: span.0.clone(), until: span.1.clone() }),
        "{warnings:?}"
    );
    assert!(
        !warnings.iter().any(|w| matches!(w, NetworkWarning::UnderTwoServed { pattern, .. } if pattern == "pat-10-0-2")),
        "A and C still serve the A–C–B pattern: {warnings:?}"
    );
    assert!(validation_warnings(&base).iter().all(|w| !matches!(w, NetworkWarning::EndpointNotServed { .. } | NetworkWarning::UnderTwoServed { .. })));
    // a retirement has no end
    let net = closed(&base, &[(b, 1, 4, None)]);
    assert!(validation_warnings(&net).contains(&NetworkWarning::EndpointNotServed {
        pattern: "pat-10-0-2".into(),
        stop: "B".into(),
        from: Some(date(4)),
        until: None
    }));
}
