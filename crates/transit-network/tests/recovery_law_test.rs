//! Required recovery, separated from slack (D-9).
//!
//! An imported layover is the whole observed gap between two trips,
//! so judging a block against its own authored layover makes every
//! later arrival infeasible. The laws: a layover is judged against the
//! recovery REQUIRED where the vehicle waits — a policy's requirement
//! when the caller supplies one (it wins), else the smallest authored
//! layover at that terminal (the derived floor). What is above the
//! requirement is slack, and slack may shrink freely.
//!
//! API notes: `recovery_floors(net)` — terminal → smallest authored
//! layover secs (the layover's `at`, else the stop the trip before
//! ends at); `validation_issues_under(net, &policy)` — the issues with
//! a policy's requirement per adjacent pair (`RecoveryAt`: the trip
//! before's route, service type, run time, and the terminal; `None`
//! falls back to the floor); `validation_issues(net)` is the same with
//! no policy. A shortfall is `LayoverInfeasible { gap_secs,
//! required_secs, required_by }`.

use std::collections::BTreeMap;

use chrono::NaiveDate;
use proptest::prelude::*;
use transit_network::{
    recovery_floors, validation_issues, validation_issues_under, Anchor, Block, CalendarRule, DateRange, Departs,
    Layover, Mode, Network, NetworkError, Pattern, PatternStop, PatternTrips, RecoveryAt, RecoverySource, Route,
    ServiceType, StopOffset, Trip, TripRef, WeekdaySet,
};

fn at(t: u32) -> StopOffset {
    StopOffset { arrival_secs: Some(t), departure_secs: Some(t) }
}

/// A one-start trip A → C, 20 minutes.
fn trip(id: &str, start_secs: u32) -> Trip {
    Trip { id: id.into(), departs: Departs::At { start_secs }, times: vec![at(0), at(1200)], ..Default::default() }
}

fn pattern(route: &str) -> Pattern {
    Pattern {
        route: route.into(),
        name: format!("{route} to C"),
        stops: vec![
            PatternStop { stop: "A".into(), timepoint: true, distance_m: 0.0, ..Default::default() },
            PatternStop { stop: "C".into(), timepoint: true, distance_m: 2000.0, ..Default::default() },
        ],
        ..Default::default()
    }
}

const H: u32 = 3600;

/// Two blocks, both waiting at C: b1 runs 07:00 → 07:20, waits a
/// planned 600 s, leaves 07:50 (1800 s gap); b2 runs 09:00 → 09:20
/// and waits the whole observed gap, 2400 s, before 10:00. C's floor
/// is 600 s.
fn network() -> Network {
    let stop = |lat: f64| transit_network::Stop { name: "s".into(), lat, lon: -122.7, ..Default::default() };
    let net = Network {
        name: "Recovery".into(),
        anchor: Anchor::NoonMinusTwelve,
        stops: [("A".to_string(), stop(45.0)), ("C".to_string(), stop(45.02))].into_iter().collect(),
        routes: ["5", "6"]
            .into_iter()
            .map(|r| (r.to_string(), Route { name: r.into(), mode: Mode::Bus, ..Default::default() }))
            .collect(),
        patterns: [("p5".to_string(), pattern("5")), ("p6".to_string(), pattern("6"))].into_iter().collect(),
        service_types: [(
            "wk".to_string(),
            ServiceType {
                name: "Weekday".into(),
                rule: CalendarRule {
                    days: WeekdaySet::weekdays(),
                    ranges: vec![DateRange {
                        start: NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
                        end: NaiveDate::from_ymd_opt(2026, 12, 31).unwrap(),
                    }],
                    exceptions: vec![],
                },
            },
        )]
        .into_iter()
        .collect(),
        pattern_trips: [
            (
                "p5-wk".to_string(),
                PatternTrips {
                    pattern: "p5".into(),
                    service_type: "wk".into(),
                    trips: vec![trip("t0700", 7 * H), trip("t0750", 7 * H + 3000), trip("t0900", 9 * H)],
                },
            ),
            (
                "p6-wk".to_string(),
                PatternTrips { pattern: "p6".into(), service_type: "wk".into(), trips: vec![trip("t1000", 10 * H)] },
            ),
        ]
        .into_iter()
        .collect(),
        blocks: [
            (
                "b1".to_string(),
                Block {
                    trips: vec![TripRef { trip: "t0700".into() }, TripRef { trip: "t0750".into() }],
                    layovers: vec![Layover { after_trip: 0, secs: 600, at: None }],
                },
            ),
            (
                "b2".to_string(),
                Block {
                    trips: vec![TripRef { trip: "t0900".into() }, TripRef { trip: "t1000".into() }],
                    layovers: vec![Layover { after_trip: 0, secs: 2400, at: None }],
                },
            ),
        ]
        .into_iter()
        .collect(),
        ..Default::default()
    };
    assert_eq!(validation_issues(&net), vec![], "the fixture is valid");
    net
}

