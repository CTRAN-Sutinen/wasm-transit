//! The gesture laws — one edit per thing a planner does, each
//! carrying the rule that keeps the document coherent, over the
//! committed planner network (testdata/network.json: stops A/B/C,
//! route 10, pattern 10-inbound A·B·C with A and C timepoints, the
//! weekday trips at 08:00 / 08:30 / 25:30, block 400 with one
//! layover).
//!
//! 1. creation mints the first free id (or takes the caller's) and
//!    reports it;
//! 2. timing vectors and every trip's times stay aligned with the
//!    pattern's stops through every pattern gesture;
//! 3. blocks stay in time order and layovers only sit between two
//!    trips;
//! 4. a gesture that cannot apply is a named error, never a silent
//!    no-op — and the document is untouched.

use transit_network::{
    apply_edits, apply_edits_reporting, Day, DayState, Departs, HeadwayBand, Network, NetworkEdit, StopOffset,
    TripRef,
};

fn network() -> Network {
    serde_json::from_str(include_str!("../../../testdata/network.json")).expect("testdata network parses")
}

fn apply(net: &Network, edits: Vec<NetworkEdit>) -> Network {
    apply_edits(net, &edits).expect("the edits apply")
}

fn refused(net: &Network, edit: NetworkEdit) -> String {
    apply_edits(net, &[edit]).expect_err("the edit is refused").reason
}

/// Every profile carries one time per timepoint, and every trip one
/// time per stop.
fn aligned(net: &Network, pattern: &str) -> bool {
    let p = &net.patterns[pattern];
    let n = p.stops.iter().filter(|s| s.timepoint).count();
    p.timing.iter().all(|t| t.at_timepoints.len() == n)
        && net.trips().filter(|(_, pt, _)| pt.pattern == pattern).all(|(_, _, t)| t.times.len() == p.stops.len())
}

fn starts(net: &Network, pattern_trips: &str) -> Vec<u32> {
    net.pattern_trips[pattern_trips].trips.iter().filter_map(|t| t.departs.first_start()).collect()
}

#[test]
fn creation_mints_ids_and_reports_them() {
    let net = network();
    let (next, created) = apply_edits_reporting(
        &net,
        &[
            NetworkEdit::AddStop { id: None, lat: None, lon: None },
            NetworkEdit::AddStop { id: None, lat: Some(45.6), lon: Some(-122.5) },
            NetworkEdit::AddRoute { id: Some("20".into()) },
            NetworkEdit::AddPattern { id: None, route: Some("20".into()) },
            NetworkEdit::AddServiceType { id: None, from: "2026-09-07".into() },
            NetworkEdit::AddPatternTrips { id: None, pattern: Some("pat1".into()), service_type: Some("cal1".into()) },
            NetworkEdit::AddBlock { id: None },
        ],
    )
    .unwrap();
    let ids: Vec<(&str, &str)> = created.iter().map(|c| (c.kind.as_str(), c.id.as_str())).collect();
    assert_eq!(
        ids,
        vec![
            ("stop", "s1"),
            ("stop", "s2"),
            ("route", "20"),
            ("pattern", "pat1"),
            ("service_type", "cal1"),
            ("pattern_trips", "svc1"),
            ("block", "b1")
        ]
    );
    // An unplaced stop lands at the centre of the network's stops;
    // a placed one where it was put.
    let (a, b, c) = (&net.stops["A"], &net.stops["B"], &net.stops["C"]);
    assert!((next.stops["s1"].lat - (a.lat + b.lat + c.lat) / 3.0).abs() < 1e-9);
    assert_eq!((next.stops["s2"].lat, next.stops["s2"].lon), (45.6, -122.5));
    // The skeleton pattern: two timepoints, one aligned profile.
    assert!(aligned(&next, "pat1"));
    assert_eq!(next.patterns["pat1"].route, "20");
    // A service type runs a year from the date it was given.
    let range = &next.service_types["cal1"].rule.ranges[0];
    assert_eq!((range.start.to_string(), range.end.to_string()), ("2026-09-07".into(), "2027-09-06".into()));
    assert_eq!(starts(&next, "svc1"), vec![8 * 3600]);

    // A taken id is refused, not overwritten.
    assert!(refused(&net, NetworkEdit::AddStop { id: Some("A".into()), lat: None, lon: None }).contains("already exists"));
    // A pattern needs a route and stops to exist.
    let empty = Network { routes: Default::default(), ..net.clone() };
    assert!(refused(&empty, NetworkEdit::AddPattern { id: None, route: None }).contains("add a route first"));
}

