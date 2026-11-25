//! The edit document keeps records whole, keyed by id, with last-writer-wins
//! per id and `retired` as the tombstone, so `merge` is a semilattice.
//! Callers stamp `updated_at` and `rev`. A revert writes the record back at
//! the next rev, so it wins the merge over a copy that a page still holds
//! (`revert_record`, D-193).

use ts_rs::TS;
use serde::{Deserialize, Serialize};

pub mod version;
pub mod load;
pub use version::{EditDocument, EditsKind, MigrationNote, KIND, SCHEMA_VERSION};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
pub struct DetourPoint {
    pub lat: f64,
    pub lon: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
pub struct Diversion {
    pub id: String,
    #[schemars(extend("x-references" = "network.route"))]
    pub route_id: String,
    /// `None` means both directions.
    #[serde(default)]
    pub direction_id: Option<u8>,
    /// The detour runs between the kept stops, and they stay served.
    #[schemars(extend("x-references" = "network.stop"))]
    pub first_kept_stop: String,
    #[schemars(extend("x-references" = "network.stop"))]
    pub last_kept_stop: String,
    /// Never includes the kept stops.
    #[serde(default)]
    #[schemars(extend("x-references" = "network.stop"))]
    pub skipped_stops: Vec<String>,
    /// `None` skips the stops and leaves the alignment unchanged.
    #[serde(default)]
    pub detour: Option<Vec<DetourPoint>>,
    /// Seconds of running time, added from the last kept stop onward.
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
    #[serde(flatten)]
    #[ts(skip)]
    pub extra: transit_version::Extra,
}

impl Diversion {
    pub fn covers(&self, date: chrono::NaiveDate) -> bool {
        self.start_date <= date && date <= self.end_date
    }
}

/// The boards list a cancelled trip among the visits that the edits of the
/// day took away (D-100). It cancels the whole trip, never from a stop on
/// (D-45).
/// Cover, hold, and swap belong to CAD/AVL (D-212).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
pub struct TripCancel {
    pub id: String,
    #[schemars(extend("x-references" = "network.trip"))]
    pub trip_id: String,
    /// Seconds after the service-day anchor. `None` cancels every instance.
    #[serde(default)]
    pub start_secs: Option<u32>,
    #[serde(with = "iso_date")]
    #[schemars(with = "chrono::NaiveDate")]
    #[ts(type = "string")]
    pub date: chrono::NaiveDate,
    #[serde(flatten)]
    #[ts(skip)]
    pub extra: transit_version::Extra,
}

impl TripCancel {
    pub fn covers(&self, date: chrono::NaiveDate) -> bool {
        self.date == date
    }

    pub fn cancels(&self, trip: &str, start_secs: u32) -> bool {
        self.trip_id == trip && self.start_secs.is_none_or(|s| s == start_secs)
    }
}

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

/// `Retired` is the tombstone: the record stays so that sync sees the
/// deletion, and the UI hides it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
#[serde(rename_all = "lowercase")]
pub enum EditStatus {
    Draft,
    Submitted,
    Accepted,
    Retired,
}

impl EditStatus {
    /// Drafts count as in force, so the dispatcher can preview them.
    pub fn in_force(self) -> bool {
        !matches!(self, EditStatus::Retired)
    }
}

/// Exactly one of `diversion` and `cancel`. [`sanitize_records`] drops a
/// record with neither or both. An absent payload is omitted, never `null`,
/// so a diversion record keeps its bytes and its hash.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
pub struct EditRecord {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diversion: Option<Diversion>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cancel: Option<TripCancel>,
    pub status: EditStatus,
    /// Monotonic per id. The writer bumps it on every change.
    #[ts(type = "number")]
    pub rev: u64,
    /// Unix epoch milliseconds.
    #[ts(type = "number")]
    pub updated_at: u64,
    #[serde(flatten)]
    #[ts(skip)]
    pub extra: transit_version::Extra,
}

impl EditRecord {
    pub fn well_formed(&self) -> bool {
        self.diversion.is_some() != self.cancel.is_some()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
pub struct EditReport {
    pub id: String,
    pub valid: bool,
    pub error: Option<String>,
    /// 0 means a no-op, even when `valid`.
    pub affected_trips: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
pub struct FeedManifest {
    /// sha256 of the authoritative GTFS zip.
    pub feed_hash: String,
    pub feed_version: Option<String>,
    /// Unix epoch seconds.
    #[ts(type = "number")]
    pub generated_at: u64,
    pub publisher: Option<String>,
}

pub fn in_force_on(records: &[EditRecord], date: chrono::NaiveDate) -> Vec<Diversion> {
    records
        .iter()
        .filter(|r| r.status.in_force())
        .filter_map(|r| r.diversion.as_ref())
        .filter(|d| d.covers(date))
        .cloned()
        .collect()
}

pub fn cancels_in_force_on(records: &[EditRecord], date: chrono::NaiveDate) -> Vec<TripCancel> {
    records
        .iter()
        .filter(|r| r.status.in_force())
        .filter_map(|r| r.cancel.as_ref())
        .filter(|c| c.covers(date))
        .cloned()
        .collect()
}

/// The LWW order is a strict total order, so `merge` is a semilattice. A
/// later `updated_at` wins, then a higher `rev`, then the greater serialized
/// record (a tie that only a buggy writer makes).
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

/// The result is sorted by id, so equal inputs serialize identically on every
/// host. A later retirement beats an older in-force version.
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

pub fn propose(records: &[EditRecord], record_id: &str, diversion: Diversion, now_ms: u64) -> Vec<EditRecord> {
    put(records, record_id, Some(diversion), None, now_ms)
}

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
            extra: Default::default(),
        }),
    }
    next
}

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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
pub struct Envelope {
    pub kind: DocumentKind,
    pub schema_version: u32,
    pub produced_by: String,
}

