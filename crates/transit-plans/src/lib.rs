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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS, schemars::JsonSchema)]
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
#[derive(Debug, Clone, PartialEq, Serialize, TS, schemars::JsonSchema)]
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
#[derive(Debug, Clone, PartialEq, Serialize, TS, schemars::JsonSchema)]
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
    /// A standalone import (no plan id) is not a pick: only a
    /// versioned plan is published.
    UnversionedPlan,
    /// No published pick has this id.
    UnknownPick { id: String },
    /// A pick that has taken effect (its date is today or earlier) is
    /// not retired: it is corrected by republishing it (D-75).
    PickTookEffect { id: String, effective_from: String },
    /// A pick that is new, restored, or moved never lands before today
    /// (D-76); a pick republished under its own date always publishes.
    PickDatedInPast { id: String, effective_from: String, today: String },
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

// ---------------------------------------------------------------------
// The published plan set (store kind `plans`).
// ---------------------------------------------------------------------

/// The published plan set's schema version, inside the document
/// (D-62): every copy anywhere says what it is.
pub const PUBLISHED_SCHEMA_VERSION: u32 = 1;

/// Whether a published pick is in the chain. Retirement is the
/// tombstone (D-75): a retired pick stays in the set, never governs,
/// and republishing its id restores it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export, export_to = "cap_plans/")]
#[serde(rename_all = "snake_case")]
pub enum PickStatus {
    #[default]
    Published,
    Retired,
}

/// One published pick: its chain metadata and the network document it
/// is, by content hash (the store's `plan.<id>` pointer holds the
/// document itself).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export, export_to = "cap_plans/")]
pub struct PublishedPlan {
    pub id: String,
    pub name: String,
    /// "YYYY-MM-DD": a published pick is always dated.
    pub effective_from: String,
    /// "sha256:<hex>" of the pick's canonical network document.
    pub network_hash: String,
    #[serde(default)]
    pub status: PickStatus,
}

impl PublishedPlan {
    fn info(&self) -> PlanInfo {
        PlanInfo { id: self.id.clone(), name: self.name.clone(), effective_from: Some(self.effective_from.clone()) }
    }
}

/// The chain members of a published set: its picks that are not retired.
fn chain(plans: &[PublishedPlan]) -> Vec<PlanInfo> {
    plans.iter().filter(|p| p.status == PickStatus::Published).map(PublishedPlan::info).collect()
}

/// The published plan set: every pick the deployment publishes — the
/// document the server's `plans` pointer holds and the manifest pins
/// as `plans_hash`. Every client resolves the pick in force from it.
/// Canonical: picks sorted by (effective_from, id), one entry per id,
/// so one set of picks is one byte sequence however it was published.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export, export_to = "cap_plans/")]
pub struct PublishedPlans {
    pub schema_version: u32,
    pub plans: Vec<PublishedPlan>,
}

impl PublishedPlans {
    pub fn empty() -> Self {
        PublishedPlans { schema_version: PUBLISHED_SCHEMA_VERSION, plans: Vec::new() }
    }
}

/// Publish a pick: the set with `plan` (re)placed by id and naming
/// `network_hash`, in canonical order — a republish is the pick's next
/// revision, and republishing a retired pick restores it. `today` is
/// the host's date: a pick that is new, restored, or moved never lands
/// before it, while a published pick republished under its own date (a
/// correction to the pick in force) always does (D-76). Refused, with
/// every reason, when the plan is not a dated pick, lands in the past,
/// or would make an invalid chain. The input is never mutated.
pub fn publish(
    set: &PublishedPlans,
    plan: &PlanInfo,
    network_hash: &str,
    today: NaiveDate,
) -> Result<PublishedPlans, Vec<PlanIssue>> {
    if plan.id.is_empty() {
        return Err(vec![PlanIssue::UnversionedPlan]);
    }
    let Some(from) = &plan.effective_from else {
        return Err(vec![PlanIssue::PlanMissingEffectiveDate { id: plan.id.clone() }]);
    };
    let Ok(date) = parse_date(from) else {
        return Err(vec![PlanIssue::PlanMissingEffectiveDate { id: plan.id.clone() }]);
    };
    let correction = set
        .plans
        .iter()
        .any(|p| p.id == plan.id && p.status == PickStatus::Published && &p.effective_from == from);
    if date < today && !correction {
        return Err(vec![PlanIssue::PickDatedInPast {
            id: plan.id.clone(),
            effective_from: from.clone(),
            today: today.format("%Y-%m-%d").to_string(),
        }]);
    }
    let mut plans: Vec<PublishedPlan> = set.plans.iter().filter(|p| p.id != plan.id).cloned().collect();
    plans.push(PublishedPlan {
        id: plan.id.clone(),
        name: plan.name.clone(),
        effective_from: from.clone(),
        network_hash: network_hash.to_string(),
        status: PickStatus::Published,
    });
    plans.sort_by(|a, b| (a.effective_from.as_str(), a.id.as_str()).cmp(&(b.effective_from.as_str(), b.id.as_str())));
    let issues = validate(&chain(&plans));
    if !issues.is_empty() {
        return Err(issues);
    }
    Ok(PublishedPlans { schema_version: PUBLISHED_SCHEMA_VERSION, plans })
}

/// Retire a pick (D-75): the set with it marked retired — it stays in
/// the set and never governs. Only a pick that has not taken effect
/// (dated after `today`, the host's date) is retired; one that has is
/// corrected by republishing it. Retiring a retired pick changes
/// nothing. The input is never mutated.
pub fn retire(set: &PublishedPlans, id: &str, today: NaiveDate) -> Result<PublishedPlans, Vec<PlanIssue>> {
    let Some(pick) = set.plans.iter().find(|p| p.id == id) else {
        return Err(vec![PlanIssue::UnknownPick { id: id.to_string() }]);
    };
    if pick.status == PickStatus::Retired {
        return Ok(set.clone());
    }
    let from = parse_date(&pick.effective_from).map_err(|_| {
        vec![PlanIssue::PlanMissingEffectiveDate { id: id.to_string() }]
    })?;
    if from <= today {
        return Err(vec![PlanIssue::PickTookEffect { id: id.to_string(), effective_from: pick.effective_from.clone() }]);
    }
    let plans = set
        .plans
        .iter()
        .map(|p| if p.id == id { PublishedPlan { status: PickStatus::Retired, ..p.clone() } } else { p.clone() })
        .collect();
    Ok(PublishedPlans { schema_version: PUBLISHED_SCHEMA_VERSION, plans })
}

/// The published pick in force on `date` — the chain's `resolve` over
/// the set's published picks (a retired pick never governs), answering
/// the pick (and so its network). None before the first pick takes
/// effect.
pub fn in_force(set: &PublishedPlans, date: NaiveDate) -> Result<Option<&PublishedPlan>, String> {
    Ok(resolve(&chain(&set.plans), date)?.and_then(|id| set.plans.iter().find(|p| p.id == id)))
}
