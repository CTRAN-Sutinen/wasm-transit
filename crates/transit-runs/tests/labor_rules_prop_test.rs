//! The work rules model over generated documents that sample every rule
//! type, none or many of each (docs/units/runs-layer.md, "The labor rules
//! model"). Order is not identity, a name the document lacks is refused,
//! a class's rules replace or add as it says, and a gap is paid by its
//! band.

use std::collections::BTreeMap;

use proptest::prelude::*;
use proptest::sample::{select, subsequence};
use transit_runs::*;

const MEASURES: [&str; 3] = ["m0", "m1", "m2"];
const ACTIVITIES: [Activity; 8] = [
    Activity::Report,
    Activity::Clear,
    Activity::Revenue,
    Activity::Deadhead,
    Activity::Layover,
    Activity::Travel,
    Activity::Standby,
    Activity::Gap,
];
const EVENTS: [Event; 6] = [Event::PullOut, Event::PullIn, Event::ReliefOn, Event::ReliefOff, Event::PieceStart, Event::PieceEnd];
const DAYS: [Day; 7] = [Day::Mon, Day::Tue, Day::Wed, Day::Thu, Day::Fri, Day::Sat, Day::Sun];
const KINDS: [RuleKind; 7] =
    [RuleKind::Allowances, RuleKind::Gaps, RuleKind::Limits, RuleKind::Breaks, RuleKind::Premiums, RuleKind::Guarantees, RuleKind::Travel];

fn measure() -> impl Strategy<Value = String> {
    select(&MEASURES[..]).prop_map(String::from)
}

fn allowance() -> impl Strategy<Value = Allowance> {
    (select(&EVENTS[..]), 1u32..3600, select(&ACTIVITIES[..]))
        .prop_map(|(event, secs, activity)| Allowance { event, secs, activity, cites: None, extra: Default::default() })
}

fn gaps() -> impl Strategy<Value = Vec<GapBand>> {
    prop_oneof![
        Just(vec![]),
        (prop::collection::btree_set(1u32..20_000, 0..3), prop::collection::vec(0u32..=100, 4)).prop_map(|(froms, pcts)| {
            std::iter::once(0)
                .chain(froms)
                .zip(pcts)
                .map(|(from_secs, paid_pct)| GapBand { from_secs, paid_pct, cites: None, extra: Default::default() })
                .collect()
        }),
    ]
}

fn limit() -> impl Strategy<Value = Limit> {
    let quantity = select(&[Quantity::Measure, Quantity::Spread, Quantity::Piece, Quantity::Pieces, Quantity::Gap, Quantity::DaysWorked, Quantity::Rest][..]);
    (quantity, measure(), 0u32..50_000, 0u32..50_000, 0usize..3, any::<bool>(), any::<bool>()).prop_map(|(quantity, m, a, b, bounds, cross, waivable)| {
        let (lo, hi) = (a.min(b), a.max(b));
        let (min, max) = match bounds {
            0 => (Some(lo), None),
            1 => (None, Some(hi)),
            _ => (Some(lo), Some(hi)),
        };
        let over = match quantity {
            Quantity::DaysWorked | Quantity::Rest => if cross { Window::Week } else { Window::Rolling24h },
            Quantity::Measure | Quantity::Spread if cross => Window::Week,
            _ => Window::Run,
        };
        Limit {
            quantity,
            measure: (quantity == Quantity::Measure).then_some(m),
            min,
            max,
            over,
            strength: if waivable { Strength::Waivable } else { Strength::Hard },
            cites: None,
            extra: Default::default(),
        }
    })
}

