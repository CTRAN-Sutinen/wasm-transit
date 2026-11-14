//! A plan takes a stop from the stops document (D-173): the
//! planner adding a stop to a pattern may pick one the stops app made,
//! and the plan gets that stop's record — the laws, before the
//! implementation:
//!
//! * `records(stops, ids, on)` answers each named stop's record in the
//!   network's terms on `on` — exactly the record a pick reads of it
//!   (`read_records`), so a stop taken in, then read at publish, is
//!   unchanged;
//! * a stop that is not there yet on `on` (its first event is later) is
//!   not answered; a stop the document does not hold is not answered;
//! * the record is copied, never linked: the stops document is not
//!   written, and the plan's stop is an ordinary stop (`upsert_stop`).

use chrono::NaiveDate;
use transit_network::stop_record::{read_records, records};
use transit_network::{apply_edits, Network, NetworkEdit};
use transit_stops::{Details, Event, EventKind, Stops};

fn network() -> Network {
    serde_json::from_str(include_str!("../../../testdata/network.json")).expect("testdata network parses")
}

fn day(s: &str) -> NaiveDate {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
}

fn planned(on: &str, lat: f64, lon: f64, name: &str, code: &str) -> Event {
    Event {
        on: Some(on.into()),
        event: EventKind::Planned,
        lat: Some(lat),
        lon: Some(lon),
        details: Details { name: Some(name.into()), code: Some(code.into()), ..Details::default() },
        ..Event::default()
    }
}

/// A document holding stop N1 (planned 2026-09-01, in service
/// 2026-11-13, renamed 2026-12-01) and N2 (planned 2027-01-04).
fn doc() -> Stops {
    Stops::empty()
        .add_event("N1", planned("2026-09-01", 45.58, -122.348, "Washougal TC", "9906"))
        .add_event("N1", Event { on: Some("2026-11-13".into()), event: EventKind::InService, ..Event::default() })
        .add_event(
            "N1",
            Event {
                on: Some("2026-12-01".into()),
                event: EventKind::Changed,
                details: Details { name: Some("Washougal Transit Center".into()), ..Details::default() },
                ..Event::default()
            },
        )
        .add_event("N2", planned("2027-01-04", 45.6727, -122.506, "East Fourth Plain TC", "9901"))
}

#[test]
fn a_stop_taken_in_is_the_record_a_pick_reads() {
    let stops = doc();
    for on in ["2026-11-13", "2026-12-05"] {
        let taken = records(&stops, &["N1".to_string()], day(on));
        let stop = taken.get("N1").expect("N1 is there on the day");
        // into the plan as an ordinary stop…
        let plan = apply_edits(&network(), &[NetworkEdit::UpsertStop { id: "N1".into(), stop: stop.clone() }]).unwrap();
        // …and read at publish: unchanged
        assert_eq!(read_records(&plan, &stops, day(on)).stops["N1"], *stop, "on {on}");
    }
    assert_eq!(records(&stops, &["N1".to_string()], day("2026-11-13"))["N1"].name, "Washougal TC");
    assert_eq!(records(&stops, &["N1".to_string()], day("2026-12-05"))["N1"].name, "Washougal Transit Center");
    assert_eq!(records(&stops, &["N1".to_string()], day("2026-12-05"))["N1"].info.code.as_deref(), Some("9906"));
}

#[test]
fn a_stop_not_there_yet_or_not_held_is_not_answered() {
    let stops = doc();
    let ids = ["N1".to_string(), "N2".to_string(), "Z".to_string()];
    let before = records(&stops, &ids, day("2026-08-31"));
    assert!(before.is_empty(), "nothing is there yet: {before:?}");
    let now = records(&stops, &ids, day("2026-11-13"));
    assert_eq!(now.keys().collect::<Vec<_>>(), vec!["N1"], "N2 is planned later; Z is not held");
    // a planned stop is there from its planned date: a plan may take it
    assert!(records(&stops, &ids, day("2027-01-04")).contains_key("N2"));
}