#[test]
fn pattern_gestures_keep_timing_aligned() {
    let net = network();
    // Make B a timepoint: the profile gains the midpoint of A and C,
    // and every trip, exact there now, is scheduled where it ran.
    let on = apply(&net, vec![NetworkEdit::ToggleTimepoint { pattern: "10-inbound".into(), index: 1 }]);
    assert!(on.patterns["10-inbound"].stops[1].timepoint);
    assert_eq!(on.patterns["10-inbound"].timing[0].at_timepoints, vec![0, 300, 600]);
    for (_, _, t) in on.trips() {
        assert_eq!(t.times[1], StopOffset { arrival_secs: Some(300), departure_secs: Some(300) });
    }
    assert!(transit_network::validation_issues(&on).is_empty());
    // …and back: the entry leaves the profile; the trips keep the
    // times they were given.
    let off = apply(&on, vec![NetworkEdit::ToggleTimepoint { pattern: "10-inbound".into(), index: 1 }]);
    assert_eq!(off.patterns["10-inbound"], net.patterns["10-inbound"]);
    assert_eq!(off.pattern_trips, on.pattern_trips);

    // Insert before the last stop: halfway along the last leg, not a
    // timepoint — the vectors do not move.
    let inserted = apply(&net, vec![NetworkEdit::InsertPatternStop { pattern: "10-inbound".into(), stop: "A".into(), index: None }]);
    let stops = &inserted.patterns["10-inbound"].stops;
    assert_eq!(stops.len(), 4);
    assert_eq!(stops[2].stop, "A");
    assert!(!stops[2].timepoint);
    assert_eq!(stops[2].distance_m, (stops[1].distance_m + stops[3].distance_m) / 2.0);
    assert!(aligned(&inserted, "10-inbound"));
    assert!(inserted.trips().all(|(_, _, t)| t.times[2] == StopOffset::default()), "unscheduled on every trip");
    assert!(transit_network::validation_issues(&inserted).is_empty());

    // Removing a middle TIMEPOINT drops its time from every profile.
    let with_profile = apply(&on, vec![NetworkEdit::AddTimingProfile { pattern: "10-inbound".into() }]);
    assert_eq!(with_profile.patterns["10-inbound"].timing.len(), 2);
    let removed = apply(&with_profile, vec![NetworkEdit::RemovePatternStop { pattern: "10-inbound".into(), index: 1 }]);
    assert!(aligned(&removed, "10-inbound"));
    assert_eq!(removed.patterns["10-inbound"].timing[1].at_timepoints, vec![0, 600]);

    // Endpoints are fixed; the last profile stays.
    assert!(refused(&net, NetworkEdit::RemovePatternStop { pattern: "10-inbound".into(), index: 0 }).contains("endpoint"));
    assert!(refused(&net, NetworkEdit::ToggleTimepoint { pattern: "10-inbound".into(), index: 2 }).contains("endpoint"));
    assert!(refused(&net, NetworkEdit::RemoveTimingProfile { pattern: "10-inbound".into(), index: 0 }).contains("at least one"));
    assert!(refused(&net, NetworkEdit::SetTimepointTime { pattern: "10-inbound".into(), profile: 0, timepoint: 5, secs: 1 }).contains("no timepoint 5"));
}

#[test]
fn bands_belong_to_frequency_trips() {
    let net = network();
    assert!(refused(&net, NetworkEdit::AddBand { trip: "10-weekday@91800".into() }).contains("runs once"));
    let mut every = net.clone();
    every.pattern_trips.get_mut("10-weekday").unwrap().trips[2].departs = Departs::Every {
        bands: vec![HeadwayBand { start_secs: 6 * 3600, end_secs: 9 * 3600, headway_secs: 900 }],
        exact: true,
    };
    let added = apply(&every, vec![NetworkEdit::AddBand { trip: "10-weekday@91800".into() }]);
    let Departs::Every { bands, .. } = &added.trip("10-weekday@91800").unwrap().2.departs else { panic!("every") };
    assert_eq!(bands[1], HeadwayBand { start_secs: 9 * 3600, end_secs: 12 * 3600, headway_secs: 900 });
    assert!(refused(&every, NetworkEdit::RemoveBand { trip: "10-weekday@91800".into(), index: 0 }).contains("at least one"));
    assert!(refused(&every, NetworkEdit::RetimeTrip { trip: "10-weekday@91800".into(), to_secs: 1 }).contains("frequency"));
    assert_eq!(starts(&net, "10-weekday"), vec![28_800, 30_600, 91_800]);
}

