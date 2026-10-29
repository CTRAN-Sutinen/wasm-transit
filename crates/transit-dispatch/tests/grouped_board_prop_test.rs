//! The grouped board's laws — what a stop's display shows: the next
//! departures of each route to each destination.
//!
//! The intent (the user's directive for the stop display, 2026-10-29):
//!
//! 1. one group per route AND destination ("4 → Downtown" and
//!    "4 → Fisher's Landing" are two groups);
//! 2. each group holds its next `per_group` departures (three on the
//!    display), soonest-leaving first, by the same realtime-first rule
//!    as THE board — so the groups together, uncut, ARE the board;
//! 3. a route and destination the stop is served by today stays on
//!    the board after its last departure, with none left — the display
//!    says it has stopped, never that it does not exist;
//! 4. groups list in the route's display order (GTFS
//!    `route_sort_order`, lower first; routes without one after, by id
//!    — D-23), then by destination;
//! 5. every board row says how long until it leaves: due under a
//!    minute, whole minutes (rounded down — D-24) under an hour, then
//!    the clock time it leaves at; live when predicted, else the
//!    schedule.
//!
//! The oracle recomputes each answer from the generated schedule
//! (start + offset + the live delay), sharing nothing with the board.

use std::collections::{BTreeMap, BTreeSet};

use chrono::NaiveDate;
use proptest::prelude::*;
use transit_dispatch::{grouped_board, stop_board, BoardRow, Wait};
use transit_live::{LiveDay, LiveStatus, LiveStop, LiveTrip, OnTimeWindow};
use transit_network::{
    CalendarRule, DateRange, Departs, Network, Pattern, PatternStop, PatternTrips, Route, ServiceType, Stop,
    StopOffset, TimingProfile, Trip, WeekdaySet,
};

const DATE: &str = "2026-10-23"; // a Friday
const TERMINALS: usize = 3;

fn date() -> NaiveDate {
    NaiveDate::parse_from_str(DATE, "%Y-%m-%d").unwrap()
}

/// How a pattern meets the board's stop X.
#[derive(Debug, Clone, Copy)]
enum Shape {
    /// A → X → terminal: departs X.
    Through(usize),
    /// X → terminal: starts at X.
    Starts(usize),
    /// A → X: ends at X — an arrival, never a departure.
    Ends,
}

#[derive(Debug, Clone)]
struct GenPattern {
    shape: Shape,
    starts: Vec<u32>,
    /// Per start: the live delay at X, if predicted.
    delays: Vec<Option<i32>>,
}

#[derive(Debug, Clone)]
struct GenRoute {
    sort_order: Option<u32>,
    patterns: Vec<GenPattern>,
}

fn terminal(k: usize) -> String {
    format!("T{k}")
}

fn terminal_name(k: usize) -> String {
    format!("Terminal {k}")
}

fn shape() -> impl Strategy<Value = Shape> {
    prop_oneof![
        (0..TERMINALS).prop_map(Shape::Through),
        (0..TERMINALS).prop_map(Shape::Starts),
        Just(Shape::Ends),
    ]
}

fn gen_pattern() -> impl Strategy<Value = GenPattern> {
    (shape(), prop::collection::btree_set(18_000u32..90_000, 1..6))
        .prop_flat_map(|(shape, starts)| {
            let starts: Vec<u32> = starts.into_iter().collect();
            let n = starts.len();
            (Just(shape), Just(starts), prop::collection::vec(prop::option::of(-300i32..1_800), n))
        })
        .prop_map(|(shape, starts, delays)| GenPattern { shape, starts, delays })
}

fn gen_routes() -> impl Strategy<Value = Vec<GenRoute>> {
    prop::collection::vec(
        (prop::option::of(0u32..4), prop::collection::vec(gen_pattern(), 1..4))
            .prop_map(|(sort_order, patterns)| GenRoute { sort_order, patterns }),
        1..5,
    )
}

fn route_id(r: usize) -> String {
    format!("r{r}")
}

fn trip_id(r: usize, p: usize, start: u32) -> String {
    format!("r{r}p{p}@{start}")
}

