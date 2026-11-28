//! Property tests of the network model. Each expected answer is recomputed
//! from the generated input, with no code shared with the implementation.
//! `trip_law_test.rs` checks the stop times that a stored trip materializes.
//!
//! * Round trips: serde and canonical serialization.
//! * Oracles: `materialize` against a different expansion, and `runs_on`
//!   against a brute-force calendar over `%u` weekdays.
//! * Structure: generated networks validate, and single mutations break
//!   validation.

use proptest::prelude::*;
use transit_date::{Date, Days};
use transit_network::{
    materialize, stop_detail, validation_issues, Block, CalendarException, CalendarRule, DateRange,
    DayState, Departs, HeadwayBand, Mode, Network, Pattern, PatternStop, PatternTrips, Route,
    ServiceType, StopOffset, TimingProfile, Trip, TripRef, WeekdaySet,
};

const YEAR_START: Date = Date::from_ymd_opt(2026, 1, 1).unwrap();

#[derive(Clone, Debug)]
struct Spec {
    n_stops: u32,    // 3..6
    n_patterns: u32, // 1..3
    n_services: u32, // 1..3
    headway: u32,    // 300..1800
    dayset: u32,     // preset index
    with_block: bool,
    with_exception: bool,
    /// A parent station pin that no pattern visits, with the first `bays`
    /// stops as its children: the shape of Fisher's Landing.
    station: bool,
    bays: u32, // 1..3
}

fn spec_strategy() -> impl Strategy<Value = Spec> {
    (
        3u32..6,
        1u32..3,
        1u32..3,
        300u32..1800,
        0u32..3,
        any::<bool>(),
        any::<bool>(),
        any::<bool>(),
        1u32..3,
    )
        .prop_map(|(a, b, c, d, e, g, h, station, bays)| Spec {
            n_stops: a,
            n_patterns: b,
            n_services: c,
            headway: d,
            dayset: e,
            with_block: g,
            with_exception: h,
            station,
            bays,
        })
}

/// A mid-range Sunday that weekday presets do not run, so `Added` exceptions
/// add service.
const EXCEPTION_DATE: Date = Date::from_ymd_opt(2026, 11, 1).unwrap();

fn dayset(index: u32) -> WeekdaySet {
    match index {
        0 => WeekdaySet::weekdays(),
        1 => WeekdaySet {
            mon: false,
            tue: false,
            wed: false,
            thu: false,
            fri: false,
            sat: true,
            sun: true,
        },
        _ => WeekdaySet {
            mon: true,
            tue: true,
            wed: true,
            thu: true,
            fri: true,
            sat: true,
            sun: true,
        },
    }
}

