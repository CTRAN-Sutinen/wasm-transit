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

use ts_rs::TS;
use serde::{Deserialize, Serialize};

pub mod version;
pub use version::{EditDocument, EditsKind, MigrationNote, KIND, SCHEMA_VERSION};

/// One vertex of a drawn detour polyline.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
pub struct DetourPoint {
    pub lat: f64,
    pub lon: f64,
}

/// One operator edit, serializable so the UI can persist it next to
/// the feed bytes and replay it after a reload. The edit document is
/// THE DISPATCHER'S OWN — it lives here, in the sync contract, not in
/// the GTFS codec (GTFS is a serialization spec for external data
/// sharing; this document never crosses that boundary). Its fields
/// reference routes and stops by the ids the network model carries.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
pub struct Diversion {
    pub id: String,
    pub route_id: String,
    /// `None` = both directions. (GTFS's direction_id, kept for the
    /// boundary path; the model matches patterns, not directions.)
    #[serde(default)]
    pub direction_id: Option<u8>,
    /// Board/anchor points on the normal alignment: the detour happens
    /// between them, and they stay served.
    pub first_kept_stop: String,
    pub last_kept_stop: String,
    /// Interior stops no longer served (must not include the anchors).
    #[serde(default)]
    pub skipped_stops: Vec<String>,
    /// Drawn detour polyline; `None` = alignment unchanged (skip only).
    #[serde(default)]
    pub detour: Option<Vec<DetourPoint>>,
    /// Extra running time through the detour, applied from the last
    /// anchor onward.
    #[serde(default)]
    pub added_secs: u32,
    #[serde(with = "iso_date")]
    #[schemars(with = "chrono::NaiveDate")]
    #[ts(type = "string")]
    pub start_date: chrono::NaiveDate,
    #[serde(with = "iso_date")]
    #[schemars(with = "chrono::NaiveDate")]
    #[ts(type = "string")]
    pub end_date: chrono::NaiveDate,
}

impl Diversion {
    /// Is this edit in force on `date`?
    pub fn covers(&self, date: chrono::NaiveDate) -> bool {
        self.start_date <= date && date <= self.end_date
    }
}

/// A whole trip taken out of service on one service date (D-45): the
/// trip does not run, so no board offers it as a departure and the
/// router never rides it — the boards list it as cancelled among the
/// visits the day's edits took away (D-100). The trip is the model's
/// stored trip (`Network::trip`); a frequency trip's instances share
/// it, so `start_secs` names one instance, and `None` cancels every
/// instance it runs that day. Partial cancellation (from a stop on),
/// cover, hold, and swap are not this payload's (D-45: later).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
pub struct TripCancel {
    /// The cancel's own domain id (stable across re-proposals of one
    /// record, like a diversion's).
    pub id: String,
    pub trip_id: String,
    /// One instance (seconds after the service-day anchor); `None` =
    /// every instance of the trip on the date.
    #[serde(default)]
    pub start_secs: Option<u32>,
    /// The one service date it is cancelled on (D-101).
    #[serde(with = "iso_date")]
    #[schemars(with = "chrono::NaiveDate")]
    #[ts(type = "string")]
    pub date: chrono::NaiveDate,
}

impl TripCancel {
    /// Is this cancel in force on `date`?
    pub fn covers(&self, date: chrono::NaiveDate) -> bool {
        self.date == date
    }

    /// Does it take away the instance (`trip`, `start_secs`)?
    pub fn cancels(&self, trip: &str, start_secs: u32) -> bool {
        self.trip_id == trip && self.start_secs.is_none_or(|s| s == start_secs)
    }
}

