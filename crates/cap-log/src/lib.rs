//! cap-log: the log capability (D-190, D-197).
//!
//! One question: **what does this write add to the log, and what does
//! the log hold?** A log is the record of one consistency domain, an
//! entry for each write. This capability never reads a record. A record
//! is a JSON value named by its kind and its id, in its kind's version.
//! The capability that owns a kind turns its document into records and
//! records back into its document (cap-stops and cap-requests
//! `log_records`, `log_document`). So one entry may change the records
//! of two capabilities, and no capability calls another.
//!
//! - `compose` answers the entry for one write: the records that differ
//!   from the fold, each with its hash as stored before.
//! - `fold` folds stored entries after a checkpoint into the records by
//!   kind, and quarantines an entry it cannot read or apply.
//! - `revert` answers the entry that writes back the states before an
//!   entry. A record the entry created leaves the fold (D-198).
//! - `close_gap` answers the entry with no changes that closes a gap.
//!
//! A record's hash is the canonical hash of the record as stored
//! (`cap_abi::canonical_hash`). The module is stateless: the host
//! stores the entries and checkpoints and gives them back in order.

#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use transit_log::{Checkpoint, Composed, Header, KindRecords, Quarantined, Records, Stored};

/// One operation call.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "cap_log/")]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Request {
    /// The entry for one write: for each kind `next` names, every record
    /// of that kind's document after the write. A record of a named kind
    /// that `next` leaves out leaves the fold. The answer is the entry,
    /// or none when nothing changed, or the entry that holds the write's
    /// idempotency key (`header.intent.key`) when it landed before.
    Compose {
        /// The fold the write read (absent: the empty log).
        #[serde(default)]
        #[ts(optional, type = "any")]
        checkpoint: Option<serde_json::Value>,
        #[ts(type = "any")]
        header: serde_json::Value,
        #[ts(type = "any")]
        next: serde_json::Value,
    },
    /// The fold of a log's entries after a checkpoint (absent: from the
    /// empty log), in `seq` order.
    Fold {
        log: String,
        #[serde(default)]
        #[ts(optional, type = "any")]
        checkpoint: Option<serde_json::Value>,
        #[ts(type = "Array<{ seq: number, text: string }>")]
        entries: serde_json::Value,
    },
    /// The entry that reverts entry `target`. `checkpoint` is at or
    /// before `target - 1`, and `entries` runs from after it to the head.
    Revert {
        log: String,
        #[serde(default)]
        #[ts(optional, type = "any")]
        checkpoint: Option<serde_json::Value>,
        #[ts(type = "Array<{ seq: number, text: string }>")]
        entries: serde_json::Value,
        #[ts(type = "number")]
        target: u64,
        #[ts(type = "any")]
        header: serde_json::Value,
    },
    /// The entry with no changes that closes a gap, its cause in
    /// `header.intent.note`.
    CloseGap {
        #[ts(type = "any")]
        header: serde_json::Value,
    },
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "cap_log/")]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Response {
    Compose {
        /// The entry's canonical bytes, which the host stores at its
        /// `seq`. Absent when the write changes nothing or landed before.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        entry: Option<String>,
        /// How many records the entry changes.
        changes: usize,
        /// The entry that holds the write's key, when it landed before.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional, type = "number")]
        landed: Option<u64>,
    },
    Fold {
        /// The last entry the fold read.
        #[ts(type = "number")]
        seq: u64,
        /// The fold, canonically serialized: the next fold starts here.
        checkpoint: String,
        /// The records by kind, then by id, each with its version: what
        /// each kind's capability assembles its document from.
        #[ts(type = "Record<string, Record<string, { version: number, value: unknown }>>")]
        records: Records,
        /// The entries the fold did not apply.
        #[ts(type = "Array<{ seq: number, reason: string }>")]
        quarantined: Vec<Quarantined>,
    },
    Revert {
        entry: String,
        changes: usize,
    },
    CloseGap {
        entry: String,
    },
}

/// A record's identity in a log: the canonical hash of the record as
/// stored. A record that does not serialize gets a name that never
/// equals a hash.
fn record_hash(value: &serde_json::Value) -> String {
    cap_abi::canonical_hash(value).unwrap_or_else(|e| format!("unhashable: {e}"))
}

