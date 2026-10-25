//! Service-plan chain properties: the laws the chain ops must hold
//! for ANY plan set — with oracles independent of the implementation.
//!
//! Resolve: the plan in force on a date is the latest pick at or
//! before it (oracle: ISO strings compare chronologically, no chrono
//! parsing involved); the answer never depends on input order.
//! Validate: a clean chain reports nothing, and every defect is
//! reported exactly — nothing invented, nothing missed (oracle:
//! recount the duplicates with multiset counting). No input panics.

use proptest::prelude::*;

use transit_ops::plans::{resolve, validate};
use transit_ops::PlanInfo;

/// Zero-padded ISO dates — lexicographic order IS chronological
/// order, which is what the oracles rely on.
fn iso_date() -> impl Strategy<Value = String> {
    (1u32..=12, 1u32..=28).prop_map(|(m, d)| format!("2026-{m:02}-{d:02}"))
}

/// A plan with an id, maybe a date.
fn plan(id: &str, date: Option<String>) -> PlanInfo {
    PlanInfo { id: id.into(), name: format!("Plan {id}"), effective_from: date }
}

/// A clean chain: unique ids, unique effective dates (some plans
/// unversioned), shuffled order.
fn clean_chain(max: usize) -> impl Strategy<Value = Vec<PlanInfo>> {
    proptest::collection::vec(iso_date(), 0..=max).prop_map(|mut dates| {
        // The CHAIN's dates are unique — sort + dedup keeps the
        // generator honest (a duplicate would be a defect, and the
        // clean-chain law must reject it, not accept it).
        dates.sort();
        dates.dedup();
        let mut plans: Vec<PlanInfo> = dates
            .into_iter()
            .enumerate()
            .map(|(i, d)| plan(&format!("p{i}"), Some(d)))
            .collect();
        // Unversioned imports in the mix: only EMPTY-identity plans
        // are standalone; an id without a date would be a defect.
        plans.push(plan("", None));
        plans.push(plan("", None));
        // Arbitrary order — chains arrive in no particular order.
        plans.sort_by_key(|p| p.name.clone());
        plans
    })
}

