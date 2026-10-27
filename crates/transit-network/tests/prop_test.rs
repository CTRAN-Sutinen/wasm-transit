//! Property-based tests (proptest) for the network model.
//!
//! Same discipline as `transit-core/tests/prop_test.rs`: laws that
//! must hold for *every* network, with the expected answer recomputed
//! **independently** from the generated input (no shared code with
//! the implementation). Three flavors:
//!
//! * round-trip laws (serde, canonical serialization);
//! * oracle properties — `materialize` is recomputed by a
//!   deliberately different expansion (per-stop scan instead of the
//!   sequential walk, count-derived band expansion, brute-force
//!   calendar over `%u` weekdays);
//! * structural invariants — generated networks validate clean,
//!   single mutations always break validation.

use chrono::{Duration, NaiveDate};
use proptest::prelude::*;
use transit_network::{
    materialize, trip_arrival_end_secs, validation_issues, Anchor, Block, CalendarException,
    CalendarRule, DateRange, DayState, HeadwayBand, Layover, Mode, Network, Pattern, PatternStop,
    Route, Departures, PatternTrips, ServiceType, TimingProfile, TripRef, WeekdaySet,
};

const YEAR_START: NaiveDate = NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();

// ---------------------------------------------------------------------
// Valid-network generator
// ---------------------------------------------------------------------

/// Parameters the generator derives a network from. Small ranges keep
/// proptest's shrinking meaningful.
#[derive(Clone, Debug)]
struct Spec {
    n_stops: u32,       // 3..6
    n_patterns: u32,    // 1..3
    n_services: u32,    // 1..3
    headway: u32,       // 300..1800
    dayset: u32,        // preset index
    layover: u32,       // 0..600
    with_block: bool,
    with_exception: bool,
}

fn spec_strategy() -> impl Strategy<Value = Spec> {
    (
        3u32..6,
        1u32..3,
        1u32..3,
        300u32..1800,
        0u32..3,
        0u32..600,
        any::<bool>(),
        any::<bool>(),
    )
        .prop_map(|(a, b, c, d, e, f, g, h)| Spec {
            n_stops: a,
            n_patterns: b,
            n_services: c,
            headway: d,
            dayset: e,
            layover: f,
            with_block: g,
            with_exception: h,
        })
}

/// Day the exception date sits on — a mid-range Sunday that weekday
/// presets don't run, so `Added` exceptions genuinely add service.
const EXCEPTION_DATE: NaiveDate = NaiveDate::from_ymd_opt(2026, 11, 1).unwrap();

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

/// Build a valid network from the spec. Valid **by construction**:
/// every invariant is satisfied here, so `validate` must accept it.
fn build_network(spec: &Spec) -> Network {
    let mut net = Network {
        name: "Generated".into(),
        plan: Default::default(),
        anchor: Anchor::NoonMinusTwelve,
        stops: Default::default(),
        routes: Default::default(),
        patterns: Default::default(),
        service_types: Default::default(),
        pattern_trips: Default::default(),
        blocks: Default::default(),
    };
    for i in 0..spec.n_stops {
        net.stops.insert(
            format!("s{i}"),
            transit_network::Stop {
                name: format!("Stop {i}"),
                lat: 45.0 + i as f64 * 0.01,
                lon: -122.7 + i as f64 * 0.005,
                parent: None,
                transfer: false,
            },
        );
    }
    net.routes.insert(
        "r0".into(),
        Route {
            directions: Default::default(),
            name: "Route 0".into(),
            mode: Mode::Bus,
            color: None,
        },
    );

    // Patterns: distinct stop sequences, first/last timepoints,
    // one timing profile covering the whole service day (0..30h).
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
            });
        }
        let tp_count = stops.iter().filter(|s| s.timepoint).count();
        // Cumulative times at timepoints: strictly increasing.
        let mut at_timepoints = Vec::new();
        let mut t = 0u32;
        for _ in 0..tp_count {
            at_timepoints.push(t);
            t += 480 + (j as u32 + 1) * 60;
        }
        net.patterns.insert(
            format!("p{j}"),
            Pattern {
                direction: None,
                route: "r0".into(),
                name: format!("Pattern {j}"),
                path: None,
                stops,
                timing: vec![TimingProfile {
                    start_secs: 0,
                    end_secs: 30 * 3600,
                    at_timepoints,
                }],
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
                end: YEAR_START + Duration::days(364),
            }],
            exceptions: if spec.with_exception {
                vec![CalendarException {
                    date: EXCEPTION_DATE,
                    state: DayState::Added,
                }]
            } else {
                vec![]
            },
            },
        },
    );

    for i in 0..spec.n_services {
        let pattern = format!("p{}", i % spec.n_patterns);
        // Headway and exact services alternate; both stay inside the
        // 0..30h timing coverage.
        let departures = if i % 2 == 0 {
            Departures::HeadwayBands {
                bands: vec![HeadwayBand {
                    start_secs: 6 * 3600 + i * 600,
                    end_secs: 9 * 3600 + i * 600,
                    headway_secs: spec.headway,
                }],
            }
        } else {
            Departures::ExactTimes {
                starts: vec![7 * 3600 + i * 300, 8 * 3600 + i * 300, 9 * 3600 + i * 300],
            }
        };
        net.pattern_trips.insert(
            format!("svc{i}"),
            PatternTrips {
                pattern,
                service_type: "cal".into(),
                departures,
            },
        );
    }

    if spec.with_block {
        // Two consecutive trips of svc0, layover sized to fit — and
        // only if the running time even allows the connection.
        if let Some(service) = net.pattern_trips.get("svc0") {
            let starts = transit_network::trip_starts(&service.departures);
            if starts.len() >= 2 {
                if let Ok(arrival) = trip_arrival_end_secs(&net, "svc0", starts[0]) {
                    if arrival <= starts[1] {
                        let gap = starts[1] - arrival;
                        let mut layovers = vec![];
                        if gap > 0 {
                            layovers.push(Layover {
                                after_trip: 0,
                                secs: spec.layover.min(gap),
                                at: None,
                            });
                        }
                        net.blocks.insert(
                            "b0".into(),
                            Block {
                                trips: vec![
                                    TripRef {
                                        pattern_trips: "svc0".into(),
                                        start_secs: starts[0],
                                    },
                                    TripRef {
                                        pattern_trips: "svc0".into(),
                                        start_secs: starts[1],
                                    },
                                ],
                                layovers,
                            },
                        );
                    }
                }
            }
        }
    }
    net
}

