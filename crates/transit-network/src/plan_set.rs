//! The store owns the key of each plan, because the `plan.id` of an import
//! is empty until it is a pick. Each network in the set carries its own
//! version, and the page brings an older one forward when it restores it.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
#[cfg(feature = "bindings")]
use ts_rs::TS;

use crate::model::Network;

pub const PLAN_SET_VERSION: u32 = 1;

transit_version::kind_tag!(PlanSetKind = "plan_set");

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS, schemars::JsonSchema))]
#[cfg_attr(feature = "bindings", ts(export))]
pub struct PlanSet {
    #[serde(default)]
    #[cfg_attr(feature = "bindings", ts(type = "\"plan_set\""))]
    pub kind: PlanSetKind,
    #[serde(default)]
    #[cfg_attr(feature = "bindings", ts(type = "number"))]
    pub schema_version: transit_version::Version<1>,
    pub plans: BTreeMap<String, Network>,
    #[serde(flatten)]
    #[cfg_attr(feature = "bindings", ts(skip))]
    pub extra: transit_version::Extra,
}

impl transit_version::Versioned for PlanSet {
    const KIND: &'static str = "plan_set";
    const READS: u32 = PLAN_SET_VERSION;
    fn schema_version(&self) -> u32 {
        self.schema_version.0
    }
}

/// A set that has no tag is a bare map of plans by store id, and reads as the
/// set. A set of another kind or version is refused, and the page never
/// writes over it.
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
    let version = root
        .get("schema_version")
        .map_or(Some(1), |v| v.as_u64().and_then(|n| u32::try_from(n).ok()));
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