/// The pattern's stops and the offset of X along it.
fn stops_of(shape: Shape) -> (Vec<String>, Option<u32>) {
    match shape {
        Shape::Through(k) => (vec!["A".into(), "X".into(), terminal(k)], Some(600)),
        Shape::Starts(k) => (vec!["X".into(), terminal(k)], Some(0)),
        Shape::Ends => (vec!["A".into(), "X".into()], None),
    }
}

fn build(routes: &[GenRoute]) -> Network {
    let plain = |name: &str| Stop { name: name.into(), lat: 45.5, lon: -122.5, ..Default::default() };
    let mut stops = BTreeMap::new();
    stops.insert("A".to_string(), plain("Origin"));
    stops.insert("X".to_string(), plain("The Stop"));
    for k in 0..TERMINALS {
        stops.insert(terminal(k), plain(&terminal_name(k)));
    }
    let mut net = Network { name: "Grouped board laws".into(), stops, ..Default::default() };
    net.service_types.insert(
        "ALL".into(),
        ServiceType {
            name: "Every day".into(),
            rule: CalendarRule {
                days: WeekdaySet::weekdays(),
                ranges: vec![DateRange {
                    start: NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
                    end: NaiveDate::from_ymd_opt(2026, 12, 31).unwrap(),
                }],
                exceptions: vec![],
            },
        },
    );
    for (r, route) in routes.iter().enumerate() {
        net.routes.insert(
            route_id(r),
            Route { name: format!("{r}"), sort_order: route.sort_order, ..Default::default() },
        );
        for (p, gp) in route.patterns.iter().enumerate() {
            let (ids, _) = stops_of(gp.shape);
            let offsets: Vec<u32> = (0..ids.len() as u32).map(|i| i * 600).collect();
            let pid = format!("r{r}p{p}");
            net.patterns.insert(
                pid.clone(),
                Pattern {
                    direction: None,
                    route: route_id(r),
                    name: pid.clone(),
                    alignment: None,
                    stops: ids
                        .iter()
                        .enumerate()
                        .map(|(i, s)| PatternStop { stop: s.clone(), timepoint: true, distance_m: i as f64 * 1000.0, ..Default::default() })
                        .collect(),
                    timing: vec![TimingProfile { start_secs: 0, end_secs: 172_800, at_timepoints: offsets.clone() }],
                },
            );
            net.pattern_trips.insert(
                pid.clone(),
                PatternTrips {
                    pattern: pid.clone(),
                    service_type: "ALL".into(),
                    trips: gp
                        .starts
                        .iter()
                        .map(|&s| Trip {
                            id: trip_id(r, p, s),
                            departs: Departs::At { start_secs: s },
                            times: offsets
                                .iter()
                                .map(|&t| StopOffset { arrival_secs: Some(t), departure_secs: Some(t) })
                                .collect(),
                            ..Default::default()
                        })
                        .collect(),
                },
            );
        }
    }
    net
}

/// The live document: each predicted visit to X, at its delay.
fn live(routes: &[GenRoute]) -> LiveDay {
    let mut trips = Vec::new();
    for (r, route) in routes.iter().enumerate() {
        for (p, gp) in route.patterns.iter().enumerate() {
            let Some(at_x) = stops_of(gp.shape).1 else { continue };
            for (&s, &delay) in gp.starts.iter().zip(&gp.delays) {
                let Some(delay) = delay else { continue };
                let scheduled = s + at_x;
                trips.push(LiveTrip {
                    trip: trip_id(r, p, s),
                    start_secs: s,
                    route: None,
                    route_name: None,
                    worst_delay_secs: Some(delay),
                    status: LiveStatus::Late,
                    stops: vec![LiveStop {
                        stop: "X".into(),
                        scheduled_departure: Some(scheduled),
                        arrival_delay: Some(delay),
                        departure_delay: Some(delay),
                        predicted_departure: Some((scheduled as i32 + delay) as u32),
                    }],
                });
            }
        }
    }
    LiveDay {
        window: OnTimeWindow::default(),
        trips,
        vehicles: vec![],
        unmatched_trips: 0,
        alerts: vec![],
        unmatched_alerts: 0,
    }
}

