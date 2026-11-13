//! The stops document's laws (D-153, D-154, D-157), derived from the
//! user's intent and written before the implementation:
//!
//! * a stop has a life of its own: planned, permitted, built, in
//!   service, suspended for a while, retired (D-157); the events are the
//!   truth — a stop on a date is its events applied in order up to that
//!   date, a temporary one (`until`) only while it lasts — checked
//!   against an oracle that answers each detail as the LAST active event
//!   that sets it, a different reading of the same rule;
//! * the stages only go forward (a stage may be skipped); a suspension
//!   is of a stop in service, and a stop returns to service from a
//!   suspension or goes into service from an earlier stage — it may
//!   also never return, and end retired;
//! * a temporary suspension or move ends on its `until`: from that date
//!   the stop is what it would be without the event;
//! * served is in service, nothing else;
//! * a stop on a date says when its status began, when it ends (a
//!   temporary one), and the note on the event that set it;
//! * a correction edits the event that was wrong: it is the event taken
//!   out and the corrected one placed by date, nothing else moving;
//! * the stops on a date are counted by status;
//! * a document that breaks the model is refused, each reason named;
//! * one document is one byte sequence however its stops and keys are
//!   ordered;
//! * what this repo does not know — the agency's own amenity names, a
//!   field another tool wrote — survives a read and a write.

use std::collections::BTreeMap;

use chrono::NaiveDate;
use proptest::prelude::*;
use serde_json::{json, Value};
use transit_stops::{Details, Event, EventKind, Problem, Status, StopOn, Stops};

fn day(n: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(2027, 1, 1).unwrap() + chrono::Days::new(n as u64)
}

fn date(n: u32) -> String {
    day(n).format("%Y-%m-%d").to_string()
}

fn event(kind: EventKind, on: Option<u32>) -> Event {
    Event { on: on.map(date), event: kind, ..Event::default() }
}

/// A stop begun at a stage, at a point, with a name.
fn begun(kind: EventKind, on: Option<u32>, lat: f64, lon: f64, name: &str) -> Event {
    Event {
        lat: Some(lat),
        lon: Some(lon),
        details: Details { name: Some(name.into()), ..Details::default() },
        ..event(kind, on)
    }
}

/// A stop begun in service.
fn in_service(on: Option<u32>, lat: f64, lon: f64, name: &str) -> Event {
    begun(EventKind::InService, on, lat, lon, name)
}

fn doc(stops: Vec<(&str, Vec<Event>)>) -> Stops {
    Stops { stops: stops.into_iter().map(|(id, e)| (id.to_string(), e)).collect(), ..Stops::default() }
}

const STAGES: [EventKind; 4] = [EventKind::Planned, EventKind::Permitted, EventKind::Built, EventKind::InService];

// ---------------------------------------------------------------------
// the oracle: each answer is the last active event that sets it
// ---------------------------------------------------------------------

fn active(e: &Event, d: NaiveDate) -> bool {
    let p = |s: &String| NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap();
    e.on.as_ref().map_or(true, |on| p(on) <= d) && e.until.as_ref().map_or(true, |u| d < p(u))
}

fn status_of(k: &EventKind) -> Option<Status> {
    match k {
        EventKind::Planned => Some(Status::Planned),
        EventKind::Permitted => Some(Status::Permitted),
        EventKind::Built => Some(Status::Built),
        EventKind::InService => Some(Status::InService),
        EventKind::Suspended => Some(Status::Suspended),
        EventKind::Retired => Some(Status::Retired),
        _ => None,
    }
}

