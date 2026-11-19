//! The disruptions log (D-190): the record of the edit document (the
//! detours and trip cancels) as a list of entries.
//!
//! Each entry is one transaction. It holds each record that the
//! transaction changed: the record's hash before, and its new state,
//! whole. The current document is the fold of the entries. A
//! checkpoint is the fold up to one `seq`, so a reader starts from a
//! checkpoint and folds only the entries after it.
//!
//! A mistake is fixed by a new entry. A revert writes back the states
//! before an entry, and is refused when a later entry changed one of
//! those records. The revert stamps each record it writes back with the
//! next rev and the host's clock, so it wins the LWW merge over a copy
//! that a page still holds. A record that the entry created is retired,
//! because retirement is the tombstone.
//!
//! Everything here is pure. The record hash is the caller's
//! (`cap-sync` passes the canonical hash), so this crate needs no hash
//! code. The host stores the entries and gives them back in order.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{EditDocument, EditRecord, EditStatus};

/// The name of the log.
pub const LOG: &str = "disruptions";

/// The entry version this build reads and writes.
pub const ENTRY_VERSION: u32 = 1;

/// The checkpoint version this build reads and writes.
pub const CHECKPOINT_VERSION: u32 = 1;

transit_version::kind_tag!(
    /// The entry's kind tag: always `"disruptions_entry"`.
    EntryKind = "disruptions_entry"
);

transit_version::kind_tag!(
    /// The checkpoint's kind tag: always `"disruptions_checkpoint"`.
    CheckpointKind = "disruptions_checkpoint"
);

transit_version::kind_tag!(
    /// The kind of a record that an entry changes: always
    /// `"edit_record"`.
    RecordKind = "edit_record"
);

/// What asked for an entry.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export, export_to = "transit_sync/")]
pub struct Intent {
    /// The endpoint or op that asked for the write, for example
    /// `POST /edits`, `compact_edits`, `migrate`,
    /// `POST /logs/disruptions/revert`, or
    /// `POST /logs/disruptions/close-gap`.
    pub op: String,
    /// The content hash of the request's arguments: the document the
    /// client sent, or the head that a migration copied.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub args_hash: Option<String>,
    /// A note from a person or a tool. An entry that closes a gap names
    /// the cause here.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub note: Option<String>,
    /// Fields this build does not know, kept as written (D-167).
    #[serde(flatten)]
    #[ts(skip)]
    pub extra: transit_version::Extra,
}

/// One record that an entry changed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export, export_to = "transit_sync/")]
pub struct Change {
    #[ts(type = "\"edit_record\"")]
    pub kind: RecordKind,
    /// The record's id.
    pub id: String,
    /// The record's hash before the entry. Absent when the record is
    /// new.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub before: Option<String>,
    /// The record's state after the entry, whole. Absent when the entry
    /// drops the record from the fold (the tombstone compaction, D-57).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub after: Option<EditRecord>,
    /// Fields this build does not know, kept as written (D-167).
    #[serde(flatten)]
    #[ts(skip)]
    pub extra: transit_version::Extra,
}

/// The place and the provenance of an entry. The host gives them: the
/// `seq` it will create, the verified writer, its clock, and the module
/// build that answers the entry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export, export_to = "transit_sync/")]
pub struct Header {
    /// The entry's place in the log, from 1.
    #[ts(type = "number")]
    pub seq: u64,
    /// The verified sign-in subject, or the tool (`gc`, `migrate:edits`).
    pub writer: String,
    /// The host's clock, Unix epoch milliseconds.
    #[ts(type = "number")]
    pub at_ms: u64,
    /// The module and its build, for example `cap-sync@sha256:<hex>`.
    pub module: String,
    pub intent: Intent,
}

/// One entry of the disruptions log: one transaction.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export, export_to = "transit_sync/")]
pub struct Entry {
    #[ts(type = "\"disruptions_entry\"")]
    pub kind: EntryKind,
    pub schema_version: u32,
    /// The log's name: always `disruptions`.
    pub log: String,
    #[ts(type = "number")]
    pub seq: u64,
    pub writer: String,
    #[ts(type = "number")]
    pub at_ms: u64,
    pub module: String,
    pub intent: Intent,
    /// The records the entry changed, in id order. Empty only in an
    /// entry that closes a gap.
    pub changes: Vec<Change>,
    /// The `seq` of the entry that this entry reverts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub reverts: Option<u64>,
    /// Fields this build does not know, kept as written (D-167).
    #[serde(flatten)]
    #[ts(skip)]
    pub extra: transit_version::Extra,
}