fn build_network(spec: &Spec) -> Network {
    let mut net = Network {
        name: "Generated".into(),
        ..Default::default()
    };
    for i in 0..spec.n_stops {
        net.stops.insert(
            format!("s{i}"),
            transit_network::Stop {
                name: format!("Stop {i}"),
                lat: 45.0 + i as f64 * 0.01,
                lon: -122.7 + i as f64 * 0.005,
                ..Default::default()
            },
        );
    }
    // The station pin is visited by no pattern. Its bays carry the trips.
    if spec.station {
        net.stops.insert(
            "STN".into(),
            transit_network::Stop {
                name: "Station".into(),
                lat: 45.0,
                lon: -122.7,
                ..Default::default()
            },
        );
        for i in 0..spec.bays.min(spec.n_stops) {
            net.stops
                .get_mut(&format!("s{i}"))
                .expect("built above")
                .parent = Some("STN".into());
        }
    }
    net.routes.insert(
        "r0".into(),
        Route {
            mode: Mode::Bus,
            ..Default::default()
        },
    );

    for j in 0..spec.n_patterns {
        let len = 2 + (j as usize % 2) + ((j + spec.n_patterns) as usize % 2); // 2..=4
        let len = len.min(spec.n_stops as usize);
        let mut stops = Vec::new();
        let mut cum = 0u32;
        for k in 0..len {
            let i = (j as usize + k) % spec.n_stops as usize;
            let timepoint = k == 0 || k == len - 1 || (j % 2 == 1 && k % 2 == 1);
            if k > 0 {
                cum += 300 + (k as u32 + j) * 120;
            }
            stops.push(PatternStop {
                stop: format!("s{i}"),
                timepoint,
                distance_m: cum as f64,
                ..Default::default()
            });
        }
        let tp_count = stops.iter().filter(|s| s.timepoint).count();
        let mut at_timepoints = Vec::new();
        let mut t = 0u32;
        for _ in 0..tp_count {
            at_timepoints.push(t);
            t += 480 + (j as u32 + 1) * 60;
        }
        net.patterns.insert(
            format!("p{j}"),
            Pattern {
                route: "r0".into(),
                name: format!("Pattern {j}"),
                stops,
                timing: vec![TimingProfile {
                    start_secs: 0,
                    end_secs: 30 * 3600,
                    at_timepoints,
                    extra: Default::default(),
                }],
                ..Default::default()
            },
        );
    }

    net.service_types.insert(
        "cal".into(),
        ServiceType {
            name: "cal".into(),
            rule: CalendarRule {
                days: dayset(spec.dayset),
                ranges: vec![DateRange {
                    start: YEAR_START,
                    end: YEAR_START + Days::new(364),
                    extra: Default::default(),
                }],
                exceptions: if spec.with_exception {
                    vec![CalendarException {
                        date: EXCEPTION_DATE,
                        state: DayState::Added,
                        extra: Default::default(),
                    }]
                } else {
                    vec![]
                },
                extra: Default::default(),
            },
            extra: Default::default(),
        },
    );

    for i in 0..spec.n_services {
        let key = format!("svc{i}");
        let pattern = format!("p{}", i % spec.n_patterns);
        let times = profile_times(&net.patterns[&pattern]);
        let trips = if i % 2 == 0 {
            vec![Trip {
                id: format!("{key}@every"),
                departs: Departs::Every {
                    bands: vec![HeadwayBand {
                        start_secs: 6 * 3600 + i * 600,
                        end_secs: 9 * 3600 + i * 600,
                        headway_secs: spec.headway,
                        extra: Default::default(),
                    }],
                    exact: true,
                },
                times,
                ..Default::default()
            }]
        } else {
            [7 * 3600 + i * 300, 8 * 3600 + i * 300, 9 * 3600 + i * 300]
                .into_iter()
                .map(|start_secs| Trip {
                    id: format!("{key}@{start_secs}"),
                    departs: Departs::At { start_secs },
                    times: times.clone(),
                    ..Default::default()
                })
                .collect()
        };
        net.pattern_trips.insert(
            key,
            PatternTrips {
                pattern,
                service_type: "cal".into(),
                trips,
                extra: Default::default(),
            },
        );
    }

    if spec.with_block {
        // GTFS: a block runs a trip whole. Only one vehicle that can run the
        // frequency trip's instances back to back carries it.
        let every = &net.pattern_trips["svc0"].trips[0];
        let run = every.times.last().and_then(|o| o.arrival_secs).unwrap();
        if spec.headway >= run {
            net.blocks.insert(
                "b0".into(),
                Block {
                    trips: vec![TripRef {
                        trip: every.id.clone(),
                        extra: Default::default(),
                    }],
                    layovers: vec![],
                    extra: Default::default(),
                },
            );
        }
    }
    net
}

fn profile_times(pattern: &Pattern) -> Vec<StopOffset> {
    let profile = &pattern.timing[0];
    let tps: Vec<usize> = pattern
        .stops
        .iter()
        .enumerate()
        .filter(|(_, s)| s.timepoint)
        .map(|(i, _)| i)
        .collect();
    let at = |k: usize| profile.at_timepoints[tps.iter().position(|&p| p == k).unwrap()];
    pattern
        .stops
        .iter()
        .enumerate()
        .map(|(i, ps)| {
            let t = if ps.timepoint {
                at(i)
            } else {
                let left = *tps.iter().filter(|&&p| p < i).last().unwrap();
                let right = *tps.iter().find(|&&p| p > i).unwrap();
                let (d0, d1) = (
                    pattern.stops[left].distance_m,
                    pattern.stops[right].distance_m,
                );
                let frac = if d1 > d0 {
                    (ps.distance_m - d0) / (d1 - d0)
                } else {
                    0.0
                };
                (at(left) as f64 + frac * (at(right) as f64 - at(left) as f64)).round() as u32
            };
            StopOffset {
                arrival_secs: Some(t),
                departure_secs: Some(t),
            }
        })
        .collect()
}

