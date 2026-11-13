//! What a pick reads of the stops document (D-157) — the laws, from the
//! user's intent, before the implementation:
//!
//! * a pick takes each stop's record — its point, name, code, station,
//!   and details — from the stops document as it stands on the pick's
//!   first date, and never writes it; a stop the document does not hold,
//!   or that is not there yet that day, keeps the pick's own record, and
//!   nothing else in the pick changes;
//! * a pick read from a document seeded from it is the pick unchanged;
//! * the planner is warned, never refused, when the pick visits a stop
//!   not in service on its dates: always for a stop planned, permitted,
//!   built, retired, or not there yet; for a suspended one only when the
//!   suspension covers the pick's whole period or has no end.

use chrono::NaiveDate;
use proptest::prelude::*;
use transit_network::stop_record::{read_records, seed, visit_warnings};
use transit_network::{Network, NetworkWarning};
use transit_stops::{Details, Event, EventKind, Status, Stops};

fn fixture() -> Network {
    let text = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../web/public/caps/sample-network-load.json")).unwrap();
    let load: serde_json::Value = serde_json::from_str(&text).unwrap();
    serde_json::from_value(load["network"].clone()).unwrap()
}

fn day(n: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(2027, 1, 1).unwrap() + chrono::Days::new(n as u64)
}

fn date(n: u32) -> String {
    day(n).format("%Y-%m-%d").to_string()
}

fn ev(kind: EventKind, on: u32) -> Event {
    Event { on: Some(date(on)), event: kind, ..Event::default() }
}

/// What may happen to a stop after the seed: a move, a rename, a
/// suspension (with or without an end), a retirement.
#[derive(Debug, Clone)]
enum Change {
    Move(u32, i32),
    Rename(u32),
    Suspend(u32, Option<u32>),
    Retire(u32),
}

fn arb_change() -> impl Strategy<Value = Change> {
    prop_oneof![
        (0u32..60, -300i32..300).prop_map(|(on, d)| Change::Move(on, d)),
        (0u32..60).prop_map(Change::Rename),
        (0u32..60, proptest::option::of(1u32..60)).prop_map(|(on, len)| Change::Suspend(on, len)),
        (0u32..60).prop_map(Change::Retire),
    ]
}

/// The fixture's stops seeded, each given at most one change, and maybe
/// a stop only planned on a day.
fn arb_stops(net: &Network) -> impl Strategy<Value = Stops> {
    let ids: Vec<String> = net.stops.keys().cloned().collect();
    let base = seed(net);
    (proptest::collection::vec(proptest::option::of(arb_change()), ids.len()), proptest::option::of((0usize..6, 0u32..60))).prop_map(
        move |(changes, planned)| {
            let mut doc = base.clone();
            for (id, change) in ids.iter().zip(changes) {
                let Some(change) = change else { continue };
                let e = match change {
                    Change::Move(on, d) => {
                        let s = &doc.stops[id][0];
                        Event { lat: Some(s.lat.unwrap() + d as f64 * 1e-5), lon: s.lon, ..ev(EventKind::Moved, on) }
                    }
                    Change::Rename(on) => Event { details: Details { name: Some(format!("Renamed {on}")), ..Details::default() }, ..ev(EventKind::Changed, on) },
                    Change::Suspend(on, len) => Event { until: len.map(|l| date(on + l)), ..ev(EventKind::Suspended, on) },
                    Change::Retire(on) => ev(EventKind::Retired, on),
                };
                doc = doc.add_event(id, e);
            }
            if let Some((i, on)) = planned {
                let id = &ids[i % ids.len()];
                let s = &doc.stops[id][0];
                doc.stops.insert(id.clone(), vec![Event { lat: s.lat, lon: s.lon, ..ev(EventKind::Planned, on) }]);
            }
            doc.checked().expect("a valid document")
        },
    )
}

/// The pick's stops a pattern visits.
fn visited(net: &Network) -> std::collections::BTreeSet<String> {
    net.patterns.values().flat_map(|p| p.stops.iter().map(|s| s.stop.clone())).collect()
}

