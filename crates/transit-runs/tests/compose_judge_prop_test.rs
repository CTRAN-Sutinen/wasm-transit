//! The cut's two steps alone, and runs judged whoever composed them
//! (docs/units/runs-layer.md, scope 1). The cut is its pieces composed. A
//! set of runs named by reference is judged as the cut judges its own, a
//! class is chosen as the cut chooses it, and the set reports the trips it
//! places twice or not at all.

mod common;

use std::collections::BTreeMap;

use common::rules_gen::*;
use common::*;
use proptest::prelude::*;
use transit_network::{apply_edits, materialize, MaterializedTrip, Network};
use transit_runs::*;

const DATE: &str = "2026-10-23";

fn trips() -> impl Strategy<Value = Vec<(&'static str, bool, u32)>> {
    proptest::collection::vec((prop_oneof![Just("5"), Just("6")], any::<bool>(), (5 * 60u32..22 * 60).prop_map(|m| m * 60)), 0..30)
}

fn day() -> impl Strategy<Value = (Network, Vec<MaterializedTrip>)> {
    trips().prop_map(|trips| {
        let net = world(&trips, &[]);
        let day = materialize(&net, date()).unwrap();
        let net = apply_edits(&net, &suggest_blocks(&net, &day, None).edits).unwrap();
        let day = materialize(&net, date()).unwrap();
        (net, day)
    })
}

fn rules() -> impl Strategy<Value = WorkRules> {
    work_rules().prop_map(|w| w.checked().unwrap())
}

/// No relief points, or C alone: a relief only after a trip into C.
fn constraints() -> impl Strategy<Value = Option<SchedulingConstraints>> {
    prop_oneof![Just(None), Just(Some(SchedulingConstraints { relief_points: vec!["C".into()], ..Default::default() }))]
}