fn oracle(events: &[Event], id: &str, d: NaiveDate) -> Option<StopOn> {
    let live: Vec<&Event> = events.iter().filter(|e| active(e, d)).collect();
    let by = live.iter().rev().find(|e| status_of(&e.event).is_some())?;
    let status = status_of(&by.event)?;
    let point = live.iter().rev().find(|e| e.lat.is_some())?;
    let last = |f: &dyn Fn(&Details) -> Option<Value>| -> Option<Value> { live.iter().rev().find_map(|e| f(&e.details)) };
    let mut details = Details::default();
    details.name = last(&|d| d.name.clone().map(Value::from)).map(|v| v.as_str().unwrap().to_string());
    details.code = last(&|d| d.code.clone().map(Value::from)).map(|v| v.as_str().unwrap().to_string());
    details.wheelchair = last(&|d| d.wheelchair.map(Value::from)).map(|v| v.as_bool().unwrap());
    details.fare_zone = last(&|d| d.fare_zone.clone().map(Value::from)).map(|v| v.as_str().unwrap().to_string());
    let mut keys: Vec<&String> = live.iter().flat_map(|e| e.details.amenities.keys()).collect();
    keys.sort();
    keys.dedup();
    for k in keys {
        let v = live.iter().rev().find_map(|e| e.details.amenities.get(k)).unwrap();
        details.amenities.insert(k.clone(), *v);
    }
    Some(StopOn {
        stop_id: id.to_string(),
        status,
        since: by.on.clone(),
        until: by.until.clone(),
        note: by.note.clone(),
        lat: point.lat.unwrap(),
        lon: point.lon.unwrap(),
        details,
    })
}

/// How far along a stop is: the stages in order, a suspension counting
/// as in service (it was in service to be suspended).
fn rank(s: Status) -> u8 {
    match s {
        Status::Planned => 0,
        Status::Permitted => 1,
        Status::Built => 2,
        Status::InService | Status::Suspended => 3,
        Status::Retired => 4,
    }
}

// ---------------------------------------------------------------------
// valid histories
// ---------------------------------------------------------------------

#[derive(Debug, Clone)]
enum Step {
    Stage(usize),
    Suspend(Option<u32>),
    Move(Option<u32>, i32),
    Change(u8, bool),
    Retire,
}

fn arb_step() -> impl Strategy<Value = Step> {
    prop_oneof![
        (0usize..4).prop_map(Step::Stage),
        proptest::option::of(1u32..60).prop_map(Step::Suspend),
        (proptest::option::of(1u32..60), -500i32..500).prop_map(|(u, d)| Step::Move(u, d)),
        (0u8..4, any::<bool>()).prop_map(|(f, b)| Step::Change(f, b)),
        Just(Step::Retire),
    ]
}

/// A stop's history: a first stage, then steps at non-decreasing dates
/// — each kept only where the model allows it (stages forward, a
/// suspension of a stop in service, a return of one not in service,
/// nothing after retirement).
fn arb_history() -> impl Strategy<Value = Vec<Event>> {
    (0usize..4, proptest::option::of(0u32..30), proptest::collection::vec((0u32..20, arb_step()), 0..10)).prop_map(
        |(stage, first_on, steps)| {
            // only a first in_service goes undated
            let first_on = if stage == 3 { first_on } else { Some(first_on.unwrap_or(0)) };
            let mut events = vec![begun(STAGES[stage].clone(), first_on, 45.6, -122.6, "Main & 1st")];
            let mut at = first_on.unwrap_or(0);
            for (gap, step) in steps {
                at += gap;
                if events.last().is_some_and(|l| l.event == EventKind::Retired) {
                    break;
                }
                let now = oracle(&events, "s", day(at)).map(|s| s.status);
                let e = match step {
                    Step::Stage(k) => {
                        let kind = STAGES[k].clone();
                        let allowed = match (&kind, now) {
                            (EventKind::InService, Some(s)) => s != Status::InService,
                            (_, Some(s)) => rank(s) < k as u8,
                            (_, None) => false,
                        };
                        if !allowed {
                            continue;
                        }
                        event(kind, Some(at))
                    }
                    Step::Suspend(u) => {
                        if now != Some(Status::InService) {
                            continue;
                        }
                        Event { until: u.map(|u| date(at + u)), ..event(EventKind::Suspended, Some(at)) }
                    }
                    Step::Move(u, d) => Event {
                        until: u.map(|u| date(at + u)),
                        lat: Some(45.6 + d as f64 * 1e-5),
                        lon: Some(-122.6 - d as f64 * 1e-5),
                        ..event(EventKind::Moved, Some(at))
                    },
                    Step::Change(f, b) => {
                        let mut details = Details::default();
                        match f {
                            0 => details.name = Some(format!("Stop {at}")),
                            1 => details.wheelchair = Some(b),
                            2 => {
                                details.amenities.insert("shelter".into(), b);
                            }
                            _ => {
                                details.amenities.insert("bench".into(), b);
                                details.code = Some(format!("{at}"));
                            }
                        }
                        Event { details, ..event(EventKind::Changed, Some(at)) }
                    }
                    Step::Retire => event(EventKind::Retired, Some(at)),
                };
                events.push(e);
            }
            events
        },
    )
}

