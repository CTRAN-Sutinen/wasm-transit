//! The new-route laws: a planner builds a route that does not
//! exist yet — the route, its directions, a pattern each way laid
//! through stops they pick in order, and trips at a headway under a
//! service key — as one change, over the committed planner network
//! (testdata/network.json: stops A/B/C, route 10, service type
//! `weekday`).
//!
//! `add_pattern_through {route, direction?, stops, run_secs, id?, name?}`
//! lays a pattern through `stops` in order:
//!
//! 1. its visits are exactly the stops given, in that order; the first
//!    and last are timepoints, the rest are not (scheduled between them
//!    by distance until the planner makes one a timepoint);
//! 2. each visit's distance is the great-circle length of the legs
//!    before it — 0 at the first, never decreasing, and estimated (never
//!    claimed as published);
//! 3. it carries one all-day timing profile, `run_secs` end to end, so
//!    trips set on it at a headway run `run_secs` first to last stop;
//! 4. it runs in `direction` (one of the route's), on `route`, under
//!    `name` (else "<route name> → <last stop's name>"), with no
//!    alignment of its own (drawn stop to stop), its id minted `pat<n>`
//!    unless given, and reported in `created`;
//! 5. what cannot be laid is a named error and the document is
//!    untouched: fewer than two stops, a stop the network does not hold,
//!    the same stop twice in a row, a route that does not exist, a
//!    direction the route does not have, a zero run time, a taken id.
//!
//! And the whole route as one change: `add_route`, `upsert_route`,
//! `set_direction` each way, `add_pattern_through` each way, and
//! `set_headway` each way under a service key validates with no issue,
//! and its trips are exactly the headway's slots.

use proptest::prelude::*;
use transit_network::{apply_edits, apply_edits_reporting, validation_issues, Mode, Network, NetworkEdit, Route, Stop};

fn network() -> Network {
    serde_json::from_str(include_str!("../../../testdata/network.json")).expect("testdata network parses")
}

fn haversine_m(a: &Stop, b: &Stop) -> f64 {
    let (p1, p2) = (a.lat.to_radians(), b.lat.to_radians());
    let (dp, dl) = ((b.lat - a.lat).to_radians(), (b.lon - a.lon).to_radians());
    let h = (dp / 2.0).sin().powi(2) + p1.cos() * p2.cos() * (dl / 2.0).sin().powi(2);
    2.0 * 6_371_000.0 * h.sqrt().asin()
}

fn route(name: &str, long_name: &str) -> Route {
    Route {
        name: name.into(),
        long_name: Some(long_name.into()),
        mode: Mode::Bus,
        ..Default::default()
    }
}

/// The new route 20 through `stops` each way, at `headway` from 07:00
/// until before 09:00 under `weekday`, `run_secs` end to end.
fn new_route(stops: &[&str], run_secs: u32, headway: u32) -> Vec<NetworkEdit> {
    let there: Vec<String> = stops.iter().map(|s| s.to_string()).collect();
    let back: Vec<String> = there.iter().rev().cloned().collect();
    let mut edits = vec![
        NetworkEdit::AddRoute { id: Some("20".into()) },
        NetworkEdit::UpsertRoute { id: "20".into(), route: route("20", "Crosstown") },
        NetworkEdit::SetDirection { route: "20".into(), id: "0".into(), name: "Outbound".into() },
        NetworkEdit::SetDirection { route: "20".into(), id: "1".into(), name: "Inbound".into() },
    ];
    for (dir, seq) in [("0", there), ("1", back)] {
        edits.push(NetworkEdit::AddPatternThrough {
            id: Some(format!("20-{dir}")),
            route: "20".into(),
            direction: Some(dir.into()),
            name: None,
            stops: seq,
            run_secs,
        });
        edits.push(NetworkEdit::SetHeadway {
            route: "20".into(),
            direction: Some(dir.into()),
            service_type: "weekday".into(),
            from_secs: 7 * 3600,
            to_secs: 9 * 3600,
            headway_secs: headway,
            pattern: Some(format!("20-{dir}")),
        });
    }
    edits
}

#[test]
fn a_pattern_is_laid_through_the_stops_given() {
    let net = network();
    let applied = apply_edits_reporting(&net, &new_route(&["A", "C", "B"], 1500, 1800)).expect("the new route applies");
    let next = applied.network;
    let p = &next.patterns["20-0"];
    // 1. the visits, in order; first and last timepoints only
    assert_eq!(p.stops.iter().map(|s| s.stop.as_str()).collect::<Vec<_>>(), vec!["A", "C", "B"]);
    assert_eq!(p.stops.iter().map(|s| s.timepoint).collect::<Vec<_>>(), vec![true, false, true]);
    // 2. distances: the legs' great-circle lengths, estimated
    let legs = [0.0, haversine_m(&net.stops["A"], &net.stops["C"]), haversine_m(&net.stops["C"], &net.stops["B"])];
    let mut along = 0.0;
    for (visit, leg) in p.stops.iter().zip(legs) {
        along += leg;
        assert!((visit.distance_m - along).abs() < 0.5, "{} at {} m, not {along}", visit.stop, visit.distance_m);
        assert!(!visit.distance_published);
    }
    // 3. one all-day profile, run_secs end to end
    assert_eq!(p.timing.len(), 1);
    assert_eq!(p.timing[0].at_timepoints, vec![0, 1500]);
    assert!(p.timing[0].start_secs == 0 && p.timing[0].end_secs >= 24 * 3600);
    // 4. its route, direction, default name, no alignment; reported
    assert_eq!((p.route.as_str(), p.direction.as_deref(), p.alignment.as_deref()), ("20", Some("0"), None));
    assert_eq!(p.name, format!("20 → {}", net.stops["B"].name));
    assert!(applied.created.iter().any(|c| c.kind == "pattern" && c.id == "20-0"));
    assert_eq!(next.patterns["20-1"].stops.iter().map(|s| s.stop.as_str()).collect::<Vec<_>>(), vec!["B", "C", "A"]);
}