/// One departure from X, recomputed from the generated schedule.
#[derive(Debug, Clone, PartialEq)]
struct Expected {
    route: String,
    destination: String,
    trip: String,
    start: u32,
    /// When it leaves: the prediction when live, else the schedule.
    at: u32,
    expected: Option<u32>,
}

fn departures(routes: &[GenRoute], with_live: bool) -> Vec<Expected> {
    let mut out = Vec::new();
    for (r, route) in routes.iter().enumerate() {
        for (p, gp) in route.patterns.iter().enumerate() {
            let k = match gp.shape {
                Shape::Through(k) | Shape::Starts(k) => k,
                Shape::Ends => continue,
            };
            let at_x = stops_of(gp.shape).1.unwrap();
            for (&s, &delay) in gp.starts.iter().zip(&gp.delays) {
                let expected = delay.filter(|_| with_live).map(|d| (s as i32 + at_x as i32 + d) as u32);
                out.push(Expected {
                    route: route_id(r),
                    destination: terminal_name(k),
                    trip: trip_id(r, p, s),
                    start: s,
                    at: expected.unwrap_or(s + at_x),
                    expected,
                });
            }
        }
    }
    out
}

/// The display order: sort order (none last), route id, destination.
fn order_key(routes: &[GenRoute], route: &str, destination: &str) -> (bool, u32, String, String) {
    let r: usize = route[1..].parse().unwrap();
    let so = routes[r].sort_order;
    (so.is_none(), so.unwrap_or(0), route.to_string(), destination.to_string())
}

fn wait_for(at: u32, from: u32) -> Wait {
    let d = at - from;
    if d < 60 {
        Wait::Due
    } else if d < 3_600 {
        Wait::Minutes { minutes: d / 60 }
    } else {
        Wait::Clock { secs: at }
    }
}

