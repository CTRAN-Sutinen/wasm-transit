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
//!   from the fold, each with its hash as stored before. A record the
//!   write changes that an entry its writer did not read changed too
//!   answers the overlap, and no entry.
//! - `fold` folds stored entries after a checkpoint into the records by
//!   kind, and quarantines an entry it cannot read or apply.
//! - `revert` answers the entry that writes back the states before an
//!   entry. A record the entry created leaves the fold (D-198). A kind
//!   whose merge would undo that gives the records to write back
//!   instead (`written`, from the kind's capability, over the `states`
//!   the revert answers).
//! - `close_gap` answers the entry with no changes that closes a gap.
//!
//! A record's hash is the canonical hash of the record as stored
//! (`cap_abi::canonical_hash`). The module is stateless: the host
//! stores the entries and checkpoints and gives them back in order.

#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use transit_log::{Checkpoint, Header, KindRecords, Quarantined, Records, Stored};

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
    /// `unread` are the entries after the fold the writer read: a record
    /// the write changes that one of them changed answers `overlap`, and
    /// no entry.
    Compose {
        /// The fold the write runs over (absent: the empty log).
        #[serde(default)]
        #[ts(optional, type = "any")]
        checkpoint: Option<serde_json::Value>,
        #[ts(type = "any")]
        header: serde_json::Value,
        #[ts(type = "any")]
        next: serde_json::Value,
        #[serde(default)]
        #[ts(optional, type = "Array<{ seq: number, text: string }>")]
        unread: Option<serde_json::Value>,
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
    /// `written` names records to write back in place of their states
    /// before, as a kind's capability answers them over the `states`.
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
        #[serde(default)]
        #[ts(optional, type = "Array<{ kind: string, id: string, version: number, after?: unknown }>")]
        written: Option<serde_json::Value>,
    },
    /// The entry with no changes that closes a gap, its cause in
    /// `header.intent.note`.
    CloseGap {
        #[ts(type = "any")]
        header: serde_json::Value,
    },
    /// Each broken reference of the folds (D-232, D-233): a declared
    /// reference (the kind table's `kinds`, each with its `log`,
    /// `records`, and `references`) whose target record its log's fold
    /// does not hold. `folds` are the logs' records by log, as `fold`
    /// answers them. A target whose log is not given is not judged.
    /// With `entries` (each log's stored entries), each broken reference
    /// names the entry that broke it. With `entry` (one entry of `log`,
    /// as stored), the answer is only the broken references that entry
    /// touches: what a write answers as warnings.
    References {
        #[ts(type = "Array<{ store_kind: string, log?: string, records?: Array<{ kind: string, collection: string }>, references?: Array<{ path: string, target: string }> }>")]
        kinds: serde_json::Value,
        #[ts(type = "Record<string, Record<string, Record<string, { version: number, value: unknown }>>>")]
        folds: serde_json::Value,
        #[serde(default)]
        #[ts(optional, type = "Record<string, Array<{ seq: number, text: string }>>")]
        entries: Option<serde_json::Value>,
        #[serde(default)]
        #[ts(optional)]
        log: Option<String>,
        #[serde(default)]
        #[ts(optional, type = "{ seq: number, text: string }")]
        entry: Option<serde_json::Value>,
    },
    /// Each declared reference of the folds whose value is one of
    /// `hashes`: what names the documents a pointer's revert drops.
    Naming {
        #[ts(type = "Array<{ store_kind: string, log?: string, records?: Array<{ kind: string, collection: string }>, references?: Array<{ path: string, target: string }> }>")]
        kinds: serde_json::Value,
        #[ts(type = "Record<string, Record<string, Record<string, { version: number, value: unknown }>>>")]
        folds: serde_json::Value,
        hashes: Vec<String>,
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
        /// A record the write changes that an unread entry changed too:
        /// a person settles it.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional, type = "{ kind: string, id: string, seq: number }")]
        overlap: Option<transit_log::Overlap>,
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
        /// Each record the revert writes back: its state now and before
        /// the entry, what a kind's capability answers `written` from.
        #[ts(type = "Array<{ kind: string, id: string, now?: { version: number, value: unknown }, prior?: { version: number, value: unknown } }>")]
        states: Vec<transit_log::RevertState>,
    },
    CloseGap {
        entry: String,
    },
    References {
        #[ts(type = "Array<{ log: string, kind: string, id: string, field: string, target: string, names: string, entry?: { log: string, seq: number } }>")]
        broken: Vec<transit_log::Broken>,
        /// The records in a version newer than the table reads, whose
        /// references are not judged (absent with `entry`).
        #[serde(skip_serializing_if = "Vec::is_empty")]
        #[ts(optional, type = "Array<{ log: string, kind: string, id: string, version: number, reads: number }>")]
        unjudged: Vec<transit_log::Unjudged>,
    },
    Naming {
        #[ts(type = "Array<{ log: string, kind: string, id: string, field: string, target: string, names: string }>")]
        references: Vec<transit_log::Naming>,
    },

}

