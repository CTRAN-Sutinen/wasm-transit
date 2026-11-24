//! The watchlist document: a rider's saved stops, the bus's personal
//! document, and the migration mechanism's real payload.
//!
//! Version 2, the one this build reads and writes, is
//! `{"kind": "watchlist", "schema_version": 2, "stops": [{"id", "label"}]}`.
//! Version 1 gave each stop a note and an optional color. Its step makes
//! the note the label, and drops each color. It reports each drop, so
//! no loss is silent. The chain is kept forever (D-66). cap-migrate wraps it,
//! and the kernel registers the current version only
//! (`cap_abi::kernel::WATCHLIST`).
//!
//! A copy with no tags names its version as `"version": "vN"`. Such
//! a copy of v2 reads as v2 and writes tagged. Every kind reads such a
//! copy as the version it names. A copy of v1 comes forward through the
//! chain.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

/// The watchlist version this build reads and writes.
pub const SCHEMA_VERSION: u32 = 2;

/// The kind's name, as its tag writes it.
pub const KIND: &str = "watchlist";

transit_version::kind_tag!(
    /// The watchlist's kind tag: always `"watchlist"`.
    WatchlistKind = "watchlist"
);

/// One watched stop.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export, export_to = "cap_board/")]
pub struct WatchStop {
    pub id: String,
    pub label: String,
    /// Fields this build does not know, kept as written (D-167).
    #[serde(flatten)]
    #[ts(skip)]
    pub extra: transit_version::Extra,
}

/// A rider's saved stops, in the order the rider saved them.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export, export_to = "cap_board/")]
pub struct Watchlist {
    /// The kind tag and version (D-115).
    #[serde(default)]
    #[ts(type = "\"watchlist\"")]
    pub kind: WatchlistKind,
    #[serde(default)]
    #[ts(type = "number")]
    pub schema_version: transit_version::Version<2>,
    /// The version a copy with no tags names (`"v2"`). Read,
    /// never written.
    #[serde(default, rename = "version", skip_serializing)]
    #[schemars(skip)]
    #[ts(skip)]
    pub legacy_version: Option<String>,
    pub stops: Vec<WatchStop>,
    /// Fields this build does not know, kept as written (D-167).
    #[serde(flatten)]
    #[ts(skip)]
    pub extra: transit_version::Extra,
}

impl transit_version::Versioned for Watchlist {
    const KIND: &'static str = KIND;
    const READS: u32 = SCHEMA_VERSION;
    fn schema_version(&self) -> u32 {
        self.legacy_version.as_deref().and_then(legacy).unwrap_or(self.schema_version.0)
    }
    fn version_in(value: &Value) -> u32 {
        schema_version_of(value).unwrap_or(SCHEMA_VERSION)
    }
}

impl Watchlist {
    /// A current watchlist of these stops.
    pub fn new(stops: Vec<WatchStop>) -> Self {
        Watchlist { stops, ..Default::default() }
    }

    /// Stop ids in saved order (the migration identity law: the id
    /// sequence is invariant across versions).
    pub fn stop_ids(&self) -> Vec<String> {
        self.stops.iter().map(|s| s.id.clone()).collect()
    }

    /// The label of a watched stop, or "" when it is not watched.
    pub fn label(&self, stop_id: &str) -> String {
        self.stops.iter().find(|s| s.id == stop_id).map(|s| s.label.clone()).unwrap_or_default()
    }
}

/// The number in a legacy marker ("v2").
fn legacy(marker: &str) -> Option<u32> {
    marker.strip_prefix('v').and_then(|n| n.parse().ok()).filter(|&n| n >= 1)
}

/// Every loss a migration applies to document data, reported: the
/// import-notes convention, restated for migrations.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "cap_board/")]
pub struct MigrationNote {
    /// The version the step started from.
    pub from: u32,
    /// What happened ("dropped_field", …).
    pub kind: String,
    /// The field it happened to.
    pub field: String,
    /// How many records it touched.
    pub count: u32,
}

/// The version a watchlist is in: its `schema_version`, else the
/// version a copy with no tags names (`"version": "vN"`), else 2. A copy
/// with no tags names its version, so a copy with
/// neither is the current one, as the type reads it. Not an object,
/// another kind, or a version that is no number is an error.
pub fn schema_version_of(doc: &Value) -> Result<u32, String> {
    let root = doc.as_object().ok_or("a watchlist is a JSON object")?;
    if let Some(kind) = root.get("kind") {
        if kind.as_str() != Some(KIND) {
            return Err(format!("a {kind} document is not a watchlist"));
        }
    }
    if let Some(v) = root.get("schema_version") {
        return v
            .as_u64()
            .and_then(|n| u32::try_from(n).ok())
            .filter(|&n| n >= 1)
            .ok_or_else(|| format!("watchlist schema_version {v} is not a version"));
    }
    match root.get("version") {
        None => Ok(SCHEMA_VERSION),
        Some(v) => v.as_str().and_then(legacy).ok_or_else(|| format!("watchlist version {v} is not a version (\"v1\", \"v2\")")),
    }
}

/// One step: a watchlist in version `from` to version `from + 1`, and
/// what it lost. Refuses a document that is not in `from`.
pub fn migrate_step(doc: Value, from: u32) -> Result<(Value, Vec<MigrationNote>), String> {
    let found = schema_version_of(&doc)?;
    if found != from {
        return Err(format!("the watchlist v{from} step was given a v{found} document"));
    }
    match from {
        1 => {
            let stops = doc.get("stops").and_then(Value::as_array).ok_or("a v1 watchlist has its stops")?;
            let mut dropped_colors = 0u32;
            let mut next = Vec::with_capacity(stops.len());
            for stop in stops {
                let id = stop.get("id").and_then(Value::as_str).ok_or("a v1 stop has its id")?;
                let note = stop.get("note").and_then(Value::as_str).ok_or("a v1 stop has its note")?;
                if stop.get("color").is_some_and(|c| !c.is_null()) {
                    dropped_colors += 1;
                }
                next.push(serde_json::json!({ "id": id, "label": note }));
            }
            let mut notes = Vec::new();
            if dropped_colors > 0 {
                notes.push(MigrationNote { from: 1, kind: "dropped_field".into(), field: "color".into(), count: dropped_colors });
            }
            Ok((serde_json::json!({ "kind": KIND, "schema_version": 2, "stops": next }), notes))
        }
        n => Err(format!("no watchlist step from v{n} (this build reads up to v{SCHEMA_VERSION})")),
    }
}

/// A watchlist brought to version `to` through every step between, with
/// every step's losses. Refuses a document already at or past `to`:
/// migrating never rewrites a version.
pub fn migrate_to(doc: Value, to: u32) -> Result<(Value, Vec<MigrationNote>), String> {
    let from = schema_version_of(&doc)?;
    if to > SCHEMA_VERSION {
        return Err(format!("watchlist v{to} is newer than this build (it reads up to v{SCHEMA_VERSION})"));
    }
    if from >= to {
        return Err(format!("watchlist v{from} is not older than v{to}: nothing to migrate"));
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
