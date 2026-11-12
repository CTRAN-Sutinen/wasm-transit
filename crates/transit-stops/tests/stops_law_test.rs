//! The stops document's laws (docs/units/stop-lifecycle.md, D-153,
//! D-154), derived from the user's intent and written before the
//! implementation:
//!
//! * the events are the truth: a stop on a date is its events applied
//!   in order up to that date, a temporary one (`until`) only while it
//!   lasts — checked against an oracle that answers each detail as the
//!   LAST active event that sets it, a different reading of the same
//!   rule;
//! * a temporary closure or move ends on its `until`: from that date
//!   the stop is what it would be without the event;
//! * served is open, nothing else (a closed, proposed, retired, or
//!   not-yet-opened stop is not served);
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

fn opened(on: Option<u32>, lat: f64, lon: f64, name: &str) -> Event {
    Event {
        lat: Some(lat),
        lon: Some(lon),
        details: Details { name: Some(name.into()), ..Details::default() },
        ..event(EventKind::Opened, on)
    }
}

fn doc(stops: Vec<(&str, Vec<Event>)>) -> Stops {
    Stops { stops: stops.into_iter().map(|(id, e)| (id.to_string(), e)).collect(), ..Stops::default() }
}

// ---------------------------------------------------------------------
// the oracle: each answer is the last active event that sets it
// ---------------------------------------------------------------------

fn active(e: &Event, d: NaiveDate) -> bool {
    let p = |s: &String| NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap();
    e.on.as_ref().map_or(true, |on| p(on) <= d) && e.until.as_ref().map_or(true, |u| d < p(u))
}

fn oracle(events: &[Event], id: &str, d: NaiveDate) -> Option<StopOn> {
    let live: Vec<&Event> = events.iter().filter(|e| active(e, d)).collect();
    let status_of = |k: &EventKind| match k {
        EventKind::Opened | EventKind::Reopened => Some(Status::Open),
        EventKind::Proposed => Some(Status::Proposed),
        EventKind::Closed => Some(Status::Closed),
        EventKind::Retired => Some(Status::Retired),
        _ => None,
    };
    let status = live.iter().rev().find_map(|e| status_of(&e.event))?;
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
    let plan = live.iter().rev().find_map(|e| e.plan.clone());
    Some(StopOn { stop_id: id.to_string(), status, lat: point.lat.unwrap(), lon: point.lon.unwrap(), plan, details })
}

// ---------------------------------------------------------------------
// valid histories
// ---------------------------------------------------------------------

#[derive(Debug, Clone)]
enum Step {
    Close(Option<u32>),
    Reopen,
    Move(Option<u32>, i32),
    Change(u8, bool),
    Retire,
}

fn arb_step() -> impl Strategy<Value = Step> {
    prop_oneof![
        proptest::option::of(1u32..60).prop_map(Step::Close),
        Just(Step::Reopen),
        (proptest::option::of(1u32..60), -500i32..500).prop_map(|(u, d)| Step::Move(u, d)),
        (0u8..4, any::<bool>()).prop_map(|(f, b)| Step::Change(f, b)),
        Just(Step::Retire),
    ]
}

