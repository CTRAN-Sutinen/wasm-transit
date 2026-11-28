//! The migration chain is kept forever, one pure step for each version.
//! A consumer reads the current version only.
//!
//! v1 to v2: a request's `routes` are the planner's intent, not a
//! reference into the network. Every v1 document is a v2 document, so the
//! step changes only the version.
//!
//! The stops log keeps each record in the version it was written in, so
//! [`record_forward`] brings a stored record to the version this build reads.

use serde_json::Value;

pub const KIND: &str = "stop_requests";

pub const SCHEMA_VERSION: u32 = 2;

/// A document that names no version is v1.
pub fn schema_version_of(doc: &Value) -> Result<u32, String> {
    let root = doc
        .as_object()
        .ok_or("a stop requests document is a JSON object")?;
    if let Some(kind) = root.get("kind") {
        if kind.as_str() != Some(KIND) {
            return Err(format!("a {kind} document is not a stop requests document"));
        }
    }
    match root.get("schema_version") {
        None => Ok(1),
        Some(v) => v
            .as_u64()
            .and_then(|n| u32::try_from(n).ok())
            .filter(|&n| n >= 1)
            .ok_or_else(|| format!("schema_version {v} is not a version")),
    }
}

pub fn migrate_step(doc: Value, from: u32) -> Result<Value, String> {
    let found = schema_version_of(&doc)?;
    if found != from {
        return Err(format!(
            "the stop requests v{from} step was given a v{found} document"
        ));
    }
    match from {
        1 => {
            let mut doc = doc;
            doc["schema_version"] = 2.into();
            Ok(doc)
        }
        n => Err(format!(
            "no stop requests step from v{n} (this build reads up to v{SCHEMA_VERSION})"
        )),
    }
}

/// Refuses a document already at or past `to`.
pub fn migrate_to(doc: Value, to: u32) -> Result<Value, String> {
    let from = schema_version_of(&doc)?;
    if to > SCHEMA_VERSION {
        return Err(format!(
            "stop requests v{to} is newer than this build (it reads up to v{SCHEMA_VERSION})"
        ));
    }
    if from >= to {
        return Err(format!(
            "stop requests v{from} is not older than v{to}: nothing to migrate"
        ));
    }
    (from..to).try_fold(doc, migrate_step)
}

pub fn forward(doc: Value) -> Result<Value, String> {
    if schema_version_of(&doc)? < SCHEMA_VERSION {
        migrate_to(doc, SCHEMA_VERSION)
    } else {
        Ok(doc)
    }
}

/// `kind` is `stop_request`, or the document's own record `stop_requests`.
pub fn record_forward(kind: &str, value: Value, version: u32) -> Result<Value, String> {
    if version > SCHEMA_VERSION {
        return Err(format!("a {kind} record in v{version} is newer than this build (it reads up to v{SCHEMA_VERSION})"));
    }
    match kind {
        "stop_request" => Ok(value),
        KIND if version < SCHEMA_VERSION => migrate_to(value, SCHEMA_VERSION),
        KIND => Ok(value),
        other => Err(format!("{other:?} is not a stop requests record")),
    }
}
