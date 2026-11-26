//! The cut's two steps alone, and runs judged whoever composed them
//! (docs/units/runs-layer.md). The cut is its pieces composed. A set of
//! runs named by reference is judged as the cut judges its own, a class is
//! chosen as the cut chooses it, and the set reports the trips it places
//! twice or not at all. A composition moves toward the mix only when the
//! least-pay one misses it, never ends further from it, and names the pay
//! it added.

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

/// The least-pay composition (the same rules with no mix), and the cut.
fn least_and_cut(net: &Network, day: &[MaterializedTrip], w: &WorkRules, places: &NetworkPlaces) -> (Cut, Cut) {
    let mut free = w.clone();
    free.mix.clear();
    (cut_runs(net, day, &free, None, DATE, Day::Fri, places), cut_runs(net, day, w, None, DATE, Day::Fri, places))
}

/// Each mix rule's distance below its minimum or above its maximum, in
/// percentage points, summed.
fn off(w: &WorkRules, runs: &[CutRun]) -> f64 {
    w.mix
        .iter()
        .map(|m| {
            let inside = |r: &&CutRun| r.class.as_ref().is_some_and(|c| m.classes.contains(c));
            let (part, whole) = match m.of {
                ShareOf::Runs => (runs.iter().filter(inside).count() as f64, runs.len() as f64),
                ShareOf::PaidTime => (
                    runs.iter().filter(inside).map(|r| r.judged.pay_secs as f64).sum(),
                    runs.iter().map(|r| r.judged.pay_secs as f64).sum(),
                ),
            };
            let share = if whole == 0.0 { 0.0 } else { part * 100.0 / whole };
            m.min_pct.map_or(0.0, |min| (min as f64 - share).max(0.0)) + m.max_pct.map_or(0.0, |max| (share - max as f64).max(0.0))
        })
        .sum()
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
        prop_assert_eq!(&compose_runs(&p.pieces, &w, Day::Fri, &places).runs, &cut.runs);
        prop_assert_eq!(&p.unblocked, &cut.unblocked);
        let blocks = DayBlocks::new(&net, &day);
        let refs: Vec<PieceRef> = p.pieces.iter().map(PieceRef::of).collect();
        let set = compose_refs(&blocks, &refs, &w, DATE, Day::Fri, &places).unwrap();
        prop_assert_eq!(&set.runs, &cut.runs);
        prop_assert_eq!(&set.mix, &cut.mix);
        prop_assert_eq!(set.mix_cost_secs, Some(cut.mix_cost_secs));
        prop_assert!(set.doubled.is_empty() && set.uncovered.is_empty());
    }

    #[test]
    fn a_least_pay_composition_that_meets_the_mix_is_kept_at_no_cost((net, day) in day(), w in rules()) {
        let places = NetworkPlaces::new(&net, &day);
        let (least, cut) = least_and_cut(&net, &day, &w, &places);
        let blocks = DayBlocks::new(&net, &day);
        let judged = judge_runs(&blocks, &refs(&least.runs, true), &w, None, DATE, Day::Fri, &places).unwrap();
        if judged.mix.iter().all(|m| m.met) {
            prop_assert_eq!(&cut.runs, &least.runs);
            prop_assert_eq!(cut.mix_cost_secs, 0);
        }
    }

    #[test]
    fn a_composition_stops_short_of_the_mix_only_when_no_move_closes_any_of_it((net, day) in day(), w in rules()) {
        let places = NetworkPlaces::new(&net, &day);
        let cut = cut_runs(&net, &day, &w, None, DATE, Day::Fri, &places);
        if cut.mix.iter().all(|m| m.met) {
            return Ok(());
        }
        let now = off(&w, &cut.runs);
        let clean = |pieces: &[Piece], class: &str| {
            meets(&w, class, pieces).then(|| judge(&w, Some(class), pieces, Day::Fri, &places)).filter(|j| j.broken.is_empty())
        };
        let made = |pieces: Vec<Piece>, class: &str, judged: Judged| CutRun { id: String::new(), class: Some(class.into()), pieces, judged };
        for (i, r) in cut.runs.iter().enumerate() {
            for c in w.classes.iter().filter(|c| r.class.as_deref() != Some(c.id.as_str())) {
                if let Some(j) = clean(&r.pieces, &c.id) {
                    let mut after = cut.runs.clone();
                    after[i] = made(r.pieces.clone(), &c.id, j);
                    prop_assert!(off(&w, &after) >= now - 1e-9, "giving run {} class {} closes some of the mix", r.id, c.id);
                }
            }
            for (k, q) in cut.runs.iter().enumerate() {
                if r.pieces.len() != 1 || q.pieces.len() != 1 || i == k || q.pieces[0].start_secs() < r.pieces[0].end_secs() {
                    continue;
                }
                let pair = vec![r.pieces[0].clone(), q.pieces[0].clone()];
                for c in &w.classes {
                    if let Some(j) = clean(&pair, &c.id) {
                        let mut after: Vec<CutRun> = cut.runs.iter().enumerate().filter(|(n, _)| *n != i && *n != k).map(|(_, r)| r.clone()).collect();
                        after.push(made(pair.clone(), &c.id, j));
                        prop_assert!(off(&w, &after) >= now - 1e-9, "merging runs {} and {} as {} closes some of the mix", r.id, q.id, c.id);
                    }
                }
            }
        }
    }

    #[test]
    fn the_mix_cost_is_the_pay_the_moves_added((net, day) in day(), w in rules()) {
        let places = NetworkPlaces::new(&net, &day);
        let (least, cut) = least_and_cut(&net, &day, &w, &places);
        let pay = |c: &Cut| c.runs.iter().map(|r| r.judged.pay_secs as i64).sum::<i64>();
        prop_assert_eq!(cut.mix_cost_secs, pay(&cut) - pay(&least));
        prop_assert_eq!(least.mix_cost_secs, 0);
    }

    #[test]
    fn a_composition_is_never_further_from_the_mix_than_the_least_pay_one((net, day) in day(), w in rules()) {
        let places = NetworkPlaces::new(&net, &day);
        let (least, cut) = least_and_cut(&net, &day, &w, &places);
        prop_assert!(off(&w, &cut.runs) <= off(&w, &least.runs) + 1e-9, "{} > {}", off(&w, &cut.runs), off(&w, &least.runs));
        let kept = |r: &CutRun| least.runs.iter().any(|l| (&l.pieces, &l.class) == (&r.pieces, &r.class));
        for r in cut.runs.iter().filter(|r| !kept(r)) {
            prop_assert!(r.judged.broken.is_empty(), "a run made for the mix breaks {:?}", r.judged.broken);
        }
    }

    #[test]
    fn the_cuts_runs_judged_by_reference_with_their_classes_are_judged_as_the_cut_judged_them(
        (net, day) in day(), w in rules(), c in constraints()
    ) {
        let places = NetworkPlaces::new(&net, &day);
        let cut = cut_runs(&net, &day, &w, c.as_ref(), DATE, Day::Fri, &places);
        let blocks = DayBlocks::new(&net, &day);
        let set = judge_runs(&blocks, &refs(&cut.runs, true), &w, c.as_ref(), DATE, Day::Fri, &places).unwrap();
        prop_assert_eq!(&set.runs, &cut.runs);
        prop_assert_eq!(&set.mix, &cut.mix);
        prop_assert!(set.doubled.is_empty() && set.uncovered.is_empty());
    }

    /// With no class given, a run takes its least-pay class, so the
    /// least-pay cut's runs come back as it made them.
    #[test]
    fn the_least_pay_cuts_runs_judged_with_no_class_take_the_classes_it_gave((net, day) in day(), w in rules(), c in constraints()) {
        let places = NetworkPlaces::new(&net, &day);
        let mut free = w.clone();
        free.mix.clear();
        let least = cut_runs(&net, &day, &free, c.as_ref(), DATE, Day::Fri, &places);
        let blocks = DayBlocks::new(&net, &day);
        let set = judge_runs(&blocks, &refs(&least.runs, false), &w, c.as_ref(), DATE, Day::Fri, &places).unwrap();
        prop_assert_eq!(&set.runs, &least.runs);
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
