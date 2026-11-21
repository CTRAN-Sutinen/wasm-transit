//! The existence check (D-232, D-233): each reference a kind declares
//! (the kind table's `references`, read from the `x-references` of its
//! schema) must name a record that its log's fold holds.
//!
//! The check reads the kind table and the folds, never a capability's
//! types. A reference path names a field of the kind's document:
//! `/requests/*/stop`. Its first segment is the collection, and the
//! second is the record's id, so the rest of the path is read in each
//! record of the collection's record kind. A segment is a field name, `*`
//! (each element of an array, or each value of an object), or either
//! with a filter, `name[field=value]`, which keeps an object only when
//! its field has that value.
//!
//! A target is judged when it is a record kind of the table and the
//! fold of its log is given. A record that a fold holds exists in any
//! state: retired is a state, and the rules of each reference judge the
//! states (`docs/units/kind-registration.md`). Every other target is not
//! judged here: the network's things, a trip instance, a document or an
//! attachment by hash, an entry by seq, and an external thing. A
//! referring record in a version newer than the table reads is not
//! judged either ([`unjudged`]): the declarations may not be its fields.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{read_entry, Records, Stored};

/// A kind as the kind table states it, with the fields the check reads.
/// The table's other fields are ignored.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, ts_rs::TS)]
#[ts(export)]
pub struct KindReferences {
    pub store_kind: String,
    /// The log that keeps the kind (absent: a document kept whole).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub log: Option<String>,
    /// The kind's record kinds, each with its collection in the document.
    #[serde(default)]
    pub records: Vec<RecordKind>,
    #[serde(default)]
    pub references: Vec<Declared>,
    /// The kind's versions this table reads (the table's `versions`).
    #[serde(default)]
    pub versions: Vec<KindVersion>,
}

/// One version of a kind that the table reads.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export)]
pub struct KindVersion {
    pub version: u32,
}

impl KindReferences {
    /// The newest version the table reads (None: the table names none).
    fn reads(&self) -> Option<u32> {
        self.versions.iter().map(|v| v.version).max()
    }
}

/// A record the check cannot judge: it is in a version newer than the
/// kind table reads (after a rollback, D-186), so the table's
/// declarations may not be its fields. It is a warning, never a broken
/// reference.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, ts_rs::TS)]
#[ts(export)]
pub struct Unjudged {
    pub log: String,
    pub kind: String,
    pub id: String,
    pub version: u32,
    pub reads: u32,
}

/// A reference that names a document by its hash: the referring record,
/// its field, the declared target, and the hash.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, ts_rs::TS)]
#[ts(export)]
pub struct Naming {
    pub log: String,
    pub kind: String,
    pub id: String,
    pub field: String,
    pub target: String,
    pub names: String,
}

/// A record kind of a logged kind, and its collection in the document.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export)]
pub struct RecordKind {
    pub kind: String,
    pub collection: String,
}

/// A reference field and its target, as the kind table states it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export)]
pub struct Declared {
    pub path: String,
    pub target: String,
}

/// A reference whose target the fold does not hold: the referring
/// record (its log, kind, and id), the field in it (a JSON pointer
/// inside the record), the declared target, and the id it names.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, ts_rs::TS)]
#[ts(export)]
pub struct Broken {
    pub log: String,
    pub kind: String,
    pub id: String,
    pub field: String,
    pub target: String,
    pub names: String,
    /// The entry that broke it, when the entries were given ([`blame`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub entry: Option<EntryRef>,
}

/// An entry of a log, by its seq.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, ts_rs::TS)]
#[ts(export)]
pub struct EntryRef {
    pub log: String,
    #[ts(type = "number")]
    pub seq: u64,
}

