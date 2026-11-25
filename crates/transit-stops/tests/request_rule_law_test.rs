//! The rule from a stop request to its stop (D-233, D-238). The rule is
//! documented in `request_rule.rs`.

use chrono::NaiveDate;
use transit_requests::{StopRequest, STATUSES};
use transit_stops::request_rule::{allows, request_needs, request_rule, Needs};
use transit_stops::{Event, EventKind, Status, Stops};

fn day(s: &str) -> NaiveDate {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
}

fn ev(kind: EventKind, on: &str) -> Event {
    Event { on: Some(on.into()), event: kind, ..Event::default() }
}

fn stops() -> Stops {
    let planned = Event { lat: Some(45.6), lon: Some(-122.6), ..ev(EventKind::Planned, "2026-01-01") };
    let mut doc = Stops::default();
    doc = doc.add_event("A", planned.clone()).add_event("A", ev(EventKind::InService, "2026-02-01"));
    doc = doc.add_event("B", planned.clone()).add_event("B", ev(EventKind::InService, "2026-02-01")).add_event("B", ev(EventKind::Retired, "2026-10-01"));
    let mut by_request = ev(EventKind::Retired, "2026-10-01");
    by_request.request = Some("R-MOVE".into());
    doc = doc.add_event("C", planned).add_event("C", ev(EventKind::InService, "2026-02-01")).add_event("C", by_request);
    doc.checked().unwrap()
}

fn request(stop: &str, status: &str) -> StopRequest {
    StopRequest { stop: stop.into(), job: "repair_improve".into(), status: status.into(), ..StopRequest::default() }
}

#[test]
fn every_status_a_request_can_have_is_covered() {
    for status in STATUSES {
        assert!(request_needs(status).is_some(), "the rule names what a {status} request needs");
    }
    assert_eq!(request_needs("on_hold"), None, "a status this build does not know is not judged");
}

#[test]
fn only_an_open_request_on_a_retired_stop_breaks_the_rule() {
    for status in STATUSES {
        let needs = request_needs(status).unwrap();
        let open = transit_requests::OPEN_STATUSES.contains(&status);
        assert_eq!(needs, if open { Needs::NotRetired } else { Needs::AnyState }, "{status}");
        for stop in Status::ALL {
            assert_eq!(allows(needs, stop), !(open && stop == Status::Retired), "a {status} request, a {} stop", stop.as_str());
        }
    }
}

/// A retirement dated after the write's date breaks nothing.
#[test]
fn the_rule_reads_the_stop_on_the_write_s_date() {
    let doc = stops();
    let broke = request_rule(&doc, "R-1", &request("B", "submitted"), day("2026-11-21")).expect("B is retired on the write's date");
    assert_eq!((broke.stop.as_str(), broke.status, broke.since.as_deref()), ("B", Status::Retired, Some("2026-10-01")));
    assert_eq!(request_rule(&doc, "R-1", &request("B", "submitted"), day("2026-09-30")), None, "a retirement dated later breaks nothing yet");
    assert_eq!(request_rule(&doc, "R-1", &request("A", "ordered"), day("2026-11-21")), None, "A is in service");
    for status in ["completed", "cancelled"] {
        assert_eq!(request_rule(&doc, "R-1", &request("B", status), day("2026-11-21")), None, "a {status} request is history");
    }
}

#[test]
fn the_request_that_retired_the_stop_is_exempt() {
    let doc = stops();
    assert_eq!(request_rule(&doc, "R-MOVE", &request("C", "ordered"), day("2026-11-21")), None, "C's retirement is R-MOVE's outcome");
    assert!(request_rule(&doc, "R-OTHER", &request("C", "ordered"), day("2026-11-21")).is_some(), "another open request on C breaks the rule");
}

#[test]
fn a_stop_the_document_does_not_hold_or_an_unknown_status_is_not_judged() {
    let doc = stops();
    assert_eq!(request_rule(&doc, "R-1", &request("GONE", "submitted"), day("2026-11-21")), None, "the existence check's");
    assert_eq!(request_rule(&doc, "R-1", &request("B", "on_hold"), day("2026-11-21")), None, "a newer build's status");
}