fn break_rule() -> impl Strategy<Value = BreakRule> {
    (1u32..3600, 0u32..20_000, 1u32..20_000, proptest::option::of(measure()), 1u32..4, any::<bool>(), proptest::option::of((measure(), 0u32..40_000)))
        .prop_map(|(min_secs, from, len, start, count, paid, when)| BreakRule {
            min_secs,
            window_from_secs: from,
            window_to_secs: from + len,
            from: if start.is_some() { BreakFrom::MeasureStart } else { BreakFrom::Report },
            measure: start,
            count,
            paid,
            when_measure: when.as_ref().map(|w| w.0.clone()),
            when_over_secs: when.map(|w| w.1),
            cites: None,
            extra: Default::default(),
        })
}

fn premium(classes: Vec<String>) -> impl Strategy<Value = Premium> {
    let ons: Vec<TriggerOn> = [TriggerOn::Measure, TriggerOn::Spread, TriggerOn::TimeOfDay, TriggerOn::DayOfWeek]
        .into_iter()
        .chain((!classes.is_empty()).then_some(TriggerOn::Class))
        .collect();
    let class = if classes.is_empty() { Just(String::new()).boxed() } else { select(classes).boxed() };
    (select(ons), measure(), measure(), 0u32..=1000, 0u32..80_000, 1u32..20_000, subsequence(&DAYS[..], 1..=7), class).prop_map(
        |(on, pays, m, pct, at, len, days, class)| {
            let mut p = Premium {
                on,
                pays,
                pct,
                measure: None,
                after_secs: None,
                from_secs: None,
                to_secs: None,
                days: vec![],
                class: None,
                cites: None,
                extra: Default::default(),
            };
            match on {
                TriggerOn::Measure => (p.measure, p.after_secs) = (Some(m), Some(at)),
                TriggerOn::Spread => p.after_secs = Some(at),
                TriggerOn::TimeOfDay => (p.from_secs, p.to_secs) = (Some(at), Some(at + len)),
                TriggerOn::DayOfWeek => p.days = days,
                TriggerOn::Class => p.class = Some(class),
            }
            p
        },
    )
}

fn guarantee() -> impl Strategy<Value = Guarantee> {
    (select(&[Per::Run, Per::Report, Per::Day, Per::Week][..]), 1u32..40_000, measure())
        .prop_map(|(per, min_secs, counts)| Guarantee { per, min_secs, counts, cites: None, extra: Default::default() })
}

fn travel() -> impl Strategy<Value = TravelRule> {
    (select(&[TravelAt::DayEnd, TravelAt::BetweenPieces][..]), 0u32..10_000, 0u32..2_000, 0usize..3, 1u32..3600, 1u32..100, proptest::option::of(0u32..=1000))
        .prop_map(|(at, beyond_m, within_m, how, fixed, kph, pct)| {
            let (timed, fixed_secs, speed_kph) = match how {
                0 => (Timing::NextTrip, None, None),
                1 => (Timing::Fixed, Some(fixed), None),
                _ => (Timing::Speed, None, Some(kph)),
            };
            TravelRule { at, beyond_m, within_m, timed, fixed_secs, speed_kph, pct, cites: None, extra: Default::default() }
        })
}

fn dedup<T: Ord>(mut v: Vec<T>) -> Vec<T> {
    v.sort();
    v.dedup();
    v
}

fn rule_set(classes: Vec<String>) -> impl Strategy<Value = RuleSet> {
    (
        prop::collection::vec(allowance(), 0..3),
        gaps(),
        prop::collection::vec(limit(), 0..4),
        prop::collection::vec(break_rule(), 0..3),
        prop::collection::vec(premium(classes), 0..3),
        prop::collection::vec(guarantee(), 0..3),
        prop::collection::vec(travel(), 0..2),
    )
        .prop_map(|(allowances, gaps, limits, breaks, premiums, guarantees, travel)| RuleSet {
            allowances: dedup(allowances),
            gaps,
            limits: dedup(limits),
            breaks: dedup(breaks),
            premiums: dedup(premiums),
            guarantees: dedup(guarantees),
            travel: dedup(travel),
        })
}

