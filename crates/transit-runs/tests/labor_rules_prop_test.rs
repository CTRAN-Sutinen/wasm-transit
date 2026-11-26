//! The work rules model over generated documents that sample every rule
//! type, none or many of each (docs/units/runs-layer.md, "The labor rules
//! model"). Order is not identity, a name the document lacks is refused,
//! a class's rules replace or add as it says, and a gap is paid by its
//! band.

mod common;

use common::rules_gen::*;
use proptest::prelude::*;
use proptest::sample::select;
use transit_runs::*;

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
    #![proptest_config(ProptestConfig { cases: 256, max_global_rejects: 100_000, ..ProptestConfig::default() })]

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