fn net_strategy() -> impl Strategy<Value = Network> {
    spec_strategy().prop_map(|spec| build_network(&spec))
}

// ---------------------------------------------------------------------
// Independent oracles (no shared code with the implementation)
// ---------------------------------------------------------------------

/// Oracle calendar: ISO weekday via string formatting, exception
/// scan, range check — deliberately different mechanics.
fn oracle_runs_on(rule: &CalendarRule, date: NaiveDate) -> bool {
    let iso = date.format("%u").to_string();
    let flagged = match iso.as_str() {
        "1" => rule.days.mon,
        "2" => rule.days.tue,
        "3" => rule.days.wed,
        "4" => rule.days.thu,
        "5" => rule.days.fri,
        "6" => rule.days.sat,
        _ => rule.days.sun,
    };
    let in_range = rule
        .ranges
        .iter()
        .any(|r| date >= r.start && date <= r.end);
    for e in &rule.exceptions {
        if e.date == date {
            return matches!(e.state, DayState::Added);
        }
    }
    flagged && in_range
}

/// Oracle trip starts: count-then-index instead of accumulate.
fn oracle_starts(pattern_trips: &PatternTrips) -> Vec<u32> {
    match &pattern_trips.departures {
        Departures::HeadwayBands { bands } => {
            let mut out = Vec::new();
            for band in bands {
                if band.headway_secs == 0 {
                    continue;
                }
                let count = (band.end_secs - band.start_secs).div_ceil(band.headway_secs);
                for k in 0..count {
                    out.push(band.start_secs + k * band.headway_secs);
                }
            }
            out.sort_unstable();
            out
        }
        Departures::ExactTimes { starts } => starts.clone(),
    }
}

/// Oracle stop times: per-stop bracketing scan instead of the
/// implementation's sequential cursor walk.
fn oracle_stop_times(pattern: &Pattern, start: u32) -> Vec<(String, u32, u32)> {
    let profile = pattern
        .timing
        .iter()
        .find(|p| p.start_secs <= start && start < p.end_secs)
        .expect("generator keeps trips inside timing coverage");
    let tps: Vec<usize> = pattern
        .stops
        .iter()
        .enumerate()
        .filter(|(_, s)| s.timepoint)
        .map(|(i, _)| i)
        .collect();
    let mut out = Vec::new();
    for (i, ps) in pattern.stops.iter().enumerate() {
        let arrival = if ps.timepoint {
            let k = tps.iter().position(|&p| p == i).unwrap();
            start + profile.at_timepoints[k]
        } else {
            let before: Vec<usize> = tps.iter().cloned().filter(|&p| p < i).collect();
            let after: Vec<usize> = tps.iter().cloned().filter(|&p| p > i).collect();
            let left = before.last().copied().unwrap_or(0);
            let right = after.first().copied().unwrap_or(tps.len().saturating_sub(1));
            let d0 = pattern.stops[left].distance_m;
            let d1 = pattern.stops[right].distance_m;
            let t0 = profile.at_timepoints[tps.iter().position(|&p| p == left).unwrap()];
            let t1 = profile.at_timepoints[tps.iter().position(|&p| p == right).unwrap()];
            let frac = if d1 > d0 { ((ps.distance_m - d0) / (d1 - d0)).clamp(0.0, 1.0) } else { 0.0 };
            (start as f64 + t0 as f64 + frac * (t1 as f64 - t0 as f64)).round() as u32
        };
        let departure = if i == 0 { start } else { arrival };
        out.push((ps.stop.clone(), arrival, departure));
    }
    out
}