fn class(id: String, ids: Vec<String>) -> impl Strategy<Value = RunClass> {
    (prop::collection::vec(limit(), 0..3), subsequence(&KINDS[..], 0..=7), rule_set(ids)).prop_map(move |(conditions, replaces, rules)| {
        let replaces = replaces.into_iter().filter(|&k| k != RuleKind::Gaps || !rules.gaps.is_empty()).collect();
        RunClass { id: id.clone(), conditions: dedup(conditions), replaces, rules, cites: None, extra: Default::default() }
    })
}

fn mix(ids: Vec<String>) -> impl Strategy<Value = Vec<MixRule>> {
    if ids.is_empty() {
        return Just(vec![]).boxed();
    }
    prop::collection::vec((subsequence(ids, 1..=1), prop::collection::vec(0u32..=100, 3), 1usize..8, any::<bool>()), 0..3)
        .prop_map(|rules| {
            let rules = rules.into_iter().map(|(classes, mut pcts, which, paid)| {
                pcts.sort();
                let pick = |i: usize, v: u32| (which >> i & 1 == 1).then_some(v);
                MixRule {
                    classes,
                    of: if paid { ShareOf::PaidTime } else { ShareOf::Runs },
                    min_pct: pick(0, pcts[0]),
                    target_pct: pick(1, pcts[1]),
                    max_pct: pick(2, pcts[2]),
                    cites: None,
                    extra: Default::default(),
                }
            });
            dedup(rules.collect())
        })
        .boxed()
}

fn work_rules() -> impl Strategy<Value = WorkRules> {
    (0usize..4).prop_flat_map(|k| {
        let ids: Vec<String> = (0..k).map(|i| format!("c{i}")).collect();
        let measures = prop::collection::vec(subsequence(&ACTIVITIES[..], 1..=8), 3).prop_map(|lists| {
            MEASURES.iter().map(|m| m.to_string()).zip(lists).collect::<BTreeMap<_, _>>()
        });
        let classes = ids.iter().map(|id| class(id.clone(), ids.clone())).collect::<Vec<_>>();
        let maximize = if ids.is_empty() { Just(vec![]).boxed() } else { subsequence(ids.clone(), 0..=ids.len()).boxed() };
        (measures, rule_set(ids.clone()), any::<bool>(), classes, mix(ids), proptest::option::of(maximize), any::<bool>()).prop_map(
            |(measures, rules, sum, classes, mix, maximize, minimize_premium)| WorkRules {
                measures,
                rules,
                combine: if sum { Combine::Sum } else { Combine::Highest },
                classes,
                mix,
                objective: maximize.map(|maximize| Objective { maximize, minimize_premium, extra: Default::default() }),
                ..Default::default()
            },
        )
    })
}

fn reversed(mut w: WorkRules) -> WorkRules {
    fn rev(r: &mut RuleSet) {
        r.allowances.reverse();
        r.gaps.reverse();
        r.limits.reverse();
        r.breaks.reverse();
        r.premiums.reverse();
        r.guarantees.reverse();
        r.travel.reverse();
        for p in &mut r.premiums {
            p.days.reverse();
        }
    }
    for list in w.measures.values_mut() {
        list.reverse();
    }
    rev(&mut w.rules);
    w.classes.reverse();
    for c in &mut w.classes {
        c.conditions.reverse();
        c.replaces.reverse();
        rev(&mut c.rules);
    }
    w.mix.reverse();
    for m in &mut w.mix {
        m.classes.reverse();
    }
    if let Some(o) = &mut w.objective {
        o.maximize.reverse();
    }
    w
}

