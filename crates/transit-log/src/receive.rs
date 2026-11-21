//! A log write's last step, shared by every receive op (D-228): the
//! records a write leaves become one entry over the fold, or the
//! answer that no entry is due. A kind's capability runs the write's
//! ops over the documents at the head, turns the documents into their
//! records, and asks [`receive`] for the entry. The host stores the
//! entry with a conditional create, and asks again over the new head
//! when another write took the seq.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{compose, overlap, read_entry, Checkpoint, Composed, Entry, Hash, Header, KindRecords, Overlap, Stored};

/// What the host gives a receive op about the log: the fold the write
/// runs over (absent: the empty log), the entry's header, and the
/// entries after the fold that the writer did not read.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Write {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checkpoint: Option<Value>,
    pub header: Header,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unread: Vec<Stored>,
    /// The kind table's kinds, for the reference checks (D-233). Absent:
    /// no check.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(skip)]
    pub references: Option<Vec<crate::KindReferences>>,
    /// Each log's records as of the seq the writer read, by log. With
    /// them, a reference the write adds to a record missing then refuses
    /// the write ([`Received::broken`]). A log left out is not judged.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(skip)]
    pub read: Option<BTreeMap<String, crate::Records>>,
    /// Each log's records now, by log (absent: as `read`). The write's
    /// own log is the fold after the entry. The references the entry
    /// touches that are broken now are its warnings
    /// ([`Received::references`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(skip)]
    pub now: Option<BTreeMap<String, crate::Records>>,
}

impl Write {
    /// The fold the write runs over, checked to be this log's and to end
    /// just before the entry's seq.
    pub fn fold(&self) -> Result<Checkpoint, String> {
        let fold = match &self.checkpoint {
            None => Checkpoint::empty(&self.header.log),
            Some(v) => {
                // Another version is refused, naming both (the log reads
                // no checkpoint ahead).
                let found = v.get("schema_version").and_then(Value::as_u64).unwrap_or(0) as u32;
                if !matches!(transit_version::reads::<Checkpoint>(found)?, transit_version::ReadAs::Current) {
                    return Err(transit_version::refused(crate::CheckpointKind::KIND, found, crate::CHECKPOINT_VERSION));
                }
                let c: Checkpoint = serde_json::from_value(v.clone()).map_err(|e| format!("invalid document: {e}"))?;
                if c.log != self.header.log {
                    return Err(format!("a checkpoint of the {} log, not the {} log", c.log, self.header.log));
                }
                c
            }
        };
        if self.header.seq != fold.seq + 1 {
            return Err(format!("the entry's seq is {}, and the fold is at {}", self.header.seq, fold.seq));
        }
        Ok(fold)
    }

    /// The entry that holds the write's idempotency key, when it landed
    /// before. A receive op asks first: the write's ops may no longer pass
    /// over the head they changed, and a resend answers where it landed.
    pub fn landed(&self) -> Result<Option<u64>, String> {
        let Some(key) = &self.header.intent.key else { return Ok(None) };
        Ok(self.fold()?.keys.get(key).copied())
    }

    /// The writer's verified name and the host's clock, in Unix seconds:
    /// what a receive op stamps on a record it writes.
    pub fn stamp(&self) -> (&str, u64) {
        (&self.header.writer, self.header.at_ms / 1000)
    }
}

/// A receive op's answer: what the write adds to its log, or why it adds
/// nothing. The entry's canonical bytes are absent when the write
/// changes nothing, landed before (`landed`, its key's entry), changes a
/// record that an unread entry changed (`overlap`, for a person), was
/// refused by a check (`refused`), or wrote over a head the writer did
/// not read (`moved`), or names a record that changed since the writer
/// read it (`conflict`). `heads` are the documents after the write, by
/// store kind, and `unknown_fields` the fields a written document
/// carries that this build does not know (D-167).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, ts_rs::TS)]
#[ts(export)]
pub struct Received {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub entry: Option<String>,
    pub changes: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub landed: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "{ kind: string, id: string, seq: number }")]
    pub overlap: Option<Overlap>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub refused: Option<Refused>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub moved: Option<Moved>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub conflict: Option<Conflict>,
    /// An op that did not run: one the log does not take, or arguments
    /// it cannot read.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub failed: Option<Failed>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[ts(optional, as = "Option<Vec<String>>")]
    pub unknown_fields: Vec<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    #[ts(optional, as = "Option<BTreeMap<String, String>>")]
    pub heads: BTreeMap<String, String>,
    /// The references the write adds that name a record missing from
    /// the world its writer read (`Write::read`, D-233): the write is
    /// refused, and no entry comes back.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[ts(optional, as = "Option<Vec<crate::Broken>>")]
    pub broken: Vec<crate::Broken>,
    /// The references the entry touches that are broken now
    /// (`Write::now`): the write's warnings, for a person.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[ts(optional, as = "Option<Vec<crate::Broken>>")]
    pub references: Vec<crate::Broken>,
    /// The documents the write made that the host stores before the
    /// entry, so the entry never names a document the store does not
    /// hold.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[ts(optional, as = "Option<Vec<MadeDocument>>")]
    pub made: Vec<MadeDocument>,
    /// What the op answers beside the entry, for the route's answer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "unknown")]
    pub answer: Option<Value>,
}