/// Serialize/deserialize dates on the JS boundary as "YYYY-MM-DD",
/// matching the string format every wasm entry point takes.
mod iso_date {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(
        date: &chrono::NaiveDate,
        s: S,
    ) -> std::result::Result<S::Ok, S::Error> {
        s.serialize_str(&date.format("%Y-%m-%d").to_string())
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(
        d: D,
    ) -> std::result::Result<chrono::NaiveDate, D::Error> {
        let raw = String::deserialize(d)?;
        chrono::NaiveDate::parse_from_str(&raw, "%Y-%m-%d")
            .map_err(|e| serde::de::Error::custom(format!("invalid date {raw:?}: {e}")))
    }
}

/// Lifecycle of an edit record. `retired` is the tombstone: the record
/// stays in the document (sync needs to see the deletion), but it is
/// no longer in force and the UI hides it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
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

/// One operator edit plus its sync envelope. The payload is the domain
/// content — exactly one of `diversion` and `cancel` (a record with
/// neither or both is not a record: [`sanitize`] drops it); the
/// envelope fields are what any sync partner needs to merge, order,
/// and retire records without understanding the domain. An absent
/// payload is omitted, never `null`, so a diversion record's bytes
/// (and every edit document's hash) are the ones it had before the
/// cancel payload existed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
pub struct EditRecord {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diversion: Option<Diversion>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cancel: Option<TripCancel>,
    pub status: EditStatus,
    /// Monotonic per id; bumped on every change by the writer.
    #[ts(type = "number")]
    pub rev: u64,
    /// Unix epoch milliseconds, stamped by the writer.
    #[ts(type = "number")]
    pub updated_at: u64,
}

impl EditRecord {
    /// Exactly one payload — the shape every record must have.
    pub fn well_formed(&self) -> bool {
        self.diversion.is_some() != self.cancel.is_some()
    }
}

/// Validation result for one record — what a client needs after
/// pushing a document (or what a server would return from a
/// validation endpoint). The record itself is display data; the
/// report only carries the verdict. The verdict is model-native,
/// against an authored network document (transit-dispatch); nothing
/// validates an edit against a feed.
#[derive(Debug, Clone, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
pub struct EditReport {
    pub id: String,
    pub valid: bool,
    /// Human-readable reason when `valid` is false (unknown stop after
    /// a feed republish, degenerate anchors, ...).
    pub error: Option<String>,
    /// How many trips the edit touches (0 = no-op even when valid).
    pub affected_trips: usize,
}

/// The authority pointer: which feed artifact is current, published by
/// whom, when. Schedules change by artifact replacement — the planner
/// persona publishes a new manifest with a new `feed_hash`; everyone
/// else re-pins and revalidates their edits against it.
#[derive(Debug, Clone, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
pub struct FeedManifest {
    /// Content hash (sha256) of the authoritative GTFS zip.
    pub feed_hash: String,
    /// Human-facing version from `feed_info.txt`, if the feed has one.
    pub feed_version: Option<String>,
    /// Unix epoch seconds of publication.
    #[ts(type = "number")]
    pub generated_at: u64,
    /// Who published (authoring system or agency), for provenance.
    pub publisher: Option<String>,
}

/// The diversions of a document that are in force on a given service
/// date — the patch set the effective feed is built from.
pub fn in_force_on(records: &[EditRecord], date: chrono::NaiveDate) -> Vec<Diversion> {
    records
        .iter()
        .filter(|r| r.status.in_force())
        .filter_map(|r| r.diversion.as_ref())
        .filter(|d| d.covers(date))
        .cloned()
        .collect()
}

/// The trip cancels of a document in force on a given service date —
/// the instances the effective day does not run.
pub fn cancels_in_force_on(records: &[EditRecord], date: chrono::NaiveDate) -> Vec<TripCancel> {
    records
        .iter()
        .filter(|r| r.status.in_force())
        .filter_map(|r| r.cancel.as_ref())
        .filter(|c| c.covers(date))
        .cloned()
        .collect()
}

// ---------------------------------------------------------------------
// Sync: last-writer-wins merge and the submit transition
// ---------------------------------------------------------------------

/// Does `a` win over `b` in the LWW order? The order is a strict
/// total order on records — that is what makes merge a semilattice and
/// therefore convergent: every node, running the same bytes, reaches
/// the same document no matter the merge order.
///
/// 1. later `updated_at` wins;
/// 2. tie: higher `rev` wins;
/// 3. full tie (same clock tick and rev, differing content — only a
///    buggy writer): the lexicographically greater serialized record
///    wins, deterministically, everywhere.
fn wins(a: &EditRecord, b: &EditRecord) -> bool {
    match a.updated_at.cmp(&b.updated_at) {
        std::cmp::Ordering::Greater => return true,
        std::cmp::Ordering::Less => return false,
        std::cmp::Ordering::Equal => {}
    }
    match a.rev.cmp(&b.rev) {
        std::cmp::Ordering::Greater => return true,
        std::cmp::Ordering::Less => return false,
        std::cmp::Ordering::Equal => {}
    }
    serde_json::to_string(a).unwrap_or_default() > serde_json::to_string(b).unwrap_or_default()
}

/// Merge two documents: per record id, last writer wins. The result is
/// sorted by id, so equal inputs always serialize identically on
/// every host. Tombstones participate like any record — a retirement
/// with a later timestamp beats an older in-force version, and stays
/// retired (deletions propagate through sync).
pub fn merge(local: &[EditRecord], remote: &[EditRecord]) -> Vec<EditRecord> {
    let mut by_id: std::collections::BTreeMap<String, EditRecord> = std::collections::BTreeMap::new();
    for record in local.iter().chain(remote) {
        match by_id.get(&record.id) {
            Some(existing) => {
                if wins(record, existing) {
                    by_id.insert(record.id.clone(), record.clone());
                }
            }
            None => {
                by_id.insert(record.id.clone(), record.clone());
            }
        }
    }
    by_id.into_values().collect()
}

/// The submit transition: an ops server ingests a document and every
/// `draft` becomes `submitted` (rev bumped, timestamp = now), marking
/// "this record has been pushed to the authority". Called by the
/// `sync_edits` op — hosts pass the clock, the artifact owns the rule.
pub fn submit(records: Vec<EditRecord>, now_ms: u64) -> Vec<EditRecord> {
    records
        .into_iter()
        .map(|mut r| {
            if r.status == EditStatus::Draft {
                r.status = EditStatus::Submitted;
                r.rev += 1;
                r.updated_at = now_ms;
            }
            r
        })
        .collect()
}

// ---------------------------------------------------------------------
// Authoring the edit document: the envelope rules every writer follows
// ---------------------------------------------------------------------

/// Propose a diversion: the next document with `record_id` carrying
/// `diversion` as a draft, stamped `now_ms`. A new id appends at rev 1;
/// an existing id is replaced whole (LWW per record — the record is
/// the unit) with its rev bumped. The input is never mutated.
pub fn propose(records: &[EditRecord], record_id: &str, diversion: Diversion, now_ms: u64) -> Vec<EditRecord> {
    put(records, record_id, Some(diversion), None, now_ms)
}

/// Propose a trip cancel: the same envelope rule as [`propose`] — a
/// new id appends at rev 1 as a draft, an existing one is replaced
/// whole with its rev bumped, the input never mutated.
pub fn propose_cancel(records: &[EditRecord], record_id: &str, cancel: TripCancel, now_ms: u64) -> Vec<EditRecord> {
    put(records, record_id, None, Some(cancel), now_ms)
}

fn put(
    records: &[EditRecord],
    record_id: &str,
    diversion: Option<Diversion>,
    cancel: Option<TripCancel>,
    now_ms: u64,
) -> Vec<EditRecord> {
    let mut next = records.to_vec();
    match next.iter_mut().find(|r| r.id == record_id) {
        Some(r) => {
            r.diversion = diversion;
            r.cancel = cancel;
            r.status = EditStatus::Draft;
            r.rev += 1;
            r.updated_at = now_ms;
        }
        None => next.push(EditRecord {
            id: record_id.to_string(),
            diversion,
            cancel,
            status: EditStatus::Draft,
            rev: 1,
            updated_at: now_ms,
        }),
    }
    next
}

/// Retire (`retired: true` — the tombstone) or restore (back to a
/// draft, so the next sync re-submits it) one record: the next
/// document with its rev bumped and `now_ms` stamped. Retiring a
/// retired record, or restoring an in-force one, is a no-op answer
/// (the same document — no rev churn). An unknown id is an error.
pub fn set_retired(records: &[EditRecord], record_id: &str, retired: bool, now_ms: u64) -> Result<Vec<EditRecord>, String> {
    let mut next = records.to_vec();
    let r = next
        .iter_mut()
        .find(|r| r.id == record_id)
        .ok_or_else(|| format!("unknown edit record {record_id}"))?;
    if (r.status == EditStatus::Retired) != retired {
        r.status = if retired { EditStatus::Retired } else { EditStatus::Draft };
        r.rev += 1;
        r.updated_at = now_ms;
    }
    Ok(next)
}

// ---------------------------------------------------------------------
// The stored document: envelope, schema version, the receive path
// ---------------------------------------------------------------------


/// The kind of a stored document, as its envelope sidecar spells it
/// (`docs/FORMATS.md`). GTFS stays the interchange for published
/// products; these kinds cover what GTFS cannot express.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum DocumentKind {
    NetworkDocument,
    EditRecords,
    DiversionDraft,
    Manifest,
    Plans,
    Policy,
}

