//! A pick's date moves while it is in development, so the chain (the pick in
//! force and each pick's governing period) is derived, never stored, and does
//! not depend on input order. An unversioned plan, a fresh import, is never a
//! chain member. `period_in` places a pick by id, so its own date wins.
//!
//! A retired pick is the tombstone and never governs. A freeze puts
//! nothing in force, and `adopt` is `publish` of a frozen version (D-191).

use serde::{Deserialize, Serialize};
use transit_date::Date;
#[cfg(feature = "bindings")]
use ts_rs::TS;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS, schemars::JsonSchema))]
#[cfg_attr(feature = "bindings", ts(export, export_to = "cap_plans/"))]
pub struct PlanInfo {
    pub id: String,
    pub name: String,
    /// "YYYY-MM-DD". `None` on an unversioned plan (a fresh import).
    pub effective_from: Option<String>,
}

/// The `until` end is derived, never stored, because a plan's date may move.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "bindings", derive(TS, schemars::JsonSchema))]
#[cfg_attr(feature = "bindings", ts(export, export_to = "cap_plans/"))]
pub struct PlanPeriod {
    pub id: String,
    /// "YYYY-MM-DD".
    pub effective_from: String,
    /// "YYYY-MM-DD" of the next pick, if any. The period is
    /// [effective_from, until).
    pub until: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "bindings", derive(TS, schemars::JsonSchema))]
#[cfg_attr(feature = "bindings", ts(export, export_to = "cap_plans/"))]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PlanIssue {
    DuplicatePlanId {
        id: String,
    },
    PlanMissingEffectiveDate {
        id: String,
    },
    DuplicateEffectiveDate {
        date: String,
        a: String,
        b: String,
    },
    UnversionedPlan,
    UnknownPick {
        id: String,
    },
    /// A pick that has taken effect is corrected by republishing, not retired.
    PickTookEffect {
        id: String,
        effective_from: String,
    },
    /// A new, restored, or moved pick never lands before today. A pick
    /// republished under its own date always publishes.
    PickDatedInPast {
        id: String,
        effective_from: String,
        today: String,
    },
    FreezeWithoutPurpose,
    NotFrozen {
        network_hash: String,
    },
}

/// Ties break by id, so a chain with duplicate dates resolves the same way
/// for any input order. Unversioned plans never resolve.
pub fn resolve(plans: &[PlanInfo], date: Date) -> Result<Option<String>, String> {
    let mut best: Option<(Date, String)> = None;
    for plan in plans {
        let Some(from) = &plan.effective_from else {
            continue;
        };
        let from = Date::read(from)?;
        if from <= date {
            let key = (from, plan.id.clone());
            if best.as_ref().map_or(true, |b| &key > b) {
                best = Some(key);
            }
        }
    }
    Ok(best.map(|(_, id)| id))
}

/// Unversioned plans (empty id) are standalone imports, never flagged.
pub fn validate(plans: &[PlanInfo]) -> Vec<PlanIssue> {
    let mut issues = Vec::new();
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
                issues.push(PlanIssue::DuplicatePlanId {
                    id: plan.id.clone(),
                });
            }
        }
        let Some(from) = &plan.effective_from else {
            issues.push(PlanIssue::PlanMissingEffectiveDate {
                id: plan.id.clone(),
            });
            continue;
        };
        if let Some(other) = seen_dates.get(from.as_str()) {
            // The pair is ordered by id, so the issue ignores input order.
            let (a, b) = if (**other) < *plan.id.as_str() {
                ((**other).to_string(), plan.id.clone())
            } else {
                (plan.id.clone(), (**other).to_string())
            };
            issues.push(PlanIssue::DuplicateEffectiveDate {
                date: from.clone(),
                a,
                b,
            });
        } else {
            seen_dates.insert(from.as_str(), plan.id.as_str());
        }
    }
    issues
}