/// A person sees these fields flagged. The write path never refuses them
/// (D-167).
pub fn unknown_fields(records: &[EditRecord]) -> Vec<String> {
    let mut names = std::collections::BTreeSet::new();
    for r in records {
        names.extend(r.extra.names().map(String::from));
        if let Some(d) = &r.diversion {
            names.extend(d.extra.names().map(String::from));
        }
        if let Some(c) = &r.cancel {
            names.extend(c.extra.names().map(String::from));
        }
    }
    names.into_iter().collect()
}

/// A record that claims a future `updated_at` counts as written now (D-56).
/// Clamping twice changes nothing, so redelivery converges.
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

pub fn compact(records: &[EditRecord], now_ms: u64, horizon_ms: u64) -> (Vec<EditRecord>, usize) {
    compact_keeping(records, now_ms, horizon_ms, &std::collections::BTreeSet::new())
}

/// Like [`compact`], but a record whose id is in `keep` stays, because a live
/// reference names it (D-231).
pub fn compact_keeping(records: &[EditRecord], now_ms: u64, horizon_ms: u64, keep: &std::collections::BTreeSet<String>) -> (Vec<EditRecord>, usize) {
    let horizon = now_ms.saturating_sub(horizon_ms);
    let mut dropped = 0;
    let kept = records
        .iter()
        .filter(|r| {
            let old_tombstone = r.status == EditStatus::Retired && r.updated_at < horizon && !keep.contains(&r.id);
            if old_tombstone {
                dropped += 1;
            }
            !old_tombstone
        })
        .cloned()
        .collect();
    (kept, dropped)
}

/// The record that a revert writes back (D-193): the record before the
/// reverted write, or, when that write created it, the current record
/// retired. Either gets the next rev and `now_ms`, so it wins the merge over
/// the reverted copy.
pub fn revert_record(prior: Option<&EditRecord>, now: Option<&EditRecord>, now_ms: u64) -> Option<EditRecord> {
    let next_rev = now.map_or(0, |r| r.rev).max(prior.map_or(0, |r| r.rev)) + 1;
    match (prior, now) {
        (Some(p), _) => Some(EditRecord { rev: next_rev, updated_at: now_ms, ..p.clone() }),
        (None, Some(n)) => Some(EditRecord { status: EditStatus::Retired, rev: next_rev, updated_at: now_ms, ..n.clone() }),
        (None, None) => None,
    }
}

/// An older document is refused with the op that brings it forward, and a
/// newer one naming both versions. A document one version ahead is read down
/// by a lossless step. A read-only step refuses the write (`Versioned::AHEAD`,
/// D-186).
pub fn read_document(document: &serde_json::Value) -> Result<(EditDocument, usize), String> {
    use transit_version::{ReadAs, Versioned};
    let found = version::schema_version_of(document)?;
    let (records, dropped) = match transit_version::reads::<EditDocument>(found)? {
        ReadAs::Current => sanitize_records(&document["records"]),
        ahead @ ReadAs::Ahead(_) => {
            transit_version::writes_over::<EditDocument>(found, ahead)?;
            sanitize_records(&EditDocument::step_down(document.clone())?["records"])
        }
    };
    Ok((EditDocument::new(records), dropped))
}

