//! The gesture laws — one edit per thing a planner does, each
//! carrying the rule that keeps the document coherent, over the
//! committed planner network (testdata/network.json: stops A/B/C,
//! route 10, pattern 10-inbound A·B·C with A and C timepoints, the
//! weekday schedule at 08:00 / 08:30 / 25:30, block 400 with one
//! layover).
//!
//! 1. creation mints the first free id (or takes the caller's) and
//!    reports it;
//! 2. timing vectors stay aligned with the timepoints through every
//!    pattern gesture;
//! 3. starts stay sorted and unique; blocks stay in time order and
//!    layovers only sit between two trips;
//! 4. a gesture that cannot apply is a named error, never a silent
//!    no-op — and the document is untouched.

use transit_network::{
    apply_edits, apply_edits_reporting, Day, DayState, Network, NetworkEdit, Schedule, TripRef,
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

/// Every profile carries one time per timepoint.
fn aligned(net: &Network, pattern: &str) -> bool {
    let p = &net.patterns[pattern];
    let n = p.stops.iter().filter(|s| s.timepoint).count();
    p.timing.iter().all(|t| t.at_timepoints.len() == n)
}

fn starts(net: &Network, schedule: &str) -> Vec<u32> {
    match &net.schedules[schedule].schedule {
        Schedule::ExactTimes { starts } => starts.clone(),
        Schedule::HeadwayBands { .. } => panic!("exact times"),
    }
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
            NetworkEdit::AddSchedule { id: None, pattern: Some("pat1".into()), service_type: Some("cal1".into()) },
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
            ("schedule", "svc1"),
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
    // Make B a timepoint: the profile gains the midpoint of A and C.
    let on = apply(&net, vec![NetworkEdit::ToggleTimepoint { pattern: "10-inbound".into(), index: 1 }]);
    assert!(on.patterns["10-inbound"].stops[1].timepoint);
    assert_eq!(on.patterns["10-inbound"].timing[0].at_timepoints, vec![0, 300, 600]);
    // …and back: the entry leaves.
    let off = apply(&on, vec![NetworkEdit::ToggleTimepoint { pattern: "10-inbound".into(), index: 1 }]);
    assert_eq!(off.patterns["10-inbound"], net.patterns["10-inbound"]);

    // Insert before the last stop: halfway along the last leg, not a
    // timepoint — the vectors do not move.
    let inserted = apply(&net, vec![NetworkEdit::InsertPatternStop { pattern: "10-inbound".into(), stop: "A".into() }]);
    let stops = &inserted.patterns["10-inbound"].stops;
    assert_eq!(stops.len(), 4);
    assert_eq!(stops[2].stop, "A");
    assert!(!stops[2].timepoint);
    assert_eq!(stops[2].distance_m, (stops[1].distance_m + stops[3].distance_m) / 2.0);
    assert!(aligned(&inserted, "10-inbound"));

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
fn starts_stay_sorted_and_unique() {
    let net = network();
    let set = apply(&net, vec![NetworkEdit::SetStarts { schedule: "10-weekday".into(), starts: vec![900, 300, 900] }]);
    assert_eq!(starts(&set, "10-weekday"), vec![300, 900]);
    let added = apply(&net, vec![NetworkEdit::AddStart { schedule: "10-weekday".into() }]);
    assert_eq!(starts(&added, "10-weekday"), vec![28_800, 30_600, 91_800, 92_400], "ten minutes after the last");
    let removed = apply(&net, vec![NetworkEdit::RemoveStart { schedule: "10-weekday".into(), index: 0 }]);
    assert_eq!(starts(&removed, "10-weekday"), vec![30_600, 91_800]);
    let one = apply(&net, vec![NetworkEdit::SetStarts { schedule: "10-weekday".into(), starts: vec![28_800] }]);
    assert!(refused(&one, NetworkEdit::RemoveStart { schedule: "10-weekday".into(), index: 0 }).contains("at least one"));
    assert!(refused(&net, NetworkEdit::AddBand { schedule: "10-weekday".into() }).contains("not headway bands"));
}

#[test]
fn blocks_stay_in_time_order_with_layovers_between_trips() {
    let net = network();
    // An earlier trip lands first; the layover index still points at
    // a pair, the one past the end is pruned.
    let early = apply(
        &net,
        vec![NetworkEdit::AddBlockTrip { block: "400".into(), trip: TripRef { schedule: "10-weekday".into(), start_secs: 91_800 } }],
    );
    let order: Vec<u32> = early.blocks["400"].trips.iter().map(|t| t.start_secs).collect();
    assert_eq!(order, vec![28_800, 30_600, 91_800]);
    let set = apply(&early, vec![NetworkEdit::SetLayover { block: "400".into(), after_trip: 1, secs: Some(600) }]);
    assert_eq!(set.blocks["400"].layovers.len(), 2);
    let cut = apply(&set, vec![NetworkEdit::RemoveBlockTrip { block: "400".into(), index: 2 }]);
    assert_eq!(cut.blocks["400"].layovers.len(), 1, "the layover after the new last trip goes with it");
    let cleared = apply(&net, vec![NetworkEdit::SetLayover { block: "400".into(), after_trip: 0, secs: None }]);
    assert!(cleared.blocks["400"].layovers.is_empty());
    assert!(refused(&net, NetworkEdit::SetLayover { block: "400".into(), after_trip: 1, secs: Some(60) }).contains("between two"));
    assert!(refused(
        &net,
        NetworkEdit::AddBlockTrip { block: "400".into(), trip: TripRef { schedule: "ghost".into(), start_secs: 0 } }
    )
    .contains("unknown schedule"));
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
    assert!(refused(&net, NetworkEdit::UpdatePattern { id: "10-inbound".into(), name: None, route: Some("ghost".into()) }).contains("unknown route"));
    assert!(refused(&net, NetworkEdit::SetScheduleRefs { id: "10-weekday".into(), pattern: Some("ghost".into()), service_type: None }).contains("unknown pattern"));
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
