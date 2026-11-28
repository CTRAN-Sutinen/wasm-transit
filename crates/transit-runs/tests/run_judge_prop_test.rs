//! A run judged and paid under generated work rules
//! (docs/units/runs-layer.md, "Testing"). The timeline runs without a hole
//! from the first report to the last clear, a measure totals its
//! activities, pay is at least each guarantee, premiums raise pay no more
//! than their rates allow, every limit a run breaks is named, and a gap is
//! paid by its band.

mod common;

use common::rules_gen::*;
use proptest::prelude::*;
use transit_runs::*;

struct Grid {
    trips_home: bool,
}

impl Places for Grid {
    fn meters(&self, a: &str, b: &str) -> Option<f64> {
        let n = |s: &str| s.trim_start_matches('S').parse::<i32>().ok();
        Some((n(a)? - n(b)?).abs() as f64 * 1_000.0)
    }

    fn next_trip_home(
        &self,
        _from: &str,
        after_secs: u32,
        _home: &str,
        _within_m: u32,
    ) -> Option<u32> {
        self.trips_home.then_some(after_secs + 900)
    }
}

fn pieces() -> impl Strategy<Value = Vec<Piece>> {
    let trip = (600u32..3_600, 0u32..1_800, 0usize..4);
    let drive = proptest::option::of(60u32..1_800);
    let piece = (
        prop::collection::vec(trip, 1..4),
        0u32..14_400,
        any::<bool>(),
        any::<bool>(),
        drive.clone(),
        drive,
    );
    (14_400u32..36_000, prop::collection::vec(piece, 1..4)).prop_map(|(mut at, list)| {
        let mut out = Vec::new();
        let mut stop = 0usize;
        for (i, (trips, gap, pull_out, pull_in, out_secs, in_secs)) in list.into_iter().enumerate()
        {
            if i > 0 {
                at += gap;
            }
            let mut piece = Piece {
                block: format!("B{i}"),
                pull_out,
                pull_in,
                garage: (pull_out || pull_in).then(|| "G".to_string()),
                deadhead_out_secs: out_secs.filter(|_| pull_out),
                deadhead_in_secs: in_secs.filter(|_| pull_in),
                ..Default::default()
            };
            for (j, (run, layover, next)) in trips.into_iter().enumerate() {
                if j > 0 {
                    at += layover;
                }
                piece.trips.push(PieceTrip {
                    trip: format!("t{i}.{j}"),
                    start_secs: at,
                    end_secs: at + run,
                    from: format!("S{stop}"),
                    to: format!("S{next}"),
                });
                stop = next;
                at += run;
            }
            out.push(piece);
        }
        out
    })
}

fn day() -> impl Strategy<Value = Day> {
    proptest::sample::select(&DAYS[..])
}

fn class_of(w: &WorkRules) -> impl Strategy<Value = Option<String>> {
    let ids: Vec<Option<String>> = std::iter::once(None)
        .chain(w.classes.iter().map(|c| Some(c.id.clone())))
        .collect();
    proptest::sample::select(ids)
}

fn judged() -> impl Strategy<Value = (WorkRules, Option<String>, Vec<Piece>, Day, bool)> {
    work_rules()
        .prop_map(|w| w.checked().expect("a generated document passes"))
        .prop_flat_map(|w| {
            let class = class_of(&w);
            (Just(w), class, pieces(), day(), any::<bool>())
        })
}

