//! A stop's site survey (D-158, D-160, D-161), derived from the user's
//! intent — C-TRAN's Service Stop Worksheet, filled at the stop — and
//! written before the implementation:
//!
//! * a stop's site answers and its references to other systems are
//!   details like any other: set by the events that set details (a
//!   stage, `changed`), merged by name, each name's answer the last
//!   active event's — the fold law in `stops_law_test.rs` checks them
//!   against its oracle with the rest of the stop;
//! * a core question with a list of answers takes only an answer on its
//!   list; a count takes a whole number; a written answer takes any
//!   text — and the agency's own questions and references are kept and
//!   not checked;
//! * an event recording a survey names who surveyed (`surveyed_by`),
//!   only on an event that sets details; a stop on a date says when it
//!   was last surveyed and by whom;
//! * the questions a page asks are the crate's (`SITE`), every one
//!   named once, and every list answer is a plain lowercase word.

use serde_json::json;
use transit_stops::{Answer, Details, Event, EventKind, Problem, Stops, SITE};

fn date(n: u32) -> String {
    format!("2027-01-{:02}", n + 1)
}

fn day(n: u32) -> chrono::NaiveDate {
    chrono::NaiveDate::parse_from_str(&date(n), "%Y-%m-%d").unwrap()
}

fn begun(on: u32) -> Event {
    Event { on: Some(date(on)), event: EventKind::InService, lat: Some(45.6), lon: Some(-122.6), ..Event::default() }
}

fn changed(on: u32, site: &[(&str, &str)]) -> Event {
    Event {
        on: Some(date(on)),
        event: EventKind::Changed,
        details: Details { site: site.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(), ..Details::default() },
        ..Event::default()
    }
}

fn doc(events: Vec<Event>) -> Stops {
    Stops { stops: [("a".to_string(), events)].into_iter().collect(), ..Stops::default() }
}

fn refused(d: Stops) -> Vec<Problem> {
    d.checked().err().unwrap_or_default().into_iter().map(|r| r.problem).collect()
}

fn answer(key: &str) -> &'static Answer {
    &SITE.iter().find(|(k, _)| *k == key).unwrap_or_else(|| panic!("{key} is not a core question")).1
}

#[test]
fn a_list_question_takes_only_an_answer_on_its_list() {
    for (key, a) in SITE.iter() {
        let Answer::OneOf(answers) = a else { continue };
        for v in answers.iter() {
            assert_eq!(refused(doc(vec![begun(0), changed(1, &[(key, v)])])), vec![], "{key}={v}");
        }
        assert_eq!(
            refused(doc(vec![begun(0), changed(1, &[(key, "not_an_answer")])])),
            vec![Problem::BadAnswer { key: key.to_string(), value: "not_an_answer".into() }],
            "{key}"
        );
    }
}

#[test]
fn a_count_takes_a_whole_number() {
    for (key, _) in SITE.iter().filter(|(_, a)| matches!(a, Answer::Number)) {
        assert_eq!(refused(doc(vec![begun(0), changed(1, &[(key, "35")])])), vec![], "{key}");
        assert_eq!(refused(doc(vec![begun(0), changed(1, &[(key, "0")])])), vec![], "{key}");
        for bad in ["", "3.5", "-2", "35 mph", "two"] {
            assert_eq!(
                refused(doc(vec![begun(0), changed(1, &[(key, bad)])])),
                vec![Problem::BadAnswer { key: key.to_string(), value: bad.into() }],
                "{key}={bad:?}"
            );
        }
    }
}

#[test]
fn a_written_answer_takes_any_text() {
    for (key, _) in SITE.iter().filter(|(_, a)| matches!(a, Answer::Text)) {
        for v in ["", "Mill Plain Blvd", "by the red barn, 3 ft east of pole #44"] {
            assert_eq!(refused(doc(vec![begun(0), changed(1, &[(key, v)])])), vec![], "{key}={v:?}");
        }
    }
}

#[test]
fn the_agencys_own_questions_and_references_are_kept_unchecked() {
    let mut e = changed(1, &[("x_bike_lane", "painted"), ("surface", "concrete")]);
    e.details.refs.insert("tms".into(), "TMS-0042".into());
    e.details.refs.insert("x_asset_system".into(), "".into());
    let d = doc(vec![begun(0), e]);
    assert_eq!(refused(d.clone()), vec![]);
    let read: Stops = serde_json::from_value(serde_json::to_value(&d).unwrap()).unwrap();
    assert_eq!(read, d);
    let on = d.stop_on("a", day(4)).unwrap();
    assert_eq!(on.details.site.get("x_bike_lane").map(String::as_str), Some("painted"));
    assert_eq!(on.details.refs.get("tms").map(String::as_str), Some("TMS-0042"));
}

