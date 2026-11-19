//! transit-log: the append-only log of the agency's record (D-190,
//! D-197).
//!
//! A log is the record of one consistency domain. Each entry is one
//! transaction: each record it changed, with the record's hash before
//! and its new state, whole. The current state is the fold of the
//! entries. A mistake is fixed by a new entry (a revert), so the truth
//! before the mistake stays in the record.
//!
//! This crate never reads a record. A record is a JSON value named by
//! its kind (`stop`, `stop_request`) and its id, in its kind's version.
//! The capability that owns a kind turns its document into records and
//! records back into its document ([`split`], [`join`]). So one log can
//! hold the kinds of two capabilities, and one entry can change both.
//!
//! - [`compose`] answers the entry for one write: the records that
//!   differ from the fold, each `before` filled from the fold.
//! - [`apply`] applies an entry, all or none, checking each `before`.
//! - [`fold`] folds stored entries after a checkpoint, and quarantines
//!   an entry it cannot read or apply.
//! - [`revert`] answers the changes that write back the states before an
//!   entry. A record the entry created leaves the fold (D-198). A kind
//!   whose merge would undo that writes back its own records instead
//!   ([`revert_states`], [`revert_with`]).
//!
//! A change's `before` is the hash of the record as the entry that wrote
//! it stored it, never of a migrated copy (D-197). The hash is the
//! caller's (cap-log passes the canonical hash), so the crate is pure.
//! An idempotency key in an entry's intent lands once: a write whose key
//! the fold holds answers the entry that holds it.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The entry version this build reads and writes.
pub const ENTRY_VERSION: u32 = 1;

/// The checkpoint version this build reads and writes.
pub const CHECKPOINT_VERSION: u32 = 1;

transit_version::kind_tag!(
    /// The entry's kind tag: always `"log_entry"`.
    EntryKind = "log_entry"
);

transit_version::kind_tag!(
    /// The checkpoint's kind tag: always `"log_checkpoint"`.
    CheckpointKind = "log_checkpoint"
);

/// A record's hash, as the caller computes it.
pub type Hash<'a> = &'a dyn Fn(&Value) -> String;

/// What asked for an entry.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Intent {
    /// The endpoint or op that asked for the write, for example
    /// `POST /stops` or `POST /logs/stops`.
    pub op: String,
    /// The content hash of the request's arguments.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub args_hash: Option<String>,
    /// The writer's idempotency key: a write sent again with the same
    /// key lands once.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    /// A note from a person or a tool, for example the cause that an
    /// entry closing a gap names.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    /// Fields this build does not know, kept as written (D-167).
    #[serde(flatten)]
    pub extra: transit_version::Extra,
}

/// The place and the provenance of an entry, as the host gives them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Header {
    /// The log's name, for example `stops`.
    pub log: String,
    /// The entry's place in the log, from 1.
    pub seq: u64,
    /// The verified sign-in subject, or the tool.
    pub writer: String,
    /// The host's clock, Unix epoch milliseconds.
    pub at_ms: u64,
    /// The module and its build that answered the entry.
    pub module: String,
    pub intent: Intent,
}

/// One record that an entry changed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Change {
    /// The record's kind, for example `stop` or `stop_request`.
    pub kind: String,
    /// The version of the record's document kind that `after` is in.
    pub version: u32,
    /// The record's id within its kind.
    pub id: String,
    /// The hash of the record as stored before the entry. Absent when
    /// the record is new.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub before: Option<String>,
    /// The record after the entry, whole. Absent when the entry takes
    /// the record out of the fold.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after: Option<Value>,
    /// Fields this build does not know, kept as written (D-167).
    #[serde(flatten)]
    pub extra: transit_version::Extra,
}