fn arb_doc() -> impl Strategy<Value = Stops> {
    proptest::collection::btree_map("[a-z0-9]{1,6}", arb_history(), 1..6).prop_map(|stops| Stops { stops, ..Stops::default() })
}

// ---------------------------------------------------------------------
// writing JSON with its keys in another order
// ---------------------------------------------------------------------

fn shuffled_text(v: &Value, seed: &mut u64) -> String {
    let mut next = || {
        *seed ^= *seed << 13;
        *seed ^= *seed >> 7;
        *seed ^= *seed << 17;
        *seed
    };
    fn write(v: &Value, next: &mut dyn FnMut() -> u64, out: &mut String) {
        match v {
            Value::Object(map) => {
                let mut keys: Vec<&String> = map.keys().collect();
                for i in (1..keys.len()).rev() {
                    keys.swap(i, (next() % (i as u64 + 1)) as usize);
                }
                out.push('{');
                for (i, k) in keys.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    out.push_str(&serde_json::to_string(k).unwrap());
                    out.push(':');
                    write(&map[*k], next, out);
                }
                out.push('}');
            }
            Value::Array(items) => {
                out.push('[');
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    write(item, next, out);
                }
                out.push(']');
            }
            other => out.push_str(&serde_json::to_string(other).unwrap()),
        }
    }
    let mut out = String::new();
    write(v, &mut next, &mut out);
    out
}