impl transit_version::Versioned for Entry {
    const KIND: &'static str = EntryKind::KIND;
    const READS: u32 = ENTRY_VERSION;
    fn schema_version(&self) -> u32 {
        self.schema_version
    }
}

impl Entry {
    /// An entry at the header's place, over these changes.
    pub fn new(header: Header, changes: Vec<Change>, reverts: Option<u64>) -> Self {
        Entry {
            kind: EntryKind,
            schema_version: ENTRY_VERSION,
            log: LOG.into(),
            seq: header.seq,
            writer: header.writer,
            at_ms: header.at_ms,
            module: header.module,
            intent: header.intent,
            changes,
            reverts,
            extra: Default::default(),
        }
    }
}

/// The fold of the disruptions log up to `seq`: the edit document as it was
/// after that entry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export, export_to = "transit_sync/")]
pub struct Checkpoint {
    #[ts(type = "\"disruptions_checkpoint\"")]
    pub kind: CheckpointKind,
    pub schema_version: u32,
    /// The log's name: always `disruptions`.
    pub log: String,
    /// The last entry the fold applied.
    #[ts(type = "number")]
    pub seq: u64,
    pub document: EditDocument,
    /// Fields this build does not know, kept as written (D-167).
    #[serde(flatten)]
    #[ts(skip)]
    pub extra: transit_version::Extra,
}

impl transit_version::Versioned for Checkpoint {
    const KIND: &'static str = CheckpointKind::KIND;
    const READS: u32 = CHECKPOINT_VERSION;
    fn schema_version(&self) -> u32 {
        self.schema_version
    }
}

impl Checkpoint {
    /// The checkpoint at `seq` over these records.
    pub fn new(seq: u64, records: Vec<EditRecord>) -> Self {
        Checkpoint {
            kind: CheckpointKind,
            schema_version: CHECKPOINT_VERSION,
            log: LOG.into(),
            seq,
            document: EditDocument::new(records),
            extra: Default::default(),
        }
    }
}

/// One stored entry as the host read it: the `seq` in its key, and its
/// text. The fold reads the text, so an entry that does not read is
/// quarantined and never stops the fold.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "transit_sync/")]
pub struct Stored {
    #[ts(type = "number")]
    pub seq: u64,
    pub text: String,
}

/// An entry that the fold did not apply, and why.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "transit_sync/")]
pub struct Quarantined {
    #[ts(type = "number")]
    pub seq: u64,
    pub reason: String,
}

/// The answer of a fold.
#[derive(Debug, Clone, PartialEq)]
pub struct Folded {
    /// The records, in id order.
    pub records: Vec<EditRecord>,
    /// The last entry the fold read.
    pub seq: u64,
    /// The entries the fold did not apply.
    pub quarantined: Vec<Quarantined>,
}

/// The records an entry holds when `before` becomes `after`: each id
/// whose record differs, in id order, with its hash before and its new
/// state. Equal documents give no changes.
pub fn changes(before: &[EditRecord], after: &[EditRecord], hash: &dyn Fn(&EditRecord) -> String) -> Vec<Change> {
    let old: BTreeMap<&str, &EditRecord> = before.iter().map(|r| (r.id.as_str(), r)).collect();
    let new: BTreeMap<&str, &EditRecord> = after.iter().map(|r| (r.id.as_str(), r)).collect();
    let mut ids: Vec<&str> = old.keys().chain(new.keys()).copied().collect();
    ids.sort_unstable();
    ids.dedup();
    ids.into_iter()
        .filter_map(|id| {
            let (b, a) = (old.get(id).copied(), new.get(id).copied());
            (b != a).then(|| Change {
                kind: RecordKind,
                id: id.to_string(),
                before: b.map(hash),
                after: a.cloned(),
                extra: Default::default(),
            })
        })
        .collect()
}

/// Apply one entry's changes to the state, all or none. Each change's
/// `before` must name the record the state holds (absent for a new
/// record). Otherwise the state is unchanged and the error names the
/// record.
pub fn apply(state: &mut BTreeMap<String, EditRecord>, changes: &[Change], hash: &dyn Fn(&EditRecord) -> String) -> Result<(), String> {
    for c in changes {
        let held = state.get(&c.id).map(hash);
        if held != c.before {
            return Err(format!(
                "record {} is {} in the fold, and the entry expects {}",
                c.id,
                held.as_deref().unwrap_or("absent"),
                c.before.as_deref().unwrap_or("absent"),
            ));
        }
        if c.after.as_ref().is_some_and(|r| r.id != c.id) {
            return Err(format!("the change to record {} carries record {}", c.id, c.after.as_ref().map_or("", |r| r.id.as_str())));
        }
    }
    for c in changes {
        match &c.after {
            Some(r) => state.insert(c.id.clone(), r.clone()),
            None => state.remove(&c.id),
        };
    }
    Ok(())
}