/// One entry of a log: one transaction.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Entry {
    pub kind: EntryKind,
    pub schema_version: u32,
    pub log: String,
    pub seq: u64,
    pub writer: String,
    pub at_ms: u64,
    pub module: String,
    pub intent: Intent,
    /// The records the entry changed, in kind and id order. Empty only
    /// in an entry that closes a gap.
    pub changes: Vec<Change>,
    /// The `seq` of the entry this one reverts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reverts: Option<u64>,
    /// Fields this build does not know, kept as written (D-167).
    #[serde(flatten)]
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
            log: header.log,
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

/// A record in the fold, as the entry that wrote it stored it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Held {
    /// The version of its document kind.
    pub version: u32,
    pub value: Value,
}

/// The records of a fold: by kind, then by id.
pub type Records = BTreeMap<String, BTreeMap<String, Held>>;

/// The fold of a log up to `seq`: every record, and every idempotency
/// key with the entry that holds it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Checkpoint {
    pub kind: CheckpointKind,
    pub schema_version: u32,
    pub log: String,
    /// The last entry the fold read.
    pub seq: u64,
    pub records: Records,
    /// Each idempotency key, and the entry that holds it.
    pub keys: BTreeMap<String, u64>,
    /// Fields this build does not know, kept as written (D-167).
    #[serde(flatten)]
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
    /// The empty fold of a log.
    pub fn empty(log: &str) -> Self {
        Checkpoint {
            kind: CheckpointKind,
            schema_version: CHECKPOINT_VERSION,
            log: log.into(),
            seq: 0,
            records: Records::new(),
            keys: BTreeMap::new(),
            extra: Default::default(),
        }
    }

    /// The record of `kind` with `id`, if the fold holds it.
    pub fn get(&self, kind: &str, id: &str) -> Option<&Held> {
        self.records.get(kind)?.get(id)
    }
}

/// The records of one kind after a write: every record the kind's
/// document holds, in its version. A record of the kind that the fold
/// holds and this list does not is taken out of the fold.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct KindRecords {
    pub kind: String,
    pub version: u32,
    pub records: BTreeMap<String, Value>,
}

/// The answer of [`compose`].
#[derive(Debug, Clone, PartialEq)]
pub enum Composed {
    /// The entry to store at the header's `seq`.
    Entry(Entry),
    /// The write changes nothing: no entry.
    Unchanged,
    /// The fold holds the write's idempotency key: the write landed in
    /// this entry before. No entry.
    Landed(u64),
}

/// The changes from the fold to `next`, for the kinds `next` names.
pub fn changes(fold: &Checkpoint, next: &[KindRecords], hash: Hash) -> Vec<Change> {
    let empty = BTreeMap::new();
    let mut out = Vec::new();
    let mut kinds: Vec<&KindRecords> = next.iter().collect();
    kinds.sort_by(|a, b| a.kind.cmp(&b.kind));
    for k in kinds {
        let held = fold.records.get(&k.kind).unwrap_or(&empty);
        let mut ids: Vec<&String> = held.keys().chain(k.records.keys()).collect();
        ids.sort();
        ids.dedup();
        for id in ids {
            let (was, now) = (held.get(id), k.records.get(id));
            let same = match (was, now) {
                (Some(w), Some(n)) => w.version == k.version && hash(&w.value) == hash(n),
                (None, None) => true,
                _ => false,
            };
            if same {
                continue;
            }
            out.push(Change {
                kind: k.kind.clone(),
                version: if now.is_some() { k.version } else { was.map_or(k.version, |w| w.version) },
                id: id.clone(),
                before: was.map(|w| hash(&w.value)),
                after: now.cloned(),
                extra: Default::default(),
            });
        }
    }
    out
}

/// The entry for one write: the changes from the fold to `next`. A
/// write whose key the fold holds has landed already; a write that
/// changes nothing writes no entry.
pub fn compose(fold: &Checkpoint, header: Header, next: &[KindRecords], hash: Hash) -> Composed {
    if let Some(seq) = header.intent.key.as_ref().and_then(|k| fold.keys.get(k)) {
        return Composed::Landed(*seq);
    }
    let changes = changes(fold, next, hash);
    if changes.is_empty() {
        return Composed::Unchanged;
    }
    Composed::Entry(Entry::new(header, changes, None))
}

