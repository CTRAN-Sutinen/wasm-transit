//! A write's last step, shared by every receive op (D-228). The host stores
//! the entry with a conditional create, and asks again over the new head
//! when another write took the seq.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{compose, overlap, read_entry, Checkpoint, Composed, Entry, Hash, Header, KindRecords, Overlap, Stored};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Write {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checkpoint: Option<Value>,
    pub header: Header,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unread: Vec<Stored>,
    /// Absent means no reference check (D-233).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(skip)]
    pub references: Option<Vec<crate::KindReferences>>,
    /// A reference that the write adds to a record missing then refuses the
    /// write ([`Received::broken`]). A log left out is not judged.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(skip)]
    pub read: Option<BTreeMap<String, crate::Records>>,
    /// Absent means as `read`. The write's own log is the fold after the
    /// entry. The references that the entry touches and that are broken now
    /// are its warnings ([`Received::references`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(skip)]
    pub now: Option<BTreeMap<String, crate::Records>>,
    /// "YYYY-MM-DD" in the agency's time zone, from the host's clock. A
    /// reference rule reads a dated state on it (D-238). Absent means no rule
    /// that needs a date is judged.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(skip)]
    pub today: Option<String>,
}

impl Write {
    pub fn fold(&self) -> Result<Checkpoint, transit_version::Refusal> {
        let fold = match &self.checkpoint {
            None => Checkpoint::empty(&self.header.log),
            Some(v) => {
                // The log reads no checkpoint ahead.
                let found = v.get("schema_version").and_then(Value::as_u64).unwrap_or(0) as u32;
                if !matches!(transit_version::reads::<Checkpoint>(found)?, transit_version::ReadAs::Current) {
                    return Err(transit_version::refused(crate::CheckpointKind::KIND, found, crate::CHECKPOINT_VERSION));
                }
                let c: Checkpoint = serde_json::from_value(v.clone()).map_err(|e| format!("invalid document: {e}"))?;
                if c.log != self.header.log {
                    return Err(format!("a checkpoint of the {} log, not the {} log", c.log, self.header.log).into());
                }
                c
            }
        };
        if self.header.seq != fold.seq + 1 {
            return Err(format!("the entry's seq is {}, and the fold is at {}", self.header.seq, fold.seq).into());
        }
        Ok(fold)
    }

    /// A receive op asks first, because the write's ops may no longer pass
    /// over the head they changed, and a resend answers where it landed.
    pub fn landed(&self) -> Result<Option<u64>, transit_version::Refusal> {
        let Some(key) = &self.header.intent.key else { return Ok(None) };
        Ok(self.fold()?.keys.get(key).copied())
    }

    /// What a receive op stamps on a record it writes: the writer and the
    /// host's clock in Unix seconds.
    pub fn stamp(&self) -> (&str, u64) {
        (&self.header.writer, self.header.at_ms / 1000)
    }
}

/// `entry` is absent when the write changes nothing, landed before, hit an
/// overlap, a refusal, a moved head, or a conflict. `unknown_fields` names
/// the fields that a written document carries and this build does not know
/// (D-167).
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub failed: Option<Failed>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[ts(optional, as = "Option<Vec<String>>")]
    pub unknown_fields: Vec<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    #[ts(optional, as = "Option<BTreeMap<String, String>>")]
    pub heads: BTreeMap<String, String>,
    /// The write is refused and no entry comes back (`Write::read`, D-233).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[ts(optional, as = "Option<Vec<crate::Broken>>")]
    pub broken: Vec<crate::Broken>,
    /// The write's warnings, for a person (`Write::now`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[ts(optional, as = "Option<Vec<crate::Broken>>")]
    pub references: Vec<crate::Broken>,
    /// The host stores these before the entry, so the entry never names a
    /// document that the store does not hold.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[ts(optional, as = "Option<Vec<MadeDocument>>")]
    pub made: Vec<MadeDocument>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "unknown")]
    pub answer: Option<Value>,
}

/// `pointer` is absent for a blob only.
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
    pub fn landed(seq: u64) -> Self {
        Received { landed: Some(seq), ..Default::default() }
    }

    pub fn refused(op: usize, refusals: Value) -> Self {
        Received { refused: Some(Refused { op, refusals }), ..Default::default() }
    }

    pub fn failed(op: usize, error: impl Into<transit_version::Refusal>) -> Self {
        Received { failed: Some(Failed { op, error: error.into() }), ..Default::default() }
    }

    pub fn conflict(op: usize, id: String) -> Self {
        Received { conflict: Some(Conflict { op, id }), ..Default::default() }
    }

    pub fn moved(current: Option<String>) -> Self {
        Received { moved: Some(Moved { current: current.unwrap_or_default() }), ..Default::default() }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export)]
pub struct Failed {
    pub op: usize,
    #[ts(type = "{ message: string, code?: string }")]
    pub error: transit_version::Refusal,
}

/// Nothing is written, and the writer reads again.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export)]
pub struct Conflict {
    pub op: usize,
    pub id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export)]
pub struct Moved {
    /// Empty means no head.
    pub current: String,
}

pub fn receive(write: &Write, next: &[KindRecords], hash: Hash, canonical: &dyn Fn(&Entry) -> Result<String, String>) -> Result<Received, transit_version::Refusal> {
    let fold = write.fold()?;
    // An unread entry that does not read is quarantined in the fold, so it
    // changed nothing that the write could overlap.
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
                        // The write's own log is judged as the fold after
                        // this entry.
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct OpCall {
    pub cap: String,
    pub op: String,
    #[serde(default)]
    pub args: Value,
}

impl OpCall {
    /// A capability that does not take the op answers the error that names
    /// it.
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export)]
pub struct Refused {
    pub op: usize,
    #[ts(type = "Array<unknown>")]
    pub refusals: Value,
}