/// Read one stored entry: the kind, the version, the log, and the `seq`
/// in its key.
pub fn read_entry(stored: &Stored) -> Result<Entry, String> {
    let value: serde_json::Value = serde_json::from_str(&stored.text).map_err(|e| format!("not JSON: {e}"))?;
    let found = value.get("schema_version").and_then(serde_json::Value::as_u64).unwrap_or(0);
    if value.get("kind").and_then(serde_json::Value::as_str) != Some(EntryKind::KIND) {
        return Err(format!("not an {} document", EntryKind::KIND));
    }
    if found != u64::from(ENTRY_VERSION) {
        return Err(transit_version::refused(EntryKind::KIND, found as u32, ENTRY_VERSION));
    }
    let entry: Entry = serde_json::from_value(value).map_err(|e| format!("not an entry: {e}"))?;
    if entry.log != LOG {
        return Err(format!("an entry of the {} log, not the {LOG} log", entry.log));
    }
    if entry.seq != stored.seq {
        return Err(format!("the entry says seq {}, and its key says {}", entry.seq, stored.seq));
    }
    Ok(entry)
}

/// Fold the entries after a checkpoint (or from the empty document).
/// The entries come in `seq` order, each after the one before and after
/// the checkpoint. An entry that does not read, or whose changes do not
/// apply, is quarantined with its reason, and the fold continues
/// (D-190, rule 3).
pub fn fold(start: Option<&Checkpoint>, entries: &[Stored], hash: &dyn Fn(&EditRecord) -> String) -> Result<Folded, String> {
    let mut seq = start.map_or(0, |c| c.seq);
    let mut state: BTreeMap<String, EditRecord> =
        start.map(|c| c.document.records.iter().map(|r| (r.id.clone(), r.clone())).collect()).unwrap_or_default();
    let mut quarantined = Vec::new();
    for stored in entries {
        if stored.seq <= seq {
            return Err(format!("entry {} comes after entry {seq}: the entries are not in order", stored.seq));
        }
        seq = stored.seq;
        if let Err(reason) = read_entry(stored).and_then(|e| apply(&mut state, &e.changes, hash)) {
            quarantined.push(Quarantined { seq: stored.seq, reason });
        }
    }
    Ok(Folded { records: state.into_values().collect(), seq, quarantined })
}