fn bytes(s: &Stops) -> String {
    serde_json::to_string(s).unwrap()
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 256, ..ProptestConfig::default() })]

    /// A valid history is accepted, and the stop on every date is its
    /// events applied in order — the oracle's last-writer reading.
    #[test]
    fn a_stop_on_a_date_is_its_events_applied(doc in arb_doc(), d in 0u32..400) {
        let doc = doc.checked().map_err(|r| TestCaseError::fail(format!("{r:?}")))?;
        for (id, events) in &doc.stops {
            prop_assert_eq!(doc.stop_on(id, day(d)), oracle(events, id, day(d)), "stop {} on {}", id, date(d));
        }
    }

    /// The stages only go forward: on no two dates does a stop stand at
    /// an earlier stage later (a suspension and its return aside).
    #[test]
    fn the_stages_only_go_forward(doc in arb_doc(), a in 0u32..400, b in 0u32..400) {
        let doc = doc.checked().map_err(|r| TestCaseError::fail(format!("{r:?}")))?;
        let (a, b) = (a.min(b), a.max(b));
        for id in doc.stops.keys() {
            if let (Some(x), Some(y)) = (doc.stop_on(id, day(a)), doc.stop_on(id, day(b))) {
                prop_assert!(rank(x.status) <= rank(y.status), "stop {} is {:?} on {} and {:?} on {}", id, x.status, date(a), y.status, date(b));
            }
        }
    }

    /// Served is in service, and nothing else.
    #[test]
    fn served_is_in_service(doc in arb_doc(), d in 0u32..400, secs in 0u32..100_000) {
        let doc = doc.checked().map_err(|r| TestCaseError::fail(format!("{r:?}")))?;
        for (id, events) in &doc.stops {
            let in_service = oracle(events, id, day(d)).is_some_and(|s| s.status == Status::InService);
            prop_assert_eq!(doc.served(id, day(d), secs), in_service);
        }
        prop_assert!(!doc.served("no-such-stop", day(d), secs));
    }

    /// A temporary suspension or move ends on its `until`: from that
    /// date on the stop is what it would be had the event never been
    /// recorded.
    #[test]
    fn a_temporary_event_ends_on_its_until(doc in arb_doc(), pick in any::<prop::sample::Index>(), after in 0u32..30) {
        let doc = doc.checked().map_err(|r| TestCaseError::fail(format!("{r:?}")))?;
        let temporary: Vec<(String, usize)> = doc.stops.iter()
            .flat_map(|(id, es)| es.iter().enumerate().filter(|(_, e)| e.until.is_some()).map(move |(i, _)| (id.clone(), i)))
            .collect();
        prop_assume!(!temporary.is_empty());
        let (id, i) = pick.get(&temporary).clone();
        let until = NaiveDate::parse_from_str(doc.stops[&id][i].until.as_ref().unwrap(), "%Y-%m-%d").unwrap();
        let when = until + chrono::Days::new(after as u64);
        let mut without = doc.clone();
        without.stops.get_mut(&id).unwrap().remove(i);
        prop_assert_eq!(doc.stop_on(&id, when), without.stop_on(&id, when));
    }

    /// A correction is the wrong event taken out and the corrected one
    /// placed by date; the same event in its place changes nothing; a
    /// removal takes the event out; an index past the list is no edit.
    #[test]
    fn a_correction_edits_the_event_that_was_wrong(doc in arb_doc(), pick in any::<prop::sample::Index>(), on in 0u32..400, remove in any::<bool>()) {
        let doc = doc.checked().map_err(|r| TestCaseError::fail(format!("{r:?}")))?;
        let all: Vec<(String, usize)> = doc.stops.iter().flat_map(|(id, es)| (0..es.len()).map(move |i| (id.clone(), i))).collect();
        let (id, i) = pick.get(&all).clone();
        prop_assert_eq!(doc.correct_event(&id, i, Some(doc.stops[&id][i].clone())), Some(doc.clone()));
        let mut taken = doc.clone();
        let mut was = taken.stops.get_mut(&id).unwrap().remove(i);
        if remove {
            prop_assert_eq!(doc.correct_event(&id, i, None), Some(taken));
        } else {
            if was.on.is_some() || i > 0 {
                was.on = Some(date(on));
            }
            prop_assert_eq!(doc.correct_event(&id, i, Some(was.clone())), Some(taken.add_event(&id, was)));
        }
        prop_assert_eq!(doc.correct_event(&id, doc.stops[&id].len(), None), None);
        prop_assert_eq!(doc.correct_event("no-such-stop", 0, None), None);
    }

    /// What a stop may become on a date is exactly the lifecycle events
    /// a write would accept there: each offered one passes, each other
    /// one is refused.
    #[test]
    fn what_a_stop_may_become_is_what_a_write_accepts(doc in arb_doc(), d in 0u32..400) {
        let doc = doc.checked().map_err(|r| TestCaseError::fail(format!("{r:?}")))?;
        for id in doc.stops.keys() {
            let next = doc.next_events(id, day(d));
            for kind in [EventKind::Planned, EventKind::Permitted, EventKind::Built, EventKind::InService, EventKind::Suspended, EventKind::Retired] {
                let passes = doc.add_event(id, event(kind.clone(), Some(d))).checked().is_ok();
                prop_assert_eq!(next.contains(&kind), passes, "stop {} on {}: {:?}", id, date(d), kind);
            }
        }
    }

    /// The stops on a date, counted by status: every listed stop counted
    /// once, under its status.
    #[test]
    fn the_stops_on_a_date_are_counted_by_status(doc in arb_doc(), d in 0u32..400) {
        let doc = doc.checked().map_err(|r| TestCaseError::fail(format!("{r:?}")))?;
        let on = doc.on_date(day(d));
        let counts = doc.counts_on(day(d));
        prop_assert_eq!(counts.iter().map(|c| c.stops as usize).sum::<usize>(), on.len());
        for c in &counts {
            prop_assert!(c.stops > 0, "only statuses some stop has");
            prop_assert_eq!(c.stops as usize, on.iter().filter(|s| s.status == c.status).count());
        }
    }

    /// One document is one byte sequence: its stops and every object's
    /// keys in any order read back to the same canonical bytes.
    #[test]
    fn canonical_bytes_do_not_depend_on_order(doc in arb_doc(), mut seed in 1u64..u64::MAX) {
        let doc = doc.checked().map_err(|r| TestCaseError::fail(format!("{r:?}")))?;
        let text = shuffled_text(&serde_json::to_value(&doc).unwrap(), &mut seed);
        let read: Stops = serde_json::from_str(&text).unwrap();
        prop_assert_eq!(bytes(&read.checked().unwrap()), bytes(&doc));
    }

    /// Fields this repo does not know — another tool's, the agency's
    /// own amenity names — survive a read and a write, at the top, in
    /// an event, and among the amenities.
    #[test]
    fn what_is_not_known_survives(doc in arb_doc(), name in "x_[a-z]{1,8}", value in "[a-z ]{0,12}", heated in any::<bool>()) {
        let doc = doc.checked().map_err(|r| TestCaseError::fail(format!("{r:?}")))?;
        let mut v = serde_json::to_value(&doc).unwrap();
        v[name.as_str()] = json!(value);
        let first = v["stops"].as_object_mut().unwrap().values_mut().next().unwrap();
        first[0][name.as_str()] = json!({ "nested": [value.clone(), 1] });
        first[0]["amenities"]["heated_shelter"] = json!(heated);
        let read: Stops = serde_json::from_value(v.clone()).unwrap();
        let read = read.checked().map_err(|r| TestCaseError::fail(format!("{r:?}")))?;
        prop_assert_eq!(serde_json::to_value(&read).unwrap(), v);
    }
}

