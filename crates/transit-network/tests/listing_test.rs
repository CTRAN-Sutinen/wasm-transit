//! The planner's lists: rows per kind narrowed by every query term,
//! and a route's composition — its trip counts the materialized day's.

use transit_network::{entity_rows, route_detail, stop_detail, EntityKind, HeadwayBand, Network, Schedule};

fn network() -> Network {
    serde_json::from_str(include_str!("../../../testdata/network.json")).unwrap()
}

#[test]
fn rows_match_every_term_in_name_or_id() {
    let net = network();
    let ids = |kind, q| entity_rows(&net, kind, q).into_iter().map(|r| r.id).collect::<Vec<_>>();
    assert_eq!(ids(EntityKind::Stop, ""), ["A", "B", "C"]);
    assert_eq!(ids(EntityKind::Stop, "MAIN 2nd"), ["B"]);
    assert_eq!(ids(EntityKind::Stop, "main nowhere"), Vec::<String>::new());
    assert_eq!(ids(EntityKind::Route, "10"), ["10"]);

    let pattern = &entity_rows(&net, EntityKind::Pattern, "inbound")[0];
    assert_eq!(pattern.context.as_deref(), Some("Route 10"), "a pattern is placed by its route");
    let schedule = &entity_rows(&net, EntityKind::Schedule, "")[0];
    assert_eq!(schedule.context.as_deref(), Some("10 inbound via Main · Weekday"));
    assert_eq!(schedule.trips, Some(3));
    assert_eq!(entity_rows(&net, EntityKind::Block, "400")[0].trips, Some(2));
}

#[test]
fn a_route_lists_its_composition() {
    let net = network();
    let detail = route_detail(&net, "10").unwrap();
    assert_eq!(detail.patterns.len(), 1);
    assert_eq!(detail.stops.iter().map(|s| s.id.as_str()).collect::<Vec<_>>(), ["A", "B", "C"]);
    assert_eq!(detail.schedules_by_type.len(), 1);
    assert_eq!(detail.schedules_by_type[0].name, "Weekday");
    assert_eq!(detail.schedules_by_type[0].schedules[0].trips, Some(3));
    assert!(route_detail(&net, "nope").is_none());
}

#[test]
fn a_stop_lists_the_routes_serving_it() {
    let net = network();
    let routes = stop_detail(&net, "B").unwrap().routes;
    assert_eq!(routes.iter().map(|r| (r.id.as_str(), r.name.as_str())).collect::<Vec<_>>(), [("10", "Route 10")]);
    assert!(stop_detail(&net, "nope").is_none());
}

#[test]
fn trip_counts_are_the_days_trips() {
    // Overlapping bands share a start: the day runs it once, and so
    // does the count (00:00–01:00 and 00:30–01:30 every 30 min).
    let mut net = network();
    net.schedules.get_mut("10-weekday").unwrap().schedule = Schedule::HeadwayBands {
        bands: vec![
            HeadwayBand { start_secs: 0, end_secs: 3600, headway_secs: 1800 },
            HeadwayBand { start_secs: 1800, end_secs: 5400, headway_secs: 1800 },
            HeadwayBand { start_secs: 7200, end_secs: 3600, headway_secs: 600 },
        ],
    };
    assert_eq!(entity_rows(&net, EntityKind::Schedule, "")[0].trips, Some(3));
}