#[test]
fn blocks_stay_in_time_order_with_layovers_between_trips() {
    let net = network();
    // An earlier trip lands first; the layover index still points at
    // a pair, the one past the end is pruned.
    let early = apply(
        &net,
        vec![NetworkEdit::AddBlockTrip { block: "400".into(), trip: TripRef { trip: "10-weekday@91800".into() } }],
    );
    let order: Vec<&str> = early.blocks["400"].trips.iter().map(|t| t.trip.as_str()).collect();
    assert_eq!(order, vec!["10-weekday@28800", "10-weekday@30600", "10-weekday@91800"]);
    let set = apply(&early, vec![NetworkEdit::SetLayover { block: "400".into(), after_trip: 1, secs: Some(600) }]);
    assert_eq!(set.blocks["400"].layovers.len(), 2);
    let cut = apply(&set, vec![NetworkEdit::RemoveBlockTrip { block: "400".into(), index: 2 }]);
    assert_eq!(cut.blocks["400"].layovers.len(), 1, "the layover after the new last trip goes with it");
    let cleared = apply(&net, vec![NetworkEdit::SetLayover { block: "400".into(), after_trip: 0, secs: None }]);
    assert!(cleared.blocks["400"].layovers.is_empty());
    assert!(refused(&net, NetworkEdit::SetLayover { block: "400".into(), after_trip: 1, secs: Some(60) }).contains("between two"));
    assert!(refused(
        &net,
        NetworkEdit::AddBlockTrip { block: "400".into(), trip: TripRef { trip: "ghost".into() } }
    )
    .contains("unknown trip"));
}

#[test]
fn calendars_edit_in_place_with_exceptions_in_date_order() {
    let net = network();
    let next = apply(
        &net,
        vec![
            NetworkEdit::ToggleServiceDay { id: "weekday".into(), day: Day::Sat },
            NetworkEdit::SetServiceException { id: "weekday".into(), date: "2026-12-25".into(), state: DayState::Removed },
            NetworkEdit::SetServiceException { id: "weekday".into(), date: "2026-11-26".into(), state: DayState::Removed },
            NetworkEdit::SetServiceException { id: "weekday".into(), date: "2026-12-25".into(), state: DayState::Added },
            NetworkEdit::AddServiceRange { id: "weekday".into() },
        ],
    );
    let rule = &next.service_types["weekday"].rule;
    assert!(rule.days.sat);
    let dates: Vec<String> = rule.exceptions.iter().map(|e| e.date.to_string()).collect();
    assert_eq!(dates, vec!["2026-11-26", "2026-12-25"], "one per date, in date order");
    assert_eq!(rule.exceptions[1].state, DayState::Added, "a second exception for a date replaces the first");
    let n = rule.ranges.len();
    assert_eq!(rule.ranges[n - 1].start, rule.ranges[n - 2].end, "a new range continues the last");
    let removed = apply(&next, vec![NetworkEdit::RemoveServiceException { id: "weekday".into(), date: "2026-11-26".into() }]);
    assert_eq!(removed.service_types["weekday"].rule.exceptions.len(), 1);
    assert!(refused(&net, NetworkEdit::SetServiceRange { id: "weekday".into(), index: 0, start: Some("2026-13-01".into()), end: None }).contains("invalid date"));
}

