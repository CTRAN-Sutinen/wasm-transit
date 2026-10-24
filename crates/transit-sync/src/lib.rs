//! transit-sync — the sync document and its pure operations.
//!
//! The principle this crate encodes: **the document is the contract**.
//! Both sides of any synchronization (browser wasm, transit-server, a
//! future agency ops service) hold the same serializable structures,
//! so agreement is about document identity, not about shared mutable
//! state.
//!
//! Ownership layering (see the project README):
//!
//! * the *feed artifact* is immutable and content-addressed — a
//!   [`FeedManifest`] points at the authoritative one;
//! * *operator edits* are documents: keyed by id, whole-record,
//!   last-writer-wins per id, deleted by retirement (tombstone), not
//!   by mutation.
//!
//! Everything here is pure: types plus functions over them. No I/O,
//! no clocks — callers stamp `updated_at`/`rev` when they mutate.

use serde::{Deserialize, Serialize};

use transit_core::{Diversion, Gtfs};

/// Lifecycle of an edit record. `retired` is the tombstone: the record
/// stays in the document (sync needs to see the deletion), but it is
/// no longer in force and the UI hides it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EditStatus {
    /// Created locally, not yet submitted anywhere.
    Draft,
    /// Pushed to an ops server; still intended to run.
    Submitted,
    /// Accepted by the ops authority; in force.
    Accepted,
    /// Withdrawn or failed revalidation against a new feed artifact.
    /// Kept as evidence and as a sync tombstone.
    Retired,
}

impl EditStatus {
    /// Is an edit in this status operationally in force? Retired is
    /// the only tombstone state; drafts run locally so the dispatcher
    /// can preview them.
    pub fn in_force(self) -> bool {
        !matches!(self, EditStatus::Retired)
    }
}

/// One operator edit plus its sync envelope. The `diversion` payload
/// is the domain content; the envelope fields are what any sync
/// partner needs to merge, order, and retire records without
/// understanding the domain.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EditRecord {
    pub id: String,
    pub diversion: Diversion,
    pub status: EditStatus,
    /// Monotonic per id; bumped on every change by the writer.
    pub rev: u64,
    /// Unix epoch milliseconds, stamped by the writer.
    pub updated_at: u64,
}

/// Validation result for one record against a feed artifact — what a
/// client needs after pushing a document (or what a server would
/// return from a validation endpoint). The record itself is display
/// data; the report only carries what requires the feed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EditReport {
    pub id: String,
    pub valid: bool,
    /// Human-readable reason when `valid` is false (unknown stop after
    /// a feed republish, degenerate anchors, ...).
    pub error: Option<String>,
    /// How many trips the edit touches (0 = no-op even when valid).
    pub affected_trips: usize,
}

/// Validate one record against a feed. Pure: same feed + same record
/// always yields the same report, on any platform.
pub fn report(gtfs: &Gtfs, record: &EditRecord) -> EditReport {
    let diversion = &record.diversion;
    match transit_core::validate_diversion(gtfs, diversion) {
        Err(e) => EditReport {
            id: record.id.clone(),
            valid: false,
            error: Some(e.to_string()),
            affected_trips: 0,
        },
        Ok(()) => EditReport {
            id: record.id.clone(),
            valid: true,
            error: None,
            affected_trips: transit_core::affected_trips(gtfs, diversion).len(),
        },
    }
}

/// Validate a whole document: one report per record, input order.
pub fn report_all(gtfs: &Gtfs, records: &[EditRecord]) -> Vec<EditReport> {
    records.iter().map(|r| report(gtfs, r)).collect()
}

/// The authority pointer: which feed artifact is current, published by
/// whom, when. Schedules change by artifact replacement — the planner
/// persona publishes a new manifest with a new `feed_hash`; everyone
/// else re-pins and revalidates their edits against it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeedManifest {
    /// Content hash (sha256) of the authoritative GTFS zip.
    pub feed_hash: String,
    /// Human-facing version from `feed_info.txt`, if the feed has one.
    pub feed_version: Option<String>,
    /// Unix epoch seconds of publication.
    pub generated_at: u64,
    /// Who published (authoring system or agency), for provenance.
    pub publisher: Option<String>,
}

/// The diversions of a document that are in force on a given service
/// date — the patch set the effective feed is built from.
pub fn in_force_on(records: &[EditRecord], date: chrono::NaiveDate) -> Vec<Diversion> {
    records
        .iter()
        .filter(|r| r.status.in_force() && r.diversion.covers(date))
        .map(|r| r.diversion.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn record(status: EditStatus) -> EditRecord {
        EditRecord {
            id: "e1".into(),
            diversion: Diversion {
                id: "d1".into(),
                route_id: "r1".into(),
                direction_id: None,
                first_kept_stop: "s0".into(),
                last_kept_stop: "s1".into(),
                skipped_stops: vec![],
                detour: None,
                added_secs: 60,
                start_date: NaiveDate::from_ymd_opt(2026, 10, 23).unwrap(),
                end_date: NaiveDate::from_ymd_opt(2026, 10, 23).unwrap(),
            },
            status,
            rev: 1,
            updated_at: 1_000,
        }
    }

    #[test]
    fn edit_record_round_trips_through_json() {
        let rec = record(EditStatus::Accepted);
        let json = serde_json::to_string(&rec).unwrap();
        assert!(json.contains("\"status\":\"accepted\""), "status is lowercase: {json}");
        assert!(json.contains("\"start_date\":\"2026-10-23\""), "dates are ISO: {json}");
        let back: EditRecord = serde_json::from_str(&json).unwrap();
        assert_eq!(back.id, rec.id);
        assert_eq!(back.status, EditStatus::Accepted);
        assert_eq!(back.diversion.first_kept_stop, "s0");
    }

    #[test]
    fn manifest_round_trips_through_json() {
        let manifest = FeedManifest {
            feed_hash: "abc123".into(),
            feed_version: Some("2026-10-23.1".into()),
            generated_at: 1_796_000_000,
            publisher: Some("Test Transit scheduling".into()),
        };
        let json = serde_json::to_string(&manifest).unwrap();
        let back: FeedManifest = serde_json::from_str(&json).unwrap();
        assert_eq!(back.feed_hash, manifest.feed_hash);
        assert_eq!(back.feed_version.as_deref(), Some("2026-10-23.1"));
    }

    #[test]
    fn retired_is_the_only_tombstone() {
        for status in [EditStatus::Draft, EditStatus::Submitted, EditStatus::Accepted] {
            assert!(status.in_force(), "{status:?} must be in force");
        }
        assert!(!EditStatus::Retired.in_force());
    }

    #[test]
    fn in_force_filters_by_status_and_date() {
        let date = NaiveDate::from_ymd_opt(2026, 10, 23).unwrap();
        let other = NaiveDate::from_ymd_opt(2026, 10, 24).unwrap();
        let mut docs = vec![record(EditStatus::Draft), record(EditStatus::Accepted)];
        docs[1].id = "e2".into();
        docs[1].diversion.start_date = other;
        docs[1].diversion.end_date = other;

        let live = in_force_on(&docs, date);
        assert_eq!(live.len(), 1, "only the covering-date record applies");
        assert_eq!(live[0].id, "d1");

        docs[0].status = EditStatus::Retired;
        assert!(in_force_on(&docs, date).is_empty(), "retired edits never apply");
    }
}