/// Duplicates and undated plans are `validate`'s business. Periods stay
/// deterministic on any input.
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

pub const PUBLISHED_SCHEMA_VERSION: u32 = 1;

transit_version::kind_tag!(PlansKind = "plans");

impl transit_version::Versioned for PublishedPlans {
    const KIND: &'static str = "plans";
    const READS: u32 = PUBLISHED_SCHEMA_VERSION;
    fn schema_version(&self) -> u32 {
        self.schema_version
    }
}

/// Republishing a retired pick's id restores it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS, schemars::JsonSchema))]
#[cfg_attr(feature = "bindings", ts(export, export_to = "cap_plans/"))]
#[serde(rename_all = "snake_case")]
pub enum PickStatus {
    #[default]
    Published,
    Retired,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS, schemars::JsonSchema))]
#[cfg_attr(feature = "bindings", ts(export, export_to = "cap_plans/"))]
pub struct PublishedPlan {
    pub id: String,
    pub name: String,
    /// "YYYY-MM-DD". A published pick is always dated.
    pub effective_from: String,
    /// "sha256:<hex>" of the canonical network document.
    #[cfg_attr(feature = "bindings", schemars(extend("format" = "content-hash", "x-references" = "network")))]
    pub network_hash: String,
    #[serde(default)]
    pub status: PickStatus,
    /// Absent when the adoption read no stops, or the pick predates the plans
    /// log (D-191).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    #[cfg_attr(feature = "bindings", schemars(extend("x-references" = "log.stops")))]
    pub stops_seq: Option<u64>,
    #[serde(flatten)]
    #[cfg_attr(feature = "bindings", ts(skip))]
    pub extra: transit_version::Extra,
}

impl PublishedPlan {
    fn info(&self) -> PlanInfo {
        PlanInfo {
            id: self.id.clone(),
            name: self.name.clone(),
            effective_from: Some(self.effective_from.clone()),
        }
    }
}

fn chain(plans: &[PublishedPlan]) -> Vec<PlanInfo> {
    plans
        .iter()
        .filter(|p| p.status == PickStatus::Published)
        .map(PublishedPlan::info)
        .collect()
}

/// `served` marks the deployment's feed, which production serves until a
/// pick takes effect.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS, schemars::JsonSchema))]
#[cfg_attr(feature = "bindings", ts(export, export_to = "cap_plans/"))]
pub struct DevicePlan {
    pub key: String,
    /// Empty for a plan with no pick identity.
    pub id: String,
    pub name: String,
    pub effective_from: Option<String>,
    #[serde(default)]
    pub served: bool,
}

/// `key` is absent for a published pick that the device does not hold yet.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "bindings", derive(TS, schemars::JsonSchema))]
#[cfg_attr(feature = "bindings", ts(export, export_to = "cap_plans/"))]
pub struct TimelineRow {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub key: Option<String>,
    pub id: String,
    pub name: String,
    pub effective_from: Option<String>,
    /// In production, so read-only: a published pick, or the deployment's feed.
    pub published: bool,
    pub loaded: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "bindings", derive(TS, schemars::JsonSchema))]
#[cfg_attr(feature = "bindings", ts(export, export_to = "cap_plans/"))]
pub struct Timeline {
    pub rows: Vec<TimelineRow>,
    pub in_force: Option<String>,
    pub periods: Vec<PlanPeriod>,
    pub issues: Vec<PlanIssue>,
}

