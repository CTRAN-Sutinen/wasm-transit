//! Property-based tests (proptest) for the network model.
//!
//! Same discipline as `transit-core/tests/prop_test.rs`: laws that
//! must hold for *every* network, with the expected answer recomputed
//! **independently** from the generated input (no shared code with
//! the implementation). Three flavors:
//!
//! * round-trip laws (serde, canonical serialization);
//! * oracle properties — which trips `materialize` runs on a date is
//!   recomputed by a deliberately different expansion (count-derived
//!   band expansion, brute-force calendar over `%u` weekdays); the
//!   stop times a stored trip materializes are `trip_law_test.rs`'s;
//! * structural invariants — generated networks validate clean,
//!   single mutations always break validation.

use chrono::{Duration, NaiveDate};
use proptest::prelude::*;
use transit_network::{
    materialize, stop_detail, validation_issues, Block, CalendarException, CalendarRule, DateRange, DayState,
    Departs, HeadwayBand, Mode, Network, Pattern, PatternStop, PatternTrips, Route, ServiceType,
    StopOffset, TimingProfile, Trip, TripRef, WeekdaySet,
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
    with_block: bool,
    with_exception: bool,
    /// A parent station pin none of the patterns visits, with the
    /// first `bays` stops as its children — the shape a transfer
    /// center imports as (Fisher's Landing, its bays).
    station: bool,
    bays: u32,           // 1..3
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
    let mut net = Network { name: "Generated".into(), ..Default::default() };
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
    // The optional station pin: no pattern visits it, its bays
    // carry the trips — the transfer-center shape.
    if spec.station {
        net.stops.insert(
            "STN".into(),
            transit_network::Stop { name: "Station".into(), lat: 45.0, lon: -122.7, ..Default::default() },
        );
        for i in 0..spec.bays.min(spec.n_stops) {
            net.stops.get_mut(&format!("s{i}")).expect("built above").parent = Some("STN".into());
        }
    }
    net.routes.insert("r0".into(), Route { mode: Mode::Bus, ..Default::default() });

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
            stops.push(PatternStop { stop: format!("s{i}"), timepoint, distance_m: cum as f64, ..Default::default() });
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
                route: "r0".into(),
                name: format!("Pattern {j}"),
                stops,
                timing: vec![TimingProfile { start_secs: 0, end_secs: 30 * 3600, at_timepoints }],
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
        let key = format!("svc{i}");
        let pattern = format!("p{}", i % spec.n_patterns);
        let times = profile_times(&net.patterns[&pattern]);
        // Frequency and one-start trips alternate; ids are the trips'
        // own.
        let trips = if i % 2 == 0 {
            vec![Trip {
                id: format!("{key}@every"),
                departs: Departs::Every {
                    bands: vec![HeadwayBand {
                        start_secs: 6 * 3600 + i * 600,
                        end_secs: 9 * 3600 + i * 600,
                        headway_secs: spec.headway,
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
        net.pattern_trips.insert(key, PatternTrips { pattern, service_type: "cal".into(), trips });
    }

    if spec.with_block {
        // svc0's frequency trip, whole (GTFS: a block runs a trip
        // whole) — only when one vehicle can run its instances back to
        // back.
        let every = &net.pattern_trips["svc0"].trips[0];
        let run = every.times.last().and_then(|o| o.arrival_secs).unwrap();
        if spec.headway >= run {
            net.blocks.insert("b0".into(), Block { trips: vec![TripRef { trip: every.id.clone() }], layovers: vec![] });
        }
    }
    net
}

/// A trip's times as the pattern's profile produces them, computed
/// here: timepoints at the profile's offsets, other stops by distance
/// between their bracketing timepoints.
fn profile_times(pattern: &Pattern) -> Vec<StopOffset> {
    let profile = &pattern.timing[0];
    let tps: Vec<usize> = pattern.stops.iter().enumerate().filter(|(_, s)| s.timepoint).map(|(i, _)| i).collect();
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
                let (d0, d1) = (pattern.stops[left].distance_m, pattern.stops[right].distance_m);
                let frac = if d1 > d0 { (ps.distance_m - d0) / (d1 - d0) } else { 0.0 };
                (at(left) as f64 + frac * (at(right) as f64 - at(left) as f64)).round() as u32
            };
            StopOffset { arrival_secs: Some(t), departure_secs: Some(t) }
        })
        .collect()
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

/// Oracle instances: count-then-index instead of accumulate.
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

/// Oracle day: the (trip, start) instances of every trip whose
/// service runs on `date`.
fn oracle_day(net: &Network, date: NaiveDate) -> Vec<(String, u32)> {
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

    /// Oracle law: on every probed date — weekdays, weekends, the
    /// exception date, and dates outside every range — materialization
    /// runs exactly the stored trips of the services running that day.
    #[test]
    fn materialize_runs_the_days_trips(net in net_strategy()) {
        for day in [1u32, 2, 5, 6, 100, 200, 304, 305, 400] {
            let date = YEAR_START + Duration::days(day as i64);
            let mut got: Vec<(String, u32)> =
                materialize(&net, date).unwrap().into_iter().map(|t| (t.trip, t.start_secs)).collect();
            got.sort();
            prop_assert_eq!(got, oracle_day(&net, date), "the trips of {:?}", date);
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
            let trip = &mut pattern_trips.trips[0];
            match &mut trip.departs {
                Departs::Every { bands, .. } => bands[0].headway_secs = 0,
                Departs::At { .. } => { trip.times.pop(); } // a stop left untimed
            }
        }
        prop_assert!(!validation_issues(&broken).is_empty());

        // A block naming a trip the document does not have.
        let mut broken = net.clone();
        if let Some((_, block)) = broken.blocks.iter_mut().next() {
            block.trips[0].trip = "ghost-trip".into();
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

    /// QAF-51 (C-TRAN, Fisher's Landing): a stop's detail answers its
    /// whole WALKING GROUP — a station pin whose patterns visit only
    /// its bays answers their routes, and a bay answers its
    /// siblings'. The group is spelled out here from `parent` fields
    /// (the oracle), never borrowed from the model's own helper.
    #[test]
    fn stop_detail_pools_the_walking_group(net in net_strategy()) {
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
            let mut expected: std::collections::BTreeSet<String> = Default::default();
            for p in net.patterns.values() {
                if p.stops.iter().any(|ps| group.contains(ps.stop.as_str())) {
                    expected.insert(p.route.clone());
                }
            }
            let detail = stop_detail(&net, id).expect("a known stop has a detail");
            let got: std::collections::BTreeSet<String> =
                detail.routes.iter().map(|r| r.id.clone()).collect();
            prop_assert_eq!(
                got,
                expected,
                "stop {} does not answer its walking group",
                id
            );
        }
    }
}