/// A malformed record is dropped and counted, so it never wedges the
/// document. Anything but an array holds no records.
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
                extra: Default::default(),
            }),
            cancel: None,
            status,
            rev: 1,
            updated_at: 1_000,
            extra: Default::default(),
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
        TripCancel { id: format!("c-{trip}"), trip_id: trip.into(), start_secs, date, extra: Default::default() }
    }

    #[test]
    fn a_diversion_record_keeps_its_bytes_and_a_cancel_omits_the_diversion() {
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
                extra: Default::default(),
            }),
            cancel: None,
            status,
            rev,
            updated_at,
            extra: Default::default(),
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

        let once = clamp_timestamps(&[future], 5_000);
        assert_eq!(clamp_timestamps(&once, 5_000), once);

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

        let (again, dropped_again) = compact(&kept, now, horizon);
        assert_eq!(dropped_again, 0);
        assert_eq!(again, kept);
    }

    #[test]
    fn compact_keeping_never_drops_a_named_record() {
        let now = 1_000_000u64;
        let docs = vec![
            rec("named", EditStatus::Retired, 2, now - 200_000),
            rec("free", EditStatus::Retired, 2, now - 200_000),
        ];
        let keep: std::collections::BTreeSet<String> = ["named".to_string()].into_iter().collect();
        let (kept, dropped) = compact_keeping(&docs, now, 100_000, &keep);
        assert_eq!((dropped, kept.iter().map(|r| r.id.as_str()).collect::<Vec<_>>()), (1, vec!["named"]));
        assert_eq!(compact(&docs, now, 100_000), compact_keeping(&docs, now, 100_000, &Default::default()), "compact keeps nothing named");
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
                extra: Default::default(),
            }),
            cancel: None,
            status,
            rev,
            updated_at,
            extra: Default::default(),
        }
    }

    fn sample_cancel(id: u8, rev: u64, updated_at: u64, status: EditStatus) -> EditRecord {
        EditRecord {
            diversion: None,
            cancel: Some(TripCancel {
                id: format!("c{id}"),
                trip_id: format!("t{}", updated_at % 3),
                start_secs: (updated_at % 2 == 0).then_some(3_600),
                date: chrono::NaiveDate::from_ymd_opt(2026, 10, 23).unwrap(),
                extra: Default::default(),
            }),
            ..sample_record(id, rev, updated_at, status)
        }
    }

    fn doc_strategy() -> impl Strategy<Value = Vec<EditRecord>> {
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

    proptest! {
        #[test]
        fn a_reverted_record_wins_the_merge(prior in proptest::option::of(doc_strategy()), now in doc_strategy(), at in 0u64..10) {
            let Some(now) = now.into_iter().next() else { return Ok(()) };
            let prior = prior.and_then(|p| p.into_iter().next()).map(|p| EditRecord { id: now.id.clone(), ..p });
            let now_ms = now.updated_at.max(prior.as_ref().map_or(0, |p| p.updated_at)) + at;
            let back = revert_record(prior.as_ref(), Some(&now), now_ms).unwrap();
            prop_assert_eq!(merge(std::slice::from_ref(&now), std::slice::from_ref(&back)), vec![back.clone()]);
            prop_assert_eq!(merge(std::slice::from_ref(&back), std::slice::from_ref(&now)), vec![back.clone()]);
            match &prior {
                Some(p) => prop_assert_eq!((&back.diversion, &back.cancel, back.status), (&p.diversion, &p.cancel, p.status)),
                None => prop_assert_eq!((&back.diversion, &back.cancel, back.status), (&now.diversion, &now.cancel, EditStatus::Retired)),
            }
        }
    }

    #[test]
    fn merge_is_last_writer_wins_with_tombstones() {
        let a = sample_record(1, 1, 100, EditStatus::Draft);
        let b = sample_record(1, 2, 200, EditStatus::Retired);
        let older = sample_record(1, 1, 50, EditStatus::Accepted);

        let merged = merge(&[a.clone()], &[b.clone()]);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].status, EditStatus::Retired);
        assert_eq!(merged[0].updated_at, 200);

        let merged = merge(&[older], &[b]);
        assert_eq!(merged[0].updated_at, 200);

        let merged = merge(&[sample_record(1, 1, 1, EditStatus::Draft)], &[sample_record(2, 1, 1, EditStatus::Draft)]);
        assert_eq!(merged.len(), 2);
        assert!(merged[0].id < merged[1].id, "sorted by id");
    }

    #[test]
    fn tie_breaks_deterministically_regardless_of_order() {
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
        assert_eq!(out[1].status, EditStatus::Submitted);
        assert_eq!(out[1].rev, 3);
        assert_eq!(out[1].updated_at, 100);
        assert_eq!(out[2].status, EditStatus::Retired);
        assert_eq!(out[2].rev, 7);
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(256))]

        #[test]
        fn merge_is_commutative(a in doc_strategy(), b in doc_strategy()) {
            assert_eq!(merge(&a, &b), merge(&b, &a));
        }

        #[test]
        fn merge_is_idempotent(a in doc_strategy(), b in doc_strategy()) {
            let once = merge(&a, &b);
            assert_eq!(merge(&once, &once), once);
        }

        #[test]
        fn merge_is_associative(
            a in doc_strategy(), b in doc_strategy(), c in doc_strategy(),
        ) {
            assert_eq!(merge(&merge(&a, &b), &c), merge(&a, &merge(&b, &c)));
        }

        #[test]
        fn merge_keeps_records_whole(a in doc_strategy(), b in doc_strategy()) {
            for r in merge(&a, &b) {
                assert!(a.iter().chain(&b).any(|x| x == &r));
                assert!(r.well_formed());
            }
        }

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
                    // Only a same-tick tiebreak could differ. Commutativity
                    // pins it.
                }
            }
        }
    }
}