/// The broken references that one entry of `log` touches: those whose
/// referring record it wrote, and those whose target it changed. Each
/// names that entry. This is what a write answers as warnings (D-233):
/// the references the write broke, or wrote broken. An entry that does
/// not read touches nothing.
pub fn touched(broken: Vec<Broken>, kinds: &[KindReferences], log: &str, entry: &Stored) -> Vec<Broken> {
    let Ok(e) = read_entry(log, entry) else { return Vec::new() };
    let changed = |kind: &str, id: &str| e.changes.iter().any(|c| c.kind == kind && c.id == id);
    let target_log = |kind: &str| kinds.iter().filter(|k| k.records.iter().any(|r| r.kind == kind)).find_map(|k| k.log.clone());
    broken
        .into_iter()
        .filter(|b| (b.log == log && changed(&b.kind, &b.id)) || (target_log(&b.target).as_deref() == Some(log) && changed(&b.target, &b.names)))
        .map(|b| Broken { entry: Some(EntryRef { log: log.to_string(), seq: entry.seq }), ..b })
        .collect()
}

/// Names the entry that broke each reference: the later of the entry
/// that last wrote the referring record and the entry that last changed
/// the target. In one log the later is the higher seq; across two logs
/// it is the later clock (`at_ms`). With the target never written, it
/// is the referrer's. `entries` are each log's stored entries; a log
/// not given, or an entry that does not read, names nothing.
pub fn blame(broken: &mut [Broken], kinds: &[KindReferences], entries: &BTreeMap<String, Vec<Stored>>) {
    let mut target_log: BTreeMap<&str, &str> = BTreeMap::new();
    for k in kinds {
        if let Some(log) = &k.log {
            for r in &k.records {
                target_log.insert(r.kind.as_str(), log.as_str());
            }
        }
    }
    // The last entry of each log that changed each record: its seq and
    // its clock.
    let mut last: BTreeMap<(String, String, String), (u64, u64)> = BTreeMap::new();
    for (log, stored) in entries {
        for s in stored {
            let Ok(entry) = read_entry(log, s) else { continue };
            for c in &entry.changes {
                let at = last.entry((log.clone(), c.kind.clone(), c.id.clone())).or_insert((0, 0));
                if s.seq > at.0 {
                    *at = (s.seq, entry.at_ms);
                }
            }
        }
    }
    let at = |log: &str, kind: &str, id: &str| last.get(&(log.to_string(), kind.to_string(), id.to_string())).map(|(seq, ms)| (log.to_string(), *seq, *ms));
    for b in broken.iter_mut() {
        let wrote = at(&b.log, &b.kind, &b.id);
        let changed = target_log.get(b.target.as_str()).and_then(|l| at(l, &b.target, &b.names));
        let later = match (wrote, changed) {
            (Some(w), Some(c)) => Some(if (c.0 == w.0 && c.1 > w.1) || (c.0 != w.0 && c.2 > w.2) { c } else { w }),
            (w, c) => w.or(c),
        };
        b.entry = later.map(|(log, seq, _)| EntryRef { log, seq });
    }
}

/// Each broken reference of the folds, in (log, kind, id, field) order.
/// `folds` are the logs' records by log name. A referrer whose log is
/// not given is not read, and a target whose log is not given is not
/// judged.
pub fn broken(kinds: &[KindReferences], folds: &BTreeMap<String, Records>) -> Vec<Broken> {
    // Each record kind, and the log that holds it.
    let mut target_log: BTreeMap<&str, &str> = BTreeMap::new();
    for k in kinds {
        if let Some(log) = &k.log {
            for r in &k.records {
                target_log.insert(r.kind.as_str(), log.as_str());
            }
        }
    }
    let mut out = Vec::new();
    for k in kinds {
        let Some(log) = &k.log else { continue };
        let Some(fold) = folds.get(log) else { continue };
        for r in &k.records {
            let Some(records) = fold.get(&r.kind) else { continue };
            for d in &k.references {
                let Some(rest) = record_path(&d.path, &r.collection) else { continue };
                let Some(target) = target_log.get(d.target.as_str()).and_then(|l| folds.get(*l)) else { continue };
                let held = target.get(&d.target);
                for (id, record) in records {
                    if k.reads().is_some_and(|reads| record.version > reads) {
                        continue;
                    }
                    for (field, value) in values_at(&record.value, &rest) {
                        let Value::String(names) = value else { continue };
                        if held.is_some_and(|h| h.contains_key(names.as_str())) {
                            continue;
                        }
                        out.push(Broken { log: log.clone(), kind: r.kind.clone(), id: id.clone(), field, target: d.target.clone(), names: names.clone(), entry: None });
                    }
                }
            }
        }
    }
    out.sort();
    out
}

