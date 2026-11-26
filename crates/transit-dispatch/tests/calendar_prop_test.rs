//! `active_services` and `service_dates` are one relation read in opposite
//! directions (crates/cap-calendar/OPS.md "service_dates"). The oracle is a
//! day fold computed independently per date.
//!
//! 1. For every service type and date, `service_dates` names the date if and
//!    only if `active_services` names the type.
//! 2. Each type's window answer equals the dates the day fold lists it on: the
//!    rule answers no date the day fold does not, and misses none it does.
//! 3. An unknown type, or one no pattern trips run under, answers empty however
//!    its rule reads. So does an inverted window.

use transit_date::{Days, Date};
use proptest::prelude::*;
use transit_dispatch::{active_services, service_dates};
use transit_network::{
    materialize, CalendarException, CalendarRule, DateRange, DayState, Departs, Network, StopOffset, Trip,
    Pattern, PatternTrips, PatternStop, Route, ServiceType, Stop, TimingProfile, WeekdaySet,
};

const YEAR_START: Date = Date::from_ymd_opt(2026, 1, 1).unwrap();
const TYPES: [&str; 3] = ["WK", "SAT", "HOL"];

/// An exception always wins. An `Added` exception may sit outside every range.
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

fn day(offset: u16) -> Date {
    YEAR_START + Days::new(offset as i64)
}

/// A generated rule may belong to a type that runs no trips. That type never
/// runs.
fn build_network(rules: &[GenRule], trips_per_type: u8) -> Network {
    let mut stops = std::collections::BTreeMap::new();
    for (id, name) in [("A", "Main & 1st"), ("B", "Main & 2nd"), ("C", "Main & 3rd")] {
        stops.insert(
            id.to_string(),
            Stop { name: name.into(), lat: 45.5, lon: -122.5, parent: None, transfer: false, ..Default::default() },
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
            ..Default::default()
        },
    );
    let mut patterns = std::collections::BTreeMap::new();
    patterns.insert(
        "p0".to_string(),
        Pattern {
            direction: None,
            route: "10".into(),
            name: "route 10".into(),
            alignment: None,
            stops: vec![
                PatternStop { stop: "A".into(), timepoint: true, distance_m: 0.0, ..Default::default() },
                PatternStop { stop: "B".into(), timepoint: true, distance_m: 500.0, ..Default::default() },
                PatternStop { stop: "C".into(), timepoint: true, distance_m: 1000.0, ..Default::default() },
            ],
            timing: vec![TimingProfile {
                start_secs: 0,
                end_secs: 172_800,
                at_timepoints: vec![0, 600, 1200],
                extra: Default::default(),
            }],
            extra: Default::default(),
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
                        .map(|&(s, e)| DateRange { start: day(s), end: day(e), extra: Default::default() })
                        .collect(),
                    exceptions: rule
                        .exceptions
                        .iter()
                        .map(|&(d, added)| CalendarException {
                            date: day(d),
                            state: if added { DayState::Added } else { DayState::Removed },
                            extra: Default::default(),
                        })
                        .collect(),
                        extra: Default::default(),
                },
                extra: Default::default(),
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
                        ..Default::default()
                    }],
                    extra: Default::default(),
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
        alignments: Default::default(),
        service_types,
        pattern_trips,
        blocks: Default::default(),
        ..Default::default()
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn service_dates_is_active_services_read_backwards(
        rules in proptest::collection::vec(gen_rule(), 3),
        trips_per_type in 0u8..4,
        window_start in 0u16..320,
    ) {
        let net = build_network(&rules, trips_per_type);
        let from = day(window_start);
        let to = day(window_start + 30);

        let mut actives_by_date = Vec::new();
        for d in 0..=30 {
            let date = from + Days::new(d);
            let day = materialize(&net, date).unwrap();
            actives_by_date.push(active_services(&net, &day));
        }

        for ty in TYPES {
            // Property 1.
            for d in 0..=30 {
                let date = from + Days::new(d);
                let named = service_dates(&net, &[], ty, date, date).unwrap();
                let listed = actives_by_date[d as usize].contains(&ty.to_string());
                prop_assert_eq!(named.len(), listed as usize);
            }
            // Property 2.
            let window = service_dates(&net, &[], ty, from, to).unwrap();
            let expect: Vec<Date> = (0..=30)
                .map(|d| from + Days::new(d))
                .filter(|date| actives_by_date[(*date - from).num_days() as usize].contains(&ty.to_string()))
                .collect();
            prop_assert_eq!(window, expect);
        }

        // Property 3.
        prop_assert!(service_dates(&net, &[], "NOPE", from, to).unwrap().is_empty());
        if trips_per_type < 3 {
            let idle = TYPES[trips_per_type as usize];
            prop_assert!(service_dates(&net, &[], idle, from, to).unwrap().is_empty());
        }
        prop_assert!(service_dates(&net, &[], "WK", to, from).unwrap().is_empty());
    }
}
