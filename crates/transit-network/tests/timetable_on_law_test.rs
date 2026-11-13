//! The planner's timetable read for a date (D-155) — the laws, from the user's intent, before the
//! implementation:
//!
//! * the timetable itself is date-free (one route, one direction, one
//!   service combination): the stops' events do not change it;
//! * read for a date, it keeps every column, and a column whose stop is
//!   not open that date is marked unserved, with the stop's status
//!   when the stop is there at all (closed, proposed, retired);
//! * each trip's time at an unserved column is blank that date, every
//!   other time kept, and a trip left with fewer than two times does
//!   not run that date — the day a dated day serves (`materialize`);
//! * the legend counts the trips that run that date;
//! * a network whose stops carry no events reads the same on every date.

use chrono::NaiveDate;
use proptest::prelude::*;
use transit_network::stop_record::stop_on;
use transit_network::{timetable, timetable_on, Network, Timetable};
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

/// The timetables the fixture holds: route 10 on weekdays and Sundays,
/// route 40 both ways.
const ASKS: [(&str, &str, &str); 4] = [("10", "0", "WK"), ("10", "0", "SUN"), ("40", "0", "WK"), ("40", "1", "WK")];

fn ask(net: &Network, which: usize) -> Timetable {
    let (route, direction, members) = ASKS[which];
    timetable(net, route, Some(direction), &[members.to_string()]).unwrap()
}

proptest! {
    /// The dated timetable is the date-free one with each column whose
    /// stop is not open that date marked, its times blank, and the
    /// trips left with fewer than two times taken out.
    #[test]
    fn a_dated_timetable_marks_what_is_not_served(
        closures in proptest::collection::vec((0usize..6, 0u8..3, 0u32..14, proptest::option::of(1u32..6)), 0..4),
        which in 0usize..ASKS.len(),
        d in 0u32..16,
    ) {
        let base = fixture();
        let net = closed(&base, &closures);
        let free = ask(&net, which);
        prop_assert_eq!(&free, &ask(&base, which), "the date-free timetable ignores the events");

        let dated = timetable_on(&net, free.clone(), day(d));
        let on = |stop: &str| stop_on(&net, stop, day(d));
        let unserved: Vec<bool> = free.stops.iter().map(|c| on(&c.stop).is_none_or(|s| s.status != Status::Open)).collect();
        prop_assert_eq!(dated.stops.len(), free.stops.len(), "every column is kept");
        for (i, (got, was)) in dated.stops.iter().zip(&free.stops).enumerate() {
            prop_assert_eq!((&got.stop, &got.name, got.timepoint), (&was.stop, &was.name, was.timepoint));
            prop_assert_eq!(got.unserved, unserved[i], "column {} is marked when its stop is not open", got.stop);
            let status = on(&got.stop).map(|s| s.status).filter(|s| *s != Status::Open);
            prop_assert_eq!(got.status, status);
        }
        let want: Vec<_> = free
            .trips
            .iter()
            .filter_map(|t| {
                let times: Vec<Option<u32>> = t.times.iter().zip(&unserved).map(|(x, u)| if *u { None } else { *x }).collect();
                (times.iter().flatten().count() >= 2).then_some((t.trip.clone(), t.start_secs, times, t.run_secs))
            })
            .collect();
        prop_assert_eq!(dated.trips.len(), want.len(), "the trips that still run, in order");
        for (got, (trip, start, times, run)) in dated.trips.iter().zip(&want) {
            prop_assert_eq!((&got.trip, got.start_secs, &got.times), (trip, *start, times));
            prop_assert!(got.run_secs <= *run, "a trip cut short runs no longer");
        }
        for p in &dated.patterns {
            prop_assert_eq!(p.trips as usize, dated.trips.iter().filter(|t| t.pattern == p.pattern).count());
        }
        // a stop open on the date is never marked, and with nothing
        // marked the day reads as the date-free timetable
        if !unserved.iter().any(|u| *u) {
            prop_assert_eq!(dated, free);
        }
    }
}

#[test]
fn stops_with_no_events_read_the_same_on_every_date() {
    let net = fixture();
    for which in 0..ASKS.len() {
        let free = ask(&net, which);
        for d in 0..16 {
            assert_eq!(timetable_on(&net, free.clone(), day(d)), free);
        }
    }
}

#[test]
fn a_closed_stop_keeps_its_column_marked_closed() {
    let base = fixture();
    // C closed 2026-10-23 only; on route 10 weekdays only pat-10-0-2
    // visits it (A, C, B)
    let c = base.stops.keys().position(|k| k == "C").unwrap();
    let net = closed(&base, &[(c, 0, 4, Some(1))]);
    let free = ask(&net, 0);
    let col = free.stops.iter().position(|s| s.stop == "C").expect("C is a column");
    assert!(free.trips.iter().any(|t| t.times[col].is_some()), "trips stop at C on the date-free timetable");

    let dated = timetable_on(&net, free.clone(), day(4));
    assert!(dated.stops[col].unserved && dated.stops[col].status == Some(Status::Closed), "{:?}", dated.stops[col]);
    assert!(dated.trips.iter().all(|t| t.times[col].is_none()), "no trip stops at C that day");
    assert_eq!(dated.trips.len(), free.trips.len(), "an A–C–B trip still runs A to B");
    assert_eq!(timetable_on(&net, free.clone(), day(5)), free, "C is back the next day");
}
