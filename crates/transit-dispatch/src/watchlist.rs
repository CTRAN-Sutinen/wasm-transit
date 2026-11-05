//! The watchlist document — the bus's structured, VERSIONED personal
//! document (a rider's saved stops), so the migration mechanism has
//! something real to move.
//!
//! v1: stops carry a note and an optional color. v2: stops carry a
//! label (from the note); the color is dropped AND REPORTED — never a
//! silent loss. The board speaks both versions, so a rolled-back
//! document still queries (the board just loses labels). The kernel
//! registers it (`cap_abi::kernel::WATCHLIST`); cap-board answers over
//! it and migrates it.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// One stop as version 1 authored it.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "cap_board/")]
pub struct WatchStopV1 {
    pub id: String,
    pub note: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
}

/// One stop as version 2 authors it.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "cap_board/")]
pub struct WatchStopV2 {
    pub id: String,
    pub label: String,
}

/// A watchlist, version-tagged so a reader can dispatch on it.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "cap_board/")]
#[serde(tag = "version", rename_all = "snake_case")]
pub enum Watchlist {
    V1 { stops: Vec<WatchStopV1> },
    V2 { stops: Vec<WatchStopV2> },
}

impl Watchlist {
    pub fn version(&self) -> u32 {
        match self {
            Watchlist::V1 { .. } => 1,
            Watchlist::V2 { .. } => 2,
        }
    }

    /// Stop ids in authored order (the migration identity law: the
    /// id sequence is invariant across versions).
    pub fn stop_ids(&self) -> Vec<&str> {
        match self {
            Watchlist::V1 { stops } => stops.iter().map(|s| s.id.as_str()).collect(),
            Watchlist::V2 { stops } => stops.iter().map(|s| s.id.as_str()).collect(),
        }
    }

    /// The label a watched stop carries under the document's version
    /// (v2 labels; v1 notes — the migration maps note → label).
    pub fn label(&self, stop_id: &str) -> String {
        match self {
            Watchlist::V1 { stops } => stops.iter().find(|s| s.id == stop_id).map(|s| s.note.clone()).unwrap_or_default(),
            Watchlist::V2 { stops } => stops.iter().find(|s| s.id == stop_id).map(|s| s.label.clone()).unwrap_or_default(),
        }
    }
}

/// Every loss a migration applies to document data, reported — the
/// ImportNote convention, restated for migrations.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "cap_board/")]
pub struct MigrationNote {
    pub kind: String,
    pub field: String,
    pub count: u32,
}

/// The v1 → v2 migration: pure, deterministic, loss reported. Label
/// comes from the note; the color goes and says so.
pub fn migrate(document: &Watchlist) -> Result<(Watchlist, Vec<MigrationNote>), String> {
    match document {
        Watchlist::V1 { stops } => {
            let mut dropped_colors = 0u32;
            let stops = stops
                .iter()
                .map(|s| {
                    if s.color.is_some() {
                        dropped_colors += 1;
                    }
                    WatchStopV2 { id: s.id.clone(), label: s.note.clone() }
                })
                .collect();
            let mut notes = Vec::new();
            if dropped_colors > 0 {
                notes.push(MigrationNote { kind: "dropped_field".into(), field: "color".into(), count: dropped_colors });
            }
            Ok((Watchlist::V2 { stops }, notes))
        }
        Watchlist::V2 { .. } => Err("document is already version 2".into()),
    }
}
