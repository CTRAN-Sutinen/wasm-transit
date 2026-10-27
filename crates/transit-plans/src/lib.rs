//! transit-plans — the service-plan chain, as a shared library.
//!
//! Service changes are PICKS: agencies implement a few a year at
//! defined dates. transit-network owns ONE plan's document; the chain
//! over MANY of them — resolution ("which plan is in force on a
//! date"), governing periods, and validity — is cross-document
//! domain logic and lives here, the same standing as transit-sync's
//! cross-record merge. The chain is DERIVED, never stored: a pick's
//! date is a target that moves while the pick is in development, and
//! the chain recomputes.
//!
//! Unversioned plans (fresh imports, empty id) are standalone
//! imports — never chain members: they do not resolve and are never
//! flagged. The cap-plans capability dispatches through this
//! library; every host gets the same answers.

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// One plan's chain metadata — the plan set's per-pick distillation
/// (the `plan` field of each network document).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "cap_plans/")]
pub struct PlanInfo {
    pub id: String,
    pub name: String,
    /// "YYYY-MM-DD"; None on unversioned plans (fresh imports).
    pub effective_from: Option<String>,
}

/// One plan's governing period: effective from its pick date until
/// the next pick takes over. The `until` end is DERIVED, never
/// stored — a plan's date is a target that may move while the pick
/// is in development, and the chain recomputes the windows.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export, export_to = "cap_plans/")]
pub struct PlanPeriod {
    pub id: String,
    /// "YYYY-MM-DD".
    pub effective_from: String,
    /// "YYYY-MM-DD" of the next pick, if any: the period is
    /// [effective_from, until).
    pub until: Option<String>,
}

/// A service-plan chain invariant violation.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export, export_to = "cap_plans/")]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PlanIssue {
    /// Two plans share an identity (the sync key).
    DuplicatePlanId { id: String },
    /// A versioned plan must be effective-dated to sit in a chain.
    PlanMissingEffectiveDate { id: String },
    /// Two plans take effect the same day — a chain is strictly
    /// increasing.
    DuplicateEffectiveDate { date: String, a: String, b: String },
}

fn parse_date(s: &str) -> Result<NaiveDate, String> {
    NaiveDate::parse_from_str(s, "%Y-%m-%d")
        .map_err(|e| format!("invalid date {s:?}, expected YYYY-MM-DD: {e}"))
}

/// Which plan is in force on `date`: the one with the latest
/// effective date at or before it (ties broken by id — deterministic
/// and order-insensitive, so an invalid chain with duplicate dates
/// still resolves the same way no matter the input order).
/// Unversioned plans never resolve. Invalid dates are an error,
/// not a guess.
pub fn resolve(plans: &[PlanInfo], date: NaiveDate) -> Result<Option<String>, String> {
    // (effective_from, id): the max at or before the date.
    let mut best: Option<(NaiveDate, String)> = None;
    for plan in plans {
        let Some(from) = &plan.effective_from else { continue };
        let from = parse_date(from)?;
        if from <= date {
            let key = (from, plan.id.clone());
            if best.as_ref().map_or(true, |b| &key > b) {
                best = Some(key);
            }
        }
    }
    Ok(best.map(|(_, id)| id))
}

/// Chain validity: unique ids, every versioned plan effective-dated,
/// effective dates unique. Unversioned plans (empty id) are
/// standalone imports — not chain members, never flagged.
pub fn validate(plans: &[PlanInfo]) -> Vec<PlanIssue> {
    let mut issues = Vec::new();
    // Order-stable duplicate detection: seen-sets keyed by value.
    let mut seen_ids: std::collections::BTreeMap<&str, usize> = std::collections::BTreeMap::new();
    let mut seen_dates: std::collections::BTreeMap<&str, &str> = std::collections::BTreeMap::new();
    for (i, plan) in plans.iter().enumerate() {
        if plan.id.is_empty() {
            continue; // unversioned: not a chain member
        }
        match seen_ids.entry(plan.id.as_str()) {
            std::collections::btree_map::Entry::Vacant(v) => {
                v.insert(i);
            }
            std::collections::btree_map::Entry::Occupied(_) => {
                issues.push(PlanIssue::DuplicatePlanId { id: plan.id.clone() });
            }
        }
        let Some(from) = &plan.effective_from else {
            issues.push(PlanIssue::PlanMissingEffectiveDate { id: plan.id.clone() });
            continue;
        };
        if let Some(other) = seen_dates.get(from.as_str()) {
            // The pair is canonical (a < b by id), so the issue is
            // order-insensitive like every other chain answer.
            let (a, b) = if (**other) < *plan.id.as_str() {
                ((**other).to_string(), plan.id.clone())
            } else {
                (plan.id.clone(), (**other).to_string())
            };
            issues.push(PlanIssue::DuplicateEffectiveDate { date: from.clone(), a, b });
        } else {
            seen_dates.insert(from.as_str(), plan.id.as_str());
        }
    }
    issues
}

/// Every chain member's governing period, sorted by pick date. Only
/// id-bearing, dated plans are chain members; duplicates and
/// undated plans are validate's business, and periods stay
/// deterministic on whatever input arrives.
pub fn periods(plans: &[PlanInfo]) -> Vec<PlanPeriod> {
    let mut members: Vec<&PlanInfo> = plans
        .iter()
        .filter(|p| !p.id.is_empty() && p.effective_from.is_some())
        .collect();
    // ISO dates sort chronologically as strings.
    members.sort_by(|a, b| {
        (a.effective_from.as_deref().unwrap(), a.id.as_str())
            .cmp(&(b.effective_from.as_deref().unwrap(), b.id.as_str()))
    });
    members
        .iter()
        .enumerate()
        .map(|(i, p)| PlanPeriod {
            id: p.id.clone(),
            effective_from: p.effective_from.clone().expect("filtered above"),
            until: members.get(i + 1).and_then(|n| n.effective_from.clone()),
        })
        .collect()
}