/// Apply an entry to the fold, all or none. Each change's `before` must
/// name the record the fold holds (absent for a new record), and a key
/// the entry names must be new. Otherwise the fold is unchanged and the
/// error names the record.
pub fn apply(fold: &mut Checkpoint, entry: &Entry, hash: Hash) -> Result<(), String> {
    for c in &entry.changes {
        let held = fold.get(&c.kind, &c.id).map(|h| hash(&h.value));
        if held != c.before {
            return Err(format!(
                "{} {} is {} in the fold, and the entry expects {}",
                c.kind,
                c.id,
                held.as_deref().unwrap_or("absent"),
                c.before.as_deref().unwrap_or("absent"),
            ));
        }
    }
    if let Some(seq) = entry.intent.key.as_ref().and_then(|k| fold.keys.get(k)) {
        return Err(format!("its key landed in entry {seq}"));
    }
    for c in &entry.changes {
        let records = fold.records.entry(c.kind.clone()).or_default();
        match &c.after {
            Some(value) => records.insert(c.id.clone(), Held { version: c.version, value: value.clone() }),
            None => records.remove(&c.id),
        };
        if records.is_empty() {
            fold.records.remove(&c.kind);
        }
    }
    if let Some(key) = &entry.intent.key {
        fold.keys.insert(key.clone(), entry.seq);
    }
    fold.seq = entry.seq;
    Ok(())
}

/// One stored entry as the host read it: the `seq` in its key, and its
/// text.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Stored {
    pub seq: u64,
    pub text: String,
}

/// An entry that the fold did not apply, and why.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Quarantined {
    pub seq: u64,
    pub reason: String,
}

/// Read one stored entry of `log`: its kind, its version, its log, and
/// the `seq` in its key.
pub fn read_entry(log: &str, stored: &Stored) -> Result<Entry, String> {
    let value: Value = serde_json::from_str(&stored.text).map_err(|e| format!("not JSON: {e}"))?;
    if value.get("kind").and_then(Value::as_str) != Some(EntryKind::KIND) {
        return Err(format!("not a {} document", EntryKind::KIND));
    }
    let found = value.get("schema_version").and_then(Value::as_u64).unwrap_or(0);
    if found != u64::from(ENTRY_VERSION) {
        return Err(transit_version::refused(EntryKind::KIND, found as u32, ENTRY_VERSION));
    }
    let entry: Entry = serde_json::from_value(value).map_err(|e| format!("not an entry: {e}"))?;
    if entry.log != log {
        return Err(format!("an entry of the {} log, not the {log} log", entry.log));
    }
    if entry.seq != stored.seq {
        return Err(format!("the entry says seq {}, and its key says {}", entry.seq, stored.seq));
    }
    Ok(entry)
}

/// The answer of [`fold`].
#[derive(Debug, Clone, PartialEq)]
pub struct Folded {
    pub fold: Checkpoint,
    pub quarantined: Vec<Quarantined>,
}

/// Fold the entries after a checkpoint. The entries come in `seq` order,
/// each after the one before and after the checkpoint. An entry that
/// does not read, or does not apply, is quarantined with its reason, and
/// the fold goes on.
pub fn fold(start: Checkpoint, entries: &[Stored], hash: Hash) -> Result<Folded, String> {
    let mut fold = start;
    let mut quarantined = Vec::new();
    for stored in entries {
        if stored.seq <= fold.seq {
            return Err(format!("entry {} comes after entry {}: the entries are not in order", stored.seq, fold.seq));
        }
        let log = fold.log.clone();
        match read_entry(&log, stored).and_then(|e| apply(&mut fold, &e, hash)) {
            Ok(()) => {}
            Err(reason) => quarantined.push(Quarantined { seq: stored.seq, reason }),
        }
        fold.seq = stored.seq;
    }
    Ok(Folded { fold, quarantined })
}