fn refs(runs: &[CutRun], with_class: bool) -> Vec<RunRef> {
    runs.iter()
        .map(|r| RunRef {
            id: r.id.clone(),
            class: if with_class { r.class.clone() } else { None },
            pieces: r.pieces.iter().map(PieceRef::of).collect(),
        })
        .collect()
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 64, ..ProptestConfig::default() })]

    #[test]
    fn each_piece_is_a_stretch_of_its_block_and_each_blocked_trip_is_in_one((net, day) in day(), w in rules(), c in constraints()) {
        let places = NetworkPlaces::new(&net, &day);
        let p = cut_pieces(&net, &day, &w, c.as_ref(), DATE, Day::Fri, &places);
        let blocks = DayBlocks::new(&net, &day);
        let mut seen: BTreeMap<(String, u32), u32> = BTreeMap::new();
        for piece in &p.pieces {
            prop_assert_eq!(&blocks.resolve(&PieceRef::of(piece)).unwrap(), piece);
            for t in &piece.trips {
                *seen.entry((t.trip.clone(), t.start_secs)).or_default() += 1;
            }
        }
        prop_assert!(seen.values().all(|&n| n == 1));
        let set = judge_runs(&blocks, &[], &w, None, DATE, Day::Fri, &places).unwrap();
        prop_assert_eq!(set.uncovered.len(), seen.len(), "every piece's trips are the blocks' trips");
    }

    #[test]
    fn the_cut_is_its_pieces_composed((net, day) in day(), w in rules(), c in constraints()) {
        let places = NetworkPlaces::new(&net, &day);
        let cut = cut_runs(&net, &day, &w, c.as_ref(), DATE, Day::Fri, &places);
        let p = cut_pieces(&net, &day, &w, c.as_ref(), DATE, Day::Fri, &places);
        prop_assert_eq!(&compose_runs(&p.pieces, &w, Day::Fri, &places), &cut.runs);
        prop_assert_eq!(&p.unblocked, &cut.unblocked);
        let blocks = DayBlocks::new(&net, &day);
        let refs: Vec<PieceRef> = p.pieces.iter().map(PieceRef::of).collect();
        let set = compose_refs(&blocks, &refs, &w, DATE, Day::Fri, &places).unwrap();
        prop_assert_eq!(&set.runs, &cut.runs);
        prop_assert_eq!(&set.mix, &cut.mix);
        prop_assert!(set.doubled.is_empty() && set.uncovered.is_empty());
    }

    #[test]
    fn the_cuts_runs_judged_by_reference_are_judged_as_the_cut_judged_them(
        (net, day) in day(), w in rules(), c in constraints(), with_class in any::<bool>()
    ) {
        let places = NetworkPlaces::new(&net, &day);
        let cut = cut_runs(&net, &day, &w, c.as_ref(), DATE, Day::Fri, &places);
        let blocks = DayBlocks::new(&net, &day);
        let set = judge_runs(&blocks, &refs(&cut.runs, with_class), &w, c.as_ref(), DATE, Day::Fri, &places).unwrap();
        prop_assert_eq!(&set.runs, &cut.runs);
        prop_assert_eq!(&set.mix, &cut.mix);
        prop_assert!(set.doubled.is_empty() && set.uncovered.is_empty());
    }

    #[test]
    fn a_class_whose_conditions_a_run_misses_breaks_a_hard_rule((net, day) in day(), w in rules()) {
        let places = NetworkPlaces::new(&net, &day);
        let cut = cut_runs(&net, &day, &w, None, DATE, Day::Fri, &places);
        let blocks = DayBlocks::new(&net, &day);
        for r in &cut.runs {
            for class in &w.classes {
                let mut one = refs(std::slice::from_ref(r), false);
                one[0].class = Some(class.id.clone());
                let set = judge_runs(&blocks, &one, &w, None, DATE, Day::Fri, &places).unwrap();
                let missed = set.runs[0].judged.broken.iter().any(|f| f.rule.contains("conditions of class"));
                prop_assert_eq!(missed, !meets(&w, &class.id, &r.pieces));
                if missed {
                    prop_assert!(!set.runs[0].judged.legal());
                }
            }
        }
    }

    #[test]
    fn a_piece_twice_is_doubled_overlaps_and_is_not_legal((net, day) in day(), w in rules()) {
        let places = NetworkPlaces::new(&net, &day);
        let p = cut_pieces(&net, &day, &w, None, DATE, Day::Fri, &places);
        let blocks = DayBlocks::new(&net, &day);
        if let Some(piece) = p.pieces.first() {
            let r = RunRef { id: "x".into(), class: None, pieces: vec![PieceRef::of(piece), PieceRef::of(piece)] };
            let set = judge_runs(&blocks, &[r], &w, None, DATE, Day::Fri, &places).unwrap();
            prop_assert_eq!(set.doubled.len(), piece.trips.len());
            prop_assert!(set.runs[0].judged.broken.iter().any(|f| f.rule.contains("overlap")));
            prop_assert!(!set.runs[0].judged.legal());
        }
    }

    #[test]
    fn a_piece_cut_away_from_a_relief_point_breaks_a_hard_rule((net, day) in day(), w in rules()) {
        let places = NetworkPlaces::new(&net, &day);
        let blocks = DayBlocks::new(&net, &day);
        let c = SchedulingConstraints { relief_points: vec!["C".into()], ..Default::default() };
        let cut = cut_runs(&net, &day, &w, Some(&c), DATE, Day::Fri, &places);
        for r in &cut.runs {
            for piece in &r.pieces {
                for (k, t) in piece.trips.iter().enumerate() {
                    if k + 1 == piece.trips.len() || t.to == "C" {
                        continue;
                    }
                    let cut_short = PieceRef { last_trip: t.trip.clone(), last_start_secs: Some(t.start_secs), ..PieceRef::of(piece) };
                    let run = RunRef { id: "x".into(), class: None, pieces: vec![cut_short] };
                    let set = judge_runs(&blocks, &[run], &w, Some(&c), DATE, Day::Fri, &places).unwrap();
                    prop_assert!(set.runs[0].judged.broken.iter().any(|f| f.rule.contains("not a relief point")), "{:?}", set.runs[0].judged.broken);
                }
            }
        }
    }
}

#[test]
fn a_reference_to_a_trip_the_block_does_not_run_is_refused() {
    let net = world(&[("5", true, 6 * H), ("5", false, 7 * H)], &[]);
    let day = materialize(&net, date()).unwrap();
    let net = apply_edits(&net, &suggest_blocks(&net, &day, None).edits).unwrap();
    let day = materialize(&net, date()).unwrap();
    let blocks = DayBlocks::new(&net, &day);
    let block = net.blocks.keys().next().unwrap().clone();
    let bad = PieceRef { block: block.clone(), first_trip: "nope".into(), first_start_secs: None, last_trip: "nope".into(), last_start_secs: None };
    assert!(blocks.resolve(&bad).unwrap_err().contains("no trip nope"));
    let missing = PieceRef { block: "none".into(), ..bad };
    assert!(blocks.resolve(&missing).unwrap_err().contains("block none"));
}