#[test]
fn field_updates_check_their_references() {
    let net = network();
    let next = apply(
        &net,
        vec![
            NetworkEdit::UpdateStop { id: "A".into(), name: Some("Main & 1st".into()), lat: Some(f64::NAN), lon: None, transfer: Some(true) },
            NetworkEdit::UpdateServiceType { id: "weekday".into(), name: "Weekdays".into() },
        ],
    );
    assert_eq!(next.stops["A"].name, "Main & 1st");
    assert_eq!(next.stops["A"].lat, net.stops["A"].lat, "a non-finite coordinate is ignored, never stored");
    assert!(next.stops["A"].transfer);
    assert!(refused(&net, NetworkEdit::UpdatePattern { id: "10-inbound".into(), name: None, route: Some("ghost".into()), direction: None }).contains("unknown route"));
    assert!(refused(&net, NetworkEdit::SetPatternTripsRefs { id: "10-weekday".into(), pattern: Some("ghost".into()), service_type: None }).contains("unknown pattern"));
}

#[test]
fn a_pattern_runs_in_one_of_its_routes_directions() {
    let net = network();
    let placed = apply(
        &net,
        vec![
            NetworkEdit::SetDirection { route: "10".into(), id: "in".into(), name: "To C".into() },
            NetworkEdit::UpdatePattern { id: "10-inbound".into(), name: None, route: None, direction: Some("in".into()) },
            NetworkEdit::SetDirection { route: "10".into(), id: "in".into(), name: "To Downtown".into() },
        ],
    );
    assert_eq!(placed.routes["10"].directions["in"].name, "To Downtown", "setting an existing id renames it");
    assert_eq!(placed.patterns["10-inbound"].direction.as_deref(), Some("in"));
    assert!(transit_network::validation_issues(&placed).is_empty());

    // Only a direction the route has; never one in use.
    assert!(refused(&net, NetworkEdit::UpdatePattern { id: "10-inbound".into(), name: None, route: None, direction: Some("in".into()) })
        .contains("route 10 has no direction in"));
    assert!(refused(&placed, NetworkEdit::RemoveDirection { route: "10".into(), id: "in".into() }).contains("carries pattern 10-inbound"));
    let cleared = apply(
        &placed,
        vec![
            NetworkEdit::UpdatePattern { id: "10-inbound".into(), name: None, route: None, direction: Some(String::new()) },
            NetworkEdit::RemoveDirection { route: "10".into(), id: "in".into() },
        ],
    );
    assert_eq!(cleared.patterns["10-inbound"].direction, None, "\"\" leaves the pattern in no direction");
    assert!(cleared.routes["10"].directions.is_empty());

    // Direction ids are per route: moving routes leaves the pattern
    // in none, unless the same edit places it.
    let two = apply(&placed, vec![NetworkEdit::AddRoute { id: Some("20".into()) }]);
    let moved = apply(&two, vec![NetworkEdit::UpdatePattern { id: "10-inbound".into(), name: None, route: Some("20".into()), direction: None }]);
    assert_eq!(moved.patterns["10-inbound"].direction, None);
    assert!(refused(&two, NetworkEdit::UpdatePattern { id: "10-inbound".into(), name: None, route: Some("20".into()), direction: Some("in".into()) })
        .contains("route 20 has no direction in"));

    // An upsert can still strand one — validation names it.
    let mut route = placed.routes["10"].clone();
    route.directions.clear();
    let stranded = apply(&placed, vec![NetworkEdit::UpsertRoute { id: "10".into(), route }]);
    assert!(transit_network::validation_issues(&stranded)
        .iter()
        .any(|i| matches!(i, transit_network::NetworkError::UnknownDirection { direction, .. } if direction == "in")));
}

#[test]
fn the_plan_frame_updates_field_wise() {
    let net = network();
    let named = apply(
        &net,
        vec![
            NetworkEdit::UpdatePlan { name: Some("Spring".into()), effective_from: None, id: Some("p1".into()) },
            NetworkEdit::UpdatePlan { name: None, effective_from: Some("2026-03-01".into()), id: Some("p2".into()) },
        ],
    );
    assert_eq!(named.plan.name, "Spring", "a later field update keeps the earlier one");
    assert_eq!(named.plan.effective_from.map(|d| d.to_string()), Some("2026-03-01".into()));
    let expected_id = if net.plan.id.is_empty() { "p1".to_string() } else { net.plan.id.clone() };
    assert_eq!(named.plan.id, expected_id, "only an unversioned plan is stamped, and only once");
    let cleared = apply(&named, vec![NetworkEdit::UpdatePlan { name: None, effective_from: Some("".into()), id: None }]);
    assert_eq!(cleared.plan.effective_from, None);
}