/// Each record of the folds in a version newer than its kind's table
/// reads: the existence check does not judge its references.
pub fn unjudged(kinds: &[KindReferences], folds: &BTreeMap<String, Records>) -> Vec<Unjudged> {
    let mut out = Vec::new();
    for k in kinds {
        let (Some(log), Some(reads)) = (&k.log, k.reads()) else { continue };
        let Some(fold) = folds.get(log) else { continue };
        for r in &k.records {
            for (id, record) in fold.get(&r.kind).into_iter().flatten() {
                if record.version > reads {
                    out.push(Unjudged { log: log.clone(), kind: r.kind.clone(), id: id.clone(), version: record.version, reads });
                }
            }
        }
    }
    out.sort();
    out
}

/// Each declared reference of the folds whose value is one of `hashes`:
/// what names the documents a pointer's revert drops (`store revert`).
/// Every target is read, since a document by hash is any kind's.
pub fn naming(kinds: &[KindReferences], folds: &BTreeMap<String, Records>, hashes: &[String]) -> Vec<Naming> {
    let mut out = Vec::new();
    for k in kinds {
        let Some(log) = &k.log else { continue };
        let Some(fold) = folds.get(log) else { continue };
        for r in &k.records {
            let Some(records) = fold.get(&r.kind) else { continue };
            for d in &k.references {
                let Some(rest) = record_path(&d.path, &r.collection) else { continue };
                for (id, record) in records {
                    for (field, value) in values_at(&record.value, &rest) {
                        let Value::String(names) = value else { continue };
                        if hashes.contains(names) {
                            out.push(Naming { log: log.clone(), kind: r.kind.clone(), id: id.clone(), field, target: d.target.clone(), names: names.clone() });
                        }
                    }
                }
            }
        }
    }
    out.sort();
    out
}

/// The segments of a declared path inside one record of `collection`:
/// the path less its collection and the record's id. None when the path
/// is of another collection.
fn record_path(path: &str, collection: &str) -> Option<Vec<String>> {
    let mut segments = path.strip_prefix('/')?.split('/');
    if segments.next()? != collection || segments.next()? != "*" {
        return None;
    }
    Some(segments.map(str::to_string).collect())
}

/// Each value at `path` inside `value`, with its JSON pointer.
pub fn values_at<'a>(value: &'a Value, path: &[String]) -> Vec<(String, &'a Value)> {
    let mut found = vec![(String::new(), value)];
    for segment in path {
        let (name, filter) = match segment.split_once('[') {
            Some((name, rest)) => (name, rest.strip_suffix(']').and_then(|f| f.split_once('='))),
            None => (segment.as_str(), None),
        };
        let mut next = Vec::new();
        for (at, v) in found {
            let children: Vec<(String, &Value)> = match (name, v) {
                ("*", Value::Array(items)) => items.iter().enumerate().map(|(i, c)| (format!("{at}/{i}"), c)).collect(),
                ("*", Value::Object(map)) => map.iter().map(|(k, c)| (format!("{at}/{}", escape(k)), c)).collect(),
                (field, Value::Object(map)) => map.get(field).map(|c| (format!("{at}/{}", escape(field)), c)).into_iter().collect(),
                _ => Vec::new(),
            };
            next.extend(children.into_iter().filter(|(_, c)| match filter {
                Some((field, want)) => c.get(field).and_then(Value::as_str) == Some(want),
                None => true,
            }));
        }
        found = next;
    }
    found
}

/// A JSON pointer's escape of one token (RFC 6901).
fn escape(token: &str) -> String {
    token.replace('~', "~0").replace('/', "~1")
}