/// Move a one-start trip.
fn start(net: &mut Network, trip_id: &str, secs: u32) {
    for pt in net.pattern_trips.values_mut() {
        for t in &mut pt.trips {
            if t.id == trip_id {
                t.departs = Departs::At { start_secs: secs };
            }
        }
    }
}

fn shortfalls(issues: &[NetworkError]) -> Vec<(String, u32, u32, RecoverySource)> {
    issues
        .iter()
        .filter_map(|i| match i {
            NetworkError::LayoverInfeasible { block, gap_secs, required_secs, required_by, .. } => {
                Some((block.clone(), *gap_secs, *required_secs, *required_by))
            }
            _ => None,
        })
        .collect()
}

#[test]
fn the_floor_is_the_smallest_authored_layover_where_the_vehicle_waits() {
    let mut net = network();
    assert_eq!(recovery_floors(&net), BTreeMap::from([("C".to_string(), 600)]));
    // A layover naming where it waits counts there, not at the trip's end.
    net.blocks.get_mut("b1").unwrap().layovers[0].at = Some("A".into());
    assert_eq!(recovery_floors(&net), BTreeMap::from([("A".to_string(), 600), ("C".to_string(), 2400)]));
}

proptest! {
    /// An observed gap that shrinks is slack spent, not a
    /// broken block — the block fails only below the terminal's floor.
    #[test]
    fn a_layover_is_judged_against_its_terminals_floor_not_its_own_length(cut in 0u32..=2400) {
        let mut net = network();
        start(&mut net, "t1000", 10 * H - cut);
        let gap = 2400 - cut;
        let found = shortfalls(&validation_issues(&net));
        if gap < 600 {
            prop_assert_eq!(found, vec![("b2".to_string(), gap, 600, RecoverySource::Terminal)]);
        } else {
            prop_assert_eq!(found, vec![]);
        }
    }

    /// D-9: the policy wins over the derived floor, in both directions
    /// — under it a gap below the floor passes, over it a gap above
    /// the floor fails.
    #[test]
    fn a_policy_requirement_wins_over_the_floor(required in 0u32..3000, cut in 0u32..=2400) {
        let mut net = network();
        start(&mut net, "t1000", 10 * H - cut);
        let policy = |_: &RecoveryAt| Some(required);
        let mut want = vec![];
        if 1800 < required {
            want.push(("b1".to_string(), 1800, required, RecoverySource::Policy));
        }
        if 2400 - cut < required {
            want.push(("b2".to_string(), 2400 - cut, required, RecoverySource::Policy));
        }
        prop_assert_eq!(shortfalls(&validation_issues_under(&net, &policy)), want);
    }

    /// A pair with no authored layover has no floor of its own: with
    /// no policy only its order is judged; a policy judges it like any
    /// other adjacent pair.
    #[test]
    fn a_policy_judges_a_pair_with_no_authored_layover(required in 0u32..3000) {
        let mut net = network();
        net.blocks.get_mut("b2").unwrap().layovers.clear();
        prop_assert_eq!(shortfalls(&validation_issues(&net)), vec![]);
        let policy = |_: &RecoveryAt| Some(required);
        let b2: Vec<_> = shortfalls(&validation_issues_under(&net, &policy)).into_iter().filter(|s| s.0 == "b2").collect();
        if 2400 < required {
            prop_assert_eq!(b2, vec![("b2".to_string(), 2400, required, RecoverySource::Policy)]);
        } else {
            prop_assert_eq!(b2, vec![]);
        }
    }
}

#[test]
fn the_policy_is_asked_about_the_trip_before_and_where_it_waits() {
    let net = network();
    let asked = std::cell::RefCell::new(Vec::new());
    let policy = |at: &RecoveryAt| {
        asked.borrow_mut().push((at.route.to_string(), at.service_type.to_string(), at.run_secs, at.terminal.to_string()));
        None
    };
    // None falls back to the floor: the fixture stays valid.
    assert_eq!(validation_issues_under(&net, &policy), vec![]);
    let mut asked = asked.into_inner();
    asked.sort();
    assert_eq!(
        asked,
        vec![("5".to_string(), "wk".to_string(), 1200, "C".to_string()), ("5".to_string(), "wk".to_string(), 1200, "C".to_string())]
    );
}