/// A document a write made, for the host to store before the entry: its
/// canonical bytes under their hash, with its kind and version for the
/// envelope, and the pointer it becomes the head of (absent: a blob
/// only).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export)]
pub struct MadeDocument {
    pub kind: String,
    pub version: u32,
    pub document: String,
    pub document_hash: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub pointer: Option<String>,
}

impl Received {
    /// The answer of a write whose key landed in entry `seq` before.
    pub fn landed(seq: u64) -> Self {
        Received { landed: Some(seq), ..Default::default() }
    }

    /// The answer of a refused op.
    pub fn refused(op: usize, refusals: Value) -> Self {
        Received { refused: Some(Refused { op, refusals }), ..Default::default() }
    }

    /// The answer of an op that did not run.
    pub fn failed(op: usize, error: String) -> Self {
        Received { failed: Some(Failed { op, error }), ..Default::default() }
    }

    /// The answer of an op whose record `id` changed since the writer
    /// read it.
    pub fn conflict(op: usize, id: String) -> Self {
        Received { conflict: Some(Conflict { op, id }), ..Default::default() }
    }

    /// The answer of a whole-document write over a head it did not read.
    pub fn moved(current: Option<String>) -> Self {
        Received { moved: Some(Moved { current: current.unwrap_or_default() }), ..Default::default() }
    }
}

/// An op of a write that did not run, and why.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export)]
pub struct Failed {
    pub op: usize,
    pub error: String,
}

/// The record an op names, changed since the writer read it: nothing is
/// written, and the writer reads again.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export)]
pub struct Conflict {
    pub op: usize,
    pub id: String,
}

/// The head a whole-document write did not read: the writer reads again.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export)]
pub struct Moved {
    /// The head now (empty: none).
    pub current: String,
}

/// What a write that leaves `next` adds: the entry over the fold, unless
/// the key landed, nothing changed, or an unread entry changed a record
/// this write changes. `canonical` writes the entry's bytes.
pub fn receive(write: &Write, next: &[KindRecords], hash: Hash, canonical: &dyn Fn(&Entry) -> Result<String, String>) -> Result<Received, String> {
    let fold = write.fold()?;
    // An unread entry that does not read is quarantined in the fold: it
    // changed nothing the write could overlap.
    let unread: Vec<Entry> = write.unread.iter().filter_map(|s| read_entry(&write.header.log, s).ok()).collect();
    Ok(match compose(&fold, write.header.clone(), next, hash) {
        Composed::Entry(e) => match overlap(&e.changes, &unread) {
            Some(o) => Received { overlap: Some(o), ..Default::default() },
            None => {
                let mut references = Vec::new();
                if let Some(kinds) = &write.references {
                    let log = &write.header.log;
                    if let Some(read) = &write.read {
                        let broken = crate::added_in(kinds, read, &fold.records, log, &e);
                        if !broken.is_empty() {
                            return Ok(Received { broken, ..Default::default() });
                        }
                    }
                    if let Some(mut now) = write.now.clone().or_else(|| write.read.clone()) {
                        // The write's own log, judged when it was: as the
                        // fold after this entry.
                        if now.contains_key(log) {
                            let mut after = fold.records.clone();
                            for c in &e.changes {
                                let records = after.entry(c.kind.clone()).or_default();
                                match &c.after {
                                    Some(v) => {
                                        records.insert(c.id.clone(), crate::Held { version: c.version, value: v.clone() });
                                    }
                                    None => {
                                        records.remove(&c.id);
                                    }
                                }
                            }
                            now.insert(log.clone(), after);
                        }
                        references = crate::touched_in(crate::broken(kinds, &now), kinds, log, &e, write.header.seq);
                    }
                }
                Received { changes: e.changes.len(), entry: Some(canonical(&e)?), references, ..Default::default() }
            }
        },
        Composed::Unchanged => Received::default(),
        Composed::Landed(seq) => Received { landed: Some(seq), ..Default::default() },
    })
}

/// One op of a write's intent, as a page sends it (`POST /logs/<log>`):
/// the capability, the op, and its arguments.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct OpCall {
    pub cap: String,
    pub op: String,
    #[serde(default)]
    pub args: Value,
}

impl OpCall {
    /// The op as the capability's own request: its arguments with its
    /// name as the `op` tag. A capability that does not take the op
    /// answers the error that names it.
    pub fn request<T: serde::de::DeserializeOwned>(&self) -> Result<T, String> {
        let mut args = match &self.args {
            Value::Object(m) => m.clone(),
            Value::Null => serde_json::Map::new(),
            _ => return Err("args is not an object".into()),
        };
        args.insert("op".into(), Value::String(self.op.clone()));
        serde_json::from_value(Value::Object(args)).map_err(|e| format!("{} {}: {e}", self.cap, self.op))
    }
}

/// The op of a write that a check refused, and the refusals its
/// capability answered: the write lands nothing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export)]
pub struct Refused {
    pub op: usize,
    #[ts(type = "Array<unknown>")]
    pub refusals: Value,
}
