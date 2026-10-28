//! Service alerts in model vocabulary. GTFS-rt's selector rule: the
//! fields of one informed entity are ANDed (route 12 AT stop 6277),
//! the entities of one alert are ORed, and an entity naming only the
//! agency covers everything.

use transit_live::{AlertScope, LiveAlert, Visit};

fn scope(route: Option<&str>, stop: Option<&str>, trip: Option<&str>, start: Option<u32>) -> AlertScope {
    AlertScope {
        route: route.map(Into::into),
        stop: stop.map(Into::into),
        trip: trip.map(Into::into),
        start_secs: start,
    }
}

fn alert(scopes: Vec<AlertScope>) -> LiveAlert {
    LiveAlert { id: "a1".into(), header: "Detour".into(), scopes, ..Default::default() }
}

fn visit<'a>(route: &'a str, stop: &'a str, trip: &'a str, start: u32) -> Visit<'a> {
    Visit { route, stop, trip, start_secs: start }
}

#[test]
fn an_agency_wide_alert_touches_every_visit() {
    let a = alert(vec![scope(None, None, None, None)]);
    assert!(a.touches(&visit("12", "6277", "t1", 28800)));
    assert!(a.touches_stop("anything"));
}

#[test]
fn the_fields_of_one_scope_are_anded() {
    let a = alert(vec![scope(Some("12"), Some("6277"), None, None)]);
    assert!(a.touches(&visit("12", "6277", "t1", 28800)));
    assert!(!a.touches(&visit("12", "6130", "t1", 28800)), "route 12 elsewhere");
    assert!(!a.touches(&visit("34", "6277", "t2", 28800)), "another route at the stop");
}

#[test]
fn the_scopes_of_one_alert_are_ored() {
    let a = alert(vec![scope(Some("12"), None, None, None), scope(None, Some("106"), None, None)]);
    assert!(a.touches(&visit("12", "6130", "t1", 0)));
    assert!(a.touches(&visit("34", "106", "t2", 0)));
    assert!(!a.touches(&visit("34", "6130", "t2", 0)));
}

#[test]
fn a_trip_scope_names_one_instance_or_all_of_them() {
    let one = alert(vec![scope(None, None, Some("t1"), Some(28800))]);
    assert!(one.touches(&visit("12", "A", "t1", 28800)));
    assert!(!one.touches(&visit("12", "A", "t1", 30600)), "another instance of a frequency trip");
    let every = alert(vec![scope(None, None, Some("t1"), None)]);
    assert!(every.touches(&visit("12", "A", "t1", 30600)));
    assert!(!every.touches(&visit("12", "A", "t2", 30600)));
}

#[test]
fn a_stop_is_touched_only_by_a_scope_about_the_whole_stop() {
    // "Stop 6277 is closed" is the stop's news; "route 12 skips 6277"
    // is news only where the 12 leaves from it.
    assert!(alert(vec![scope(None, Some("6277"), None, None)]).touches_stop("6277"));
    assert!(!alert(vec![scope(None, Some("6277"), None, None)]).touches_stop("6130"));
    assert!(!alert(vec![scope(Some("12"), Some("6277"), None, None)]).touches_stop("6277"));
    assert!(!alert(vec![scope(Some("12"), None, None, None)]).touches_stop("6277"));
}