fn checkpoint_of(log: &str, value: Option<serde_json::Value>) -> Result<Checkpoint, String> {
    match value {
        None => Ok(Checkpoint::empty(log)),
        Some(v) => {
            let c: Checkpoint = cap_abi::versioned(v)?;
            if c.log != log {
                return Err(format!("a checkpoint of the {} log, not the {log} log", c.log));
            }
            Ok(c)
        }
    }
}

fn parse<T: serde::de::DeserializeOwned>(what: &str, value: serde_json::Value) -> Result<T, String> {
    serde_json::from_value(value).map_err(|e| format!("{what}: {e}"))
}

/// The typed dispatch. Failures are `{"error": ...}`, never panics.
pub fn dispatch(request: Request) -> Result<Response, String> {
    match request {
        Request::Compose { checkpoint, header, next } => {
            let header: Header = parse("header", header)?;
            let next: Vec<KindRecords> = parse("next", next)?;
            let fold = checkpoint_of(&header.log, checkpoint)?;
            if header.seq != fold.seq + 1 {
                return Err(format!("the entry's seq is {}, and the fold is at {}", header.seq, fold.seq));
            }
            Ok(match transit_log::compose(&fold, header, &next, &record_hash) {
                Composed::Entry(e) => Response::Compose { changes: e.changes.len(), entry: Some(cap_abi::canonical_bytes(&e)?), landed: None },
                Composed::Unchanged => Response::Compose { entry: None, changes: 0, landed: None },
                Composed::Landed(seq) => Response::Compose { entry: None, changes: 0, landed: Some(seq) },
            })
        }
        Request::Fold { log, checkpoint, entries } => {
            let entries: Vec<Stored> = parse("entries", entries)?;
            let folded = transit_log::fold(checkpoint_of(&log, checkpoint)?, &entries, &record_hash)?;
            Ok(Response::Fold {
                seq: folded.fold.seq,
                checkpoint: cap_abi::canonical_bytes(&folded.fold)?,
                records: folded.fold.records,
                quarantined: folded.quarantined,
            })
        }
        Request::Revert { log, checkpoint, entries, target, header } => {
            let entries: Vec<Stored> = parse("entries", entries)?;
            let header: Header = parse("header", header)?;
            let start = checkpoint_of(&log, checkpoint)?;
            if target <= start.seq {
                return Err(format!("entry {target} is not after the checkpoint at {}", start.seq));
            }
            let at = entries.iter().position(|e| e.seq == target).ok_or_else(|| format!("entry {target} is not in the entries given"))?;
            let before = transit_log::fold(start.clone(), &entries[..at], &record_hash)?;
            if before.fold.seq != target - 1 {
                return Err(format!("the entries before {target} end at {}: the log has a gap", before.fold.seq));
            }
            let head = transit_log::fold(start, &entries, &record_hash)?;
            if let Some(q) = head.quarantined.iter().find(|q| q.seq == target) {
                return Err(format!("entry {target} is quarantined ({}): it changed nothing to revert", q.reason));
            }
            if header.seq != head.fold.seq + 1 {
                return Err(format!("the entry's seq is {}, and the log's head is {}", header.seq, head.fold.seq));
            }
            let entry = transit_log::read_entry(&log, &entries[at])?;
            let changes = transit_log::revert(&before.fold, &head.fold, &entry, &record_hash)?;
            if changes.is_empty() {
                return Err(format!("entry {target} changed no record"));
            }
            let n = changes.len();
            Ok(Response::Revert { entry: cap_abi::canonical_bytes(&transit_log::Entry::new(header, changes, Some(target)))?, changes: n })
        }
        Request::CloseGap { header } => {
            let header: Header = parse("header", header)?;
            if header.intent.note.as_deref().is_none_or(|n| n.trim().is_empty()) {
                return Err("an entry that closes a gap names its cause (header.intent.note)".into());
            }
            Ok(Response::CloseGap { entry: cap_abi::canonical_bytes(&transit_log::Entry::new(header, Vec::new(), None))? })
        }
    }
}

/// The JSON seam: `{"op": ...}` in, `{"ok": ...}` / `{"error": ...}` out.
pub fn dispatch_json(text: &str) -> String {
    cap_abi::answer(text, dispatch)
}

/// The log is JSON: no bulk op exists.
pub fn dispatch_bytes(op: u32, _bytes: &[u8]) -> String {
    serde_json::json!({ "error": format!("unknown bulk op {op}: the log is JSON (compose, fold)") }).to_string()
}

cap_abi::capability! {
    id: "log",
    json: dispatch_json,
    bytes: dispatch_bytes,
    documents: [],
}