proptest! {
    /// Each stop the document holds and has on the first date takes the
    /// document's record; every other stop, and everything else in the
    /// pick, is as it was.
    #[test]
    fn a_pick_takes_each_stops_record_on_its_first_date(stops in arb_stops(&fixture()), first in 0u32..80) {
        let net = fixture();
        let read = read_records(&net, &stops, day(first));
        let mut rest = read.clone();
        rest.stops = net.stops.clone();
        prop_assert_eq!(&rest, &net, "only the stops change");
        for (id, was) in &net.stops {
            let now = &read.stops[id];
            match stops.stop_on(id, day(first)) {
                None => prop_assert_eq!(now, was, "stop {} is not there yet: the pick's own record", id),
                Some(on) => {
                    prop_assert_eq!((now.lat, now.lon), (on.lat, on.lon));
                    prop_assert_eq!(&now.name, &on.details.name.clone().unwrap_or_default());
                    prop_assert_eq!(&now.info.code, &on.details.code);
                    prop_assert_eq!(&now.parent, &on.details.parent);
                }
            }
        }
    }

    /// A stop warns exactly when, on some date of the pick's period, it
    /// is not in service — and, when that is a suspension, the suspension
    /// covers the whole period or has no end.
    #[test]
    fn a_pick_is_warned_of_the_stops_it_visits_not_in_service(stops in arb_stops(&fixture()), first in 0u32..80, len in proptest::option::of(1u32..60)) {
        let net = fixture();
        let until = len.map(|l| day(first + l));
        let warned: std::collections::BTreeSet<String> = visit_warnings(&net, &stops, day(first), until)
            .into_iter()
            .map(|w| match w {
                NetworkWarning::StopNotInService { stop, .. } => stop,
                other => panic!("only stop warnings: {other:?}"),
            })
            .collect();
        let last = until.map_or(first + 150, |u| (u - day(0)).num_days() as u32);
        let want: std::collections::BTreeSet<String> = visited(&net)
            .into_iter()
            .filter(|id| stops.stops.contains_key(id))
            .filter(|id| {
                (first..last).any(|d| match stops.stop_on(id, day(d)) {
                    None => true,
                    Some(s) => match s.status {
                        Status::InService => false,
                        Status::Suspended => {
                            let since = s.since.as_deref().map(|x| NaiveDate::parse_from_str(x, "%Y-%m-%d").unwrap());
                            let back = s.until.as_deref().map(|x| NaiveDate::parse_from_str(x, "%Y-%m-%d").unwrap());
                            back.is_none() || (since.is_none_or(|x| x <= day(first)) && until.is_some_and(|u| back.unwrap() >= u))
                        }
                        _ => true,
                    },
                })
            })
            .collect();
        prop_assert_eq!(warned, want);
    }
}

#[test]
fn a_pick_read_from_its_own_seed_is_unchanged() {
    let net = fixture();
    assert_eq!(read_records(&net, &seed(&net), day(0)), net);
    assert!(visit_warnings(&net, &seed(&net), day(0), None).is_empty());
}

#[test]
fn the_warnings_name_the_stop_its_status_and_the_dates() {
    let net = fixture();
    let mut stops = seed(&net);
    stops = stops.add_event("B", ev(EventKind::Retired, 10));
    stops = stops.add_event("A", Event { until: Some(date(5)), ..ev(EventKind::Suspended, 2) });
    let stops = stops.checked().unwrap();
    let w = visit_warnings(&net, &stops, day(0), Some(day(30)));
    assert_eq!(
        w,
        vec![NetworkWarning::StopNotInService { stop: "B".into(), status: Some(Status::Retired), from: date(10), until: Some(date(30)) }],
        "a short suspension inside the period is real time's, not the pick's"
    );
    // a suspension with no end warns
    let stops = seed(&net).add_event("A", ev(EventKind::Suspended, 2)).checked().unwrap();
    assert_eq!(
        visit_warnings(&net, &stops, day(0), Some(day(30))),
        vec![NetworkWarning::StopNotInService { stop: "A".into(), status: Some(Status::Suspended), from: date(2), until: Some(date(30)) }]
    );
    // the record read: B retired on day 10 still has its point and name
    let read = read_records(&net, &seed(&net).add_event("B", ev(EventKind::Retired, 10)), day(20));
    assert_eq!(read.stops["B"], net.stops["B"]);
}
