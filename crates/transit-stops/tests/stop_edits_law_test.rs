//! An edit of the stops document (D-153, D-157) — the law, from the
//! user's intent, before the implementation: an event added to a stop
//! takes its place by date, and nothing else moves (every edit of the
//! stops app is one event added).

use chrono::NaiveDate;
use proptest::prelude::*;
use transit_stops::{Details, Event, EventKind, Status, Stops};

fn day(n: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(2027, 1, 1).unwrap() + chrono::Days::new(n as u64)
}

fn date(n: u32) -> String {
    day(n).format("%Y-%m-%d").to_string()
}

/// One stop: 0 in service from before any record, 1 planned, 2
/// permitted, 3 planned and then moved.
fn stop(kind: u8, on: u32, i: usize) -> Vec<Event> {
    let point = |e: Event| Event { lat: Some(45.0 + i as f64 / 100.0), lon: Some(-122.0), ..e };
    let named = Details { name: Some(format!("stop {i}")), wheelchair: Some(i % 2 == 0), ..Details::default() };
    let begun = |kind: EventKind| point(Event { on: Some(date(on)), event: kind, details: named.clone(), ..Event::default() });
    match kind {
        0 => vec![point(Event { event: EventKind::InService, details: named.clone(), ..Event::default() })],
        1 => vec![begun(EventKind::Planned)],
        2 => vec![begun(EventKind::Permitted)],
        _ => vec![begun(EventKind::Planned), Event { on: Some(date(on + 3)), event: EventKind::Moved, lat: Some(46.0), lon: Some(-122.0), ..Event::default() }],
    }
}

fn arb_doc() -> impl Strategy<Value = Stops> {
    proptest::collection::vec((0u8..4, 0u32..20), 1..6).prop_map(|stops| Stops {
        stops: stops.into_iter().enumerate().map(|(i, (kind, on))| (format!("s{i}"), stop(kind, on, i))).collect(),
        ..Stops::default()
    })
}

// ---------------------------------------------------------------------
// an edit: one event added to one stop (each of the stops app's
// edits)
// ---------------------------------------------------------------------

fn on_of(e: &Event) -> Option<NaiveDate> {
    e.on.as_deref().map(|d| NaiveDate::parse_from_str(d, "%Y-%m-%d").unwrap())
}

proptest! {
    /// An added event takes its place by date — after every event dated
    /// on or before it, before every later one — and nothing else moves.
    #[test]
    fn an_added_event_takes_its_place_by_date(doc in arb_doc(), which in 0usize..6, on in 0u32..40) {
        let id = format!("s{}", which % doc.stops.len());
        let added = Event { on: Some(date(on)), event: EventKind::Suspended, note: Some("added".into()), ..Event::default() };
        let next = doc.add_event(&id, added.clone());
        for (other, events) in &doc.stops {
            if *other != id {
                prop_assert_eq!(&next.stops[other], events);
            }
        }
        let list = &next.stops[&id];
        let at = list.iter().position(|e| *e == added).expect("the event is there");
        let mut without = list.clone();
        without.remove(at);
        prop_assert_eq!(&without, &doc.stops[&id], "the others keep their order");
        prop_assert!(list[..at].iter().all(|e| on_of(e).is_none_or(|d| d <= day(on))));
        prop_assert!(list[at + 1..].iter().all(|e| on_of(e).is_some_and(|d| d > day(on))));
    }
}

#[test]
fn a_suspension_added_suspends_the_stop_and_a_new_id_begins_a_stop() {
    let doc = Stops { stops: [("o".to_string(), stop(0, 0, 0))].into(), ..Stops::default() };
    let suspended = doc.add_event("o", Event { on: Some(date(3)), event: EventKind::Suspended, until: Some(date(5)), ..Event::default() });
    let suspended = suspended.checked().unwrap();
    assert_eq!(suspended.stop_on("o", day(4)).unwrap().status, Status::Suspended);
    assert_eq!(suspended.stop_on("o", day(5)).unwrap().status, Status::InService);
    let back = suspended.add_event("o", Event { on: Some(date(4)), event: EventKind::InService, ..Event::default() });
    assert!(back.checked().is_ok(), "a return on a suspended date is a valid write");
    let fresh = doc.add_event("n", stop(1, 2, 1)[0].clone());
    assert_eq!(fresh.stops["n"], stop(1, 2, 1));
}
