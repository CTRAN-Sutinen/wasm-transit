//! The stop requests document's versions and its migration chain.
//!
//! The chain is kept forever (D-66): one pure step for each version.
//! The migration capability wraps it, and a consumer reads the current
//! version only.
//!
//! - v1 → v2: a request's `routes` are the planner's intent, written by
//!   hand, and no longer a reference into the network (D-234). Every v1
//!   document is a v2 document, so the step changes only the version.
//!
//! The stops log keeps each record in the version it was written in.
//! [`record_forward`] brings one stored record to the version this build
//! reads.

use serde_json::Value;

/// The kind the stop requests document names itself.
pub const KIND: &str = "stop_requests";

/// The version this build reads and writes.
pub const SCHEMA_VERSION: u32 = 2;

/// The version a stop requests document is in: its `schema_version`,
/// or 1 when it names none.
pub fn schema_version_of(doc: &Value) -> Result<u32, String> {
    let root = doc.as_object().ok_or("a stop requests document is a JSON object")?;
    if let Some(kind) = root.get("kind") {
        if kind.as_str() != Some(KIND) {
            return Err(format!("a {kind} document is not a stop requests document"));
        }
    }
    match root.get("schema_version") {
        None => Ok(1),
        Some(v) => v.as_u64().and_then(|n| u32::try_from(n).ok()).filter(|&n| n >= 1).ok_or_else(|| format!("schema_version {v} is not a version")),
    }
}

/// One step: a document in version `from` to `from + 1`. No step loses
/// anything yet.
pub fn migrate_step(doc: Value, from: u32) -> Result<Value, String> {
    let found = schema_version_of(&doc)?;
    if found != from {
        return Err(format!("the stop requests v{from} step was given a v{found} document"));
    }
    match from {
        1 => {
            let mut doc = doc;
            doc["schema_version"] = 2.into();
            Ok(doc)
        }
        n => Err(format!("no stop requests step from v{n} (this build reads up to v{SCHEMA_VERSION})")),
    }
}

/// A document brought to version `to` through every step between.
/// Refuses a document already at or past `to`.
pub fn migrate_to(doc: Value, to: u32) -> Result<Value, String> {
    let from = schema_version_of(&doc)?;
    if to > SCHEMA_VERSION {
        return Err(format!("stop requests v{to} is newer than this build (it reads up to v{SCHEMA_VERSION})"));
    }
    if from >= to {
        return Err(format!("stop requests v{from} is not older than v{to}: nothing to migrate"));
    }
    (from..to).try_fold(doc, migrate_step)
}

/// A document in the version this build reads: an older one brought
/// forward, the current one as it is.
pub fn forward(doc: Value) -> Result<Value, String> {
    if schema_version_of(&doc)? < SCHEMA_VERSION {
        migrate_to(doc, SCHEMA_VERSION)
    } else {
        Ok(doc)
    }
}

/// One record of the stops log, of `kind` (`stop_request`, or the
/// document's own record `stop_requests`), stored in `version`, brought
/// to [`SCHEMA_VERSION`]. A record already there is unchanged.
pub fn record_forward(kind: &str, value: Value, version: u32) -> Result<Value, String> {
    if version > SCHEMA_VERSION {
        return Err(format!("a {kind} record in v{version} is newer than this build (it reads up to v{SCHEMA_VERSION})"));
    }
    match kind {
        // Every v1 request is a v2 request.
        "stop_request" => Ok(value),
        KIND if version < SCHEMA_VERSION => migrate_to(value, SCHEMA_VERSION),
        KIND => Ok(value),
        other => Err(format!("{other:?} is not a stop requests record")),
    }
}