/// A device plan is published when its id is a published pick's, or when it
/// is the deployment's feed. A retired pick is no row.
pub fn timeline(
    device: &[DevicePlan],
    set: Option<&PublishedPlans>,
    today: Date,
) -> Result<Timeline, String> {
    let picks: Vec<&PublishedPlan> = set
        .map(|s| {
            s.plans
                .iter()
                .filter(|p| p.status == PickStatus::Published)
                .collect()
        })
        .unwrap_or_default();
    let mut rows: Vec<TimelineRow> = device
        .iter()
        .map(|d| TimelineRow {
            key: Some(d.key.clone()),
            id: d.id.clone(),
            name: d.name.clone(),
            effective_from: d.effective_from.clone(),
            published: (!d.id.is_empty() && picks.iter().any(|p| p.id == d.id)) || d.served,
            loaded: true,
        })
        .collect();
    for p in picks {
        if !rows.iter().any(|r| r.id == p.id) {
            rows.push(TimelineRow {
                key: None,
                id: p.id.clone(),
                name: p.name.clone(),
                effective_from: Some(p.effective_from.clone()),
                published: true,
                loaded: false,
            });
        }
    }
    let infos: Vec<PlanInfo> = rows
        .iter()
        .filter(|r| !r.id.is_empty())
        .map(|r| PlanInfo {
            id: r.id.clone(),
            name: r.name.clone(),
            effective_from: r.effective_from.clone(),
        })
        .collect();
    let in_force = match set {
        Some(s) => in_force(s, today)?.map(|p| p.id.clone()),
        None => None,
    };
    Ok(Timeline {
        rows,
        in_force,
        periods: periods(&infos),
        issues: validate(&infos),
    })
}

/// The pick is placed by its id and replaces any entry that the set holds
/// under it, so its own date wins while it moves. `None` is open-ended.
pub fn period_in(set: Option<&PublishedPlans>, pick: &PlanInfo) -> Option<String> {
    let mut plans: Vec<PlanInfo> = set.map(|s| chain(&s.plans)).unwrap_or_default();
    plans.retain(|p| p.id != pick.id);
    plans.push(pick.clone());
    periods(&plans)
        .into_iter()
        .find(|p| p.id == pick.id)
        .and_then(|p| p.until)
}

/// Picks are sorted by (effective_from, id), one entry per id, so one set of
/// picks is one byte sequence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS, schemars::JsonSchema))]
#[cfg_attr(feature = "bindings", ts(export, export_to = "cap_plans/"))]
pub struct PublishedPlans {
    /// A copy without a kind tag reads as the plans document.
    #[serde(default)]
    #[cfg_attr(feature = "bindings", ts(type = "\"plans\""))]
    pub kind: PlansKind,
    pub schema_version: u32,
    pub plans: Vec<PublishedPlan>,
    #[serde(flatten)]
    #[cfg_attr(feature = "bindings", ts(skip))]
    pub extra: transit_version::Extra,
}

impl PublishedPlans {
    pub fn empty() -> Self {
        PublishedPlans {
            kind: PlansKind,
            schema_version: PUBLISHED_SCHEMA_VERSION,
            plans: Vec::new(),
            extra: Default::default(),
        }
    }
}

/// A pick that is new, restored, or moved never lands before `today`.
/// A published pick republished under its own date, a correction to the pick
/// in force, always does. Refused with every reason.
pub fn publish(
    set: &PublishedPlans,
    plan: &PlanInfo,
    network_hash: &str,
    today: Date,
) -> Result<PublishedPlans, Vec<PlanIssue>> {
    if plan.id.is_empty() {
        return Err(vec![PlanIssue::UnversionedPlan]);
    }
    let Some(from) = &plan.effective_from else {
        return Err(vec![PlanIssue::PlanMissingEffectiveDate {
            id: plan.id.clone(),
        }]);
    };
    let Ok(date) = Date::read(from) else {
        return Err(vec![PlanIssue::PlanMissingEffectiveDate {
            id: plan.id.clone(),
        }]);
    };
    let correction = set
        .plans
        .iter()
        .any(|p| p.id == plan.id && p.status == PickStatus::Published && &p.effective_from == from);
    if date < today && !correction {
        return Err(vec![PlanIssue::PickDatedInPast {
            id: plan.id.clone(),
            effective_from: from.clone(),
            today: today.to_string(),
        }]);
    }
    let mut plans: Vec<PublishedPlan> = set
        .plans
        .iter()
        .filter(|p| p.id != plan.id)
        .cloned()
        .collect();
    plans.push(PublishedPlan {
        id: plan.id.clone(),
        name: plan.name.clone(),
        effective_from: from.clone(),
        network_hash: network_hash.to_string(),
        status: PickStatus::Published,
        stops_seq: None,
        extra: Default::default(),
    });
    plans.sort_by(|a, b| {
        (a.effective_from.as_str(), a.id.as_str()).cmp(&(b.effective_from.as_str(), b.id.as_str()))
    });
    let issues = validate(&chain(&plans));
    if !issues.is_empty() {
        return Err(issues);
    }
    Ok(PublishedPlans {
        kind: PlansKind,
        schema_version: PUBLISHED_SCHEMA_VERSION,
        plans,
        extra: Default::default(),
    })
}

