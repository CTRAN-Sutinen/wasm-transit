//! The runs cut over generated days and generated work rules
//! (docs/units/runs-layer.md, "Testing"). Every trip of every block that
//! runs is in exactly one piece of one run, a piece keeps its block's
//! order, a run's pieces do not overlap, each run is judged as the run
//! module judges it, a run's class meets its conditions, and the same
//! input gives the same bytes.

mod common;

use std::collections::BTreeMap;

use common::rules_gen::*;
use common::*;
use proptest::prelude::*;
use transit_network::{apply_edits, materialize};
use transit_runs::*;

fn trips() -> impl Strategy<Value = Vec<(&'static str, bool, u32)>> {
    proptest::collection::vec((prop_oneof![Just("5"), Just("6")], any::<bool>(), (5 * 60u32..22 * 60).prop_map(|m| m * 60)), 0..30)
}

/// A day whose trips the suggested blocking chains, with some blocks
/// dropped so that some trips run in no block.
fn day() -> impl Strategy<Value = (transit_network::Network, Vec<transit_network::MaterializedTrip>)> {
    (trips(), any::<u64>()).prop_map(|(trips, drop)| {
        let net = world(&trips, &[]);
        let day = materialize(&net, date()).unwrap();
        let suggestion = suggest_blocks(&net, &day, None);
        let mut net = apply_edits(&net, &suggestion.edits).unwrap();
        let ids: Vec<String> = net.blocks.keys().cloned().collect();
        for (i, id) in ids.iter().enumerate() {
            if drop >> (i % 64) & 1 == 1 && i % 3 == 0 {
                net.blocks.remove(id);
            }
        }
        let day = materialize(&net, date()).unwrap();
        (net, day)
    })
}

fn rules() -> impl Strategy<Value = WorkRules> {
    work_rules().prop_map(|w| w.checked().unwrap())
}

fn cut(net: &transit_network::Network, day: &[transit_network::MaterializedTrip], w: &WorkRules) -> Cut {
    let places = NetworkPlaces::new(net, day);
    cut_runs(net, day, w, None, "2026-10-23", Day::Fri, &places)
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 64, ..ProptestConfig::default() })]

    #[test]
    fn every_blocked_trip_is_in_exactly_one_piece((net, day) in day(), w in rules()) {
        let c = cut(&net, &day, &w);
        let mut seen: BTreeMap<(String, u32), u32> = BTreeMap::new();
        for r in &c.runs {
            for p in &r.pieces {
                for t in &p.trips {
                    *seen.entry((t.trip.clone(), t.start_secs)).or_default() += 1;
                }
            }
        }
        prop_assert!(seen.values().all(|&n| n == 1), "a trip in two pieces: {:?}", seen);
        let blocked: std::collections::BTreeSet<&str> = net.blocks.values().flat_map(|b| b.trips.iter().map(|t| t.trip.as_str())).collect();
        for t in &day {
            let key = (t.trip.clone(), t.start_secs);
            if blocked.contains(t.trip.as_str()) {
                prop_assert!(seen.contains_key(&key), "{} is in no piece", t.trip);
            } else {
                prop_assert!(c.unblocked.contains(&format!("{}@{}", t.trip, t.start_secs)), "{} is not listed unblocked", t.trip);
            }
        }
    }

    #[test]
    fn each_run_is_whole_ordered_and_judged_as_the_run_module_judges_it((net, day) in day(), w in rules()) {
        let c = cut(&net, &day, &w);
        let places = NetworkPlaces::new(&net, &day);
        for r in &c.runs {
            prop_assert!(!r.pieces.is_empty());
            for pair in r.pieces.windows(2) {
                prop_assert!(pair[0].end_secs() <= pair[1].start_secs(), "run {} overlaps itself", r.id);
            }
            for p in &r.pieces {
                prop_assert!(!p.trips.is_empty());
                for pair in p.trips.windows(2) {
                    prop_assert!(pair[0].start_secs <= pair[1].start_secs, "piece of {} is out of order", p.block);
                }
            }
            prop_assert_eq!(&r.judged, &judge(&w, r.class.as_deref(), &r.pieces, Day::Fri, &places));
            if let Some(class) = &r.class {
                prop_assert!(meets(&w, class, &r.pieces), "run {} does not meet class {}", r.id, class);
            }
        }
    }

    #[test]
    fn the_same_day_and_rules_give_the_same_bytes((net, day) in day(), w in rules()) {
        let a = serde_json::to_string(&cut(&net, &day, &w)).unwrap();
        let b = serde_json::to_string(&cut(&net, &day, &w)).unwrap();
        prop_assert_eq!(a, b);
    }

    #[test]
    fn the_mix_counts_the_runs_in_its_classes((net, day) in day(), w in rules()) {
        let c = cut(&net, &day, &w);
        for (m, report) in w.mix.iter().zip(&c.mix) {
            if m.of == ShareOf::Runs && !c.runs.is_empty() {
                let inside = c.runs.iter().filter(|r| r.class.as_ref().is_some_and(|k| m.classes.contains(k))).count();
                prop_assert_eq!(report.share_pct as usize, inside * 100 / c.runs.len());
            }
        }
    }
}
