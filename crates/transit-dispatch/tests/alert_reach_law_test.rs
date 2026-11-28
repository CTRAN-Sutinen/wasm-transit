//! `alert_reach` returns the departures that an alert's scopes select
//! (crates/cap-board/OPS.md "alert_reach").
//!
//! The fixture is the e2e network on Friday 2026-10-23. t10 (route 10, A to
//! B), t20 (route 20, A to B), t30 (route 10, A to C to B), t40 and t41
//! (route 40, between the bays SA and SB of station STN).

use proptest::prelude::*;
use transit_dispatch::alert_reach;
use transit_live::AlertScope;
use transit_network::Network;

fn network() -> Network {
    let feed = transit_codec::Feed::from_zip_bytes(include_bytes!(
        "../../../web/e2e/fixtures/sample-gtfs.zip"
    ))
    .unwrap();
    transit_codec::import(&feed).network
}

const DATE: &str = "2026-10-23";

fn day(net: &Network) -> Vec<transit_network::MaterializedTrip> {
    transit_dispatch::day(
        net,
        transit_date::Date::from_ymd_opt(2026, 10, 23).unwrap(),
        &[],
    )
    .unwrap()
    .trips
}

fn scope(route: Option<&str>, stop: Option<&str>) -> AlertScope {
    AlertScope {
        route: route.map(String::from),
        stop: stop.map(String::from),
        ..Default::default()
    }
}

fn stops(r: &transit_dispatch::Reach) -> Vec<(&str, u32)> {
    r.stops
        .iter()
        .map(|s| (s.stop.as_str(), s.departures))
        .collect()
}

#[test]
fn route_and_stop_together_reach_that_route_at_that_stop_only() {
    let net = network();
    let r = alert_reach(&net, &day(&net), &[scope(Some("10"), Some("A"))], DATE);
    assert_eq!(stops(&r), [("A", 2)]);
    assert_eq!(r.routes, ["10"]);
    assert_eq!(r.departures, 2);
}

#[test]
fn a_route_alone_reaches_its_departures_at_every_stop() {
    let net = network();
    let r = alert_reach(&net, &day(&net), &[scope(Some("10"), None)], DATE);
    assert_eq!(stops(&r), [("A", 2), ("C", 1)]);
}

#[test]
fn a_stop_alone_reaches_every_route_there() {
    let net = network();
    let r = alert_reach(&net, &day(&net), &[scope(None, Some("A"))], DATE);
    assert_eq!(stops(&r), [("A", 3)]);
    assert_eq!(r.routes, ["10", "20"]);
}

#[test]
fn a_station_reaches_no_departure_at_its_bays() {
    let net = network();
    let r = alert_reach(&net, &day(&net), &[scope(None, Some("STN"))], DATE);
    assert_eq!(r.departures, 0);
    let bay = alert_reach(&net, &day(&net), &[scope(None, Some("SA"))], DATE);
    assert_eq!(stops(&bay), [("SA", 1)]);
}

#[test]
fn the_whole_agency_reaches_every_departure() {
    let net = network();
    let r = alert_reach(&net, &day(&net), &[AlertScope::default()], DATE);
    assert_eq!(r.departures, 6);
}

#[test]
fn a_trip_reaches_only_on_its_date() {
    let net = network();
    let on = |date: Option<&str>| AlertScope {
        trip: Some("t10".into()),
        date: date.map(String::from),
        start_secs: Some(28800),
        ..Default::default()
    };
    assert_eq!(
        stops(&alert_reach(&net, &day(&net), &[on(Some(DATE))], DATE)),
        [("A", 1)]
    );
    assert_eq!(
        alert_reach(&net, &day(&net), &[on(Some("2026-10-24"))], DATE).departures,
        0
    );
    assert_eq!(
        alert_reach(&net, &day(&net), &[on(None)], DATE).departures,
        1
    );
}

fn arb_scope() -> impl Strategy<Value = AlertScope> {
    (
        proptest::option::of(prop::sample::select(vec!["10", "20", "40"])),
        proptest::option::of(prop::sample::select(vec!["A", "B", "C", "SA", "SB", "STN"])),
        proptest::option::of(prop::sample::select(vec!["t10", "t30", "t41"])),
    )
        .prop_map(|(route, stop, trip)| AlertScope {
            route: route.map(String::from),
            stop: stop.map(String::from),
            trip: trip.map(String::from),
            ..Default::default()
        })
}

proptest! {
    #[test]
    fn several_scopes_reach_what_each_reaches(scopes in prop::collection::vec(arb_scope(), 1..4)) {
        let net = network();
        let d = day(&net);
        let all = alert_reach(&net, &d, &scopes, DATE);
        let mut each: std::collections::BTreeSet<(String, String, u32)> = Default::default();
        for s in &scopes {
            each.extend(alert_reach(&net, &d, std::slice::from_ref(s), DATE).visits);
        }
        prop_assert_eq!(all.visits.into_iter().collect::<std::collections::BTreeSet<_>>(), each);
    }
}

#[test]
fn a_cancelled_trip_is_reached_where_the_board_lists_it_skipped() {
    let net = network();
    let cancel: transit_sync::TripCancel = serde_json::from_value(
        serde_json::json!({ "id": "c30", "trip_id": "t30", "start_secs": 31200, "date": DATE }),
    )
    .unwrap();
    let record = transit_sync::EditRecord {
        id: "r30".into(),
        diversion: None,
        cancel: Some(cancel),
        status: transit_sync::EditStatus::Accepted,
        rev: 1,
        updated_at: 0,
        extra: Default::default(),
    };
    let d = transit_dispatch::day(
        &net,
        transit_date::Date::from_ymd_opt(2026, 10, 23).unwrap(),
        &[record],
    )
    .unwrap();
    let trip = AlertScope {
        route: Some("10".into()),
        trip: Some("t30".into()),
        date: Some(DATE.into()),
        start_secs: Some(31200),
        ..Default::default()
    };
    let r = transit_dispatch::alert_reach_day(&net, &d, &[trip], DATE);
    assert_eq!(stops(&r), [("A", 1), ("C", 1)]);
}
