//! The plan set: the service plans a device keeps, as one document.
//!
//! A planner's page keeps the plans it works on (each a network
//! document framed by its `plan`) in the browser's store, keyed by a
//! store id. The key is the store's, not the plan's: an import's
//! `plan.id` is empty until it is a pick. The page writes the whole set
//! on every change, and reads it back on start. Like every persisted
//! document, the set says its kind and version inside it (D-115), so a
//! copy read anywhere names both (docs/FORMATS.md, "Versions").
//!
//! The page owns the set (JS owns the documents); this type is its
//! shape, its schema, and its tag. Each network in it carries its own
//! version, and a page brings an older one forward when it restores it.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::model::Network;

/// The plan set version this build reads and writes.
pub const PLAN_SET_VERSION: u32 = 1;

transit_version::kind_tag!(
    /// The plan set's kind tag: always `"plan_set"`.
    PlanSetKind = "plan_set"
);

/// The plans a device keeps, by store id.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
pub struct PlanSet {
    /// The kind tag (D-115): `"plan_set"`.
    #[serde(default)]
    #[ts(type = "\"plan_set\"")]
    pub kind: PlanSetKind,
    #[serde(default)]
    #[ts(type = "number")]
    pub schema_version: transit_version::Version<1>,
    /// Each plan's network document, by the store's id for it.
    pub plans: BTreeMap<String, Network>,
    /// Fields this build does not know, kept as written (D-167).
    #[serde(flatten)]
    #[ts(skip)]
    pub extra: transit_version::Extra,
}

impl transit_version::Versioned for PlanSet {
    const KIND: &'static str = "plan_set";
    const READS: u32 = PLAN_SET_VERSION;
    fn schema_version(&self) -> u32 {
        self.schema_version.0
    }
}

/// The plans a stored set holds, by store id, each as stored: a page
/// reads each plan's own version when it uses the plan. The set itself
/// is read here. A tagged set in this version answers its `plans`. A set
/// written before its tag is a bare map of plans by store id, and reads
/// as the set. A set of another kind or version is refused, naming both,
/// and the page never writes over it.
pub fn stored_plans(doc: serde_json::Value) -> Result<BTreeMap<String, serde_json::Value>, String> {
    let serde_json::Value::Object(mut root) = doc else {
        return Err("a plan set is a JSON object".into());
    };
    let Some(kind) = root.get("kind") else {
        return Ok(root.into_iter().collect());
    };
    if kind.as_str() != Some(<PlanSet as transit_version::Versioned>::KIND) {
        return Err(format!("a {kind} document is not a plan set"));
    }
    let version = root.get("schema_version").map_or(Some(1), |v| v.as_u64().and_then(|n| u32::try_from(n).ok()));
    match version {
        Some(PLAN_SET_VERSION) => {}
        Some(v) if v > PLAN_SET_VERSION => {
            return Err(format!(
                "plan_set v{v} is newer than this build, which reads v{PLAN_SET_VERSION}: a newer build wrote it (refused, never rewritten)"
            ))
        }
        _ => return Err(format!("the plan set's schema_version {:?} is not a version this build reads", root.get("schema_version"))),
    }
    match root.remove("plans") {
        Some(serde_json::Value::Object(plans)) => Ok(plans.into_iter().collect()),
        _ => Err("a plan set has its plans".into()),
    }
}