fn len(s: &Span) -> u32 {
    s.to_secs - s.from_secs
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 256, ..ProptestConfig::default() })]

    #[test]
    fn the_timeline_has_no_hole_and_carries_every_trip(w in work_rules().prop_map(|w| w.checked().unwrap()), pieces in pieces()) {
        let t = timeline(&pieces, &w.rules);
        for pair in t.windows(2) {
            prop_assert_eq!(pair[0].to_secs, pair[1].from_secs, "{:?}", t);
        }
        prop_assert!(t.iter().all(|s| s.to_secs > s.from_secs), "no empty span: {:?}", t);
        for tr in pieces.iter().flat_map(|p| &p.trips) {
            let covered: u32 = t.iter().filter(|s| s.activity == Activity::Revenue)
                .map(|s| s.to_secs.min(tr.end_secs).saturating_sub(s.from_secs.max(tr.start_secs))).sum();
            prop_assert_eq!(covered, tr.end_secs - tr.start_secs, "{} is revenue throughout: {:?}", tr.trip, t);
        }
        prop_assert!(t.first().unwrap().from_secs <= pieces[0].start_secs());
        prop_assert!(t.last().unwrap().to_secs >= pieces.last().unwrap().end_secs());
    }

    #[test]
    fn the_run_view_reads_the_timeline_and_the_pieces((w, class, pieces, day, home) in judged()) {
        let j = judge(&w, class.as_deref(), &pieces, day, &Grid { trips_home: home });
        let (first, last) = (pieces.first().unwrap(), pieces.last().unwrap());
        prop_assert_eq!(j.report.secs, j.timeline.first().unwrap().from_secs);
        prop_assert_eq!(j.clear.secs, j.timeline.last().unwrap().to_secs);
        let want = if first.pull_out { first.garage.clone() } else { first.trips.first().map(|t| t.from.clone()) };
        prop_assert_eq!(&j.report.place, &want);
        let want = if last.pull_in { last.garage.clone() } else { last.trips.last().map(|t| t.to.clone()) };
        prop_assert_eq!(&j.clear.place, &want);
        let on = [Activity::Revenue, Activity::Deadhead, Activity::Layover];
        prop_assert_eq!(j.platform_secs, j.timeline.iter().filter(|s| on.contains(&s.activity)).map(len).sum::<u32>());
    }

    /// A drive that is not known is never counted as none: the run says
    /// which it lacks.
    #[test]
    fn a_deadhead_is_driven_when_known_and_named_when_not((mut w, class, pieces, day, home) in judged()) {
        // An allowance may be deadhead time too; only the drives are counted here.
        w.rules.allowances.retain(|a| a.activity != Activity::Deadhead);
        for c in &mut w.classes {
            c.rules.allowances.retain(|a| a.activity != Activity::Deadhead);
        }
        let j = judge(&w, class.as_deref(), &pieces, day, &Grid { trips_home: home });
        let ends: Vec<(bool, Option<u32>)> = pieces.iter().flat_map(|p| [(p.pull_out, p.deadhead_out_secs), (p.pull_in, p.deadhead_in_secs)]).filter(|e| e.0).collect();
        let unknown = ends.iter().filter(|e| e.1.is_none()).count();
        let named = j.not_judged.iter().filter(|f| f.rule.starts_with("the deadhead")).count();
        prop_assert_eq!(named, unknown, "{:?}", j.not_judged);
        prop_assert_eq!(j.deadhead_secs.is_some(), unknown == 0);
        let driven: u32 = j.timeline.iter().filter(|s| s.activity == Activity::Deadhead).map(len).sum();
        prop_assert!(driven <= ends.iter().filter_map(|e| e.1).sum::<u32>());
        let first = &pieces[0];
        if let (true, Some(d)) = (first.pull_out, first.deadhead_out_secs) {
            let span = j.timeline.iter().find(|s| s.activity == Activity::Deadhead).unwrap();
            prop_assert_eq!((span.from_secs, span.to_secs), (first.start_secs() - d, first.start_secs()), "the first drive out is whole");
        }
    }

    #[test]
    fn a_measure_totals_its_activities((w, class, pieces, day, home) in judged()) {
        let j = judge(&w, class.as_deref(), &pieces, day, &Grid { trips_home: home });
        for (m, activities) in &w.measures {
            let want: u32 = j.timeline.iter().filter(|s| activities.contains(&s.activity)).map(len).sum();
            prop_assert_eq!(j.measures[m], want, "{}", m);
        }
        prop_assert_eq!(j.spread_secs, j.timeline.last().unwrap().to_secs - j.timeline.first().unwrap().from_secs);
        prop_assert_eq!(j.pay_secs, j.paid_secs + j.premium_secs + j.guarantee_secs);
    }

    #[test]
    fn pay_is_at_least_each_guarantee((w, class, pieces, day, home) in judged()) {
        let j = judge(&w, class.as_deref(), &pieces, day, &Grid { trips_home: home });
        for g in &w.rules_for(class.as_deref()).guarantees {
            if g.per != Per::Week {
                prop_assert!(j.pay_secs >= g.min_secs, "{:?}: pay {} under {}", g, j.pay_secs, g.min_secs);
            }
        }
    }

    #[test]
    fn premiums_raise_pay_no_more_than_their_rates((w, class, pieces, day, home) in judged()) {
        let j = judge(&w, class.as_deref(), &pieces, day, &Grid { trips_home: home });
        let rates: Vec<u64> = w.rules_for(class.as_deref()).premiums.iter().map(|p| p.pct as u64).collect();
        let most = match w.combine {
            Combine::Highest => rates.iter().copied().max().unwrap_or(100).max(100),
            Combine::Sum => 100 + rates.iter().map(|r| r.saturating_sub(100)).sum::<u64>(),
        };
        let weighted = (j.paid_secs + j.premium_secs) as u64;
        // Pay is rounded down after weighting, so the bound allows one second
        // at the highest rate.
        prop_assert!(weighted <= (j.paid_secs as u64 + 1) * most / 100, "{} over {} at {}%", weighted, j.paid_secs, most);
        if rates.is_empty() {
            prop_assert_eq!(j.premium_secs, 0);
        }
    }

    #[test]
    fn every_limit_a_run_breaks_is_named((w, class, pieces, day) in judged().prop_map(|(w, c, p, d, _)| (w, c, p, d))) {
        let j = judge(&w, class.as_deref(), &pieces, day, &Grid { trips_home: false });
        let set = w.rules_for(class.as_deref());
        let mut want = 0;
        for l in set.limits.iter().filter(|l| l.over == Window::Run) {
            let values: Vec<u32> = match l.quantity {
                Quantity::Measure => vec![j.measures[l.measure.as_ref().unwrap()]],
                Quantity::Spread => vec![j.spread_secs],
                Quantity::Piece => pieces.iter().map(|p| p.end_secs() - p.start_secs()).collect(),
                Quantity::Pieces => vec![pieces.len() as u32],
                Quantity::Gap => j.timeline.iter().filter(|s| s.activity == Activity::Gap).map(len).collect(),
                Quantity::DaysWorked | Quantity::Rest => vec![],
            };
            if values.iter().any(|&v| l.max.is_some_and(|m| v > m) || l.min.is_some_and(|m| v < m)) {
                want += 1;
            }
        }
        let named = j.broken.iter().filter(|f| !f.rule.contains("break of")).count();
        prop_assert_eq!(named, want, "{:?}", j.broken);
        let hard = j.broken.iter().any(|f| f.strength == Strength::Hard);
        prop_assert_eq!(j.legal(), !hard);
    }

    #[test]
    fn a_gap_is_paid_at_its_bands_share(bands in gaps(), first in 600u32..7_200, gap in 0u32..20_000, second in 600u32..7_200) {
        let w = WorkRules { rules: RuleSet { gaps: bands.clone(), ..Default::default() }, ..Default::default() }.checked().unwrap();
        let trip = |name: &str, start: u32, run: u32| PieceTrip { trip: name.into(), start_secs: start, end_secs: start + run, from: "S0".into(), to: "S0".into() };
        let start = 6 * 3_600;
        let pieces = vec![
            Piece { block: "B1".into(), trips: vec![trip("a", start, first)], pull_out: true, pull_in: true, ..Default::default() },
            Piece { block: "B2".into(), trips: vec![trip("b", start + first + gap, second)], pull_out: true, pull_in: true, ..Default::default() },
        ];
        let j = judge(&w, None, &pieces, Day::Mon, &Grid { trips_home: false });
        let pct = w.rules.gap_paid_pct(gap);
        prop_assert_eq!(j.paid_secs, first + second + ((gap as u64 * pct as u64) / 100) as u32);
    }
}