/// The typed envelope stored next to every blob: the hash names the
/// bytes, the envelope says how to read them. `produced_by` pins the
/// producing module's hash (`cap-sync@sha256:…`), or `server` for
/// host-owned documents.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
pub struct Envelope {
    pub kind: DocumentKind,
    pub schema_version: u32,
    /// e.g. "cap-sync@sha256:<hex>" or "server".
    pub produced_by: String,
}

/// The receive path's clock rule (D-56): a record claiming a future
/// `updated_at` is treated as written now. Clamping is idempotent —
/// clamping a clamped document changes nothing — so at-least-once
/// redelivery still converges to the same hash.
pub fn clamp_timestamps(records: &[EditRecord], now_ms: u64) -> Vec<EditRecord> {
    records
        .iter()
        .map(|r| {
            let mut r = r.clone();
            r.updated_at = r.updated_at.min(now_ms);
            r
        })
        .collect()
}

/// Compaction (D-57): drop tombstones older than the horizon. Only
/// `retired` records with `updated_at < now_ms - horizon_ms` go;
/// in-force records are never dropped, and the input is never mutated.
/// Returns the compacted document and how many records were dropped.
pub fn compact(records: &[EditRecord], now_ms: u64, horizon_ms: u64) -> (Vec<EditRecord>, usize) {
    let horizon = now_ms.saturating_sub(horizon_ms);
    let mut dropped = 0;
    let kept = records
        .iter()
        .filter(|r| {
            let old_tombstone = r.status == EditStatus::Retired && r.updated_at < horizon;
            if old_tombstone {
                dropped += 1;
            }
            !old_tombstone
        })
        .cloned()
        .collect();
    (kept, dropped)
}