#[test]
fn a_survey_is_named_on_an_event_that_sets_details() {
    let surveyed = |kind: EventKind| Event { surveyed_by: Some("R. Ortiz".into()), ..Event { event: kind, ..changed(1, &[]) } };
    assert_eq!(refused(doc(vec![begun(0), surveyed(EventKind::Changed)])), vec![]);
    let first = Event { surveyed_by: Some("R. Ortiz".into()), ..Event { event: EventKind::Planned, ..begun(0) } };
    assert_eq!(refused(doc(vec![first])), vec![]);
    assert_eq!(
        refused(doc(vec![begun(0), surveyed(EventKind::Suspended)])),
        vec![Problem::Misplaced { field: "surveyed_by".into() }]
    );
}

#[test]
fn a_stop_says_when_it_was_last_surveyed_and_by_whom() {
    let survey = |on: u32, by: &str, site: &[(&str, &str)]| Event { surveyed_by: Some(by.into()), ..changed(on, site) };
    let d = doc(vec![
        begun(0),
        survey(2, "A. Lee", &[("surface", "gravel"), ("position", "near_side")]),
        changed(4, &[("landmark", "the library")]),
        survey(6, "R. Ortiz", &[("surface", "concrete")]),
    ]);
    let at = |n: u32| d.stop_on("a", day(n)).unwrap();
    let s = at(1);
    assert_eq!((s.surveyed_on, s.surveyed_by), (None, None));
    let s = at(5);
    assert_eq!((s.surveyed_on.as_deref(), s.surveyed_by.as_deref()), (Some("2027-01-03"), Some("A. Lee")));
    // a change that is not a survey moves neither
    assert_eq!(s.details.site.get("landmark").map(String::as_str), Some("the library"));
    let s = at(7);
    assert_eq!((s.surveyed_on.as_deref(), s.surveyed_by.as_deref()), (Some("2027-01-07"), Some("R. Ortiz")));
    // answers merge by name: the later survey's surface, the earlier's position
    assert_eq!(s.details.site.get("surface").map(String::as_str), Some("concrete"));
    assert_eq!(s.details.site.get("position").map(String::as_str), Some("near_side"));
}

#[test]
fn the_core_questions_are_named_once_in_plain_words() {
    let mut keys: Vec<&str> = SITE.iter().map(|(k, _)| *k).collect();
    let n = keys.len();
    keys.sort();
    keys.dedup();
    assert_eq!(keys.len(), n, "a question named twice");
    for (key, a) in SITE.iter() {
        assert!(key.chars().all(|c| c.is_ascii_lowercase() || c == '_' || c == ':'), "{key}");
        if let Answer::OneOf(answers) = a {
            assert!(!answers.is_empty(), "{key}");
            for v in answers.iter() {
                assert!(v.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'), "{key}={v}");
            }
        }
    }
    // the worksheet's questions, as the paper asks them
    assert!(matches!(answer("position"), Answer::OneOf(a) if a == &["near_side", "far_side", "opposite", "mid_block"]));
    assert!(matches!(answer("lanes"), Answer::Number));
    assert!(matches!(answer("landmark"), Answer::Text));
}

#[test]
fn the_worked_survey_reads() {
    let d: Stops = serde_json::from_value(json!({
        "kind": "stops",
        "schema_version": 1,
        "stops": {
            "N1": [
                {
                    "on": "2027-01-01", "event": "planned", "lat": 45.6381234, "lon": -122.6612345,
                    "name": "Mill Plain & 112th", "surveyed_by": "R. Ortiz",
                    "amenities": {"boarding_pad": true, "rear_boarding_pad": false, "seat": true},
                    "site": {
                        "on_street": "Mill Plain Blvd", "cross_street": "NE 112th Ave",
                        "direction": "eastbound", "side": "south", "position": "far_side",
                        "lanes": "4", "speed_limit_mph": "35", "surface": "concrete",
                        "sidewalk": "yes", "sidewalk:connection": "continuous", "curb_cut": "at_all_corners",
                        "seat:type": "Simme Seat"
                    },
                    "refs": {"sign": "S-1180", "gis": "BS-20931"}
                }
            ]
        }
    }))
    .unwrap();
    assert_eq!(refused(d.clone()), vec![]);
    let s = d.stop_on("N1", day(1)).unwrap();
    assert_eq!(s.surveyed_by.as_deref(), Some("R. Ortiz"));
    assert_eq!(s.details.site.len(), 12);
    assert_eq!(s.details.refs.get("sign").map(String::as_str), Some("S-1180"));
}