/// One record that a revert writes back: its state at the head (`now`,
/// what the reverted entry wrote) and its state before the entry
/// (`prior`). Absent is a record the fold does not hold.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct RevertState {
    pub kind: String,
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub now: Option<Held>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prior: Option<Held>,
}

/// A record that a kind's capability writes back in a revert in place
/// of its state before. A kind whose merge would bring the state before
/// back over the revert writes something else (the edit document's
/// records are stamped to win the merge, and a record the entry created
/// is retired, D-193). Absent `after` takes the record out of the fold.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Written {
    pub kind: String,
    pub id: String,
    pub version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after: Option<Value>,
}

/// The records a revert of `entry` writes back, with their state now and
/// before the entry (`before`, the fold just before it). A record that a
/// later entry changed (`head` does not hold what the entry wrote)
/// refuses the revert, and the error names it, because a person decides.
pub fn revert_states(before: &Checkpoint, head: &Checkpoint, entry: &Entry, hash: Hash) -> Result<Vec<RevertState>, String> {
    let mut out = Vec::new();
    for c in &entry.changes {
        let now = head.get(&c.kind, &c.id);
        if now.map(|h| hash(&h.value)) != c.after.as_ref().map(|v| hash(v)) {
            return Err(format!("{} {} changed after entry {}: a person decides what it becomes", c.kind, c.id, entry.seq));
        }
        let prior = before.get(&c.kind, &c.id);
        if prior.map(|h| hash(&h.value)) != c.before {
            return Err(format!("{} {}: the fold before entry {} does not match the entry", c.kind, c.id, entry.seq));
        }
        out.push(RevertState { kind: c.kind.clone(), id: c.id.clone(), now: now.cloned(), prior: prior.cloned() });
    }
    Ok(out)
}

/// The changes of a revert: each record goes back to its state before,
/// or to what `written` names for it. A record the entry created leaves
/// the fold (D-198), unless `written` names it. A record already in the
/// state it goes to is no change. A written record that the revert does
/// not write back is an error.
pub fn revert_with(states: &[RevertState], written: &[Written], hash: Hash) -> Result<Vec<Change>, String> {
    for w in written {
        if !states.iter().any(|s| s.kind == w.kind && s.id == w.id) {
            return Err(format!("{} {} is not a record the revert writes back", w.kind, w.id));
        }
    }
    let mut out = Vec::new();
    for s in states {
        let to = match written.iter().find(|w| w.kind == s.kind && w.id == s.id) {
            Some(w) => w.after.clone().map(|value| Held { version: w.version, value }),
            None => s.prior.clone(),
        };
        let same = match (&to, &s.now) {
            (Some(t), Some(n)) => t.version == n.version && hash(&t.value) == hash(&n.value),
            (None, None) => true,
            _ => false,
        };
        if same {
            continue;
        }
        out.push(Change {
            kind: s.kind.clone(),
            version: to.as_ref().or(s.now.as_ref()).map_or(0, |h| h.version),
            id: s.id.clone(),
            before: s.now.as_ref().map(|h| hash(&h.value)),
            after: to.map(|h| h.value),
            extra: Default::default(),
        });
    }
    Ok(out)
}

/// The changes that revert `entry`: each record it changed goes back to
/// its state in `before` (the fold just before the entry). A record the
/// entry created leaves the fold (D-198). A record that a later entry
/// changed refuses the revert ([`revert_states`]).
pub fn revert(before: &Checkpoint, head: &Checkpoint, entry: &Entry, hash: Hash) -> Result<Vec<Change>, String> {
    revert_with(&revert_states(before, head, entry, hash)?, &[], hash)
}