/// Only a pick dated after `today` is retired. One that has taken effect is
/// corrected by republishing it.
pub fn retire(
    set: &PublishedPlans,
    id: &str,
    today: Date,
) -> Result<PublishedPlans, Vec<PlanIssue>> {
    let Some(pick) = set.plans.iter().find(|p| p.id == id) else {
        return Err(vec![PlanIssue::UnknownPick { id: id.to_string() }]);
    };
    if pick.status == PickStatus::Retired {
        return Ok(set.clone());
    }
    let from = Date::read(&pick.effective_from)
        .map_err(|_| vec![PlanIssue::PlanMissingEffectiveDate { id: id.to_string() }])?;
    if from <= today {
        return Err(vec![PlanIssue::PickTookEffect {
            id: id.to_string(),
            effective_from: pick.effective_from.clone(),
        }]);
    }
    let plans = set
        .plans
        .iter()
        .map(|p| {
            if p.id == id {
                PublishedPlan {
                    status: PickStatus::Retired,
                    ..p.clone()
                }
            } else {
                p.clone()
            }
        })
        .collect();
    Ok(PublishedPlans {
        kind: PlansKind,
        schema_version: PUBLISHED_SCHEMA_VERSION,
        plans,
        extra: Default::default(),
    })
}

pub fn in_force(set: &PublishedPlans, date: Date) -> Result<Option<&PublishedPlan>, String> {
    Ok(resolve(&chain(&set.plans), date)?.and_then(|id| set.plans.iter().find(|p| p.id == id)))
}

/// The server brings these picks' networks forward at startup
/// (docs/FORMATS.md, "Versions"). The others stay in their version.
pub fn in_use(set: &PublishedPlans, today: Date) -> Result<Vec<&PublishedPlan>, String> {
    let current = in_force(set, today)?.map(|p| p.id.as_str());
    let mut used = Vec::new();
    for pick in set
        .plans
        .iter()
        .filter(|p| p.status == PickStatus::Published)
    {
        if Some(pick.id.as_str()) == current || Date::read(&pick.effective_from)? > today {
            used.push(pick);
        }
    }
    Ok(used)
}

pub const FROZEN_SCHEMA_VERSION: u32 = 1;

transit_version::kind_tag!(FrozenKind = "frozen_versions");

impl transit_version::Versioned for FrozenVersions {
    const KIND: &'static str = "frozen_versions";
    const READS: u32 = FROZEN_SCHEMA_VERSION;
    fn schema_version(&self) -> u32 {
        self.schema_version
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS, schemars::JsonSchema))]
#[cfg_attr(feature = "bindings", ts(export, export_to = "cap_plans/"))]
pub struct FrozenVersion {
    #[cfg_attr(feature = "bindings", schemars(extend("format" = "content-hash", "x-references" = "network")))]
    pub network_hash: String,
    pub plan_id: String,
    pub name: String,
    pub effective_from: String,
    /// In the agency's words, such as "board packet".
    pub purpose: String,
    /// The network hash of the frozen version that a migration brought
    /// forward (D-232).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    #[cfg_attr(feature = "bindings", schemars(extend("format" = "content-hash", "x-references" = "network")))]
    pub from_network_hash: Option<String>,
    #[serde(flatten)]
    #[cfg_attr(feature = "bindings", ts(skip))]
    pub extra: transit_version::Extra,
}