fn net_strategy() -> impl Strategy<Value = Network> {
    spec_strategy().prop_map(|spec| build_network(&spec))
}

/// Oracle calendar. Its mechanics differ from the model's on purpose.
fn oracle_runs_on(rule: &CalendarRule, date: Date) -> bool {
    let iso = date.weekday().number_from_monday().to_string();
    let flagged = match iso.as_str() {
        "1" => rule.days.mon,
        "2" => rule.days.tue,
        "3" => rule.days.wed,
        "4" => rule.days.thu,
        "5" => rule.days.fri,
        "6" => rule.days.sat,
        _ => rule.days.sun,
    };
    let in_range = rule.ranges.iter().any(|r| date >= r.start && date <= r.end);
    for e in &rule.exceptions {
        if e.date == date {
            return matches!(e.state, DayState::Added);
        }
    }
    flagged && in_range
}

fn oracle_instances(departs: &Departs) -> Vec<u32> {
    match departs {
        Departs::At { start_secs } => vec![*start_secs],
        Departs::Every { bands, .. } => bands
            .iter()
            .filter(|b| b.headway_secs > 0)
            .flat_map(|b| {
                let count = (b.end_secs - b.start_secs).div_ceil(b.headway_secs);
                (0..count).map(move |k| b.start_secs + k * b.headway_secs)
            })
            .collect(),
    }
}