/// A stop's history: a first event, then steps at non-decreasing
/// dates — each kept only where the model allows it (a reopening only
/// of a closed stop, nothing after retirement).
fn arb_history() -> impl Strategy<Value = Vec<Event>> {
    (any::<bool>(), proptest::option::of(0u32..30), proptest::collection::vec((0u32..20, arb_step()), 0..10)).prop_map(
        |(proposed, first_on, steps)| {
            let mut events = vec![if proposed {
                Event { plan: Some("fall".into()), ..Event { event: EventKind::Proposed, on: Some(date(first_on.unwrap_or(0))), ..opened(None, 45.6, -122.6, "Main & 1st") } }
            } else {
                opened(first_on, 45.6, -122.6, "Main & 1st")
            }];
            let mut at = first_on.unwrap_or(0);
            for (gap, step) in steps {
                at += gap;
                let e = match step {
                    Step::Close(u) => Event { until: u.map(|u| date(at + u)), ..event(EventKind::Closed, Some(at)) },
                    Step::Reopen => event(EventKind::Reopened, Some(at)),
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
                if events.last().is_some_and(|l| l.event == EventKind::Retired) {
                    break;
                }
                if e.event == EventKind::Reopened && oracle(&events, "s", day(at)).map(|s| s.status) != Some(Status::Closed) {
                    continue;
                }
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

    /// Served is open, and nothing else.
    #[test]
    fn served_is_open(doc in arb_doc(), d in 0u32..400, secs in 0u32..100_000) {
        let doc = doc.checked().map_err(|r| TestCaseError::fail(format!("{r:?}")))?;
        for (id, events) in &doc.stops {
            let open = oracle(events, id, day(d)).is_some_and(|s| s.status == Status::Open);
            prop_assert_eq!(doc.served(id, day(d), secs), open);
        }
        prop_assert!(!doc.served("no-such-stop", day(d), secs));
    }

    /// A temporary closure or move ends on its `until`: from that date
    /// on the stop is what it would be had the event never been
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
    let d = doc(vec![(
        "6124",
        vec![
            opened(Some(0), 45.69, -122.65, "NE 99th St & Hwy 99"),
            Event { until: Some(date(30)), ..event(EventKind::Closed, Some(10)) },
            Event { lat: Some(45.691), lon: Some(-122.651), ..event(EventKind::Moved, Some(40)) },
        ],
    )]);
    assert!(d.checked().is_ok());
}

#[test]
fn an_unknown_event_is_refused() {
    let p = refused_json(json!({"stops": {"a": [
        {"event": "opened", "lat": 45.0, "lon": -122.0},
        {"on": "2027-01-02", "event": "teleported"}
    ]}}));
    assert_eq!(p, vec![Problem::UnknownEvent { event: "teleported".into() }]);
}

#[test]
fn events_out_of_date_order_are_refused() {
    let p = refused(doc(vec![("a", vec![opened(Some(5), 45.0, -122.0, "A"), event(EventKind::Closed, Some(3))])]));
    assert_eq!(p, vec![Problem::OutOfOrder]);
}

#[test]
fn a_stop_must_begin_opened_or_proposed() {
    assert_eq!(refused(doc(vec![("a", vec![])])), vec![Problem::NoEvents]);
    let p = refused(doc(vec![("a", vec![Event { lat: Some(45.0), lon: Some(-122.0), ..event(EventKind::Moved, Some(1)) }])]));
    assert!(p.contains(&Problem::FirstEvent), "{p:?}");
    let p = refused(doc(vec![(
        "a",
        vec![opened(Some(0), 45.0, -122.0, "A"), Event { plan: Some("fall".into()), ..event(EventKind::Proposed, Some(3)) }],
    )]));
    assert!(p.contains(&Problem::FirstEvent), "a proposal is only a stop's first event: {p:?}");
}

#[test]
fn nothing_follows_retirement() {
    let p = refused(doc(vec![("a", vec![opened(Some(0), 45.0, -122.0, "A"), event(EventKind::Retired, Some(3)), event(EventKind::Closed, Some(4))])]));
    assert_eq!(p, vec![Problem::AfterRetired]);
}

#[test]
fn a_reopening_needs_a_closed_stop() {
    let p = refused(doc(vec![("a", vec![opened(Some(0), 45.0, -122.0, "A"), event(EventKind::Reopened, Some(3))])]));
    assert_eq!(p, vec![Problem::NothingClosed]);
    // a temporary closure that has ended leaves nothing to reopen
    let p = refused(doc(vec![(
        "a",
        vec![
            opened(Some(0), 45.0, -122.0, "A"),
            Event { until: Some(date(5)), ..event(EventKind::Closed, Some(1)) },
            event(EventKind::Reopened, Some(6)),
        ],
    )]));
    assert_eq!(p, vec![Problem::NothingClosed]);
}

#[test]
fn a_proposal_names_its_plan() {
    let p = refused(doc(vec![("a", vec![Event { lat: Some(45.0), lon: Some(-122.0), ..event(EventKind::Proposed, Some(0)) }])]));
    assert_eq!(p, vec![Problem::NoPlan]);
}

#[test]
fn until_comes_after_its_date() {
    let p = refused(doc(vec![("a", vec![opened(Some(0), 45.0, -122.0, "A"), Event { until: Some(date(3)), ..event(EventKind::Closed, Some(3)) }])]));
    assert_eq!(p, vec![Problem::UntilNotAfter]);
}

#[test]
fn only_an_opening_may_go_undated() {
    let p = refused(doc(vec![("a", vec![opened(None, 45.0, -122.0, "A"), event(EventKind::Closed, None)])]));
    assert_eq!(p, vec![Problem::NoDate]);
}

#[test]
fn a_date_is_a_service_date() {
    let p = refused_json(json!({"stops": {"a": [{"on": "March 3", "event": "opened", "lat": 45.0, "lon": -122.0}]}}));
    assert_eq!(p, vec![Problem::BadDate { date: "March 3".into() }]);
}

#[test]
fn an_event_carries_only_what_its_kind_sets() {
    let misplaced = |e: Event| refused(doc(vec![("a", vec![opened(Some(0), 45.0, -122.0, "A"), e])]));
    assert_eq!(misplaced(Event { until: Some(date(9)), ..event(EventKind::Changed, Some(1)) }), vec![Problem::Misplaced { field: "until".into() }]);
    assert_eq!(misplaced(Event { plan: Some("p".into()), ..event(EventKind::Closed, Some(1)) }), vec![Problem::Misplaced { field: "plan".into() }]);
    assert_eq!(
        misplaced(Event { lat: Some(45.0), lon: Some(-122.0), ..event(EventKind::Changed, Some(1)) }),
        vec![Problem::Misplaced { field: "point".into() }]
    );
    assert_eq!(
        misplaced(Event { details: Details { name: Some("B".into()), ..Details::default() }, ..event(EventKind::Closed, Some(1)) }),
        vec![Problem::Misplaced { field: "details".into() }]
    );
    assert_eq!(
        misplaced(Event { replaces: vec!["a".into()], ..event(EventKind::Changed, Some(1)) }),
        vec![Problem::Misplaced { field: "replaces".into() }]
    );
}

#[test]
fn a_point_is_whole_and_on_the_earth() {
    let p = refused(doc(vec![("a", vec![Event { lat: None, ..opened(Some(0), 45.0, -122.0, "A") }])]));
    assert_eq!(p, vec![Problem::NoPoint]);
    let p = refused(doc(vec![("a", vec![opened(Some(0), 95.0, -122.0, "A")])]));
    assert_eq!(p, vec![Problem::BadPoint]);
    let p = refused(doc(vec![("a", vec![opened(Some(0), 45.0, -122.0, "A"), event(EventKind::Moved, Some(2))])]));
    assert_eq!(p, vec![Problem::NoPoint]);
}

#[test]
fn a_consolidation_names_other_stops_that_exist() {
    let with = |replaces: Vec<&str>| {
        refused(doc(vec![
            ("a", vec![opened(Some(0), 45.0, -122.0, "A")]),
            ("b", vec![Event { replaces: replaces.into_iter().map(String::from).collect(), ..opened(Some(4), 45.0, -122.0, "B") }]),
        ]))
    };
    assert!(with(vec!["a"]).is_empty());
    assert_eq!(with(vec!["zz"]), vec![Problem::UnknownStop { stop: "zz".into() }]);
    assert_eq!(with(vec!["b"]), vec![Problem::ReplacesItself]);
}

#[test]
fn every_refusal_names_its_stop_and_event() {
    let refusals = doc(vec![("a", vec![opened(Some(5), 45.0, -122.0, "A"), event(EventKind::Closed, Some(3))])]).checked().unwrap_err();
    assert_eq!((refusals[0].stop.as_str(), refusals[0].event), ("a", Some(1)));
}

// ---------------------------------------------------------------------
// worked answers
// ---------------------------------------------------------------------

#[test]
fn the_worked_example_reads_as_the_unit_says() {
    let d: Stops = serde_json::from_value(json!({
        "kind": "stops", "schema_version": 1,
        "stops": {"6124": [
            {"on": "2004-09-01", "event": "opened", "lat": 45.6934, "lon": -122.6512, "name": "NE 99th St & Hwy 99", "code": "6124", "wheelchair": true},
            {"on": "2025-05-12", "event": "changed", "amenities": {"shelter": true, "bench": true, "bin": true}},
            {"on": "2027-03-03", "event": "closed", "until": "2027-07-01"},
            {"on": "2027-09-01", "event": "moved", "lat": 45.6936, "lon": -122.6509}
        ]}
    }))
    .unwrap();
    let d = d.checked().unwrap();
    let at = |s: &str| d.stop_on("6124", NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap());
    assert_eq!(at("2004-08-31"), None, "before it opened, the stop is not there");
    assert_eq!(at("2025-05-11").unwrap().details.amenities, BTreeMap::new(), "amenities not recorded yet");
    assert_eq!(at("2025-05-12").unwrap().details.amenities.get("shelter"), Some(&true));
    assert_eq!(at("2027-03-03").unwrap().status, Status::Closed);
    assert_eq!(at("2027-06-30").unwrap().status, Status::Closed);
    assert_eq!(at("2027-07-01").unwrap().status, Status::Open, "until is the first day back");
    assert_eq!((at("2027-08-31").unwrap().lat, at("2027-09-01").unwrap().lat), (45.6934, 45.6936));
    assert_eq!(at("2030-01-01").unwrap().details.name.as_deref(), Some("NE 99th St & Hwy 99"));
}

#[test]
fn every_stop_on_a_date_in_id_order() {
    let d = doc(vec![
        ("b", vec![opened(Some(0), 45.0, -122.0, "B")]),
        ("a", vec![opened(Some(0), 45.0, -122.0, "A")]),
        ("later", vec![opened(Some(9), 45.0, -122.0, "L")]),
    ]);
    let ids: Vec<String> = d.on_date(day(1)).into_iter().map(|s| s.stop_id).collect();
    assert_eq!(ids, vec!["a", "b"], "a stop not yet opened is not listed");
}