// ---------------------------------------------------------------------
// refusals, one by one
// ---------------------------------------------------------------------

fn refused(d: Stops) -> Vec<Problem> {
    d.checked().err().unwrap_or_default().into_iter().map(|r| r.problem).collect()
}

fn refused_json(v: Value) -> Vec<Problem> {
    refused(serde_json::from_value(v).unwrap())
}

#[test]
fn a_valid_document_is_accepted() {
    let d = doc(vec![
        (
            "6124",
            vec![
                in_service(Some(0), 45.69, -122.65, "NE 99th St & Hwy 99"),
                Event { until: Some(date(30)), ..event(EventKind::Suspended, Some(10)) },
                Event { lat: Some(45.691), lon: Some(-122.651), ..event(EventKind::Moved, Some(40)) },
            ],
        ),
        (
            "7001",
            vec![
                begun(EventKind::Planned, Some(0), 45.70, -122.66, "NE 104th St"),
                event(EventKind::Permitted, Some(20)),
                event(EventKind::Built, Some(60)),
                event(EventKind::InService, Some(70)),
            ],
        ),
    ]);
    assert!(d.checked().is_ok());
}

#[test]
fn an_unknown_event_is_refused() {
    let p = refused_json(json!({"stops": {"a": [
        {"event": "in_service", "lat": 45.0, "lon": -122.0},
        {"on": "2027-01-02", "event": "teleported"}
    ]}}));
    assert_eq!(p, vec![Problem::UnknownEvent { event: "teleported".into() }]);
}