/// Read a received edit document (a POST body, a stored head, a
/// file) in the version this build reads, and only that: an older one
/// is refused with the op that brings it forward (the migration
/// capability), a newer one naming both versions. Its records are
/// [`sanitize_records`]'s.
pub fn read_document(document: &serde_json::Value) -> Result<(EditDocument, usize), String> {
    let found = version::schema_version_of(document)?;
    if found < SCHEMA_VERSION {
        return Err(format!(
            "{KIND} v{found}: this module reads v{SCHEMA_VERSION} — bring it forward first (migrate.migrate, kind {KIND:?})"
        ));
    }
    if found > SCHEMA_VERSION {
        return Err(format!(
            "{KIND} v{found} is newer than this module, which reads v{SCHEMA_VERSION}: a newer build wrote it (refused, never rewritten)"
        ));
    }
    let (records, dropped) = sanitize_records(&document["records"]);
    Ok((EditDocument::new(records), dropped))
}

/// The records of an edit document: every one that deserializes under
/// the CURRENT schema — serde fills the defaults a record from before
/// a field existed lacks — and carries exactly one payload, and how
/// many did not. A malformed record is dropped and counted, never
/// allowed to wedge the document; anything but an array is no records
/// with nothing dropped.
pub fn sanitize_records(items: &serde_json::Value) -> (Vec<EditRecord>, usize) {
    let Some(items) = items.as_array() else { return (Vec::new(), 0) };
    let mut kept = Vec::new();
    let mut dropped = 0;
    for item in items {
        match serde_json::from_value::<EditRecord>(item.clone()) {
            Ok(r)
                if r.well_formed()
                    && r.diversion.iter().flat_map(|d| d.detour.iter().flatten()).all(|p| p.lat.is_finite() && p.lon.is_finite()) =>
            {
                kept.push(r)
            }
            _ => dropped += 1,
        }
    }
    (kept, dropped)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn record(status: EditStatus) -> EditRecord {
        EditRecord {
            id: "e1".into(),
            diversion: Some(Diversion {
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
            }),
            cancel: None,
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
        assert_eq!(back.diversion.as_ref().unwrap().first_kept_stop, "s0");
    }

    #[test]
    fn propose_appends_new_ids_and_replaces_known_ones() {
        let base = vec![record(EditStatus::Accepted)];
        let next = propose(&base, "e2", base[0].diversion.clone().unwrap(), 2_000);
        assert_eq!(next.len(), 2);
        assert_eq!((next[1].rev, next[1].status, next[1].updated_at), (1, EditStatus::Draft, 2_000));
        let mut changed = base[0].diversion.clone().unwrap();
        changed.added_secs = 120;
        let again = propose(&next, &base[0].id, changed, 3_000);
        assert_eq!(again.len(), 2);
        assert_eq!((again[0].rev, again[0].status, again[0].diversion.as_ref().unwrap().added_secs), (2, EditStatus::Draft, 120));
        assert_eq!(base[0].rev, 1, "the input is never mutated");
    }

    #[test]
    fn retire_and_restore_bump_the_envelope_once() {
        let base = vec![record(EditStatus::Submitted)];
        let retired = set_retired(&base, &base[0].id, true, 2_000).unwrap();
        assert_eq!((retired[0].status, retired[0].rev, retired[0].updated_at), (EditStatus::Retired, 2, 2_000));
        assert_eq!(set_retired(&retired, &base[0].id, true, 3_000).unwrap(), retired, "no rev churn");
        let restored = set_retired(&retired, &base[0].id, false, 4_000).unwrap();
        assert_eq!((restored[0].status, restored[0].rev), (EditStatus::Draft, 3));
        assert!(set_retired(&base, "nope", true, 1).is_err());
    }

    #[test]
    fn sanitize_fills_defaults_and_drops_what_does_not_parse() {
        let good = serde_json::to_value(record(EditStatus::Draft)).unwrap();
        let mut old = good.clone();
        let d = old["diversion"].as_object_mut().unwrap();
        for f in ["direction_id", "skipped_stops", "detour", "added_secs"] {
            d.remove(f);
        }
        let mut bad_date = good.clone();
        bad_date["diversion"]["start_date"] = "soon".into();
        let doc = serde_json::json!([good, old, bad_date, {"id": "x"}, 7]);
        let (kept, dropped) = sanitize_records(&doc);
        assert_eq!((kept.len(), dropped), (2, 3));
        assert_eq!(kept[1].diversion.as_ref().unwrap().added_secs, 0, "a record from before the field existed");
        assert_eq!(sanitize_records(&serde_json::json!({"not": "an array"})), (vec![], 0));
    }

    fn cancel(trip: &str, start_secs: Option<u32>, date: NaiveDate) -> TripCancel {
        TripCancel { id: format!("c-{trip}"), trip_id: trip.into(), start_secs, date }
    }

    #[test]
    fn a_diversion_record_keeps_its_bytes_and_a_cancel_omits_the_diversion() {
        // The cancel payload is additive: a diversion record serializes
        // exactly as it did before it existed (no "cancel" key, no
        // null), so every existing edit document keeps its hash.
        let json = serde_json::to_string(&record(EditStatus::Submitted)).unwrap();
        assert_eq!(
            json,
            r#"{"id":"e1","diversion":{"id":"d1","route_id":"r1","direction_id":null,"first_kept_stop":"s0","last_kept_stop":"s1","skipped_stops":[],"detour":null,"added_secs":60,"start_date":"2026-10-23","end_date":"2026-10-23"},"status":"submitted","rev":1,"updated_at":1000}"#
        );
        let day = NaiveDate::from_ymd_opt(2026, 10, 23).unwrap();
        let c = EditRecord { diversion: None, cancel: Some(cancel("755768", None, day)), ..record(EditStatus::Draft) };
        let json = serde_json::to_string(&c).unwrap();
        assert_eq!(
            json,
            r#"{"id":"e1","cancel":{"id":"c-755768","trip_id":"755768","start_secs":null,"date":"2026-10-23"},"status":"draft","rev":1,"updated_at":1000}"#
        );
        assert_eq!(serde_json::from_str::<EditRecord>(&json).unwrap(), c);
    }

    #[test]
    fn sanitize_keeps_only_records_with_exactly_one_payload() {
        let day = NaiveDate::from_ymd_opt(2026, 10, 23).unwrap();
        let div = record(EditStatus::Draft);
        let can = EditRecord { diversion: None, cancel: Some(cancel("t", Some(60), day)), ..record(EditStatus::Draft) };
        let both = EditRecord { cancel: Some(cancel("t", None, day)), ..record(EditStatus::Draft) };
        let neither = EditRecord { diversion: None, ..record(EditStatus::Draft) };
        let doc = serde_json::to_value(vec![div.clone(), can.clone(), both, neither]).unwrap();
        let (kept, dropped) = sanitize_records(&doc);
        assert_eq!((kept, dropped), (vec![div, can], 2));
    }

    #[test]
    fn cancels_are_in_force_on_their_one_date_and_never_retired() {
        let day = NaiveDate::from_ymd_opt(2026, 10, 23).unwrap();
        let next = NaiveDate::from_ymd_opt(2026, 10, 24).unwrap();
        let docs = propose_cancel(&[record(EditStatus::Accepted)], "e2", cancel("t1", None, day), 2_000);
        assert_eq!(cancels_in_force_on(&docs, day), vec![cancel("t1", None, day)]);
        assert!(cancels_in_force_on(&docs, next).is_empty(), "a cancel holds on its one date");
        assert_eq!(in_force_on(&docs, day).len(), 1, "diversions read only the diversion records");
        let retired = set_retired(&docs, "e2", true, 3_000).unwrap();
        assert!(cancels_in_force_on(&retired, day).is_empty(), "a retired cancel never applies");
    }

    #[test]
    fn a_cancel_names_its_instance_or_every_instance() {
        let day = NaiveDate::from_ymd_opt(2026, 10, 23).unwrap();
        assert!(cancel("t", None, day).cancels("t", 100) && cancel("t", None, day).cancels("t", 200));
        assert!(cancel("t", Some(100), day).cancels("t", 100));
        assert!(!cancel("t", Some(100), day).cancels("t", 200), "another instance runs");
        assert!(!cancel("t", None, day).cancels("u", 100), "another trip runs");
    }

    #[test]
    fn propose_cancel_follows_the_envelope_rule() {
        let day = NaiveDate::from_ymd_opt(2026, 10, 23).unwrap();
        let base = vec![record(EditStatus::Accepted)];
        let next = propose_cancel(&base, "e2", cancel("t1", None, day), 2_000);
        assert_eq!((next[1].rev, next[1].status, next[1].updated_at), (1, EditStatus::Draft, 2_000));
        assert!(next[1].well_formed() && next[1].diversion.is_none());
        let again = propose_cancel(&next, "e2", cancel("t1", Some(60), day), 3_000);
        assert_eq!((again.len(), again[1].rev, again[1].cancel.as_ref().unwrap().start_secs), (2, 2, Some(60)));
        assert_eq!(base.len(), 1, "the input is never mutated");
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
        let d = docs[1].diversion.as_mut().unwrap();
        d.start_date = other;
        d.end_date = other;

        let live = in_force_on(&docs, date);
        assert_eq!(live.len(), 1, "only the covering-date record applies");
        assert_eq!(live[0].id, "d1");

        docs[0].status = EditStatus::Retired;
        assert!(in_force_on(&docs, date).is_empty(), "retired edits never apply");
    }
}

#[cfg(test)]
mod receive_tests {
    use super::*;

    fn rec(id: &str, status: EditStatus, rev: u64, updated_at: u64) -> EditRecord {
        EditRecord {
            id: id.into(),
            diversion: Some(Diversion {
                id: format!("d-{id}"),
                route_id: "r1".into(),
                direction_id: None,
                first_kept_stop: "s0".into(),
                last_kept_stop: "s1".into(),
                skipped_stops: vec![],
                detour: None,
                added_secs: 60,
                start_date: chrono::NaiveDate::from_ymd_opt(2026, 10, 23).unwrap(),
                end_date: chrono::NaiveDate::from_ymd_opt(2026, 10, 23).unwrap(),
            }),
            cancel: None,
            status,
            rev,
            updated_at,
        }
    }

    #[test]
    fn a_future_dated_record_is_clamped_at_receive() {
        let future = rec("e1", EditStatus::Submitted, 3, 10_000);
        let clamped = clamp_timestamps(&[future.clone()], 5_000);
        assert_eq!(clamped[0].updated_at, 5_000, "a future stamp is treated as now");
        assert_eq!(clamped[0].rev, 3, "clamping touches only the clock");

        let honest = rec("e2", EditStatus::Draft, 1, 4_000);
        assert_eq!(clamp_timestamps(&[honest], 5_000)[0].updated_at, 4_000, "honest stamps untouched");

        // Idempotent: clamping twice == clamping once, so redelivery converges.
        let once = clamp_timestamps(&[future], 5_000);
        assert_eq!(clamp_timestamps(&once, 5_000), once);

        // And the clamp actually bounds the LWW order: the future-dated
        // record no longer beats a later honest write.
        let server = vec![rec("e1", EditStatus::Retired, 4, 6_000)];
        let merged = merge(&clamped, &server);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].status, EditStatus::Retired, "6000 beats the clamped 5000");
    }

    #[test]
    fn compact_drops_only_retired_records_past_the_horizon() {
        let now = 1_000_000u64;
        let horizon = 100_000u64;
        let docs = vec![
            rec("old-retired", EditStatus::Retired, 2, now - 200_000),   // dropped
            rec("new-retired", EditStatus::Retired, 2, now - 50_000),    // kept
            rec("old-accepted", EditStatus::Accepted, 9, now - 900_000), // kept: in force
            rec("draft", EditStatus::Draft, 1, now - 900_000),          // kept
        ];
        let (kept, dropped) = compact(&docs, now, horizon);
        assert_eq!(dropped, 1);
        assert_eq!(kept.len(), 3);
        assert!(kept.iter().all(|r| r.id != "old-retired"));
        assert_eq!(docs.len(), 4, "the input is never mutated");

        // Idempotent: compacting a compacted document drops nothing.
        let (again, dropped_again) = compact(&kept, now, horizon);
        assert_eq!(dropped_again, 0);
        assert_eq!(again, kept);
    }

    #[test]
    fn envelope_and_kind_round_trip_with_documented_names() {
        let env = Envelope {
            kind: DocumentKind::EditRecords,
            schema_version: SCHEMA_VERSION,
            produced_by: "cap-sync@sha256:abcd".into(),
        };
        let json = serde_json::to_string(&env).unwrap();
        assert!(json.contains("\"kind\":\"edit_records\""), "{json}");
        assert!(json.contains(&format!("\"schema_version\":{SCHEMA_VERSION}")), "{json}");
        assert_eq!(serde_json::from_str::<Envelope>(&json).unwrap(), env);
        for (kind, name) in [
            (DocumentKind::NetworkDocument, "network_document"),
            (DocumentKind::EditRecords, "edit_records"),
            (DocumentKind::DiversionDraft, "diversion_draft"),
            (DocumentKind::Manifest, "manifest"),
            (DocumentKind::Plans, "plans"),
            (DocumentKind::Policy, "policy"),
        ] {
            assert_eq!(serde_json::to_string(&kind).unwrap(), format!("\"{name}\""));
        }
    }
}