fn names_measure(r: &RuleSet, m: &str) -> bool {
    let is = |x: &Option<String>| x.as_deref() == Some(m);
    r.limits.iter().any(|l| is(&l.measure))
        || r.breaks.iter().any(|b| is(&b.measure) || is(&b.when_measure))
        || r.premiums.iter().any(|p| p.pays == m || is(&p.measure))
        || r.guarantees.iter().any(|g| g.counts == m)
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 256, ..ProptestConfig::default() })]

    #[test]
    fn a_generated_document_passes_and_order_is_not_identity(w in work_rules()) {
        let checked = w.clone().checked().map_err(TestCaseError::fail)?;
        let again = reversed(w).checked().map_err(TestCaseError::fail)?;
        prop_assert_eq!(&again, &checked);
        prop_assert_eq!(checked.clone().checked().unwrap(), checked.clone(), "checked is idempotent");
        let text = serde_json::to_string(&checked).unwrap();
        let back: WorkRules = serde_json::from_str(&text).unwrap();
        prop_assert_eq!(back, checked);
    }

    #[test]
    fn a_measure_the_document_lacks_is_refused(w in work_rules(), which in select(&MEASURES[..])) {
        let named = names_measure(&w.rules, which) || w.classes.iter().any(|c| names_measure(&c.rules, which) || c.conditions.iter().any(|l| l.measure.as_deref() == Some(which)));
        prop_assume!(named);
        let mut w = w;
        w.measures.remove(which);
        let err = w.checked().unwrap_err();
        prop_assert!(err.contains(&format!("{which:?}")) && err.contains("does not define"), "{}", err);
    }

    #[test]
    fn a_class_the_document_lacks_is_refused(w in work_rules()) {
        prop_assume!(!w.classes.is_empty());
        let named = |id: &str| w.mix.iter().any(|m| m.classes.iter().any(|c| c == id))
            || w.objective.as_ref().is_some_and(|o| o.maximize.iter().any(|c| c == id))
            || w.rules.premiums.iter().chain(w.classes.iter().filter(|c| c.id != id).flat_map(|c| &c.rules.premiums)).any(|p| p.class.as_deref() == Some(id));
        let gone = w.classes.iter().map(|c| c.id.clone()).find(|id| named(id));
        prop_assume!(gone.is_some());
        let gone = gone.unwrap();
        let mut w = w;
        w.classes.retain(|c| c.id != gone);
        let err = w.checked().unwrap_err();
        prop_assert!(err.contains("does not define"), "{}", err);
    }

    #[test]
    fn a_class_replaces_what_it_names_and_adds_the_rest(w in work_rules()) {
        let w = w.checked().map_err(TestCaseError::fail)?;
        prop_assert_eq!(w.rules_for(None), w.rules.clone());
        prop_assert_eq!(w.rules_for(Some("no such class")), w.rules.clone());
        for c in &w.classes {
            let got = w.rules_for(Some(&c.id));
            fn expect<T: Clone + Ord + std::fmt::Debug>(base: &[T], class: &[T], replace: bool, got: &[T]) -> Result<(), TestCaseError> {
                let mut want: Vec<T> = if replace { vec![] } else { base.to_vec() };
                want.extend(class.iter().cloned());
                want.sort();
                want.dedup();
                prop_assert_eq!(got, &want[..]);
                Ok(())
            }
            let r = |k: RuleKind| c.replaces.contains(&k);
            expect(&w.rules.allowances, &c.rules.allowances, r(RuleKind::Allowances), &got.allowances)?;
            expect(&w.rules.gaps, &c.rules.gaps, r(RuleKind::Gaps) || !c.rules.gaps.is_empty(), &got.gaps)?;
            expect(&w.rules.limits, &c.rules.limits, r(RuleKind::Limits), &got.limits)?;
            expect(&w.rules.breaks, &c.rules.breaks, r(RuleKind::Breaks), &got.breaks)?;
            expect(&w.rules.premiums, &c.rules.premiums, r(RuleKind::Premiums), &got.premiums)?;
            expect(&w.rules.guarantees, &c.rules.guarantees, r(RuleKind::Guarantees), &got.guarantees)?;
            expect(&w.rules.travel, &c.rules.travel, r(RuleKind::Travel), &got.travel)?;
        }
    }

    #[test]
    fn a_gap_is_paid_by_the_last_band_it_reaches(bands in gaps(), gap in 0u32..30_000) {
        let rules = RuleSet { gaps: bands.clone(), ..Default::default() };
        let want = bands.iter().filter(|b| b.from_secs <= gap).max_by_key(|b| b.from_secs).map_or(0, |b| b.paid_pct);
        prop_assert_eq!(rules.gap_paid_pct(gap), want);
    }
}