#[test]
fn events_out_of_date_order_are_refused() {
    let p = refused(doc(vec![("a", vec![in_service(Some(5), 45.0, -122.0, "A"), event(EventKind::Suspended, Some(3))])]));
    assert_eq!(p, vec![Problem::OutOfOrder]);
}

#[test]
fn a_stop_begins_at_a_stage() {
    assert_eq!(refused(doc(vec![("a", vec![])])), vec![Problem::NoEvents]);
    let p = refused(doc(vec![("a", vec![Event { lat: Some(45.0), lon: Some(-122.0), ..event(EventKind::Moved, Some(1)) }])]));
    assert!(p.contains(&Problem::FirstEvent), "{p:?}");
    for stage in STAGES {
        assert!(refused(doc(vec![("a", vec![begun(stage.clone(), Some(0), 45.0, -122.0, "A")])])).is_empty(), "{stage:?}");
    }
}

#[test]
fn the_stages_do_not_go_back() {
    let p = refused(doc(vec![("a", vec![begun(EventKind::Built, Some(0), 45.0, -122.0, "A"), event(EventKind::Permitted, Some(3))])]));
    assert_eq!(p, vec![Problem::StageBack]);
    let p = refused(doc(vec![("a", vec![begun(EventKind::Permitted, Some(0), 45.0, -122.0, "A"), event(EventKind::Permitted, Some(3))])]));
    assert_eq!(p, vec![Problem::StageBack], "a stage is not entered twice");
    let p = refused(doc(vec![("a", vec![in_service(Some(0), 45.0, -122.0, "A"), event(EventKind::Planned, Some(3))])]));
    assert_eq!(p, vec![Problem::StageBack]);
    // a stage skipped is fine
    assert!(refused(doc(vec![("a", vec![begun(EventKind::Planned, Some(0), 45.0, -122.0, "A"), event(EventKind::InService, Some(3))])])).is_empty());
}

#[test]
fn a_suspension_is_of_a_stop_in_service() {
    let p = refused(doc(vec![("a", vec![begun(EventKind::Built, Some(0), 45.0, -122.0, "A"), event(EventKind::Suspended, Some(3))])]));
    assert_eq!(p, vec![Problem::NotInService]);
    let p = refused(doc(vec![(
        "a",
        vec![in_service(Some(0), 45.0, -122.0, "A"), event(EventKind::Suspended, Some(2)), event(EventKind::Suspended, Some(3))],
    )]));
    assert_eq!(p, vec![Problem::NotInService], "a suspended stop is not suspended again");
}

#[test]
fn a_return_to_service_needs_a_stop_not_in_service() {
    let p = refused(doc(vec![("a", vec![in_service(Some(0), 45.0, -122.0, "A"), event(EventKind::InService, Some(3))])]));
    assert_eq!(p, vec![Problem::AlreadyInService]);
    // a temporary suspension that has ended leaves nothing to return from
    let p = refused(doc(vec![(
        "a",
        vec![
            in_service(Some(0), 45.0, -122.0, "A"),
            Event { until: Some(date(5)), ..event(EventKind::Suspended, Some(1)) },
            event(EventKind::InService, Some(6)),
        ],
    )]));
    assert_eq!(p, vec![Problem::AlreadyInService]);
    // back early from a temporary one, or from one with no end
    assert!(refused(doc(vec![(
        "a",
        vec![
            in_service(Some(0), 45.0, -122.0, "A"),
            Event { until: Some(date(9)), ..event(EventKind::Suspended, Some(1)) },
            event(EventKind::InService, Some(6)),
        ],
    )]))
    .is_empty());
}

#[test]
fn a_suspended_stop_may_never_return() {
    let d = doc(vec![(
        "a",
        vec![in_service(Some(0), 45.0, -122.0, "A"), event(EventKind::Suspended, Some(2)), event(EventKind::Retired, Some(90))],
    )]);
    let d = d.checked().unwrap();
    assert_eq!(d.stop_on("a", day(50)).unwrap().status, Status::Suspended);
    assert_eq!(d.stop_on("a", day(90)).unwrap().status, Status::Retired);
}

