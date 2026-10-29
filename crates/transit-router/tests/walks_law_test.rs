//! The router over a walk table: a plan's transfers walk the streets.
//!
//! The intent (docs/units/streets-model.md, the walk table): with a
//! walk table, the router's footpaths ARE the table's walks — every
//! walk between two of the network's stops at the table's seconds,
//! and nothing else (no straight-line pair survives, and a walk naming
//! a stop the network does not have is ignored). With none, the
//! router keeps its straight-line transfers (D-32, encoded
//! provisionally).
//!
//! The planning laws (`prop_test.rs`, `arrive_by_law_test.rs`) hold
//! over whatever footpaths the network has; these laws pin which
//! footpaths those are.

#[path = "prop_support.rs"]
mod support;

use std::collections::BTreeMap;

use chrono::NaiveDate;
use proptest::collection::vec;
use proptest::prelude::*;
use support::{build_zip_spaced, model_day, TripSpec};
use transit_router::{earliest_arrival, Leg, RouterNetwork};
use transit_streets::walking::{Walk, WalkTable};

fn date() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 10, 23).unwrap()
}

fn table(walks: Vec<Walk>) -> WalkTable {
    WalkTable {
        streets_hash: "sha256:streets".into(),
        network_hash: "sha256:network".into(),
        speed_mps: 1.3,
        max_walk_secs: 600,
        walks,
    }
}

/// A symmetric table over `n` stops (`s0`…), with walks naming a stop
/// the network does not have mixed in.
fn walks(n: usize) -> impl Strategy<Value = Vec<Walk>> {
    vec((0..n + 1, 0..n + 1, 1u32..600), 0..12).prop_map(move |pairs| {
        let id = |i: usize| if i == n { "ghost".to_string() } else { format!("s{i}") };
        let mut by_pair = BTreeMap::new();
        for (a, b, secs) in pairs {
            if a != b {
                by_pair.insert((id(a.min(b)), id(a.max(b))), secs);
            }
        }
        let mut out = Vec::new();
        for ((a, b), walk_secs) in by_pair {
            out.push(Walk { from: a.clone(), to: b.clone(), walk_secs });
            out.push(Walk { from: b, to: a, walk_secs });
        }
        out.sort_by(|x, y| (&x.from, &x.to).cmp(&(&y.from, &y.to)));
        out
    })
}

fn case() -> impl Strategy<Value = (usize, Vec<Walk>)> {
    (2usize..7).prop_flat_map(|n| (Just(n), walks(n)))
}

fn footpaths(net: &RouterNetwork) -> BTreeMap<(String, String), u32> {
    let mut out = BTreeMap::new();
    for node in &net.stops {
        for edge in &node.footpaths {
            out.insert((node.stop_id.clone(), net.stops[edge.to as usize].stop_id.clone()), edge.walk_secs);
        }
    }
    out
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    /// With a walk table, the footpaths are exactly the table's walks
    /// between the network's stops — the straight-line pairs (every
    /// stop here is ~89 m from the next) are gone.
    #[test]
    fn the_footpaths_are_the_walk_table((n, walks) in case()) {
        let trips = [TripSpec { stops: (0..n as u32).collect(), start: 28_800, step: 120 }];
        let (network, day) = model_day(&build_zip_spaced(n, 0.0008, &trips), date());
        let net = RouterNetwork::build(&network, &day, date(), None).with_walks(&table(walks.clone()));
        let want: BTreeMap<_, _> = walks
            .iter()
            .filter(|w| w.from != "ghost" && w.to != "ghost")
            .map(|w| ((w.from.clone(), w.to.clone()), w.walk_secs))
            .collect();
        prop_assert_eq!(footpaths(&net), want);
    }
}

/// Two stops far apart with no trip between them: the table's walk is
/// the plan. Two stops a few meters apart that the table says no walk
/// joins (a freeway between them): no plan.
#[test]
fn a_plan_walks_where_the_table_walks_and_nowhere_else() {
    let far = [TripSpec { stops: vec![0], start: 28_800, step: 60 }];
    let (network, day) = model_day(&build_zip_spaced(2, 0.003, &far), date());
    let walk = |secs| vec![
        Walk { from: "s0".into(), to: "s1".into(), walk_secs: secs },
        Walk { from: "s1".into(), to: "s0".into(), walk_secs: secs },
    ];
    let net = RouterNetwork::build(&network, &day, date(), None).with_walks(&table(walk(420)));
    let plan = earliest_arrival(&net, "s0", "s1", 28_000, 3).unwrap().expect("the walk reaches it");
    assert_eq!(plan.arrival_secs, 28_420);
    assert!(matches!(&plan.legs[..], [Leg::Walk { walk_secs: 420, .. }]), "{:?}", plan.legs);

    let (network, day) = model_day(&build_zip_spaced(2, 0.0001, &far), date());
    let straight = RouterNetwork::build(&network, &day, date(), None);
    assert!(earliest_arrival(&straight, "s0", "s1", 28_000, 3).unwrap().is_some(), "11 m apart: a straight-line walk");
    let none = straight.with_walks(&table(vec![]));
    assert!(earliest_arrival(&none, "s0", "s1", 28_000, 3).unwrap().is_none(), "the table joins them by no walk");
}