/// The records of a document's collection: the object (or the array of
/// records with an `id`) under `field`, by id. A record with no id in an
/// array is an error.
pub fn split(document: &Value, field: &str) -> Result<BTreeMap<String, Value>, String> {
    match document.get(field) {
        Some(Value::Object(map)) => Ok(map.iter().map(|(k, v)| (k.clone(), v.clone())).collect()),
        Some(Value::Array(items)) => items
            .iter()
            .map(|item| match item.get("id").and_then(Value::as_str) {
                Some(id) => Ok((id.to_string(), item.clone())),
                None => Err(format!("a record in {field} has no id")),
            })
            .collect(),
        None => Ok(BTreeMap::new()),
        Some(_) => Err(format!("{field} is not a collection")),
    }
}

/// A document with `records` placed under `field` of `template`: an
/// object by id, or (`as_array`) an array in id order.
pub fn join(template: Value, field: &str, records: &BTreeMap<String, Value>, as_array: bool) -> Value {
    let mut doc = template;
    let collection = if as_array {
        Value::Array(records.values().cloned().collect())
    } else {
        Value::Object(records.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
    };
    if let Value::Object(map) = &mut doc {
        map.insert(field.into(), collection);
    }
    doc
}

/// The id of a document's own record: the document with its collection
/// empty (its tags, and every field this build does not know, D-167),
/// recorded under the document's kind. It is a valid document of its
/// kind and version, with no records.
pub const DOCUMENT_ID: &str = "document";

/// A document as a log's records: the collection under `field` as
/// records of `record_kind`, and the document with that collection
/// empty as the one record `DOCUMENT_ID` of `document_kind`. So a field
/// the document carries outside its collection is kept too.
pub fn document_records(document: &Value, field: &str, record_kind: &str, document_kind: &str, version: u32) -> Result<Vec<KindRecords>, String> {
    let records = split(document, field)?;
    let mut rest = document.clone();
    if let Value::Object(map) = &mut rest {
        let empty = match map.get(field) {
            Some(Value::Array(_)) => Value::Array(Vec::new()),
            _ => Value::Object(Default::default()),
        };
        map.insert(field.into(), empty);
    }
    Ok(vec![
        KindRecords { kind: record_kind.into(), version, records },
        KindRecords { kind: document_kind.into(), version, records: [(DOCUMENT_ID.to_string(), rest)].into() },
    ])
}

/// A kind's document assembled from its records in a fold: each record
/// in the version this build reads, joined under `field` of the
/// document's own record (`document`; absent: `template`, the empty
/// document). A record in an older version is refused, naming the
/// migration that brings it forward as a one-record document (D-197); a
/// newer one is refused naming both versions.
pub fn assemble<T>(template: Value, document: Option<&Held>, field: &str, records: &BTreeMap<String, Held>, as_array: bool) -> Result<T, String>
where
    T: serde::de::DeserializeOwned + transit_version::Versioned,
{
    let template = match document {
        Some(held) if held.version != T::READS => {
            return Err(format!("the document's own record: {}", transit_version::refused(T::KIND, held.version, T::READS)))
        }
        Some(held) => held.value.clone(),
        None => template,
    };
    for (id, held) in records {
        if held.version != T::READS {
            return Err(format!("record {id}: {}", transit_version::refused(T::KIND, held.version, T::READS)));
        }
    }
    let values: BTreeMap<String, Value> = records.iter().map(|(k, h)| (k.clone(), h.value.clone())).collect();
    serde_json::from_value(join(template, field, &values, as_array)).map_err(|e| format!("the records do not make a {} document: {e}", T::KIND))
}

#[cfg(test)]
mod tests {
    //! The laws of a log (D-190, D-197, D-198):
    //!
    //! 1. folding the entries of a sequence of writes gives the records
    //!    of the last write;
    //! 2. a write that changes nothing gives no entry, and a write whose
    //!    key landed gives the entry that holds it;
    //! 3. `apply` is all or none, and a stale `before` applies nothing;
    //! 4. a fold from a checkpoint equals the fold from the start;
    //! 5. an entry that does not read or apply is quarantined, and the
    //!    fold goes on;
    //! 6. a revert writes back the states before, or what the kind
    //!    writes in their place; a record the entry created leaves the
    //!    fold; a record a later entry changed refuses it;
    //! 7. one entry may change records of two kinds;
    //! 8. `split` and `join` are inverse over a collection, and a
    //!    document's fields outside its collection (D-167) survive its
    //!    records and its assembly.
    use super::*;
    use proptest::prelude::*;
    use serde_json::json;

    fn hash(v: &Value) -> String {
        serde_json::to_string(v).unwrap()
    }

    fn header(seq: u64, key: Option<&str>) -> Header {
        Header {
            log: "stops".into(),
            seq,
            writer: "w".into(),
            at_ms: seq,
            module: "cap-log@test".into(),
            intent: Intent { op: "test".into(), key: key.map(String::from), ..Default::default() },
        }
    }

    fn stops(records: &[(&str, Value)]) -> KindRecords {
        KindRecords { kind: "stop".into(), version: 1, records: records.iter().map(|(k, v)| (k.to_string(), v.clone())).collect() }
    }

    fn requests(records: &[(&str, Value)]) -> KindRecords {
        KindRecords { kind: "stop_request".into(), version: 1, records: records.iter().map(|(k, v)| (k.to_string(), v.clone())).collect() }
    }

    fn stored(e: &Entry) -> Stored {
        Stored { seq: e.seq, text: serde_json::to_string(e).unwrap() }
    }

    /// Write `next` over `fold`: the entry, applied, or nothing.
    fn write(fold: &mut Checkpoint, next: &[KindRecords], key: Option<&str>) -> Option<Entry> {
        match compose(fold, header(fold.seq + 1, key), next, &hash) {
            Composed::Entry(e) => {
                apply(fold, &e, &hash).unwrap();
                Some(e)
            }
            _ => None,
        }
    }

    fn records_of(fold: &Checkpoint, kind: &str) -> BTreeMap<String, Value> {
        fold.records.get(kind).map(|m| m.iter().map(|(k, h)| (k.clone(), h.value.clone())).collect()).unwrap_or_default()
    }

    fn doc() -> impl Strategy<Value = BTreeMap<String, Value>> {
        proptest::collection::btree_map("s[0-4]", (0u8..4).prop_map(|n| json!({"events": n})), 0..5)
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(256))]

        /// Laws 1 and 4.
        #[test]
        fn the_fold_is_the_last_write_from_anywhere(writes in proptest::collection::vec(doc(), 1..8), cut in 0usize..8) {
            let mut live = Checkpoint::empty("stops");
            let mut log = Vec::new();
            for w in &writes {
                let next = KindRecords { kind: "stop".into(), version: 1, records: w.clone() };
                if let Some(e) = write(&mut live, &[next], None) {
                    log.push(stored(&e));
                }
            }
            let folded = fold(Checkpoint::empty("stops"), &log, &hash).unwrap();
            prop_assert!(folded.quarantined.is_empty());
            prop_assert_eq!(&records_of(&folded.fold, "stop"), writes.last().unwrap());
            prop_assert_eq!(&folded.fold, &live);
            let cut = cut.min(log.len());
            let at = fold(Checkpoint::empty("stops"), &log[..cut], &hash).unwrap().fold;
            prop_assert_eq!(fold(at, &log[cut..], &hash).unwrap().fold, live);
        }

        /// Law 8.
        #[test]
        fn split_and_join_are_inverse(records in doc()) {
            let template = json!({"kind": "stops", "schema_version": 1});
            let doc = join(template.clone(), "stops", &records, false);
            prop_assert_eq!(&split(&doc, "stops").unwrap(), &records);
            let as_array: BTreeMap<String, Value> = records.iter().map(|(k, v)| {
                let mut v = v.clone();
                v["id"] = json!(k);
                (k.clone(), v)
            }).collect();
            let doc = join(template, "records", &as_array, true);
            prop_assert_eq!(split(&doc, "records").unwrap(), as_array);
        }
    }

    #[test]
    fn nothing_changed_is_no_entry_and_a_key_lands_once() {
        let mut fold = Checkpoint::empty("stops");
        let next = [stops(&[("A", json!({"e": 1}))])];
        let first = write(&mut fold, &next, Some("device-1/42")).expect("an entry");
        assert_eq!(compose(&fold, header(2, None), &next, &hash), Composed::Unchanged);
        let other = [stops(&[("A", json!({"e": 2}))])];
        assert_eq!(compose(&fold, header(2, Some("device-1/42")), &other, &hash), Composed::Landed(first.seq), "the resend lands once");
        assert!(matches!(compose(&fold, header(2, Some("device-1/43")), &other, &hash), Composed::Entry(_)));
    }

    #[test]
    fn a_stale_before_applies_nothing() {
        let mut fold = Checkpoint::empty("stops");
        write(&mut fold, &[stops(&[("A", json!(1))])], None);
        let Composed::Entry(e) = compose(&fold, header(2, None), &[stops(&[("A", json!(2))])], &hash) else { panic!() };
        write(&mut fold, &[stops(&[("A", json!(3))])], None);
        let held = fold.clone();
        let refused = apply(&mut fold, &e, &hash).unwrap_err();
        assert!(refused.contains("stop A"), "{refused}");
        assert_eq!(fold, held, "all or none");
    }

    #[test]
    fn one_entry_changes_two_kinds() {
        let mut fold = Checkpoint::empty("stops");
        write(&mut fold, &[stops(&[("B", json!({"events": 1}))]), requests(&[("B-1", json!({"status": "ordered"}))])], None);
        let e = write(
            &mut fold,
            &[stops(&[("B", json!({"events": 2}))]), requests(&[("B-1", json!({"status": "completed"}))])],
            Some("complete-B-1"),
        )
        .unwrap();
        let kinds: Vec<&str> = e.changes.iter().map(|c| c.kind.as_str()).collect();
        assert_eq!(kinds, vec!["stop", "stop_request"], "one entry, both kinds, in kind order");
        // A write that names only one kind leaves the other kind alone.
        write(&mut fold, &[stops(&[("B", json!({"events": 3}))])], None);
        assert_eq!(records_of(&fold, "stop_request")["B-1"], json!({"status": "completed"}));
    }

    #[test]
    fn a_bad_entry_is_quarantined_and_the_fold_goes_on() {
        let mut live = Checkpoint::empty("stops");
        let e1 = write(&mut live, &[stops(&[("A", json!(1))])], None).unwrap();
        let e3 = write(&mut live, &[stops(&[("A", json!(1)), ("B", json!(2))])], None).unwrap();
        let e3 = Entry { seq: 4, ..e3 };
        let garbage = Stored { seq: 2, text: "{".into() };
        let other_log = Stored { seq: 3, text: serde_json::to_string(&Entry { log: "plans".into(), seq: 3, ..e1.clone() }).unwrap() };
        let folded = fold(Checkpoint::empty("stops"), &[stored(&e1), garbage, other_log, stored(&e3)], &hash).unwrap();
        assert_eq!(folded.quarantined.iter().map(|q| q.seq).collect::<Vec<_>>(), vec![2, 3]);
        assert!(folded.quarantined[1].reason.contains("plans log"));
        assert_eq!(records_of(&folded.fold, "stop").len(), 2, "entry 4 applied");
        assert_eq!(folded.fold.seq, 4);
    }

    #[test]
    fn a_revert_writes_back_and_takes_out_what_the_entry_created() {
        let mut fold = Checkpoint::empty("stops");
        write(&mut fold, &[stops(&[("A", json!(1))])], None);
        let before = fold.clone();
        let e = write(&mut fold, &[stops(&[("A", json!(2)), ("B", json!(9))])], None).unwrap();
        let back = revert(&before, &fold, &e, &hash).unwrap();
        let mut after = fold.clone();
        apply(&mut after, &Entry::new(header(3, None), back, Some(2)), &hash).unwrap();
        assert_eq!(records_of(&after, "stop"), records_of(&before, "stop"), "A back to 1, B out of the fold (D-198)");

        // A later entry changed A: a second revert of entry 2 is refused.
        let refused = revert(&before, &after, &e, &hash).unwrap_err();
        assert!(refused.contains("stop A") && refused.contains("a person decides"), "{refused}");
    }

    /// Law 6, a kind's own write-back: the records a kind's capability
    /// writes in place of the states before, and only those.
    #[test]
    fn a_kind_writes_back_its_own_records_in_a_revert() {
        let mut fold = Checkpoint::empty("stops");
        write(&mut fold, &[stops(&[("A", json!(1))])], None);
        let before = fold.clone();
        let e = write(&mut fold, &[stops(&[("A", json!(2)), ("B", json!(9))])], None).unwrap();
        let states = revert_states(&before, &fold, &e, &hash).unwrap();
        assert_eq!(states.len(), 2);
        let retired = Written { kind: "stop".into(), id: "B".into(), version: 1, after: Some(json!("retired")) };
        let back = revert_with(&states, std::slice::from_ref(&retired), &hash).unwrap();
        let mut after = fold.clone();
        apply(&mut after, &Entry::new(header(3, None), back, Some(2)), &hash).unwrap();
        assert_eq!(records_of(&after, "stop"), BTreeMap::from([("A".into(), json!(1)), ("B".into(), json!("retired"))]));
        let stranger = Written { kind: "stop".into(), id: "C".into(), version: 1, after: None };
        assert!(revert_with(&states, &[stranger], &hash).unwrap_err().contains("stop C"));
    }

    #[test]
    fn an_entry_reads_back_as_written_and_only_in_its_log() {
        let mut fold = Checkpoint::empty("stops");
        let e = write(&mut fold, &[stops(&[("A", json!(1))])], Some("k")).unwrap();
        assert_eq!(read_entry("stops", &stored(&e)).unwrap(), e);
        assert!(read_entry("plans", &stored(&e)).is_err());
        let newer = stored(&e).text.replace("\"schema_version\":1", "\"schema_version\":2");
        assert!(read_entry("stops", &Stored { seq: 1, text: newer }).is_err());
    }

    /// Law 8, the document's own record: a field outside the collection
    /// that this build does not know is kept.
    #[test]
    fn a_documents_own_fields_survive_its_records() {
        #[derive(Debug, serde::Deserialize, serde::Serialize, PartialEq)]
        struct Doc {
            kind: String,
            schema_version: u32,
            stops: BTreeMap<String, Value>,
            #[serde(flatten)]
            extra: BTreeMap<String, Value>,
        }
        impl transit_version::Versioned for Doc {
            const KIND: &'static str = "stops";
            const READS: u32 = 1;
            fn schema_version(&self) -> u32 {
                self.schema_version
            }
        }
        let doc = json!({"kind": "stops", "schema_version": 1, "survey_round": 3, "stops": {"A": [1], "B": [2]}});
        let next = document_records(&doc, "stops", "stop", "stops", 1).unwrap();
        let mut fold = Checkpoint::empty("stops");
        write(&mut fold, &next, None);
        let back: Doc = assemble(json!({}), fold.get("stops", DOCUMENT_ID), "stops", &fold.records["stop"], false).unwrap();
        assert_eq!(serde_json::to_value(back).unwrap(), doc);
        let newer = Held { version: 2, value: json!({}) };
        assert!(assemble::<Doc>(json!({}), Some(&newer), "stops", &fold.records["stop"], false).is_err());
    }
}