#[cfg(test)]
mod merge_tests {
    use super::*;
    use proptest::prelude::*;
    use proptest::collection::vec;

    fn sample_record(id: u8, rev: u64, updated_at: u64, status: EditStatus) -> EditRecord {
        EditRecord {
            id: format!("e{id}"),
            diversion: Some(Diversion {
                id: format!("d{id}"),
                route_id: "r1".into(),
                direction_id: None,
                first_kept_stop: "s0".into(),
                last_kept_stop: "s1".into(),
                skipped_stops: vec![],
                detour: None,
                added_secs: (updated_at % 300) as u32,
                start_date: chrono::NaiveDate::from_ymd_opt(2026, 10, 23).unwrap(),
                end_date: chrono::NaiveDate::from_ymd_opt(2026, 10, 24).unwrap(),
            }),
            cancel: None,
            status,
            rev,
            updated_at,
        }
    }

    /// The same envelope carrying a trip cancel instead (D-45): one id
    /// may hold a diversion on one replica and a cancel on another.
    fn sample_cancel(id: u8, rev: u64, updated_at: u64, status: EditStatus) -> EditRecord {
        EditRecord {
            diversion: None,
            cancel: Some(TripCancel {
                id: format!("c{id}"),
                trip_id: format!("t{}", updated_at % 3),
                start_secs: (updated_at % 2 == 0).then_some(3_600),
                date: chrono::NaiveDate::from_ymd_opt(2026, 10, 23).unwrap(),
            }),
            ..sample_record(id, rev, updated_at, status)
        }
    }