#[test]
fn the_new_route_validates_and_runs_its_headway() {
    let next = apply_edits(&network(), &new_route(&["A", "B", "C"], 1200, 1800)).expect("the new route applies");
    assert!(validation_issues(&next).is_empty(), "{:?}", validation_issues(&next));
    assert_eq!(next.routes["20"].long_name.as_deref(), Some("Crosstown"));
    for dir in ["0", "1"] {
        let trips: Vec<_> = next.trips().filter(|(_, pt, _)| pt.pattern == format!("20-{dir}")).collect();
        let mut starts: Vec<u32> = trips.iter().filter_map(|(_, _, t)| t.departs.first_start()).collect();
        starts.sort_unstable();
        assert_eq!(starts, vec![25200, 27000, 28800, 30600], "direction {dir}: every 30 min from 07:00 until before 09:00");
        for (_, pt, t) in &trips {
            assert_eq!(pt.service_type, "weekday");
            assert_eq!(t.times.first().and_then(|o| o.departure_secs), Some(0));
            assert_eq!(t.times.last().and_then(|o| o.arrival_secs), Some(1200), "each trip runs run_secs end to end");
        }
    }
}

#[test]
fn what_cannot_be_laid_is_named_and_changes_nothing() {
    let net = network();
    let lay = |stops: &[&str], route: &str, direction: Option<&str>, run_secs: u32, id: Option<&str>| NetworkEdit::AddPatternThrough {
        id: id.map(str::to_string),
        route: route.into(),
        direction: direction.map(str::to_string),
        name: None,
        stops: stops.iter().map(|s| s.to_string()).collect(),
        run_secs,
    };
    let taken = net.patterns.keys().next().unwrap().clone();
    let cases: Vec<(NetworkEdit, &str)> = vec![
        (lay(&["A"], "10", None, 600, None), "two stops"),
        (lay(&[], "10", None, 600, None), "two stops"),
        (lay(&["A", "Z"], "10", None, 600, None), "Z"),
        (lay(&["A", "A", "B"], "10", None, 600, None), "twice in a row"),
        (lay(&["A", "B"], "99", None, 600, None), "99"),
        (lay(&["A", "B"], "10", Some("nowhere"), 600, None), "nowhere"),
        (lay(&["A", "B"], "10", None, 0, None), "run time"),
        (lay(&["A", "B"], "10", None, 600, Some(&taken)), &taken),
    ];
    for (edit, says) in cases {
        let reason = apply_edits(&net, &[edit.clone()]).expect_err("refused").reason;
        assert!(reason.contains(says), "{edit:?} refused as {reason:?}, not naming {says:?}");
    }
}

proptest! {
    /// Any stops, in any order (no stop twice in a row): the pattern
    /// visits them in order, its distances never decrease from 0, its
    /// timing is aligned, and its trips at a headway validate.
    #[test]
    fn any_stops_lay_an_aligned_pattern(
        points in prop::collection::vec((45.0f64..46.0, -123.0f64..-122.0), 2..8),
        run_secs in 60u32..7200,
        headway in 300u32..3600,
    ) {
        let mut net = network();
        let ids: Vec<String> = (0..points.len()).map(|i| format!("p{i}")).collect();
        for (id, (lat, lon)) in ids.iter().zip(&points) {
            net.stops.insert(id.clone(), Stop { name: id.clone(), lat: *lat, lon: *lon, ..Default::default() });
        }
        let refs: Vec<&str> = ids.iter().map(String::as_str).collect();
        let next = apply_edits(&net, &new_route(&refs, run_secs, headway)).expect("applies");
        let p = &next.patterns["20-0"];
        prop_assert_eq!(p.stops.iter().map(|s| s.stop.clone()).collect::<Vec<_>>(), ids.clone());
        prop_assert_eq!(p.stops[0].distance_m, 0.0);
        prop_assert!(p.stops.windows(2).all(|w| w[1].distance_m >= w[0].distance_m));
        let timepoints = p.stops.iter().filter(|s| s.timepoint).count();
        prop_assert!(p.timing.iter().all(|t| t.at_timepoints.len() == timepoints));
        prop_assert!(next.trips().filter(|(_, pt, _)| pt.pattern == "20-0").all(|(_, _, t)| t.times.len() == p.stops.len()));
        prop_assert!(validation_issues(&next).is_empty(), "{:?}", validation_issues(&next));
    }
}