fn oracle_day(net: &Network, date: Date) -> Vec<(String, u32)> {
    let mut out = Vec::new();
    for pt in net.pattern_trips.values() {
        if !oracle_runs_on(&net.service_types[&pt.service_type].rule, date) {
            continue;
        }
        for trip in &pt.trips {
            for start in oracle_instances(&trip.departs) {
                out.push((trip.id.clone(), start));
            }
        }
    }
    out.sort();
    out
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    #[test]
    fn generated_networks_validate(net in net_strategy()) {
        prop_assert!(net.validate().is_ok(), "issues: {:?}", validation_issues(&net));
    }

    #[test]
    fn route_lines_draw_what_focus_frames(net in net_strategy(), pick in proptest::collection::vec(any::<bool>(), 3)) {
        use std::collections::BTreeSet;
        let all = transit_network::route_lines(&net, None);
        prop_assert!(all.windows(2).all(|w| w[0].route_id < w[1].route_id), "id order");
        for (id, route) in &net.routes {
            let focus = transit_network::entity_focus(&net, transit_network::EntityKind::Route, id).unwrap();
            let framed: BTreeSet<String> = focus.lines.iter().map(|l| format!("{l:?}")).collect();
            let row = all.iter().find(|r| &r.route_id == id);
            if framed.is_empty() {
                prop_assert!(row.is_none(), "{} draws nothing", id);
                continue;
            }
            let row = row.unwrap();
            prop_assert_eq!(&row.name, &route.name);
            prop_assert_eq!(&row.color, &route.color);
            let drawn: Vec<String> = row.lines.iter().map(|l| format!("{l:?}")).collect();
            prop_assert_eq!(drawn.len(), framed.len(), "{} draws each line once", id);
            prop_assert_eq!(drawn.into_iter().collect::<BTreeSet<_>>(), framed);
        }

        let members: Vec<String> = net.service_types.keys().zip(&pick).filter(|(_, p)| **p).map(|(k, _)| k.clone()).collect();
        let narrowed = transit_network::route_lines(&net, Some(&members));
        for row in &narrowed {
            let expected: BTreeSet<String> = net
                .pattern_trips
                .values()
                .filter(|pt| members.contains(&pt.service_type) && !pt.trips.is_empty())
                .filter(|pt| net.patterns[&pt.pattern].route == row.route_id)
                .map(|pt| format!("{:?}", transit_network::pattern_alignment(&net, &pt.pattern)))
                .collect();
            prop_assert_eq!(row.lines.iter().map(|l| format!("{l:?}")).collect::<BTreeSet<_>>(), expected, "{} under {:?}", row.route_id, members);
        }
        for route in net.routes.keys() {
            let runs = net.pattern_trips.values().any(|pt| {
                members.contains(&pt.service_type) && !pt.trips.is_empty() && net.patterns[&pt.pattern].route == *route
            });
            prop_assert_eq!(narrowed.iter().any(|r| &r.route_id == route), runs, "{} drawn iff it runs", route);
        }
    }

    #[test]
    fn service_keys_are_the_authored_calendars_and_what_runs_on_them(net in net_strategy()) {
        let keys = transit_network::service_keys(&net);
        prop_assert_eq!(keys.iter().map(|k| k.id.clone()).collect::<Vec<_>>(), net.service_types.keys().cloned().collect::<Vec<_>>());
        for k in &keys {
            let st = &net.service_types[&k.id];
            prop_assert_eq!(&k.name, &st.name);
            prop_assert_eq!(&k.rule, &st.rule);
            let mut by_route: std::collections::BTreeMap<String, (Vec<String>, u32)> = Default::default();
            for (id, pt) in &net.pattern_trips {
                if pt.service_type != k.id {
                    continue;
                }
                let n = transit_network::materialize_service(&net, id).map(|t| t.len() as u32).unwrap_or(0);
                if n == 0 {
                    continue;
                }
                let e = by_route.entry(net.patterns[&pt.pattern].route.clone()).or_default();
                e.0.push(id.clone());
                e.1 += n;
            }
            let got: Vec<(String, Vec<String>, u32)> = k.routes.iter().map(|r| (r.route_id.clone(), r.pattern_trips.clone(), r.trips)).collect();
            let want: Vec<(String, Vec<String>, u32)> = by_route.into_iter().map(|(r, (p, n))| (r, p, n)).collect();
            prop_assert_eq!(got, want, "{} routes", k.id);
            prop_assert_eq!(k.trips, k.routes.iter().map(|r| r.trips).sum::<u32>());
        }
    }

    #[test]
    fn network_round_trips_through_json(net in net_strategy()) {
        let json = serde_json::to_string(&net).unwrap();
        let back: Network = serde_json::from_str(&json).unwrap();
        prop_assert_eq!(back, net);
    }

    #[test]
    fn serialization_is_deterministic(net in net_strategy()) {
        let a = serde_json::to_string(&net).unwrap();
        let b = serde_json::to_string(&net).unwrap();
        prop_assert_eq!(a, b);
    }

    /// Materialization runs exactly the stored trips of the services running
    /// that day, on weekdays, weekends, the exception date, and dates outside
    /// every range.
    #[test]
    fn materialize_runs_the_days_trips(net in net_strategy()) {
        for day in [1u32, 2, 5, 6, 100, 200, 304, 305, 400] {
            let date = YEAR_START + Days::new(day as i64);
            let mut got: Vec<(String, u32)> =
                materialize(&net, date).unwrap().into_iter().map(|t| (t.trip, t.start_secs)).collect();
            got.sort();
            prop_assert_eq!(got, oracle_day(&net, date), "the trips of {:?}", date);
        }
    }

    #[test]
    fn calendar_runs_on_matches_brute_force(net in net_strategy()) {
        let rule = &net.service_types["cal"];
        for day in 0..365 {
            let date = YEAR_START + Days::new(day);
            prop_assert_eq!(rule.rule.runs_on(date), oracle_runs_on(&rule.rule, date));
        }
    }

    #[test]
    fn single_mutations_break_validation(net in net_strategy()) {
        let mut broken = net.clone();
        if let Some((_, pattern)) = broken.patterns.iter_mut().next() {
            pattern.stops[0].stop = "ghost-stop".into();
        }
        prop_assert!(!validation_issues(&broken).is_empty());

        let mut broken = net.clone();
        if let Some((_, pattern_trips)) = broken.pattern_trips.iter_mut().next() {
            pattern_trips.service_type = "ghost-service-type".into();
        }
        prop_assert!(!validation_issues(&broken).is_empty());

        let mut broken = net.clone();
        if let Some((_, pattern)) = broken.patterns.iter_mut().next() {
            pattern.timing[0].at_timepoints[0] = 1;
        }
        prop_assert!(!validation_issues(&broken).is_empty());

        let mut broken = net.clone();
        if let Some((_, pattern_trips)) = broken.pattern_trips.iter_mut().next() {
            let trip = &mut pattern_trips.trips[0];
            match &mut trip.departs {
                Departs::Every { bands, .. } => bands[0].headway_secs = 0,
                Departs::At { .. } => { trip.times.pop(); } // a stop left untimed
            }
        }
        prop_assert!(!validation_issues(&broken).is_empty());

        let mut broken = net.clone();
        if let Some((_, block)) = broken.blocks.iter_mut().next() {
            block.trips[0].trip = "ghost-trip".into();
            prop_assert!(!validation_issues(&broken).is_empty());
        }
    }

    #[test]
    fn materialize_never_panics_on_broken_documents(net in net_strategy()) {
        let mut broken = net.clone();
        if let Some((_, pattern)) = broken.patterns.iter_mut().next() {
            pattern.timing.clear();
        }
        let date = YEAR_START + Days::new(3);
        let _ = materialize(&broken, date);

        let mut broken = net.clone();
        if let Some((_, pattern_trips)) = broken.pattern_trips.iter_mut().next() {
            pattern_trips.pattern = "ghost-pattern".into();
        }
        let _ = materialize(&broken, date);
    }

    /// A stop with service of its own never pools. A stop that no pattern
    /// visits answers for its whole walking group, which this test builds
    /// from `parent` fields instead of the model's helper.
    #[test]
    fn stop_detail_answers_its_own_routes_or_its_walking_group(net in net_strategy()) {
        for id in net.stops.keys() {
            let stop = &net.stops[id];
            let mut group: std::collections::BTreeSet<&str> =
                [id.as_str()].into_iter().collect();
            if let Some(parent) = stop.parent.as_deref() {
                group.insert(parent);
            }
            for (other, s) in &net.stops {
                if s.parent.as_deref() == Some(id.as_str()) {
                    group.insert(other.as_str());
                }
                if stop.parent.is_some() && s.parent == stop.parent {
                    group.insert(other.as_str());
                }
            }
            let serving = |members: &std::collections::BTreeSet<&str>| {
                let mut routes: std::collections::BTreeSet<String> = Default::default();
                for p in net.patterns.values() {
                    if p.stops.iter().any(|ps| members.contains(ps.stop.as_str())) {
                        routes.insert(p.route.clone());
                    }
                }
                routes
            };
            let own = serving(&[id.as_str()].into_iter().collect());
            let expected = if own.is_empty() { serving(&group) } else { own };
            let detail = stop_detail(&net, id, None).expect("a known stop has a detail");
            let got: std::collections::BTreeSet<String> =
                detail.routes.iter().map(|r| r.id.clone()).collect();
            prop_assert_eq!(
                got,
                expected,
                "stop {} does not answer its own routes or its walking group",
                id
            );
        }
    }

    /// A stop's detail has its own record and every pattern position that
    /// visits it, never its walking group's positions. Positions come from
    /// scanning every pattern, and trips from the oracle's expansion.
    #[test]
    fn stop_detail_lists_every_visit_of_the_stop_itself(net in net_strategy()) {
        let all: Vec<String> = net.service_types.keys().cloned().collect();
        for id in net.stops.keys() {
            for members in [None, Some(all.as_slice()), Some(&[][..])] {
                let detail = stop_detail(&net, id, members).expect("a known stop has a detail");
                prop_assert_eq!(&detail.stop, &net.stops[id], "the card carries the stop's record");
                let mut expected: Vec<(String, String, u32, bool, u32)> = Vec::new();
                for (pid, p) in &net.patterns {
                    for (i, ps) in p.stops.iter().enumerate() {
                        if &ps.stop != id {
                            continue;
                        }
                        let trips: u32 = net
                            .pattern_trips
                            .values()
                            .filter(|s| &s.pattern == pid)
                            .filter(|s| members.is_none_or(|m| m.contains(&s.service_type)))
                            .flat_map(|s| s.trips.iter())
                            .map(|t| oracle_instances(&t.departs).len() as u32)
                            .sum();
                        let endpoint = i == 0 || i + 1 == p.stops.len();
                        expected.push((p.route.clone(), pid.clone(), i as u32, endpoint, trips));
                    }
                }
                expected.sort();
                let got: Vec<(String, String, u32, bool, u32)> = detail
                    .visits
                    .iter()
                    .map(|v| (v.route.clone(), v.pattern.clone(), v.index, v.endpoint, v.trips))
                    .collect();
                prop_assert_eq!(got, expected, "stop {} under {:?}", id, members);
                for v in &detail.visits {
                    let p = &net.patterns[&v.pattern];
                    prop_assert_eq!(&v.direction, &p.direction);
                    prop_assert_eq!(v.timepoint, p.stops[v.index as usize].timepoint);
                }
            }
        }
    }
}