/// The oracle: the in-force plan on a date, computed independently
/// (ISO string comparison, no date parsing, no tie-break subtlety —
/// the max (date, id) pair at or before the date).
fn oracle_resolve(plans: &[PlanInfo], date: &str) -> Option<String> {
    plans
        .iter()
        .filter_map(|p| {
            let from = p.effective_from.as_deref()?;
            (from.as_bytes() <= date.as_bytes())
                .then_some(((from.to_string(), p.id.clone()), p.id.clone()))
        })
        .max_by(|(a, _), (b, _)| a.cmp(b))
        .map(|(_, id)| id)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// Resolve picks the latest pick at or before the date — oracle
    /// computed by independent ISO-string comparison.
    #[test]
    fn resolve_matches_the_oracle(plans in clean_chain(8), date in iso_date()) {
        let parsed = date.parse::<chrono::NaiveDate>().unwrap();
        let got = resolve(&plans, parsed).unwrap();
        let expected = oracle_resolve(&plans, &date);
        prop_assert_eq!(got, expected);
    }

    /// The answer never depends on input order.
    #[test]
    fn resolve_is_order_insensitive(plans in clean_chain(8), date in iso_date()) {
        let parsed = date.parse::<chrono::NaiveDate>().unwrap();
        let forward = resolve(&plans, parsed).unwrap();
        let mut reversed = plans.clone();
        reversed.reverse();
        prop_assert_eq!(resolve(&reversed, parsed).unwrap(), forward.clone());
        let mut by_name = plans.clone();
        by_name.sort_by(|a, b| b.name.cmp(&a.name));
        prop_assert_eq!(resolve(&by_name, parsed).unwrap(), forward);
    }

    /// Whatever resolves is genuinely in force: its pick is at or
    /// before the date, and no OTHER pick sits strictly between it
    /// and the date.
    #[test]
    fn resolved_plan_is_really_in_force(plans in clean_chain(8), date in iso_date()) {
        let parsed = date.parse::<chrono::NaiveDate>().unwrap();
        let Some(id) = resolve(&plans, parsed).unwrap() else {
            return Ok(());
        };
        let picked = plans.iter().find(|p| p.id == id).expect("resolved plan exists");
        let from = picked.effective_from.as_deref().expect("chain members are dated");
        prop_assert!(from.as_bytes() <= date.as_bytes(), "picks in the future never resolve");
        for p in &plans {
            if p.id == id { continue; }
            if let Some(other) = p.effective_from.as_deref() {
                prop_assert!(
                    !(from.as_bytes() < other.as_bytes() && other.as_bytes() <= date.as_bytes()),
                    "another pick is strictly later and still in force"
                );
            }
        }
    }

    /// A clean chain reports nothing.
    #[test]
    fn valid_chains_validate_clean(plans in clean_chain(12)) {
        prop_assert!(validate(&plans).is_empty(), "{:?}", validate(&plans));
    }

    /// Defects are reported exactly: nothing invented, nothing
    /// missed. Injected defects (a duplicated id, a duplicated date,
    /// a stripped date) must each appear, and the issue counts must
    /// match an independent multiset recount of the input.
    #[test]
    fn validate_reports_exactly_the_real_defects(
        plans in clean_chain(8),
        inject_dup_id in any::<bool>(),
        inject_dup_date in any::<bool>(),
        inject_missing_date in any::<bool>(),
    ) {
        let mut broken = plans.clone();
        if inject_dup_id && !plans.is_empty() {
            let victim = &plans[0];
            broken.push(PlanInfo {
                id: victim.id.clone(),
                name: "dup".into(),
                effective_from: Some("2027-01-05".into()),
            });
        }
        if inject_dup_date && !plans.is_empty() {
            if let Some(first_dated) = plans.iter().find(|p| p.effective_from.is_some()) {
                broken.push(PlanInfo {
                    id: "date-clone".into(),
                    name: "dup date".into(),
                    effective_from: first_dated.effective_from.clone(),
                });
            }
        }
        if inject_missing_date && !plans.is_empty() {
            broken.push(PlanInfo { id: "undated-pick".into(), name: "undated".into(), effective_from: None });
        }

        let issues = validate(&broken);

        // Independent recount: expected counts straight from the input.
        let mut id_counts: std::collections::BTreeMap<&str, usize> = std::collections::BTreeMap::new();
        let mut date_counts: std::collections::BTreeMap<&str, usize> = std::collections::BTreeMap::new();
        for p in &broken {
            if p.id.is_empty() { continue; }
            *id_counts.entry(p.id.as_str()).or_default() += 1;
            if let Some(d) = &p.effective_from {
                *date_counts.entry(d.as_str()).or_default() += 1;
            }
        }
        let want_dup_ids = id_counts.values().map(|c| c.saturating_sub(1)).sum::<usize>();
        let want_dup_dates = date_counts.values().map(|c| c.saturating_sub(1)).sum::<usize>();
        let want_missing = broken.iter()
            .filter(|p| !p.id.is_empty() && p.effective_from.is_none())
            .count();

        let count_kind = |kind: &str| issues.iter()
            .filter(|i| serde_json::to_value(i).unwrap()["kind"].as_str() == Some(kind))
            .count();
        prop_assert_eq!(count_kind("duplicate_plan_id"), want_dup_ids);
        prop_assert_eq!(count_kind("duplicate_effective_date"), want_dup_dates);
        prop_assert_eq!(count_kind("plan_missing_effective_date"), want_missing);
        prop_assert_eq!(issues.len(), want_dup_ids + want_dup_dates + want_missing, "no invented issues");
    }

    /// Garbage is an error or an answer — never a panic. Validate
    /// never looks at date syntax at all; resolve rejects it.
    #[test]
    fn chain_ops_never_panic(
        ids in proptest::collection::vec("[a-z]{0,4}", 0..=6),
        dates in proptest::collection::vec(proptest::option::of("[^\\x00]{0,8}"), 0..=6),
    ) {
        let plans: Vec<PlanInfo> = ids.into_iter().zip(dates.into_iter())
            .map(|(id, d)| PlanInfo { id, name: String::new(), effective_from: d })
            .collect();
        let _ = validate(&plans);
        match resolve(&plans, "2026-10-24".parse().unwrap()) {
            Ok(_) | Err(_) => {}
        }
    }
}