#[test]
fn each_refusal_names_what_is_wrong() {
    let refused = |w: serde_json::Value| -> String {
        let mut doc = serde_json::json!({"schema_version": SCHEMA_VERSION, "measures": {"worked": ["revenue"]}});
        doc.as_object_mut().unwrap().extend(w.as_object().unwrap().clone());
        serde_json::from_value::<WorkRules>(doc).unwrap().checked().unwrap_err()
    };
    let cases = [
        (serde_json::json!({"measures": {"worked": []}}), "holds no activity"),
        (serde_json::json!({"gaps": [{"from_secs": 60, "paid_pct": 0}]}), "not 0"),
        (serde_json::json!({"gaps": [{"from_secs": 0, "paid_pct": 101}]}), "at most 100"),
        (serde_json::json!({"limits": [{"quantity": "spread"}]}), "neither min nor max"),
        (serde_json::json!({"limits": [{"quantity": "spread", "min": 2, "max": 1}]}), "over max"),
        (serde_json::json!({"limits": [{"quantity": "spread", "measure": "worked", "max": 1}]}), "only a measure limit"),
        (serde_json::json!({"limits": [{"quantity": "rest", "min": 1}]}), "spans days"),
        (serde_json::json!({"limits": [{"quantity": "gap", "max": 1, "over": "week"}]}), "over a run only"),
        (serde_json::json!({"breaks": [{"min_secs": 600, "window_from_secs": 10, "window_to_secs": 10}]}), "ends at or before"),
        (serde_json::json!({"breaks": [{"min_secs": 600, "window_from_secs": 0, "window_to_secs": 10, "when_over_secs": 5}]}), "half a trigger"),
        (serde_json::json!({"premiums": [{"on": "spread", "pays": "worked", "pct": 150}]}), "lacks one it needs"),
        (serde_json::json!({"premiums": [{"on": "spread", "after_secs": 1, "pays": "worked", "pct": 1001}]}), "at most 1000"),
        (serde_json::json!({"guarantees": [{"per": "run", "min_secs": 0, "counts": "worked"}]}), "guarantees nothing"),
        (serde_json::json!({"travel": [{"at": "day_end", "beyond_m": 1, "within_m": 1, "timed": "fixed"}]}), "other timing fields"),
        (serde_json::json!({"classes": [{"id": "A"}, {"id": "A"}]}), "given twice"),
        (serde_json::json!({"classes": [{"id": "A", "replaces": ["gaps"]}]}), "with none"),
        (serde_json::json!({"mix": [{"classes": ["A"], "min_pct": 1}]}), "does not define"),
        (serde_json::json!({"classes": [{"id": "A"}], "mix": [{"classes": ["A"], "min_pct": 50, "max_pct": 10}]}), "out of order"),
        (serde_json::json!({"source": {"title": "t", "effective_from": "2025-01-01", "effective_to": "2024-01-01"}}), "ends before it starts"),
        (serde_json::json!({"source": {"title": "t", "effective_from": "2025-1-1"}}), "invalid date"),
        (serde_json::json!({"allowances": [{"event": "pull_out", "secs": 60, "activity": "report"}, {"event": "pull_out", "secs": 60, "activity": "report"}]}), "given twice"),
    ];
    for (doc, want) in cases {
        let err = refused(doc.clone());
        assert!(err.contains(want), "{doc}: {err}");
    }
}
