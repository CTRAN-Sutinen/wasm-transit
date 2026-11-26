//! The migration chain of the work rules is kept forever, one pure step for
//! each version. A consumer reads the current version only.
//!
//! v1 to v2: recovery, interline, and relief points left the work rules
//! for the scheduling constraints, which are the agency's choices and
//! not an agreement's. The step drops them. A planner who had them loads
//! them again as scheduling constraints.

use serde_json::Value;

pub const KIND: &str = "work_rules";

pub const SCHEMA_VERSION: u32 = 2;

/// A document that names no version is v1.
pub fn schema_version_of(doc: &Value) -> Result<u32, String> {
    let root = doc.as_object().ok_or("a work rules document is a JSON object")?;
    if let Some(kind) = root.get("kind") {
        if kind.as_str() != Some(KIND) {
            return Err(format!("a {kind} document is not a work rules document"));
        }
    }
    match root.get("schema_version") {
        None => Ok(1),
        Some(v) => v.as_u64().and_then(|n| u32::try_from(n).ok()).filter(|&n| n >= 1).ok_or_else(|| format!("schema_version {v} is not a version")),
    }
}

pub fn migrate_step(doc: Value, from: u32) -> Result<Value, String> {
    let found = schema_version_of(&doc)?;
    if found != from {
        return Err(format!("the work rules v{from} step was given a v{found} document"));
    }
    match from {
        1 => {
            let mut doc = doc;
            let root = doc.as_object_mut().ok_or("a work rules document is a JSON object")?;
            root.remove("recovery");
            root.remove("interline");
            if let Some(runs) = root.get_mut("runs").and_then(Value::as_object_mut) {
                runs.remove("relief_points");
                if runs.is_empty() {
                    root.remove("runs");
                }
            }
            root.insert("schema_version".into(), 2.into());
            Ok(doc)
        }
        n => Err(format!("no work rules step from v{n} (this build reads up to v{SCHEMA_VERSION})")),
    }
}

/// Refuses a document already at or past `to`.
pub fn migrate_to(doc: Value, to: u32) -> Result<Value, String> {
    let from = schema_version_of(&doc)?;
    if to > SCHEMA_VERSION {
        return Err(format!("work rules v{to} is newer than this build (it reads up to v{SCHEMA_VERSION})"));
    }
    if from >= to {
        return Err(format!("work rules v{from} is not older than v{to}: nothing to migrate"));
    }
    (from..to).try_fold(doc, migrate_step)
}