/// Oracle materialization: rebuild the whole day independently and
/// compare with `materialize`.
fn oracle_materialize(net: &Network, date: NaiveDate) -> Vec<(String, String, u32, Vec<(String, u32, u32)>)> {
    let mut out = Vec::new();
    for (service_id, service) in &net.pattern_trips {
        let rule = &net.service_types[&service.service_type];
        if !oracle_runs_on(&rule.rule, date) {
            continue;
        }
        let pattern = &net.patterns[&service.pattern];
        for start in oracle_starts(service) {
            out.push((
                service_id.clone(),
                pattern.route.clone(),
                start,
                oracle_stop_times(pattern, start),
            ));
        }
    }
    out.sort_by(|a, b| a.2.cmp(&b.2).then_with(|| a.0.cmp(&b.0)));
    out
}

// ---------------------------------------------------------------------
// The laws
// ---------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    /// Every generated network validates — validity by construction
    /// is only meaningful if `validate` agrees.
    #[test]
    fn generated_networks_validate(net in net_strategy()) {
        prop_assert!(net.validate().is_ok(), "issues: {:?}", validation_issues(&net));
    }

    /// serde round-trip: the document survives JSON exactly.
    #[test]
    fn network_round_trips_through_json(net in net_strategy()) {
        let json = serde_json::to_string(&net).unwrap();
        let back: Network = serde_json::from_str(&json).unwrap();
        prop_assert_eq!(back, net);
    }

    /// Canonical serialization: identical bytes every time — the
    /// substrate per-entity sync and content addressing needs.
    #[test]
    fn serialization_is_deterministic(net in net_strategy()) {
        let a = serde_json::to_string(&net).unwrap();
        let b = serde_json::to_string(&net).unwrap();
        prop_assert_eq!(a, b);
    }

    /// Oracle law: materialization matches the independent expansion
    /// on every probed date — weekdays, weekends, the exception
    /// date, and dates outside every range.
    #[test]
    fn materialize_matches_the_independent_oracle(net in net_strategy()) {
        for day in [1u32, 2, 5, 6, 100, 200, 304, 305, 400] {
            let date = YEAR_START + Duration::days(day as i64);
            let got = materialize(&net, date).unwrap();
            let expected = oracle_materialize(&net, date);
            prop_assert_eq!(got.len(), expected.len(), "trip count on {:?}", date);
            for (trip, (pattern_trips, route, start, stop_times)) in got.iter().zip(&expected) {
                prop_assert_eq!(trip.pattern_trips.as_str(), pattern_trips.as_str());
                prop_assert_eq!(&trip.route, route);
                prop_assert_eq!(trip.start_secs, *start);
                prop_assert_eq!(trip.stop_times.len(), stop_times.len());
                for (st, (stop, arr, dep)) in trip.stop_times.iter().zip(stop_times) {
                    prop_assert_eq!(&st.stop, stop);
                    prop_assert_eq!(st.arrival_secs, *arr);
                    prop_assert_eq!(st.departure_secs, *dep);
                }
            }
        }
    }

    /// Calendar law: `runs_on` matches the brute-force oracle over a
    /// full year, exceptions included.
    #[test]
    fn calendar_runs_on_matches_brute_force(net in net_strategy()) {
        let rule = &net.service_types["cal"];
        for day in 0..365 {
            let date = YEAR_START + Duration::days(day);
            prop_assert_eq!(rule.rule.runs_on(date), oracle_runs_on(&rule.rule, date));
        }
    }

    /// Mutation law: any single referential break invalidates the
    /// document (validation has no blind spots the generator feeds).
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
            if let Departures::HeadwayBands { bands } = &mut pattern_trips.departures {
                bands[0].headway_secs = 0;
            } else if let Departures::ExactTimes { starts } = &mut pattern_trips.departures {
                starts[0] = 31 * 3600; // outside every timing profile
            }
        }
        prop_assert!(!validation_issues(&broken).is_empty());

        // A block trip its schedule does not run (QAF-1: a retimed
        // schedule leaves its blocks behind) is an invalidity.
        let mut broken = net.clone();
        if let Some((_, block)) = broken.blocks.iter_mut().next() {
            block.trips[0].start_secs += 1;
            prop_assert!(!validation_issues(&broken).is_empty());
        }
    }

    /// Totality law: materialization never panics on *any* generated
    /// network, valid or mutated — it returns Ok or Err.
    #[test]
    fn materialize_never_panics_on_broken_documents(net in net_strategy()) {
        let mut broken = net.clone();
        if let Some((_, pattern)) = broken.patterns.iter_mut().next() {
            pattern.timing.clear();
        }
        let date = YEAR_START + Duration::days(3);
        let _ = materialize(&broken, date);

        let mut broken = net.clone();
        if let Some((_, pattern_trips)) = broken.pattern_trips.iter_mut().next() {
            pattern_trips.pattern = "ghost-pattern".into();
        }
        let _ = materialize(&broken, date);
    }
}
