//! A pick's stops are a copy, refreshed from the stops document
//! (docs/units/stop-lifecycle.md item 1, D-156) — the laws, derived from
//! the user's intent and written before the implementation:
//!
//! * the copy never wins: after a refresh, each stop the document holds
//!   answers on every date of the pick's window exactly as the document
//!   does, whatever the pick's copy held;
//! * a stop the document does not hold is left as the pick had it, and
//!   a retired one is added only where the pick already has it (a
//!   pattern still visiting it);
//! * refreshing twice changes nothing;
//! * the network document keeps its shape: seeding a stops document from
//!   a network and refreshing the network from it gives the same network
//!   back, byte for byte — no stop gains an event it did not need;
//! * a draft's own proposed stops are in its pick, opening as proposed;
//!   another plan's are not.

use std::collections::BTreeMap;

use chrono::NaiveDate;
use proptest::prelude::*;
use transit_network::stop_record::{refresh, seed, stop_on};
use transit_network::places::{StopInfo, StopKind};
use transit_network::{Network, Stop};
use transit_stops::{Details, Event, EventKind, Status, Stops};

fn day(n: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(2027, 1, 1).unwrap() + chrono::Days::new(n as u64)
}

fn date(n: u32) -> String {
    day(n).format("%Y-%m-%d").to_string()
}

fn network(stops: BTreeMap<String, Stop>) -> Network {
    Network { stops, ..Network::default() }
}

// ---------------------------------------------------------------------
// generators
// ---------------------------------------------------------------------

fn arb_stop(ids: Vec<String>) -> impl Strategy<Value = Stop> {
    (
        "[A-Za-z &]{0,12}",
        -90.0f64..90.0,
        -180.0f64..180.0,
        proptest::option::of(proptest::sample::select(ids)),
        any::<bool>(),
        any::<bool>(),
        proptest::option::of("[0-9]{1,5}"),
        proptest::option::of(any::<bool>()),
        proptest::option::of("[A-C]"),
    )
        .prop_map(|(name, lat, lon, parent, station, transfer, code, wheelchair, fare_zone)| Stop {
            name,
            lat,
            lon,
            parent,
            kind: if station { StopKind::Station } else { StopKind::Stop },
            info: StopInfo { code, wheelchair, fare_zone, ..StopInfo::default() },
            transfer,
            ..Stop::default()
        })
}

fn arb_network() -> impl Strategy<Value = Network> {
    proptest::collection::btree_set("[a-z]{1,4}", 1..8).prop_flat_map(|ids| {
        let ids: Vec<String> = ids.into_iter().collect();
        proptest::collection::vec(arb_stop(ids.clone()), ids.len())
            .prop_map(move |stops| network(ids.iter().cloned().zip(stops).collect()))
    })
}

#[derive(Debug, Clone)]
enum Step {
    Close(Option<u32>),
    Reopen,
    Move(Option<u32>, i32),
    Rename,
    Retire,
}

fn arb_step() -> impl Strategy<Value = Step> {
    prop_oneof![
        proptest::option::of(1u32..40).prop_map(Step::Close),
        Just(Step::Reopen),
        (proptest::option::of(1u32..40), -300i32..300).prop_map(|(u, d)| Step::Move(u, d)),
        Just(Step::Rename),
        Just(Step::Retire),
    ]
}

/// A stops document: each stop opened (undated, or on a date) or
/// proposed for a plan, then steps at non-decreasing dates — each kept
/// only where the document still passes `checked`.
fn arb_stops() -> impl Strategy<Value = Stops> {
    let history = (0u8..4, 0u32..60, proptest::collection::vec((0u32..30, arb_step()), 0..6));
    proptest::collection::btree_map("[a-z]{1,4}", history, 1..8).prop_map(|stops| {
        let mut doc = Stops::default();
        for (id, (start, first, steps)) in stops {
            let opening = Event {
                lat: Some(45.6),
                lon: Some(-122.6),
                details: Details { name: Some(format!("{id} stop")), code: Some(id.clone()), ..Details::default() },
                ..Event::default()
            };
            let mut events = vec![match start {
                0 => Event { event: EventKind::Opened, ..opening },
                1 => Event { event: EventKind::Opened, on: Some(date(first)), ..opening },
                2 => Event { event: EventKind::Proposed, on: Some(date(first)), plan: Some("fall".into()), ..opening },
                _ => Event { event: EventKind::Proposed, on: Some(date(first)), plan: Some("other".into()), ..opening },
            }];
            let mut at = first;
            for (gap, step) in steps {
                at += gap;
                let on = Some(date(at));
                let e = match step {
                    Step::Close(u) => Event { event: EventKind::Closed, on, until: u.map(|u| date(at + u)), ..Event::default() },
                    Step::Reopen => Event { event: EventKind::Reopened, on, ..Event::default() },
                    Step::Move(u, d) => Event {
                        event: EventKind::Moved,
                        on,
                        until: u.map(|u| date(at + u)),
                        lat: Some(45.6 + d as f64 * 1e-5),
                        lon: Some(-122.6),
                        ..Event::default()
                    },
                    Step::Rename => Event {
                        event: EventKind::Changed,
                        on,
                        details: Details { name: Some(format!("{id} at {at}")), ..Details::default() },
                        ..Event::default()
                    },
                    Step::Retire => Event { event: EventKind::Retired, on, ..Event::default() },
                };
                events.push(e);
                let trial = Stops { stops: BTreeMap::from([(id.clone(), events.clone())]), ..Stops::default() };
                if trial.checked().is_err() {
                    events.pop();
                }
            }
            doc.stops.insert(id, events);
        }
        doc.checked().expect("every kept history passes")
    })
}