#[test]
fn nothing_follows_retirement() {
    let p = refused(doc(vec![(
        "a",
        vec![in_service(Some(0), 45.0, -122.0, "A"), event(EventKind::Retired, Some(3)), event(EventKind::Suspended, Some(4))],
    )]));
    assert_eq!(p, vec![Problem::AfterRetired]);
}

#[test]
fn until_comes_after_its_date() {
    let p = refused(doc(vec![(
        "a",
        vec![in_service(Some(0), 45.0, -122.0, "A"), Event { until: Some(date(3)), ..event(EventKind::Suspended, Some(3)) }],
    )]));
    assert_eq!(p, vec![Problem::UntilNotAfter]);
}

#[test]
fn only_a_first_in_service_may_go_undated() {
    let p = refused(doc(vec![("a", vec![in_service(None, 45.0, -122.0, "A"), event(EventKind::Suspended, None)])]));
    assert_eq!(p, vec![Problem::NoDate]);
    let p = refused(doc(vec![("a", vec![begun(EventKind::Planned, None, 45.0, -122.0, "A")])]));
    assert_eq!(p, vec![Problem::NoDate], "a stop still being planned has a date");
}

#[test]
fn a_date_is_a_service_date() {
    let p = refused_json(json!({"stops": {"a": [{"on": "March 3", "event": "in_service", "lat": 45.0, "lon": -122.0}]}}));
    assert_eq!(p, vec![Problem::BadDate { date: "March 3".into() }]);
}

#[test]
fn an_event_carries_only_what_its_kind_sets() {
    let misplaced = |e: Event| refused(doc(vec![("a", vec![in_service(Some(0), 45.0, -122.0, "A"), e])]));
    assert_eq!(misplaced(Event { until: Some(date(9)), ..event(EventKind::Changed, Some(1)) }), vec![Problem::Misplaced { field: "until".into() }]);
    assert_eq!(
        misplaced(Event { lat: Some(45.0), lon: Some(-122.0), ..event(EventKind::Changed, Some(1)) }),
        vec![Problem::Misplaced { field: "point".into() }]
    );
    assert_eq!(
        misplaced(Event { details: Details { name: Some("B".into()), ..Details::default() }, ..event(EventKind::Suspended, Some(1)) }),
        vec![Problem::Misplaced { field: "details".into() }]
    );
    assert_eq!(
        misplaced(Event { replaces: vec!["a".into()], ..event(EventKind::Changed, Some(1)) }),
        vec![Problem::Misplaced { field: "replaces".into() }]
    );
}

#[test]
fn a_point_is_whole_and_on_the_earth() {
    let p = refused(doc(vec![("a", vec![Event { lat: None, ..in_service(Some(0), 45.0, -122.0, "A") }])]));
    assert_eq!(p, vec![Problem::NoPoint]);
    let p = refused(doc(vec![("a", vec![in_service(Some(0), 95.0, -122.0, "A")])]));
    assert_eq!(p, vec![Problem::BadPoint]);
    let p = refused(doc(vec![("a", vec![in_service(Some(0), 45.0, -122.0, "A"), event(EventKind::Moved, Some(2))])]));
    assert_eq!(p, vec![Problem::NoPoint]);
}

#[test]
fn a_consolidation_names_other_stops_that_exist() {
    let with = |replaces: Vec<&str>| {
        refused(doc(vec![
            ("a", vec![in_service(Some(0), 45.0, -122.0, "A")]),
            ("b", vec![Event { replaces: replaces.into_iter().map(String::from).collect(), ..begun(EventKind::Planned, Some(4), 45.0, -122.0, "B") }]),
        ]))
    };
    assert!(with(vec!["a"]).is_empty());
    assert_eq!(with(vec!["zz"]), vec![Problem::UnknownStop { stop: "zz".into() }]);
    assert_eq!(with(vec!["b"]), vec![Problem::ReplacesItself]);
}

