//! The calendar's two-way law, as a property test:
//! `active_services` (date → service keys) and `service_dates`
//! (type → dates) are the same relation read in opposite directions.
//! For every generated network (authored model vocabulary — weekday
//! flags, date ranges, per-date exceptions) and every date window:
//!
//! 1. **per-date two-way agreement** — for every service type and
//!    every date, `service_dates` names the date iff
//!    `active_services` names the type (the day fold is the
//!    oracle, computed independently per date);
//! 2. **window agreement** — each type's window answer equals the
//!    dates the day fold lists it on, and a type with a non-empty
//!    window answer is exactly one the day fold ever lists in the
//!    window — no date the rule answers that the day fold does
//!    not, and none the day fold has that the rule misses;
//! 3. **id-lookup honesty** — an unknown type, or one no pattern
//!    trips run under, answers empty however its rule reads; so
//!    does an inverted window.

use chrono::{Duration, NaiveDate};
use proptest::prelude::*;
use transit_dispatch::{active_services, service_dates};
use transit_network::{
    materialize, CalendarException, CalendarRule, DateRange, DayState, Departs, Network, StopOffset, Trip,
    Pattern, PatternTrips, PatternStop, Route, ServiceType, Stop, TimingProfile, WeekdaySet,
};

const YEAR_START: NaiveDate = NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();
const TYPES: [&str; 3] = ["WK", "SAT", "HOL"];

/// An authored calendar rule: weekday flags, date ranges within the
/// fixture year, and per-date exceptions that always win (an `Added`
/// exception may sit outside every range).
#[derive(Clone, Debug)]
struct GenRule {
    days: [bool; 7],
    ranges: Vec<(u16, u16)>,
    exceptions: Vec<(u16, bool)>,
}

fn gen_rule() -> impl Strategy<Value = GenRule> {
    (
        any::<[bool; 7]>(),
        proptest::collection::vec((0u16..320, 0u16..40), 0..=2),
        proptest::collection::vec((0u16..364, any::<bool>()), 0..=3),
    )
        .prop_map(|(days, ranges, exceptions)| GenRule {
            days,
            ranges: ranges.into_iter().map(|(s, len)| (s, s + len)).collect(),
            exceptions,
        })
}

fn day(offset: u16) -> NaiveDate {
    YEAR_START + Duration::days(offset as i64)
}

/// The authored test network: one route, one pattern A→B→C, and one
/// pattern-trips entry per service type up to `trips_per_type` (a
/// generated rule may belong to a type that runs no trips — the law
/// pins that it then never runs, however its rule reads).
fn build_network(rules: &[GenRule], trips_per_type: u8) -> Network {
    let mut stops = std::collections::BTreeMap::new();
    for (id, name) in [("A", "Main & 1st"), ("B", "Main & 2nd"), ("C", "Main & 3rd")] {
        stops.insert(
            id.to_string(),
            Stop { name: name.into(), lat: 45.5, lon: -122.5, parent: None, transfer: false },
        );
    }
    let mut routes = std::collections::BTreeMap::new();
    routes.insert(
        "10".to_string(),
        Route {
            name: "10".into(),
            mode: transit_network::Mode::Bus,
            color: None,
            directions: Default::default(),
        },
    );
    let mut patterns = std::collections::BTreeMap::new();
    patterns.insert(
        "p0".to_string(),
        Pattern {
            direction: None,
            route: "10".into(),
            name: "route 10".into(),
            path: None,
            stops: vec![
                PatternStop { stop: "A".into(), timepoint: true, distance_m: 0.0 },
                PatternStop { stop: "B".into(), timepoint: true, distance_m: 500.0 },
                PatternStop { stop: "C".into(), timepoint: true, distance_m: 1000.0 },
            ],
            timing: vec![TimingProfile {
                start_secs: 0,
                end_secs: 172_800,
                at_timepoints: vec![0, 600, 1200],
            }],
        },
    );
    let mut pattern_trips = std::collections::BTreeMap::new();
    let mut service_types = std::collections::BTreeMap::new();
    for (i, rule) in rules.iter().enumerate() {
        let ty = TYPES[i];
        service_types.insert(
            ty.to_string(),
            ServiceType {
                name: ty.into(),
                rule: CalendarRule {
                    days: WeekdaySet {
                        mon: rule.days[0],
                        tue: rule.days[1],
                        wed: rule.days[2],
                        thu: rule.days[3],
                        fri: rule.days[4],
                        sat: rule.days[5],
                        sun: rule.days[6],
                    },
                    ranges: rule
                        .ranges
                        .iter()
                        .map(|&(s, e)| DateRange { start: day(s), end: day(e) })
                        .collect(),
                    exceptions: rule
                        .exceptions
                        .iter()
                        .map(|&(d, added)| CalendarException {
                            date: day(d),
                            state: if added { DayState::Added } else { DayState::Removed },
                        })
                        .collect(),
                },
            },
        );
        if (i as u8) < trips_per_type {
            pattern_trips.insert(
                format!("svc-{ty}"),
                PatternTrips {
                    pattern: "p0".into(),
                    service_type: ty.into(),
                    trips: vec![Trip {
                        id: format!("svc-{ty}@28800"),
                        departs: Departs::At { start_secs: 28_800 },
                        times: [0, 600, 1200].map(|t| StopOffset { arrival_secs: Some(t), departure_secs: Some(t) }).to_vec(),
                    }],
                },
            );
        }
    }
    Network {
        name: "law net".into(),
        plan: Default::default(),
        anchor: Default::default(),
        stops,
        routes,
        patterns,
        service_types,
        pattern_trips,
        blocks: Default::default(),
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// Laws 1 and 2 over a window, plus the honesty edges (3).
    #[test]
    fn service_dates_is_active_services_read_backwards(
        rules in proptest::collection::vec(gen_rule(), 3),
        trips_per_type in 0u8..4,
        window_start in 0u16..320,
    ) {
        let net = build_network(&rules, trips_per_type);
        let from = day(window_start);
        let to = day(window_start + 30);

        // The oracle: the day fold, computed independently per date.
        let mut actives_by_date = Vec::new();
        for d in 0..=30 {
            let date = from + Duration::days(d);
            let day = materialize(&net, date).unwrap();
            actives_by_date.push(active_services(&net, &day));
        }

        for ty in TYPES {
            // Law 1: per-date two-way agreement.
            for d in 0..=30 {
                let date = from + Duration::days(d);
                let named = service_dates(&net, &[], ty, date, date).unwrap();
                let listed = actives_by_date[d as usize].contains(&ty.to_string());
                prop_assert_eq!(named.len(), listed as usize);
            }
            // Law 2: the window answer equals the day-fold set.
            let window = service_dates(&net, &[], ty, from, to).unwrap();
            let expect: Vec<NaiveDate> = (0..=30)
                .map(|d| from + Duration::days(d))
                .filter(|date| actives_by_date[(*date - from).num_days() as usize].contains(&ty.to_string()))
                .collect();
            prop_assert_eq!(window, expect);
        }

        // Law 3: unknown types and no-trips types answer empty, and
        // so does an inverted window.
        prop_assert!(service_dates(&net, &[], "NOPE", from, to).unwrap().is_empty());
        if trips_per_type < 3 {
            let idle = TYPES[trips_per_type as usize];
            prop_assert!(service_dates(&net, &[], idle, from, to).unwrap().is_empty());
        }
        prop_assert!(service_dates(&net, &[], "WK", to, from).unwrap().is_empty());
    }
}