/// A version stays when adopted, so every network that was ever in force is
/// here (D-191).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS, schemars::JsonSchema))]
#[cfg_attr(feature = "bindings", ts(export, export_to = "cap_plans/"))]
pub struct FrozenVersions {
    #[serde(default)]
    #[cfg_attr(feature = "bindings", ts(type = "\"frozen_versions\""))]
    pub kind: FrozenKind,
    pub schema_version: u32,
    pub versions: std::collections::BTreeMap<String, FrozenVersion>,
    #[serde(flatten)]
    #[cfg_attr(feature = "bindings", ts(skip))]
    pub extra: transit_version::Extra,
}

impl FrozenVersions {
    pub fn empty() -> Self {
        FrozenVersions {
            kind: FrozenKind,
            schema_version: FROZEN_SCHEMA_VERSION,
            versions: Default::default(),
            extra: Default::default(),
        }
    }
}

/// A freeze changes no plan set. Freezing a network again replaces its
/// purpose.
pub fn freeze(
    frozen: &FrozenVersions,
    plan: &PlanInfo,
    network_hash: &str,
    purpose: &str,
) -> Result<FrozenVersions, Vec<PlanIssue>> {
    freeze_from(frozen, plan, network_hash, purpose, None)
}

/// Without `source`, a version already frozen keeps its source (D-232).
pub fn freeze_from(
    frozen: &FrozenVersions,
    plan: &PlanInfo,
    network_hash: &str,
    purpose: &str,
    source: Option<&str>,
) -> Result<FrozenVersions, Vec<PlanIssue>> {
    if plan.id.is_empty() {
        return Err(vec![PlanIssue::UnversionedPlan]);
    }
    let Some(from) = plan
        .effective_from
        .as_ref()
        .filter(|f| Date::read(f).is_ok())
    else {
        return Err(vec![PlanIssue::PlanMissingEffectiveDate {
            id: plan.id.clone(),
        }]);
    };
    if purpose.trim().is_empty() {
        return Err(vec![PlanIssue::FreezeWithoutPurpose]);
    }
    let mut next = frozen.clone();
    let held = frozen.versions.get(network_hash);
    let extra = held.map(|v| v.extra.clone()).unwrap_or_default();
    let from_network_hash = source
        .map(str::to_string)
        .or_else(|| held.and_then(|v| v.from_network_hash.clone()));
    next.versions.insert(
        network_hash.to_string(),
        FrozenVersion {
            network_hash: network_hash.to_string(),
            plan_id: plan.id.clone(),
            name: plan.name.clone(),
            effective_from: from.clone(),
            purpose: purpose.trim().to_string(),
            from_network_hash,
            extra,
        },
    );
    Ok(next)
}

pub fn adopt(
    set: &PublishedPlans,
    frozen: &FrozenVersions,
    network_hash: &str,
    today: Date,
    stops_seq: Option<u64>,
) -> Result<PublishedPlans, Vec<PlanIssue>> {
    let Some(version) = frozen.versions.get(network_hash) else {
        return Err(vec![PlanIssue::NotFrozen {
            network_hash: network_hash.to_string(),
        }]);
    };
    let plan = PlanInfo {
        id: version.plan_id.clone(),
        name: version.name.clone(),
        effective_from: Some(version.effective_from.clone()),
    };
    let mut next = publish(set, &plan, network_hash, today)?;
    if let Some(pick) = next.plans.iter_mut().find(|p| p.id == plan.id) {
        pick.stops_seq = stops_seq;
    }
    Ok(next)
}