#[test]
fn every_refusal_names_its_stop_and_event() {
    let refusals = doc(vec![("a", vec![in_service(Some(5), 45.0, -122.0, "A"), event(EventKind::Suspended, Some(3))])]).checked().unwrap_err();
    assert_eq!((refusals[0].stop.as_str(), refusals[0].event), ("a", Some(1)));
}

// ---------------------------------------------------------------------
// worked answers
// ---------------------------------------------------------------------

#[test]
fn the_worked_example_reads_as_the_unit_says() {
    let d: Stops = serde_json::from_value(json!({
        "kind": "stops", "schema_version": 1,
        "stops": {
            "6124": [
                {"on": "2004-09-01", "event": "in_service", "lat": 45.6934, "lon": -122.6512, "name": "NE 99th St & Hwy 99", "code": "6124", "wheelchair": true},
                {"on": "2025-05-12", "event": "changed", "amenities": {"shelter": true, "bench": true, "bin": true}},
                {"on": "2027-03-03", "event": "suspended", "until": "2027-07-01"},
                {"on": "2027-09-01", "event": "moved", "lat": 45.6936, "lon": -122.6509}
            ],
            "7001": [
                {"on": "2026-11-02", "event": "planned", "lat": 45.70, "lon": -122.66, "name": "NE 104th St"},
                {"on": "2027-02-15", "event": "permitted"},
                {"on": "2027-06-01", "event": "built"},
                {"on": "2027-09-06", "event": "in_service"}
            ]
        }
    }))
    .unwrap();
    let d = d.checked().unwrap();
    let at = |id: &str, s: &str| d.stop_on(id, NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap());
    assert_eq!(at("6124", "2004-08-31"), None, "before it went into service, the stop is not there");
    assert_eq!(at("6124", "2025-05-11").unwrap().details.amenities, BTreeMap::new(), "amenities not recorded yet");
    assert_eq!(at("6124", "2025-05-12").unwrap().details.amenities.get("shelter"), Some(&true));
    assert_eq!(at("6124", "2027-03-03").unwrap().status, Status::Suspended);
    let away = at("6124", "2027-04-01").unwrap();
    assert_eq!((away.since.as_deref(), away.until.as_deref()), (Some("2027-03-03"), Some("2027-07-01")), "suspended since, back on");
    assert_eq!(at("6124", "2027-06-30").unwrap().status, Status::Suspended);
    assert_eq!(at("6124", "2027-07-01").unwrap().status, Status::InService, "until is the first day back");
    assert_eq!((at("6124", "2027-08-31").unwrap().lat, at("6124", "2027-09-01").unwrap().lat), (45.6934, 45.6936));
    assert_eq!(at("7001", "2027-01-01").unwrap().status, Status::Planned);
    assert_eq!(at("7001", "2027-06-01").unwrap().status, Status::Built);
    assert!(!d.served("7001", NaiveDate::from_ymd_opt(2027, 9, 5).unwrap(), 0) && d.served("7001", NaiveDate::from_ymd_opt(2027, 9, 6).unwrap(), 0));
}

#[test]
fn every_stop_on_a_date_in_id_order() {
    let d = doc(vec![
        ("b", vec![in_service(Some(0), 45.0, -122.0, "B")]),
        ("a", vec![begun(EventKind::Planned, Some(0), 45.0, -122.0, "A")]),
        ("later", vec![in_service(Some(9), 45.0, -122.0, "L")]),
    ]);
    let ids: Vec<String> = d.on_date(day(1)).into_iter().map(|s| s.stop_id).collect();
    assert_eq!(ids, vec!["a", "b"], "a stop not yet there is not listed; a planned one is");
}
