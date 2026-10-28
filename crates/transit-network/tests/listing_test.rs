//! The planner's lists: rows per kind narrowed by every query term,
//! and a route's composition — its trip counts the materialized day's.

use transit_network::{
    apply_edits, entity_rows, route_detail, stop_detail, timetable, timetable_against, Departs, EntityKind, HeadwayBand,
    Network, NetworkEdit, PatternTrips, StopOffset, Trip,
};

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
    let pattern_trips = &entity_rows(&net, EntityKind::PatternTrips, "")[0];
    assert_eq!(pattern_trips.context.as_deref(), Some("10 inbound via Main · Weekday"));
    assert_eq!(pattern_trips.trips, Some(3));
    assert_eq!(entity_rows(&net, EntityKind::Block, "400")[0].trips, Some(2));
}

#[test]
fn a_route_lists_its_composition() {
    let net = network();
    let detail = route_detail(&net, "10").unwrap();
    assert_eq!(detail.patterns.len(), 1);
    assert_eq!(detail.stops.iter().map(|s| s.id.as_str()).collect::<Vec<_>>(), ["A", "B", "C"]);
    assert_eq!(detail.pattern_trips_by_type.len(), 1);
    assert_eq!(detail.pattern_trips_by_type[0].name, "Weekday");
    assert_eq!(detail.pattern_trips_by_type[0].pattern_trips[0].trips, Some(3));
    assert!(route_detail(&net, "nope").is_none());
}

#[test]
fn a_stop_lists_the_routes_serving_it() {
    let net = network();
    let routes = stop_detail(&net, "B").unwrap().routes;
    assert_eq!(routes.iter().map(|r| (r.id.as_str(), r.name.as_str())).collect::<Vec<_>>(), [("10", "Route 10")]);
    assert!(stop_detail(&net, "nope").is_none());
}

/// QAF-51 regression (C-TRAN, Fisher's Landing): a parent station
/// whose patterns visit only its bays answers their routes — the
/// detail read pools the walking group, the same group the station
/// board departs across. It used to answer nothing.
#[test]
fn a_station_answers_its_bays_routes() {
    let net = apply_edits(&network(), &[
        NetworkEdit::UpsertStop {
            id: "STN".into(),
            stop: transit_network::Stop {
                name: "Main & 2nd Station".into(),
                lat: 45.510,
                lon: -73.510,
                parent: None,
                transfer: false,
                ..Default::default()
            },
        },
        NetworkEdit::UpsertStop {
            id: "B".into(),
            stop: transit_network::Stop {
                name: "Main & 2nd".into(),
                lat: 45.510,
                lon: -73.510,
                parent: Some("STN".into()),
                transfer: false,
                ..Default::default()
            },
        },
        NetworkEdit::UpsertStop {
            id: "N1".into(),
            stop: transit_network::Stop {
                name: "Main & 2nd across".into(),
                lat: 45.5105,
                lon: -73.5095,
                parent: Some("STN".into()),
                transfer: false,
                ..Default::default()
            },
        },
    ])
    .unwrap();
    let routes = |stop: &str| {
        stop_detail(&net, stop).unwrap().routes.iter().map(|r| r.id.clone()).collect::<Vec<_>>()
    };
    assert_eq!(routes("STN"), ["10"], "the station answers its bays' routes");
    assert_eq!(routes("N1"), ["10"], "a bay answers its sibling's routes too");
    assert_eq!(routes("B"), ["10"]);
}

#[test]
fn trip_counts_are_the_days_trips() {
    // A frequency trip counts its instances (00:00–01:00 every 30
    // min: two; an empty band: none) beside the two one-start trips.
    let mut net = network();
    net.pattern_trips.get_mut("10-weekday").unwrap().trips[2].departs = Departs::Every {
        bands: vec![
            HeadwayBand { start_secs: 0, end_secs: 3600, headway_secs: 1800 },
            HeadwayBand { start_secs: 7200, end_secs: 3600, headway_secs: 600 },
        ],
        exact: true,
    };
    assert_eq!(entity_rows(&net, EntityKind::PatternTrips, "")[0].trips, Some(4));
}

/// Route 10 in direction "in" (To Downtown): the committed pattern
/// A·B·C plus a bay variant A·B·D that leaves at B, on the weekday
/// type every 30 minutes 09:00–10:00.
fn with_a_bay_variant() -> Network {
    let net = network();
    let mut variant = net.patterns["10-inbound"].clone();
    variant.name = "10 inbound to Bay D".into();
    variant.stops[2].stop = "D".into();
    let mut bay = net.stops["C"].clone();
    bay.name = "Bay D".into();
    apply_edits(
        &net,
        &[
            NetworkEdit::UpsertStop { id: "D".into(), stop: bay },
            NetworkEdit::SetDirection { route: "10".into(), id: "in".into(), name: "To Downtown".into() },
            NetworkEdit::UpdatePattern { id: "10-inbound".into(), name: None, route: None, direction: Some("in".into()) },
            NetworkEdit::UpsertPattern { id: "10-bay".into(), pattern: variant },
            NetworkEdit::UpsertPatternTrips {
                id: "10-bay-weekday".into(),
                pattern_trips: PatternTrips {
                    pattern: "10-bay".into(),
                    service_type: "weekday".into(),
                    trips: vec![Trip {
                        id: "bay@every".into(),
                        departs: Departs::Every {
                            bands: vec![HeadwayBand { start_secs: 32_400, end_secs: 36_000, headway_secs: 1800 }],
                            exact: true,
                        },
                        times: vec![
                            StopOffset { arrival_secs: Some(0), departure_secs: Some(0) },
                            StopOffset::default(),
                            StopOffset { arrival_secs: Some(600), departure_secs: Some(600) },
                        ],
                        ..Default::default()
                    }],
                },
            },
        ],
    )
    .unwrap()
}

