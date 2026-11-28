//! The existence check (D-232, D-233, `crates/cap-log/OPS.md` "references").
//! It reads the kind table and the folds, never a capability's types. A
//! record that a fold holds exists in any state, because retired is a state
//! and the rules of each reference judge the states.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{read_entry, Entry, Records, Stored};

/// The fields of a kind table entry that the check reads.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
#[cfg_attr(feature = "bindings", ts(export))]
pub struct KindReferences {
    pub store_kind: String,
    /// Absent for a document kept whole.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub log: Option<String>,
    #[serde(default)]
    pub records: Vec<RecordKind>,
    #[serde(default)]
    pub references: Vec<Declared>,
    #[serde(default)]
    pub versions: Vec<KindVersion>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
#[cfg_attr(feature = "bindings", ts(export))]
pub struct KindVersion {
    pub version: u32,
}

impl KindReferences {
    fn reads(&self) -> Option<u32> {
        self.versions.iter().map(|v| v.version).max()
    }
}

/// A warning, never a broken reference.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
#[cfg_attr(feature = "bindings", ts(export))]
pub struct Unjudged {
    pub log: String,
    pub kind: String,
    pub id: String,
    pub version: u32,
    pub reads: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
#[cfg_attr(feature = "bindings", ts(export))]
pub struct Naming {
    pub log: String,
    pub kind: String,
    pub id: String,
    pub field: String,
    pub target: String,
    pub names: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
#[cfg_attr(feature = "bindings", ts(export))]
pub struct RecordKind {
    pub kind: String,
    pub collection: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
#[cfg_attr(feature = "bindings", ts(export))]
pub struct Declared {
    pub path: String,
    pub target: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
#[cfg_attr(feature = "bindings", ts(export))]
pub struct Broken {
    pub log: String,
    pub kind: String,
    pub id: String,
    pub field: String,
    pub target: String,
    pub names: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub entry: Option<EntryRef>,
    /// Set when the target is held but a rule of the kinds at both ends
    /// does not accept its state (D-233). It ends the sentence "… names stop
    /// B, which " on a page, as in "was retired on 2026-10-01".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub rule: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(ts_rs::TS))]
#[cfg_attr(feature = "bindings", ts(export))]
pub struct EntryRef {
    pub log: String,
    #[cfg_attr(feature = "bindings", ts(type = "number"))]
    pub seq: u64,
}

pub fn touched(
    broken: Vec<Broken>,
    kinds: &[KindReferences],
    log: &str,
    entry: &Stored,
) -> Vec<Broken> {
    match read_entry(log, entry) {
        Ok(e) => touched_in(broken, kinds, log, &e, entry.seq),
        Err(_) => Vec::new(),
    }
}

pub fn touched_in(
    broken: Vec<Broken>,
    kinds: &[KindReferences],
    log: &str,
    e: &Entry,
    seq: u64,
) -> Vec<Broken> {
    let changed = |kind: &str, id: &str| e.changes.iter().any(|c| c.kind == kind && c.id == id);
    let target_log = |kind: &str| {
        kinds
            .iter()
            .filter(|k| k.records.iter().any(|r| r.kind == kind))
            .find_map(|k| k.log.clone())
    };
    broken
        .into_iter()
        .filter(|b| {
            (b.log == log && changed(&b.kind, &b.id))
                || (target_log(&b.target).as_deref() == Some(log) && changed(&b.target, &b.names))
        })
        .map(|b| Broken {
            entry: Some(EntryRef {
                log: log.to_string(),
                seq,
            }),
            ..b
        })
        .collect()
}

/// Across two logs the later entry is the one with the later clock
/// (`at_ms`).
pub fn blame(
    broken: &mut [Broken],
    kinds: &[KindReferences],
    entries: &BTreeMap<String, Vec<Stored>>,
) {
    let mut target_log: BTreeMap<&str, &str> = BTreeMap::new();
    for k in kinds {
        if let Some(log) = &k.log {
            for r in &k.records {
                target_log.insert(r.kind.as_str(), log.as_str());
            }
        }
    }
    let mut last: BTreeMap<(String, String, String), (u64, u64)> = BTreeMap::new();
    for (log, stored) in entries {
        for s in stored {
            let Ok(entry) = read_entry(log, s) else {
                continue;
            };
            for c in &entry.changes {
                let at = last
                    .entry((log.clone(), c.kind.clone(), c.id.clone()))
                    .or_insert((0, 0));
                if s.seq > at.0 {
                    *at = (s.seq, entry.at_ms);
                }
            }
        }
    }
    let at = |log: &str, kind: &str, id: &str| {
        last.get(&(log.to_string(), kind.to_string(), id.to_string()))
            .map(|(seq, ms)| (log.to_string(), *seq, *ms))
    };
    for b in broken.iter_mut() {
        let wrote = at(&b.log, &b.kind, &b.id);
        let changed = target_log
            .get(b.target.as_str())
            .and_then(|l| at(l, &b.target, &b.names));
        let later = match (wrote, changed) {
            (Some(w), Some(c)) => Some(if (c.0 == w.0 && c.1 > w.1) || (c.0 != w.0 && c.2 > w.2) {
                c
            } else {
                w
            }),
            (w, c) => w.or(c),
        };
        b.entry = later.map(|(log, seq, _)| EntryRef { log, seq });
    }
}

pub fn broken(kinds: &[KindReferences], folds: &BTreeMap<String, Records>) -> Vec<Broken> {
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
            let Some(records) = fold.get(&r.kind) else {
                continue;
            };
            for d in &k.references {
                let Some(rest) = record_path(&d.path, &r.collection) else {
                    continue;
                };
                let Some(target) = target_log
                    .get(d.target.as_str())
                    .and_then(|l| folds.get(*l))
                else {
                    continue;
                };
                let held = target.get(&d.target);
                for (id, record) in records {
                    if k.reads().is_some_and(|reads| record.version > reads) {
                        continue;
                    }
                    for (field, value) in values_at(&record.value, &rest) {
                        let Value::String(names) = value else {
                            continue;
                        };
                        if held.is_some_and(|h| h.contains_key(names.as_str())) {
                            continue;
                        }
                        out.push(Broken {
                            log: log.clone(),
                            kind: r.kind.clone(),
                            id: id.clone(),
                            field,
                            target: d.target.clone(),
                            names: names.clone(),
                            entry: None,
                            rule: None,
                        });
                    }
                }
            }
        }
    }
    out.sort();
    out
}

/// The write path refuses these (D-233). A reference that the record already
/// had in `before` is not added, so a person can still change or cancel a
/// record that was broken before.
pub fn added(
    kinds: &[KindReferences],
    read: &BTreeMap<String, Records>,
    before: &Records,
    log: &str,
    entry: &Stored,
) -> Vec<Broken> {
    match read_entry(log, entry) {
        Ok(e) => added_in(kinds, read, before, log, &e),
        Err(_) => Vec::new(),
    }
}

pub fn added_in(
    kinds: &[KindReferences],
    read: &BTreeMap<String, Records>,
    before: &Records,
    log: &str,
    e: &Entry,
) -> Vec<Broken> {
    let mut target_log: BTreeMap<&str, &str> = BTreeMap::new();
    for k in kinds {
        if let Some(l) = &k.log {
            for r in &k.records {
                target_log.insert(r.kind.as_str(), l.as_str());
            }
        }
    }
    let in_entry = |kind: &str, id: &str| {
        e.changes
            .iter()
            .find(|c| c.kind == kind && c.id == id)
            .map(|c| c.after.is_some())
    };
    let mut out = Vec::new();
    for c in &e.changes {
        let Some(after) = &c.after else { continue };
        for k in kinds.iter().filter(|k| k.log.as_deref() == Some(log)) {
            if k.reads().is_some_and(|reads| c.version > reads) {
                continue;
            }
            for r in k.records.iter().filter(|r| r.kind == c.kind) {
                for d in &k.references {
                    let Some(rest) = record_path(&d.path, &r.collection) else {
                        continue;
                    };
                    let Some(tlog) = target_log.get(d.target.as_str()) else {
                        continue;
                    };
                    let had: Vec<(String, Value)> = before
                        .get(&c.kind)
                        .and_then(|m| m.get(&c.id))
                        .map(|h| {
                            values_at(&h.value, &rest)
                                .into_iter()
                                .map(|(f, v)| (f, v.clone()))
                                .collect()
                        })
                        .unwrap_or_default();
                    for (field, value) in values_at(after, &rest) {
                        let Value::String(names) = value else {
                            continue;
                        };
                        if had.iter().any(|(f, v)| f == &field && v == value) {
                            continue;
                        }
                        let exists = match in_entry(&d.target, names) {
                            Some(created) => created,
                            None => match read.get(*tlog) {
                                Some(fold) => fold
                                    .get(&d.target)
                                    .is_some_and(|m| m.contains_key(names.as_str())),
                                None => continue,
                            },
                        };
                        if !exists {
                            out.push(Broken {
                                log: log.to_string(),
                                kind: c.kind.clone(),
                                id: c.id.clone(),
                                field,
                                target: d.target.clone(),
                                names: names.clone(),
                                entry: None,
                                rule: None,
                            });
                        }
                    }
                }
            }
        }
    }
    out.sort();
    out
}

/// What compaction keeps. The existence check judges every held
/// referrer, so a record that one names must stay in its fold.
pub fn named(
    kinds: &[KindReferences],
    folds: &BTreeMap<String, Records>,
    target: &str,
) -> std::collections::BTreeSet<String> {
    let mut out = std::collections::BTreeSet::new();
    for k in kinds {
        let Some(fold) = k.log.as_ref().and_then(|l| folds.get(l)) else {
            continue;
        };
        for r in &k.records {
            let Some(records) = fold.get(&r.kind) else {
                continue;
            };
            for d in k.references.iter().filter(|d| d.target == target) {
                let Some(rest) = record_path(&d.path, &r.collection) else {
                    continue;
                };
                for record in records.values() {
                    for (_, value) in values_at(&record.value, &rest) {
                        if let Value::String(names) = value {
                            out.insert(names.clone());
                        }
                    }
                }
            }
        }
    }
    out
}

pub fn unjudged(kinds: &[KindReferences], folds: &BTreeMap<String, Records>) -> Vec<Unjudged> {
    let mut out = Vec::new();
    for k in kinds {
        let (Some(log), Some(reads)) = (&k.log, k.reads()) else {
            continue;
        };
        let Some(fold) = folds.get(log) else { continue };
        for r in &k.records {
            for (id, record) in fold.get(&r.kind).into_iter().flatten() {
                if record.version > reads {
                    out.push(Unjudged {
                        log: log.clone(),
                        kind: r.kind.clone(),
                        id: id.clone(),
                        version: record.version,
                        reads,
                    });
                }
            }
        }
    }
    out.sort();
    out
}

/// Reads every target, since a document by hash is any kind's.
pub fn naming(
    kinds: &[KindReferences],
    folds: &BTreeMap<String, Records>,
    hashes: &[String],
) -> Vec<Naming> {
    let mut out = Vec::new();
    for k in kinds {
        let Some(log) = &k.log else { continue };
        let Some(fold) = folds.get(log) else { continue };
        for r in &k.records {
            let Some(records) = fold.get(&r.kind) else {
                continue;
            };
            for d in &k.references {
                let Some(rest) = record_path(&d.path, &r.collection) else {
                    continue;
                };
                for (id, record) in records {
                    for (field, value) in values_at(&record.value, &rest) {
                        let Value::String(names) = value else {
                            continue;
                        };
                        if hashes.contains(names) {
                            out.push(Naming {
                                log: log.clone(),
                                kind: r.kind.clone(),
                                id: id.clone(),
                                field,
                                target: d.target.clone(),
                                names: names.clone(),
                            });
                        }
                    }
                }
            }
        }
    }
    out.sort();
    out
}

fn record_path(path: &str, collection: &str) -> Option<Vec<String>> {
    let mut segments = path.strip_prefix('/')?.split('/');
    if segments.next()? != collection || segments.next()? != "*" {
        return None;
    }
    Some(segments.map(str::to_string).collect())
}

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
                ("*", Value::Array(items)) => items
                    .iter()
                    .enumerate()
                    .map(|(i, c)| (format!("{at}/{i}"), c))
                    .collect(),
                ("*", Value::Object(map)) => map
                    .iter()
                    .map(|(k, c)| (format!("{at}/{}", escape(k)), c))
                    .collect(),
                (field, Value::Object(map)) => map
                    .get(field)
                    .map(|c| (format!("{at}/{}", escape(field)), c))
                    .into_iter()
                    .collect(),
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

/// RFC 6901.
fn escape(token: &str) -> String {
    token.replace('~', "~0").replace('/', "~1")
}