/// A window: its first date and, maybe, the first date after it.
fn arb_window() -> impl Strategy<Value = (u32, Option<u32>)> {
    (0u32..120, proptest::option::of(1u32..150)).prop_map(|(from, len)| (from, len.map(|l| from + l)))
}

/// The dates a law looks at in a window (an open-ended one, 400 days).
fn window_days(from: u32, until: Option<u32>) -> impl Iterator<Item = u32> {
    from..until.unwrap_or(from + 400)
}

fn proposal_of(events: &[Event]) -> Option<&str> {
    events.first().filter(|e| e.event == EventKind::Proposed).and_then(|e| e.plan.as_deref())
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 128, ..ProptestConfig::default() })]

    /// The network document keeps its shape: a network's stops seeded
    /// into a stops document, and the network refreshed from it, are the
    /// same network, byte for byte.
    #[test]
    fn seeding_and_refreshing_gives_the_network_back(net in arb_network(), (from, until) in arb_window()) {
        let doc = seed(&net).checked().map_err(|r| TestCaseError::fail(format!("{r:?}")))?;
        let back = refresh(&net, &doc, day(from), until.map(day), None);
        prop_assert_eq!(serde_json::to_string(&back).unwrap(), serde_json::to_string(&net).unwrap());
    }

    /// The copy never wins: each stop the document holds answers on
    /// every date of the window as the document does — a draft's own
    /// proposals as proposed, another plan's left out — and a stop the
    /// document does not hold is the pick's own.
    #[test]
    fn a_refreshed_stop_answers_like_the_document(
        doc in arb_stops(),
        copy in arb_network(),
        (from, until) in arb_window(),
    ) {
        let net = refresh(&copy, &doc, day(from), until.map(day), Some("fall"));
        for (id, events) in &doc.stops {
            if proposal_of(events) == Some("other") {
                prop_assert_eq!(net.stops.get(id), copy.stops.get(id), "another plan's proposal: {}", id);
                continue;
            }
            for d in window_days(from, until) {
                let held = net.stops.contains_key(id);
                let want = doc.stop_on(id, day(d));
                if want.as_ref().is_some_and(|s| s.status != Status::Retired) {
                    prop_assert!(held, "stop {} exists on {} but is not in the pick", id, date(d));
                }
                if held {
                    prop_assert_eq!(stop_on(&net, id, day(d)), want, "stop {} on {}", id, date(d));
                }
            }
        }
        for (id, stop) in &copy.stops {
            if !doc.stops.contains_key(id) {
                prop_assert_eq!(net.stops.get(id), Some(stop), "a stop the document does not hold: {}", id);
            }
        }
    }

    /// Refreshing twice changes nothing.
    #[test]
    fn refreshing_twice_changes_nothing(doc in arb_stops(), copy in arb_network(), (from, until) in arb_window()) {
        let once = refresh(&copy, &doc, day(from), until.map(day), Some("fall"));
        let twice = refresh(&once, &doc, day(from), until.map(day), Some("fall"));
        prop_assert_eq!(serde_json::to_string(&twice).unwrap(), serde_json::to_string(&once).unwrap());
    }
}

#[test]
fn a_stop_open_all_window_carries_no_events() {
    let doc: Stops = serde_json::from_value(serde_json::json!({"stops": {
        "a": [{"event": "opened", "lat": 45.0, "lon": -122.0, "name": "A"},
              {"on": "2027-01-05", "event": "closed", "until": "2027-01-10"}]
    }}))
    .unwrap();
    let after = refresh(&Network::default(), &doc, day(20), None, None);
    assert!(after.stops["a"].events.is_empty(), "the closure ended before the window");
    let during = refresh(&Network::default(), &doc, day(0), Some(day(30)), None);
    assert_eq!(during.stops["a"].events.len(), 2, "the closure falls inside the window: {:?}", during.stops["a"].events);
    // day(n) is January 1 + n: closed from the 5th (day 4) through the
    // 9th (day 8), open again on its until, the 10th (day 9)
    assert_eq!(stop_on(&during, "a", day(3)).unwrap().status, Status::Open);
    assert_eq!(stop_on(&during, "a", day(4)).unwrap().status, Status::Closed);
    assert_eq!(stop_on(&during, "a", day(8)).unwrap().status, Status::Closed);
    assert_eq!(stop_on(&during, "a", day(9)).unwrap().status, Status::Open);
    let written = serde_json::to_value(&after).unwrap();
    assert!(written["stops"]["a"].get("events").is_none(), "no events, no field: {written}");
}

#[test]
fn the_record_is_the_stop_as_the_window_opens() {
    let doc: Stops = serde_json::from_value(serde_json::json!({"stops": {
        "a": [{"event": "opened", "lat": 45.0, "lon": -122.0, "name": "Old name"},
              {"on": "2027-01-05", "event": "changed", "name": "New name"}],
        "b": [{"on": "2027-02-01", "event": "opened", "lat": 45.1, "lon": -122.1, "name": "Later"}]
    }}))
    .unwrap();
    let net = refresh(&Network::default(), &doc, day(10), None, None);
    assert_eq!(net.stops["a"].name, "New name");
    assert_eq!(net.stops["b"].name, "Later", "a stop opening inside the window is in the pick, as it opens");
    assert_eq!(stop_on(&net, "b", day(11)), None, "and not there before it opens");
}