fn case() -> impl Strategy<Value = (Vec<GenRoute>, u32, usize, bool)> {
    (gen_routes(), 0u32..95_000, 1usize..5, any::<bool>())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// Laws 1 and 3: the groups are exactly the (route, destination)
    /// pairs that leave the stop at any time of the day — however late
    /// it is asked — and none other (a pattern ending here makes none).
    #[test]
    fn the_groups_are_the_days_routes_and_destinations((routes, from, per_group, with_live) in case()) {
        let net = build(&routes);
        let day = transit_network::materialize(&net, date()).unwrap();
        let lv = live(&routes);
        let board = grouped_board(&net, &day, "X", from, per_group, with_live.then_some(&lv), None);
        let got: BTreeSet<(String, String)> = board.groups.iter().map(|g| (g.route.clone(), g.destination.clone())).collect();
        let want: BTreeSet<(String, String)> = departures(&routes, with_live).into_iter().map(|e| (e.route, e.destination)).collect();
        prop_assert_eq!(got.len(), board.groups.len(), "one group per route and destination");
        prop_assert_eq!(got, want);
        prop_assert!(!board.pooled);
        prop_assert_eq!(board.live, with_live);
    }

    /// Law 2: each group holds its next `per_group` departures, by when
    /// the bus actually leaves (a late bus not yet gone is still next;
    /// an early one already gone is not), soonest first.
    #[test]
    fn each_group_holds_its_next_departures((routes, from, per_group, with_live) in case()) {
        let net = build(&routes);
        let day = transit_network::materialize(&net, date()).unwrap();
        let lv = live(&routes);
        let board = grouped_board(&net, &day, "X", from, per_group, with_live.then_some(&lv), None);
        let all = departures(&routes, with_live);
        for g in &board.groups {
            let mut want: Vec<&Expected> = all
                .iter()
                .filter(|e| e.route == g.route && e.destination == g.destination && e.at >= from)
                .collect();
            want.sort_by(|a, b| (a.at, &a.trip, a.start).cmp(&(b.at, &b.trip, b.start)));
            want.truncate(per_group);
            let got: Vec<(String, u32, Option<u32>)> =
                g.departures.iter().map(|r| (r.departure.trip.clone(), r.departure.start_secs, r.expected_secs)).collect();
            let want: Vec<(String, u32, Option<u32>)> = want.iter().map(|e| (e.trip.clone(), e.start, e.expected)).collect();
            prop_assert_eq!(got, want, "group {} → {}", &g.route, &g.destination);
            prop_assert!(g.departures.iter().all(|r| r.departure.route == g.route && r.departure.destination == g.destination));
        }
    }

    /// Law 2, the other way round: uncut, the groups together are THE
    /// board — the same rows, nothing added, nothing lost.
    #[test]
    fn uncut_the_groups_are_the_board((routes, from, _per_group, with_live) in case()) {
        let net = build(&routes);
        let day = transit_network::materialize(&net, date()).unwrap();
        let lv = live(&routes);
        let live = with_live.then_some(&lv);
        let grouped = grouped_board(&net, &day, "X", from, usize::MAX, live, None);
        let mut flat: Vec<BoardRow> = grouped.groups.into_iter().flat_map(|g| g.departures).collect();
        let at = |r: &BoardRow| r.expected_secs.unwrap_or(r.departure.departure_secs);
        flat.sort_by(|a, b| (at(a), &a.departure.trip, a.departure.start_secs).cmp(&(at(b), &b.departure.trip, b.departure.start_secs)));
        prop_assert_eq!(flat, stop_board(&net, &day, "X", from, usize::MAX, live, None).rows);
    }

    /// Law 4: the groups list in the route's display order, then by
    /// destination.
    #[test]
    fn the_groups_list_in_display_order((routes, from, per_group, with_live) in case()) {
        let net = build(&routes);
        let day = transit_network::materialize(&net, date()).unwrap();
        let lv = live(&routes);
        let board = grouped_board(&net, &day, "X", from, per_group, with_live.then_some(&lv), None);
        let keys: Vec<_> = board.groups.iter().map(|g| order_key(&routes, &g.route, &g.destination)).collect();
        let mut sorted = keys.clone();
        sorted.sort();
        prop_assert_eq!(keys, sorted);
    }

    /// Law 5: every row — on the grouped board and on THE board — says
    /// how long until it leaves, from when it actually leaves.
    #[test]
    fn every_row_says_how_long_until_it_leaves((routes, from, per_group, with_live) in case()) {
        let net = build(&routes);
        let day = transit_network::materialize(&net, date()).unwrap();
        let lv = live(&routes);
        let live = with_live.then_some(&lv);
        let grouped = grouped_board(&net, &day, "X", from, per_group, live, None);
        let plain = stop_board(&net, &day, "X", from, usize::MAX, live, None);
        for r in grouped.groups.iter().flat_map(|g| &g.departures).chain(&plain.rows) {
            let e = departures(&routes, with_live)
                .into_iter()
                .find(|e| e.trip == r.departure.trip && e.start == r.departure.start_secs)
                .expect("every row is a generated departure");
            prop_assert_eq!(&r.wait, &wait_for(e.at, from));
        }
    }
}

/// The edges of the wait, pinned: 59 s is due, 60 s one minute,
/// 3 599 s fifty-nine minutes, 3 600 s the clock.
#[test]
fn the_wait_turns_at_a_minute_and_an_hour() {
    let routes = vec![GenRoute {
        sort_order: None,
        patterns: vec![GenPattern { shape: Shape::Starts(0), starts: vec![36_000], delays: vec![None] }],
    }];
    let net = build(&routes);
    let day = transit_network::materialize(&net, date()).unwrap();
    let wait = |from: u32| grouped_board(&net, &day, "X", from, 3, None, None).groups[0].departures[0].wait.clone();
    assert_eq!(wait(36_000), Wait::Due);
    assert_eq!(wait(36_000 - 59), Wait::Due);
    assert_eq!(wait(36_000 - 60), Wait::Minutes { minutes: 1 });
    assert_eq!(wait(36_000 - 119), Wait::Minutes { minutes: 1 });
    assert_eq!(wait(36_000 - 3_599), Wait::Minutes { minutes: 59 });
    assert_eq!(wait(36_000 - 3_600), Wait::Clock { secs: 36_000 });
    // After its last departure the group stays, with nothing left.
    let late = grouped_board(&net, &day, "X", 36_001, 3, None, None);
    assert_eq!(late.groups.len(), 1);
    assert!(late.groups[0].departures.is_empty());
}
