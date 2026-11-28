//! Version 1 gave each stop a note and an optional color. Its migration step
//! makes the note the label, drops each color, and reports each drop. The
//! chain stays forever, and the kernel registers the current version
//! only (`cap_abi::kernel::WATCHLIST`).
//!
//! A copy with no tags names its version as `"version": "vN"`. A copy of v2
//! reads as v2 and writes tagged. A copy of v1 comes forward through the
//! chain.

use serde::{Deserialize, Serialize};
use serde_json::Value;
#[cfg(feature = "bindings")]
use ts_rs::TS;

pub const SCHEMA_VERSION: u32 = 2;

pub const KIND: &str = "watchlist";

transit_version::kind_tag!(WatchlistKind = "watchlist");

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS, schemars::JsonSchema))]
#[cfg_attr(feature = "bindings", ts(export, export_to = "cap_board/"))]
pub struct WatchStop {
    pub id: String,
    pub label: String,
    #[serde(flatten)]
    #[cfg_attr(feature = "bindings", ts(skip))]
    pub extra: transit_version::Extra,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS, schemars::JsonSchema))]
#[cfg_attr(feature = "bindings", ts(export, export_to = "cap_board/"))]
pub struct Watchlist {
    #[serde(default)]
    #[cfg_attr(feature = "bindings", ts(type = "\"watchlist\""))]
    pub kind: WatchlistKind,
    #[serde(default)]
    #[cfg_attr(feature = "bindings", ts(type = "number"))]
    pub schema_version: transit_version::Version<2>,
    /// The version that a copy with no tags names (`"v2"`). Read, never
    /// written.
    #[serde(default, rename = "version", skip_serializing)]
    #[cfg_attr(feature = "bindings", schemars(skip))]
    #[cfg_attr(feature = "bindings", ts(skip))]
    pub legacy_version: Option<String>,
    pub stops: Vec<WatchStop>,
    #[serde(flatten)]
    #[cfg_attr(feature = "bindings", ts(skip))]
    pub extra: transit_version::Extra,
}

impl transit_version::Versioned for Watchlist {
    const KIND: &'static str = KIND;
    const READS: u32 = SCHEMA_VERSION;
    fn schema_version(&self) -> u32 {
        self.legacy_version
            .as_deref()
            .and_then(legacy)
            .unwrap_or(self.schema_version.0)
    }
    fn version_in(value: &Value) -> u32 {
        schema_version_of(value).unwrap_or(SCHEMA_VERSION)
    }
}

impl Watchlist {
    pub fn new(stops: Vec<WatchStop>) -> Self {
        Watchlist {
            stops,
            ..Default::default()
        }
    }

    /// The id sequence is invariant across versions.
    pub fn stop_ids(&self) -> Vec<String> {
        self.stops.iter().map(|s| s.id.clone()).collect()
    }

    pub fn label(&self, stop_id: &str) -> String {
        self.stops
            .iter()
            .find(|s| s.id == stop_id)
            .map(|s| s.label.clone())
            .unwrap_or_default()
    }
}

fn legacy(marker: &str) -> Option<u32> {
    marker
        .strip_prefix('v')
        .and_then(|n| n.parse().ok())
        .filter(|&n| n >= 1)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS))]
#[cfg_attr(feature = "bindings", ts(export, export_to = "cap_board/"))]
pub struct MigrationNote {
    pub from: u32,
    pub kind: String,
    pub field: String,
    pub count: u32,
}

/// A copy with neither tags nor marker is the current version, as the type
/// reads it. A non-object, another kind, or a version that is no number is an
/// error.
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
        Some(v) => v
            .as_str()
            .and_then(legacy)
            .ok_or_else(|| format!("watchlist version {v} is not a version (\"v1\", \"v2\")")),
    }
}

pub fn migrate_step(doc: Value, from: u32) -> Result<(Value, Vec<MigrationNote>), String> {
    let found = schema_version_of(&doc)?;
    if found != from {
        return Err(format!(
            "the watchlist v{from} step was given a v{found} document"
        ));
    }
    match from {
        1 => {
            let stops = doc
                .get("stops")
                .and_then(Value::as_array)
                .ok_or("a v1 watchlist has its stops")?;
            let mut dropped_colors = 0u32;
            let mut next = Vec::with_capacity(stops.len());
            for stop in stops {
                let id = stop
                    .get("id")
                    .and_then(Value::as_str)
                    .ok_or("a v1 stop has its id")?;
                let note = stop
                    .get("note")
                    .and_then(Value::as_str)
                    .ok_or("a v1 stop has its note")?;
                if stop.get("color").is_some_and(|c| !c.is_null()) {
                    dropped_colors += 1;
                }
                next.push(serde_json::json!({ "id": id, "label": note }));
            }
            let mut notes = Vec::new();
            if dropped_colors > 0 {
                notes.push(MigrationNote {
                    from: 1,
                    kind: "dropped_field".into(),
                    field: "color".into(),
                    count: dropped_colors,
                });
            }
            Ok((
                serde_json::json!({ "kind": KIND, "schema_version": 2, "stops": next }),
                notes,
            ))
        }
        n => Err(format!(
            "no watchlist step from v{n} (this build reads up to v{SCHEMA_VERSION})"
        )),
    }
}

/// Refuses a document already at or past `to`.
pub fn migrate_to(doc: Value, to: u32) -> Result<(Value, Vec<MigrationNote>), String> {
    let from = schema_version_of(&doc)?;
    if to > SCHEMA_VERSION {
        return Err(format!(
            "watchlist v{to} is newer than this build (it reads up to v{SCHEMA_VERSION})"
        ));
    }
    if from >= to {
        return Err(format!(
            "watchlist v{from} is not older than v{to}: nothing to migrate"
        ));
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