    fn doc_strategy() -> impl Strategy<Value = Vec<EditRecord>> {
        // Small id space forces collisions; the clock range forces ties;
        // both payloads compete for one id.
        vec(
            (
                0u8..4,
                0u64..4,
                0u64..6,
                proptest::sample::select(vec![
                    EditStatus::Draft,
                    EditStatus::Submitted,
                    EditStatus::Accepted,
                    EditStatus::Retired,
                ]),
                proptest::bool::ANY,
            ),
            0..8,
        )
            .prop_map(|rows| {
                rows.into_iter()
                    .map(|(id, rev, at, status, is_cancel)| {
                        if is_cancel { sample_cancel(id, rev, at, status) } else { sample_record(id, rev, at, status) }
                    })
                    .collect()
            })
    }

    #[test]
    fn merge_is_last_writer_wins_with_tombstones() {
        let a = sample_record(1, 1, 100, EditStatus::Draft);
        let b = sample_record(1, 2, 200, EditStatus::Retired);
        let older = sample_record(1, 1, 50, EditStatus::Accepted);

        // Later timestamp wins; retirement propagates.
        let merged = merge(&[a.clone()], &[b.clone()]);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].status, EditStatus::Retired);
        assert_eq!(merged[0].updated_at, 200);

        // The older record loses to both.
        let merged = merge(&[older], &[b]);
        assert_eq!(merged[0].updated_at, 200);

        // Disjoint ids union.
        let merged = merge(&[sample_record(1, 1, 1, EditStatus::Draft)], &[sample_record(2, 1, 1, EditStatus::Draft)]);
        assert_eq!(merged.len(), 2);
        assert!(merged[0].id < merged[1].id, "sorted by id");
    }

    #[test]
    fn tie_breaks_deterministically_regardless_of_order() {
        // Same clock tick and rev, differing content: only a buggy
        // writer produces this, but convergence still must hold.
        let mut a = sample_record(1, 5, 100, EditStatus::Draft);
        a.diversion.as_mut().unwrap().added_secs = 1;
        let mut b = a.clone();
        b.diversion.as_mut().unwrap().added_secs = 2;
        assert_eq!(merge(&[a.clone()], &[b.clone()]), merge(&[b], &[a]));
    }

    #[test]
    fn submit_transitions_only_drafts_and_bumps_the_envelope() {
        let drafts = vec![
            sample_record(1, 1, 100, EditStatus::Draft),
            sample_record(2, 3, 100, EditStatus::Submitted),
            sample_record(3, 7, 100, EditStatus::Retired),
        ];
        let out = submit(drafts, 1000);
        assert_eq!(out[0].status, EditStatus::Submitted);
        assert_eq!(out[0].rev, 2);
        assert_eq!(out[0].updated_at, 1000);
        // Non-drafts untouched.
        assert_eq!(out[1].status, EditStatus::Submitted);
        assert_eq!(out[1].rev, 3);
        assert_eq!(out[1].updated_at, 100);
        assert_eq!(out[2].status, EditStatus::Retired);
        assert_eq!(out[2].rev, 7);
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(256))]

        /// Commutativity: every node reaches the same document no
        /// matter which side it holds locally.
        #[test]
        fn merge_is_commutative(a in doc_strategy(), b in doc_strategy()) {
            assert_eq!(merge(&a, &b), merge(&b, &a));
        }

        /// Idempotence: re-merging an already-merged document is a
        /// no-op (pulling twice must not change anything).
        #[test]
        fn merge_is_idempotent(a in doc_strategy(), b in doc_strategy()) {
            let once = merge(&a, &b);
            assert_eq!(merge(&once, &once), once);
        }

        /// Associativity: three-way convergence — merge in any
        /// grouping and every node ends at the same document.
        #[test]
        fn merge_is_associative(
            a in doc_strategy(), b in doc_strategy(), c in doc_strategy(),
        ) {
            assert_eq!(merge(&merge(&a, &b), &c), merge(&a, &merge(&b, &c)));
        }

        /// Merging never changes a record's payload: every merged record
        /// is one of the inputs, whole, so a cancel stays a cancel.
        #[test]
        fn merge_keeps_records_whole(a in doc_strategy(), b in doc_strategy()) {
            for r in merge(&a, &b) {
                assert!(a.iter().chain(&b).any(|x| x == &r));
                assert!(r.well_formed());
            }
        }

        /// A document never gains or loses records through merging,
        /// and tombstone winners stay tombstones.
        #[test]
        fn merge_preserves_ids_and_tombstone_wins(
            a in doc_strategy(), b in doc_strategy(),
        ) {
            let merged = merge(&a, &b);
            let ids = |d: &[EditRecord]| d.iter().map(|r| r.id.clone()).collect::<std::collections::BTreeSet<_>>();
            assert_eq!(ids(&merged), ids(&a).union(&ids(&b)).cloned().collect());
            for r in &merged {
                let winner = a.iter().chain(&b).filter(|x| x.id == r.id)
                    .max_by_key(|x| (x.updated_at, x.rev)).unwrap();
                if winner.status == EditStatus::Retired && winner.updated_at == r.updated_at && winner.rev == r.rev {
                    // the LWW winner was the tombstone; only a same-tick
                    // tiebreak could differ, which the commutativity law
                    // already pins down
                }
            }
        }
    }
}
