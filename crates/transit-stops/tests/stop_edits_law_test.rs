//! The edits of the stops document (D-144, D-147, D-153) — the laws, from the user's intent, before the
//! implementation:
//!
//! * an event added to a stop takes its place by date, and nothing
//!   else moves (the Stops stage's close and reopen);
//! * publishing plan P from its first date opens each stop P proposed:
//!   from that date (or from the proposal's own date, when it is later)
//!   the stop is open, at the proposal's point with its details; before
//!   it, the stop is what it was;
//! * nothing else changes: another plan's proposals, open stops, and a
//!   proposal whose history has gone on (a later event already recorded)
//!   keep their events;
//! * the answer names the stops it opened, and the document still
//!   passes what a write must pass;
//! * publishing again opens nothing more.

use chrono::NaiveDate;
use proptest::prelude::*;
use transit_stops::{Details, Event, EventKind, Status, Stops};

fn day(n: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(2027, 1, 1).unwrap() + chrono::Days::new(n as u64)
}

fn date(n: u32) -> String {
    day(n).format("%Y-%m-%d").to_string()
}

/// One stop: 0 open, 1 proposed for P, 2 proposed for Q, 3 proposed for
/// P and then moved.
fn stop(kind: u8, on: u32, i: usize) -> Vec<Event> {
    let point = |e: Event| Event { lat: Some(45.0 + i as f64 / 100.0), lon: Some(-122.0), ..e };
    let named = Details { name: Some(format!("stop {i}")), wheelchair: Some(i % 2 == 0), ..Details::default() };
    let proposed = |plan: &str| point(Event { on: Some(date(on)), event: EventKind::Proposed, plan: Some(plan.into()), details: named.clone(), ..Event::default() });
    match kind {
        0 => vec![point(Event { event: EventKind::Opened, details: named.clone(), ..Event::default() })],
        1 => vec![proposed("P")],
        2 => vec![proposed("Q")],
        _ => vec![proposed("P"), Event { on: Some(date(on + 3)), event: EventKind::Moved, lat: Some(46.0), lon: Some(-122.0), ..Event::default() }],
    }
}

fn arb_doc() -> impl Strategy<Value = Stops> {
    proptest::collection::vec((0u8..4, 0u32..20), 1..6).prop_map(|stops| Stops {
        stops: stops.into_iter().enumerate().map(|(i, (kind, on))| (format!("s{i}"), stop(kind, on, i))).collect(),
        ..Stops::default()
    })
}

proptest! {
    #[test]
    fn publishing_a_plan_opens_what_it_proposed(doc in arb_doc(), first in 0u32..30) {
        let doc = doc.checked().expect("the generated document is valid");
        let (next, opened) = doc.open_plan("P", day(first));
        prop_assert!(next.clone().checked().is_ok(), "{:?}", next.clone().checked());
        let want: Vec<String> = doc
            .stops
            .iter()
            .filter(|(_, e)| e.len() == 1 && e[0].event == EventKind::Proposed && e[0].plan.as_deref() == Some("P"))
            .map(|(id, _)| id.clone())
            .collect();
        prop_assert_eq!(&opened, &want, "the answer names the stops it opened, in id order");
        for (id, events) in &doc.stops {
            if !opened.contains(id) {
                prop_assert_eq!(&next.stops[id], events, "stop {} keeps its events", id);
                continue;
            }
            let proposed_on = NaiveDate::parse_from_str(events[0].on.as_ref().unwrap(), "%Y-%m-%d").unwrap();
            let opens = proposed_on.max(day(first));
            for d in 0..40 {
                let (was, now) = (doc.stop_on(id, day(d)), next.stop_on(id, day(d)));
                if day(d) < opens {
                    prop_assert_eq!(now, was, "before it opens, stop {} is what it was", id);
                } else {
                    let (was, now) = (was.unwrap(), now.unwrap());
                    prop_assert_eq!(now.status, Status::Open);
                    prop_assert_eq!((now.lat, now.lon, &now.details), (was.lat, was.lon, &was.details), "at the proposal's point, with its details");
                }
            }
        }
        let (again, none) = next.open_plan("P", day(first));
        prop_assert_eq!(again, next, "publishing again changes nothing");
        prop_assert!(none.is_empty());
    }
}

#[test]
fn a_proposal_opens_on_the_plans_first_date() {
    let doc = Stops { stops: [("n".to_string(), stop(1, 5, 0))].into(), ..Stops::default() }.checked().unwrap();
    let (next, opened) = doc.open_plan("P", day(10));
    assert_eq!(opened, vec!["n".to_string()]);
    assert_eq!(next.stops["n"].len(), 2);
    assert_eq!(next.stops["n"][1], Event { on: Some(date(10)), event: EventKind::Opened, ..Event::default() });
    assert_eq!(next.stop_on("n", day(9)).unwrap().status, Status::Proposed);
    assert_eq!(next.stop_on("n", day(10)).unwrap().status, Status::Open);
    // a proposal dated after the plan's first date opens on its own date
    let (late, _) = doc.open_plan("P", day(1));
    assert_eq!(late.stops["n"][1].on, Some(date(5)));
}

// ---------------------------------------------------------------------
// an edit: one event added to one stop (the Stops stage's close and
// reopen, and later its create, move, and change)
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
        let added = Event { on: Some(date(on)), event: EventKind::Closed, note: Some("added".into()), ..Event::default() };
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
fn a_closure_added_closes_the_stop_and_a_new_id_begins_a_stop() {
    let doc = Stops { stops: [("o".to_string(), stop(0, 0, 0))].into(), ..Stops::default() };
    let closed = doc.add_event("o", Event { on: Some(date(3)), event: EventKind::Closed, until: Some(date(5)), ..Event::default() });
    let closed = closed.checked().unwrap();
    assert_eq!(closed.stop_on("o", day(4)).unwrap().status, Status::Closed);
    assert_eq!(closed.stop_on("o", day(5)).unwrap().status, Status::Open);
    let reopened = closed.add_event("o", Event { on: Some(date(4)), event: EventKind::Reopened, ..Event::default() });
    assert!(reopened.checked().is_ok(), "a reopening on a closed date is a valid write");
    let fresh = doc.add_event("n", stop(1, 2, 1)[0].clone());
    assert_eq!(fresh.stops["n"], stop(1, 2, 1));
}
