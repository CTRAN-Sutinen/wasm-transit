//! The chain is kept forever (D-66). Service alerts v1 to v2 added a trip
//! instance to `explains` (D-216). The alerts log keeps each record in the
//! version it was written in, and [`record_forward`] brings one to the
//! current version.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

pub const KIND: &str = "service_alerts";

pub const ALERTS_VERSION: u32 = 2;

pub const TEMPLATES_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "transit_alerts/")]
pub struct MigrationNote {
    pub from: u32,
    pub kind: String,
    pub field: String,
    pub count: u32,
}

pub fn schema_version_of(doc: &Value) -> Result<u32, String> {
    let root = doc.as_object().ok_or("a service alerts document is a JSON object")?;
    if let Some(kind) = root.get("kind") {
        if kind.as_str() != Some(KIND) {
            return Err(format!("a {kind} document is not a service alerts document"));
        }
    }
    match root.get("schema_version") {
        None => Ok(1),
        Some(v) => v.as_u64().and_then(|n| u32::try_from(n).ok()).filter(|&n| n >= 1).ok_or_else(|| format!("schema_version {v} is not a version")),
    }
}

pub fn migrate_step(doc: Value, from: u32) -> Result<(Value, Vec<MigrationNote>), String> {
    let found = schema_version_of(&doc)?;
    if found != from {
        return Err(format!("the service alerts v{from} step was given a v{found} document"));
    }
    match from {
        1 => {
            let mut doc = doc;
            doc["schema_version"] = 2.into();
            Ok((doc, Vec::new()))
        }
        n => Err(format!("no service alerts step from v{n} (this build reads up to v{ALERTS_VERSION})")),
    }
}

pub fn migrate_to(doc: Value, to: u32) -> Result<(Value, Vec<MigrationNote>), String> {
    let from = schema_version_of(&doc)?;
    if to > ALERTS_VERSION {
        return Err(format!("service alerts v{to} is newer than this build (it reads up to v{ALERTS_VERSION})"));
    }
    if from >= to {
        return Err(format!("service alerts v{from} is not older than v{to}: nothing to migrate"));
    }
    let mut doc = doc;
    let mut notes = Vec::new();
    for step in from..to {
        let (next, lost) = migrate_step(doc, step)?;
        doc = next;
        notes.extend(lost);
    }
    Ok((doc, notes))
}

pub fn record_forward(kind: &str, value: Value, version: u32) -> Result<Value, String> {
    if version > ALERTS_VERSION {
        return Err(format!("a {kind} record in v{version} is newer than this build (it reads up to v{ALERTS_VERSION})"));
    }
    match kind {
        "service_alert" => Ok(value),
        KIND if version < ALERTS_VERSION => migrate_to(value, ALERTS_VERSION).map(|(doc, _)| doc),
        KIND => Ok(value),
        other => Err(format!("{other:?} is not a service alerts record")),
    }
}