/// The changes that revert `entry`: each record it changed goes back to
/// its state before the entry (`before`, the fold just before it),
/// stamped with the next rev and `now_ms` so it wins the merge. A record
/// the entry created is retired instead. A record that a later entry
/// changed (`head` no longer holds what the entry wrote) refuses the
/// revert, and the error names it, because a person decides.
pub fn revert(
    before: &[EditRecord],
    head: &[EditRecord],
    entry: &Entry,
    now_ms: u64,
    hash: &dyn Fn(&EditRecord) -> String,
) -> Result<Vec<Change>, String> {
    let before: BTreeMap<&str, &EditRecord> = before.iter().map(|r| (r.id.as_str(), r)).collect();
    let head: BTreeMap<&str, &EditRecord> = head.iter().map(|r| (r.id.as_str(), r)).collect();
    let mut out = Vec::new();
    for c in &entry.changes {
        let now = head.get(c.id.as_str()).copied();
        if now != c.after.as_ref() {
            return Err(format!("record {} changed after entry {}: a person decides what it becomes", c.id, entry.seq));
        }
        let prior = before.get(c.id.as_str()).copied();
        if prior.map(hash) != c.before {
            return Err(format!("record {}: the fold before entry {} does not match the entry", c.id, entry.seq));
        }
        let next_rev = now.map_or(0, |r| r.rev).max(prior.map_or(0, |r| r.rev)) + 1;
        let back = match (prior, now) {
            (Some(p), _) => Some(EditRecord { rev: next_rev, updated_at: now_ms, ..p.clone() }),
            (None, Some(n)) => Some(EditRecord { status: EditStatus::Retired, rev: next_rev, updated_at: now_ms, ..n.clone() }),
            (None, None) => None,
        };
        if back.as_ref() == now {
            continue;
        }
        out.push(Change { kind: RecordKind, id: c.id.clone(), before: now.map(hash), after: back, extra: Default::default() });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    //! The laws of the disruptions log (D-190):
    //!
    //! 1. the fold of the entries equals the document that whole-document
    //!    commits give for the same writes;
    //! 2. a write that changes nothing gives no changes;
    //! 3. `apply` of `changes(a, b)` turns `a` into `b`, and a stale
    //!    `before` applies nothing;
    //! 4. a fold from a checkpoint equals the fold from the start;
    //! 5. an entry that does not read or apply is quarantined, and the
    //!    fold continues;
    //! 6. a revert writes back the states before, wins the merge over the
    //!    reverted copy, and is refused when a later entry changed the
    //!    same record.
    use super::*;
    use crate::{merge, submit, clamp_timestamps, Diversion};
    use proptest::collection::vec;
    use proptest::prelude::*;

    fn hash(r: &EditRecord) -> String {
        serde_json::to_string(r).unwrap()
    }

    fn record(id: u8, rev: u64, updated_at: u64, status: EditStatus) -> EditRecord {
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

    fn doc() -> impl Strategy<Value = Vec<EditRecord>> {
        vec(
            (
                0u8..5,
                0u64..4,
                0u64..50,
                proptest::sample::select(vec![EditStatus::Draft, EditStatus::Submitted, EditStatus::Accepted, EditStatus::Retired]),
            ),
            0..6,
        )
        .prop_map(|rows| rows.into_iter().map(|(id, rev, at, s)| record(id, rev, at, s)).collect())
    }

    fn header(seq: u64) -> Header {
        Header { seq, writer: "w".into(), at_ms: seq * 10, module: "cap-sync@test".into(), intent: Intent { op: "POST /edits".into(), ..Default::default() } }
    }

    fn stored(entry: &Entry) -> Stored {
        Stored { seq: entry.seq, text: serde_json::to_string(entry).unwrap() }
    }

    /// The server's receive step today: the whole document after one
    /// client write.
    fn receive(client: &[EditRecord], head: &[EditRecord], now_ms: u64) -> Vec<EditRecord> {
        submit(merge(&clamp_timestamps(client, now_ms), head), now_ms)
    }

    /// Run the writes both ways: as whole documents, and as entries.
    fn both_ways(writes: &[Vec<EditRecord>]) -> (Vec<EditRecord>, Vec<Stored>) {
        let mut head: Vec<EditRecord> = Vec::new();
        let mut log = Vec::new();
        for (i, client) in writes.iter().enumerate() {
            let next = receive(client, &head, 100 + i as u64);
            let c = changes(&head, &next, &hash);
            if !c.is_empty() {
                log.push(stored(&Entry::new(header(log.len() as u64 + 1), c, None)));
            }
            head = next;
        }
        (head, log)
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(256))]

        /// Law 1.
        #[test]
        fn fold_equals_whole_document_commits(writes in vec(doc(), 0..8)) {
            let (whole, log) = both_ways(&writes);
            let folded = fold(None, &log, &hash).unwrap();
            prop_assert!(folded.quarantined.is_empty());
            prop_assert_eq!(folded.records, whole);
            prop_assert_eq!(folded.seq, log.len() as u64);
        }

        /// Law 2: a resend of the converged document changes nothing.
        #[test]
        fn a_resend_gives_no_changes(writes in vec(doc(), 1..6)) {
            let (whole, _) = both_ways(&writes);
            let again = receive(&whole, &whole, 1_000);
            prop_assert!(changes(&whole, &again, &hash).is_empty());
        }

        /// Law 3.
        #[test]
        fn apply_of_changes_is_the_next_document(a in doc(), b in doc()) {
            let a = merge(&a, &[]);
            let b = merge(&b, &[]);
            let mut state: BTreeMap<String, EditRecord> = a.iter().map(|r| (r.id.clone(), r.clone())).collect();
            apply(&mut state, &changes(&a, &b, &hash), &hash).unwrap();
            prop_assert_eq!(state.into_values().collect::<Vec<_>>(), b);
        }

        /// Law 4.
        #[test]
        fn a_checkpoint_folds_the_same(writes in vec(doc(), 0..8), cut in 0usize..8) {
            let (_, log) = both_ways(&writes);
            let cut = cut.min(log.len());
            let at = fold(None, &log[..cut], &hash).unwrap();
            let checkpoint = Checkpoint::new(at.seq, at.records);
            let from_checkpoint = fold(Some(&checkpoint), &log[cut..], &hash).unwrap();
            prop_assert_eq!(from_checkpoint, fold(None, &log, &hash).unwrap());
        }
    }

    #[test]
    fn a_stale_before_applies_nothing() {
        let a = vec![record(1, 1, 10, EditStatus::Submitted)];
        let b = vec![record(1, 2, 20, EditStatus::Accepted)];
        let c = changes(&a, &b, &hash);
        let mut state: BTreeMap<String, EditRecord> = BTreeMap::new();
        state.insert("e1".into(), record(1, 9, 99, EditStatus::Retired));
        let held = state.clone();
        assert!(apply(&mut state, &c, &hash).unwrap_err().contains("record e1"));
        assert_eq!(state, held, "all or none");
    }

    #[test]
    fn a_bad_entry_is_quarantined_and_the_fold_continues() {
        let one = vec![record(1, 1, 10, EditStatus::Submitted)];
        let two = vec![record(1, 1, 10, EditStatus::Submitted), record(2, 1, 20, EditStatus::Submitted)];
        let e1 = stored(&Entry::new(header(1), changes(&[], &one, &hash), None));
        let garbage = Stored { seq: 2, text: "{not json".into() };
        let wrong_seq = Stored { seq: 3, text: stored(&Entry::new(header(9), vec![], None)).text };
        let e4 = stored(&Entry::new(header(4), changes(&one, &two, &hash), None));
        let folded = fold(None, &[e1, garbage, wrong_seq, e4], &hash).unwrap();
        assert_eq!(folded.records, two, "the fold applied entries 1 and 4");
        assert_eq!(folded.seq, 4);
        let seqs: Vec<u64> = folded.quarantined.iter().map(|q| q.seq).collect();
        assert_eq!(seqs, vec![2, 3]);
        assert!(folded.quarantined[1].reason.contains("key says 3"));
    }

    #[test]
    fn entries_out_of_order_are_the_hosts_error() {
        let e = stored(&Entry::new(header(2), vec![], None));
        assert!(fold(Some(&Checkpoint::new(2, vec![])), &[e], &hash).is_err());
    }

    #[test]
    fn a_revert_writes_back_the_states_before() {
        let before = vec![record(1, 1, 10, EditStatus::Accepted)];
        let after = vec![record(1, 2, 20, EditStatus::Retired), record(2, 1, 20, EditStatus::Submitted)];
        let entry = Entry::new(header(5), changes(&before, &after, &hash), None);
        let back = revert(&before, &after, &entry, 500, &hash).unwrap();
        let mut state: BTreeMap<String, EditRecord> = after.iter().map(|r| (r.id.clone(), r.clone())).collect();
        apply(&mut state, &back, &hash).unwrap();
        let e1 = &state["e1"];
        assert_eq!((e1.status, e1.diversion.clone()), (EditStatus::Accepted, before[0].diversion.clone()), "the state before");
        assert_eq!((e1.rev, e1.updated_at), (3, 500), "stamped to win the merge");
        assert_eq!(state["e2"].status, EditStatus::Retired, "a record the entry created is retired");
        // A page that still holds the reverted copy does not undo the revert.
        let reverted: Vec<EditRecord> = state.into_values().collect();
        assert_eq!(merge(&after, &reverted), reverted);
    }

    #[test]
    fn a_revert_is_refused_when_a_later_entry_changed_the_record() {
        let before = vec![record(1, 1, 10, EditStatus::Accepted)];
        let after = vec![record(1, 2, 20, EditStatus::Retired)];
        let later = vec![record(1, 3, 30, EditStatus::Accepted)];
        let entry = Entry::new(header(5), changes(&before, &after, &hash), None);
        let refused = revert(&before, &later, &entry, 500, &hash).unwrap_err();
        assert!(refused.contains("record e1") && refused.contains("a person decides"), "{refused}");
    }

    #[test]
    fn an_entry_reads_back_as_written() {
        let entry = Entry::new(header(1), changes(&[], &[record(1, 1, 1, EditStatus::Draft)], &hash), Some(0));
        assert_eq!(read_entry(&stored(&entry)).unwrap(), entry);
        let newer = stored(&entry).text.replace("\"schema_version\":1", "\"schema_version\":2");
        assert!(read_entry(&Stored { seq: 1, text: newer }).is_err());
    }
}