#[test]
fn a_route_lists_its_patterns_by_direction() {
    let net = with_a_bay_variant();
    let dirs = route_detail(&net, "10").unwrap().directions;
    assert_eq!(dirs.len(), 2, "the named direction, then the patterns in none");
    assert_eq!((dirs[0].direction.as_deref(), dirs[0].name.as_str()), (Some("in"), "To Downtown"));
    assert_eq!(dirs[0].patterns.iter().map(|p| p.id.as_str()).collect::<Vec<_>>(), ["10-inbound"]);
    assert_eq!((dirs[1].direction.as_deref(), dirs[1].patterns[0].id.as_str()), (None, "10-bay"));
    // With every pattern placed, no "No direction" group appears.
    assert_eq!(route_detail(&network(), "10").unwrap().directions.len(), 1);
}

#[test]
fn a_timetable_is_one_direction_on_one_service_type_across_its_patterns() {
    let net = apply_edits(
        &with_a_bay_variant(),
        &[NetworkEdit::UpdatePattern { id: "10-bay".into(), name: None, route: None, direction: Some("in".into()) }],
    )
    .unwrap();
    let t = timetable(&net, "10", Some("in"), "weekday").unwrap();
    assert_eq!((t.direction_name.as_str(), t.service_type_name.as_str()), ("To Downtown", "Weekday"));
    // The variant's bay sits where it leaves the others.
    assert_eq!(t.stops.iter().map(|s| s.stop.as_str()).collect::<Vec<_>>(), ["A", "B", "D", "C"]);
    assert_eq!(t.stops.iter().map(|s| s.timepoint).collect::<Vec<_>>(), [true, false, true, true]);
    // Legend: most trips first; the bands are marked.
    let legend: Vec<_> = t.patterns.iter().map(|p| (p.pattern.as_str(), p.trips, p.headway)).collect();
    assert_eq!(legend, [("10-inbound", 3, false), ("10-bay", 2, true)]);
    // Rows by start, each with its trip, block and times.
    let rows: Vec<_> =
        t.trips.iter().map(|r| (r.start_secs, r.trip.as_str(), r.pattern.as_str(), r.block.as_deref())).collect();
    assert_eq!(
        rows,
        [(28_800, "10-weekday@28800", "10-inbound", Some("400")), (30_600, "10-weekday@30600", "10-inbound", Some("400")),
         (32_400, "bay@every", "10-bay", None), (34_200, "bay@every", "10-bay", None),
         (91_800, "10-weekday@91800", "10-inbound", None)]
    );
    assert_eq!(t.trips[0].times, [Some(28_800), Some(29_100), None, Some(29_400)]);
    assert_eq!(t.trips[2].times, [Some(32_400), Some(32_700), Some(33_000), None]);

    // The patterns in no direction are their own timetable; an
    // unknown route, direction or type is no timetable.
    assert!(timetable(&net, "10", None, "weekday").unwrap().trips.is_empty());
    assert_eq!(timetable(&with_a_bay_variant(), "10", None, "weekday").unwrap().trips.len(), 2);
    assert!(timetable(&net, "10", Some("out"), "weekday").is_none());
    assert!(timetable(&net, "ghost", Some("in"), "weekday").is_none());
    assert!(timetable(&net, "10", Some("in"), "ghost").is_none());
}

#[test]
fn a_timetable_against_its_baseline_marks_what_changed() {
    // Every 30 min 07:50–09:00: 08:00 and 08:30 move to 07:50 and
    // 08:20 (a row is a trip at a start: removed at the old, added at
    // the new), 08:50 is a new trip, 25:30 is untouched.
    let base = network();
    let edit = NetworkEdit::SetHeadway {
        route: "10".into(),
        direction: None,
        service_type: "weekday".into(),
        from_secs: 28_200,
        to_secs: 32_400,
        headway_secs: 1800,
        pattern: None,
    };
    let next = apply_edits(&base, &[edit]).unwrap();
    let t = timetable_against(&next, &base, "10", None, "weekday").unwrap();
    let marks: Vec<_> = t.trips.iter().map(|x| (x.start_secs, x.added)).collect();
    assert_eq!(marks, [(28_200, true), (30_000, true), (31_800, true), (91_800, false)]);
    assert_eq!(t.removed.iter().map(|r| r.start_secs).collect::<Vec<_>>(), [28_800, 30_600]);
    assert_eq!(t.trips[0].trip, "10-weekday@28800", "the moved trip keeps its id");
    let plain = timetable(&next, "10", None, "weekday").unwrap();
    assert!(plain.trips.iter().all(|x| !x.added) && plain.removed.is_empty(), "no baseline, no marks");
}