use cap_abi::record_hash;

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
        Request::Compose { checkpoint, header, next, unread } => {
            let header: Header = parse("header", header)?;
            let next: Vec<KindRecords> = parse("next", next)?;
            let unread: Vec<Stored> = parse("unread", unread.unwrap_or(serde_json::Value::Array(Vec::new())))?;
            let write = transit_log::Write { checkpoint, header, unread, references: None, read: None, now: None, today: None };
            let received = transit_log::receive(&write, &next, &record_hash, &|e| cap_abi::canonical_bytes(e))?;
            Ok(Response::Compose { entry: received.entry, changes: received.changes, landed: received.landed, overlap: received.overlap })
        }
        Request::References { kinds, folds, entries, log, entry } => {
            let kinds: Vec<transit_log::KindReferences> = parse("kinds", kinds)?;
            let folds: std::collections::BTreeMap<String, Records> = parse("folds", folds)?;
            let mut broken = transit_log::broken(&kinds, &folds);
            if let Some(entry) = entry {
                let log = log.ok_or("an entry needs its log")?;
                let entry: Stored = parse("entry", entry)?;
                return Ok(Response::References { broken: transit_log::touched(broken, &kinds, &log, &entry), unjudged: Vec::new() });
            }
            let unjudged = transit_log::unjudged(&kinds, &folds);
            if let Some(entries) = entries {
                let entries: std::collections::BTreeMap<String, Vec<Stored>> = parse("entries", entries)?;
                transit_log::blame(&mut broken, &kinds, &entries);
            }
            Ok(Response::References { broken, unjudged })
        }
        Request::Naming { kinds, folds, hashes } => {
            let kinds: Vec<transit_log::KindReferences> = parse("kinds", kinds)?;
            let folds: std::collections::BTreeMap<String, Records> = parse("folds", folds)?;
            Ok(Response::Naming { references: transit_log::naming(&kinds, &folds, &hashes) })
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
        Request::Revert { log, checkpoint, entries, target, header, written } => {
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
            let written: Vec<transit_log::Written> = parse("written", written.unwrap_or(serde_json::Value::Array(Vec::new())))?;
            let states = transit_log::revert_states(&before.fold, &head.fold, &entry, &record_hash)?;
            let changes = transit_log::revert_with(&states, &written, &record_hash)?;
            if changes.is_empty() {
                return Err(format!("entry {target} changed no record"));
            }
            let n = changes.len();
            Ok(Response::Revert { entry: cap_abi::canonical_bytes(&transit_log::Entry::new(header, changes, Some(target)))?, changes: n, states })
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

/// The worked examples, which the generator runs (no module build turns
/// the feature on).
#[cfg(feature = "examples")]
pub mod examples;

cap_abi::capability! {
    id: "log",
    json: dispatch_json,
    bytes: dispatch_bytes,
    documents: [],
    request: Request,
    speaks: &[
        ("log_entry", &[transit_log::ENTRY_VERSION]),
        ("log_checkpoint", &[transit_log::CHECKPOINT_VERSION]),
    ],
}
