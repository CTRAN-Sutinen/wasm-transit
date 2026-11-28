//! The log capability (D-190). `OPS.md` holds the op contracts. The
//! server keeps the disruptions, stops, and plans logs through it and folds
//! and reverts every log with it. The alerts and notes logs are written by
//! their own receive ops.

#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
#[cfg(feature = "bindings")]
use ts_rs::TS;

use transit_log::{Checkpoint, Header, KindRecords, Quarantined, Records, Stored};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS))]
#[cfg_attr(feature = "bindings", ts(export, export_to = "cap_log/"))]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Request {
    Compose {
        #[serde(default)]
        #[cfg_attr(feature = "bindings", ts(optional, type = "any"))]
        checkpoint: Option<serde_json::Value>,
        #[cfg_attr(feature = "bindings", ts(type = "any"))]
        header: serde_json::Value,
        #[cfg_attr(feature = "bindings", ts(type = "any"))]
        next: serde_json::Value,
        #[serde(default)]
        #[cfg_attr(
            feature = "bindings",
            ts(optional, type = "Array<{ seq: number, text: string }>")
        )]
        unread: Option<serde_json::Value>,
    },
    Fold {
        log: String,
        #[serde(default)]
        #[cfg_attr(feature = "bindings", ts(optional, type = "any"))]
        checkpoint: Option<serde_json::Value>,
        #[cfg_attr(
            feature = "bindings",
            ts(type = "Array<{ seq: number, text: string }>")
        )]
        entries: serde_json::Value,
    },
    Revert {
        log: String,
        #[serde(default)]
        #[cfg_attr(feature = "bindings", ts(optional, type = "any"))]
        checkpoint: Option<serde_json::Value>,
        #[cfg_attr(
            feature = "bindings",
            ts(type = "Array<{ seq: number, text: string }>")
        )]
        entries: serde_json::Value,
        #[cfg_attr(feature = "bindings", ts(type = "number"))]
        target: u64,
        #[cfg_attr(feature = "bindings", ts(type = "any"))]
        header: serde_json::Value,
        #[serde(default)]
        #[cfg_attr(
            feature = "bindings",
            ts(
                optional,
                type = "Array<{ kind: string, id: string, version: number, after?: unknown }>"
            )
        )]
        written: Option<serde_json::Value>,
    },
    CloseGap {
        #[cfg_attr(feature = "bindings", ts(type = "any"))]
        header: serde_json::Value,
    },
    References {
        #[cfg_attr(
            feature = "bindings",
            ts(
                type = "Array<{ store_kind: string, log?: string, records?: Array<{ kind: string, collection: string }>, references?: Array<{ path: string, target: string }> }>"
            )
        )]
        kinds: serde_json::Value,
        #[cfg_attr(
            feature = "bindings",
            ts(
                type = "Record<string, Record<string, Record<string, { version: number, value: unknown }>>>"
            )
        )]
        folds: serde_json::Value,
        #[serde(default)]
        #[cfg_attr(
            feature = "bindings",
            ts(
                optional,
                type = "Record<string, Array<{ seq: number, text: string }>>"
            )
        )]
        entries: Option<serde_json::Value>,
        #[serde(default)]
        #[cfg_attr(feature = "bindings", ts(optional))]
        log: Option<String>,
        #[serde(default)]
        #[cfg_attr(
            feature = "bindings",
            ts(optional, type = "{ seq: number, text: string }")
        )]
        entry: Option<serde_json::Value>,
    },
    Naming {
        #[cfg_attr(
            feature = "bindings",
            ts(
                type = "Array<{ store_kind: string, log?: string, records?: Array<{ kind: string, collection: string }>, references?: Array<{ path: string, target: string }> }>"
            )
        )]
        kinds: serde_json::Value,
        #[cfg_attr(
            feature = "bindings",
            ts(
                type = "Record<string, Record<string, Record<string, { version: number, value: unknown }>>>"
            )
        )]
        folds: serde_json::Value,
        hashes: Vec<String>,
    },
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "bindings", derive(TS))]
#[cfg_attr(feature = "bindings", ts(export, export_to = "cap_log/"))]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Response {
    Compose {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[cfg_attr(feature = "bindings", ts(optional))]
        entry: Option<String>,
        changes: usize,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[cfg_attr(feature = "bindings", ts(optional, type = "number"))]
        landed: Option<u64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[cfg_attr(
            feature = "bindings",
            ts(optional, type = "{ kind: string, id: string, seq: number }")
        )]
        overlap: Option<transit_log::Overlap>,
    },
    Fold {
        #[cfg_attr(feature = "bindings", ts(type = "number"))]
        seq: u64,
        checkpoint: String,
        #[cfg_attr(
            feature = "bindings",
            ts(type = "Record<string, Record<string, { version: number, value: unknown }>>")
        )]
        records: Records,
        #[cfg_attr(
            feature = "bindings",
            ts(type = "Array<{ seq: number, reason: string }>")
        )]
        quarantined: Vec<Quarantined>,
    },
    Revert {
        entry: String,
        changes: usize,
        #[cfg_attr(
            feature = "bindings",
            ts(
                type = "Array<{ kind: string, id: string, now?: { version: number, value: unknown }, prior?: { version: number, value: unknown } }>"
            )
        )]
        states: Vec<transit_log::RevertState>,
    },
    CloseGap {
        entry: String,
    },
    References {
        #[cfg_attr(
            feature = "bindings",
            ts(
                type = "Array<{ log: string, kind: string, id: string, field: string, target: string, names: string, entry?: { log: string, seq: number } }>"
            )
        )]
        broken: Vec<transit_log::Broken>,
        /// Absent with `entry`.
        #[serde(skip_serializing_if = "Vec::is_empty")]
        #[cfg_attr(
            feature = "bindings",
            ts(
                optional,
                type = "Array<{ log: string, kind: string, id: string, version: number, reads: number }>"
            )
        )]
        unjudged: Vec<transit_log::Unjudged>,
    },
    Naming {
        #[cfg_attr(
            feature = "bindings",
            ts(
                type = "Array<{ log: string, kind: string, id: string, field: string, target: string, names: string }>"
            )
        )]
        references: Vec<transit_log::Naming>,
    },
}

use cap_abi::record_hash;

fn checkpoint_of(
    log: &str,
    value: Option<serde_json::Value>,
) -> Result<Checkpoint, cap_abi::Refusal> {
    match value {
        None => Ok(Checkpoint::empty(log)),
        Some(v) => {
            let c: Checkpoint = cap_abi::versioned(v)?;
            if c.log != log {
                return Err(format!("a checkpoint of the {} log, not the {log} log", c.log).into());
            }
            Ok(c)
        }
    }
}

fn parse<T: serde::de::DeserializeOwned>(
    what: &str,
    value: serde_json::Value,
) -> Result<T, String> {
    serde_json::from_value(value).map_err(|e| format!("{what}: {e}"))
}

pub fn dispatch(request: Request) -> Result<Response, cap_abi::Refusal> {
    match request {
        Request::Compose {
            checkpoint,
            header,
            next,
            unread,
        } => {
            let header: Header = parse("header", header)?;
            let next: Vec<KindRecords> = parse("next", next)?;
            let unread: Vec<Stored> = parse(
                "unread",
                unread.unwrap_or(serde_json::Value::Array(Vec::new())),
            )?;
            let write = transit_log::Write {
                checkpoint,
                header,
                unread,
                references: None,
                read: None,
                now: None,
                today: None,
            };
            let received = transit_log::receive(&write, &next, &record_hash, &|e| {
                cap_abi::canonical_bytes(e)
            })?;
            Ok(Response::Compose {
                entry: received.entry,
                changes: received.changes,
                landed: received.landed,
                overlap: received.overlap,
            })
        }
        Request::References {
            kinds,
            folds,
            entries,
            log,
            entry,
        } => {
            let kinds: Vec<transit_log::KindReferences> = parse("kinds", kinds)?;
            let folds: std::collections::BTreeMap<String, Records> = parse("folds", folds)?;
            let mut broken = transit_log::broken(&kinds, &folds);
            if let Some(entry) = entry {
                let log = log.ok_or("an entry needs its log")?;
                let entry: Stored = parse("entry", entry)?;
                return Ok(Response::References {
                    broken: transit_log::touched(broken, &kinds, &log, &entry),
                    unjudged: Vec::new(),
                });
            }
            let unjudged = transit_log::unjudged(&kinds, &folds);
            if let Some(entries) = entries {
                let entries: std::collections::BTreeMap<String, Vec<Stored>> =
                    parse("entries", entries)?;
                transit_log::blame(&mut broken, &kinds, &entries);
            }
            Ok(Response::References { broken, unjudged })
        }
        Request::Naming {
            kinds,
            folds,
            hashes,
        } => {
            let kinds: Vec<transit_log::KindReferences> = parse("kinds", kinds)?;
            let folds: std::collections::BTreeMap<String, Records> = parse("folds", folds)?;
            Ok(Response::Naming {
                references: transit_log::naming(&kinds, &folds, &hashes),
            })
        }
        Request::Fold {
            log,
            checkpoint,
            entries,
        } => {
            let entries: Vec<Stored> = parse("entries", entries)?;
            let folded =
                transit_log::fold(checkpoint_of(&log, checkpoint)?, &entries, &record_hash)?;
            Ok(Response::Fold {
                seq: folded.fold.seq,
                checkpoint: cap_abi::canonical_bytes(&folded.fold)?,
                records: folded.fold.records,
                quarantined: folded.quarantined,
            })
        }
        Request::Revert {
            log,
            checkpoint,
            entries,
            target,
            header,
            written,
        } => {
            let entries: Vec<Stored> = parse("entries", entries)?;
            let header: Header = parse("header", header)?;
            let start = checkpoint_of(&log, checkpoint)?;
            if target <= start.seq {
                return Err(format!(
                    "entry {target} is not after the checkpoint at {}",
                    start.seq
                )
                .into());
            }
            let at = entries
                .iter()
                .position(|e| e.seq == target)
                .ok_or_else(|| format!("entry {target} is not in the entries given"))?;
            let before = transit_log::fold(start.clone(), &entries[..at], &record_hash)?;
            if before.fold.seq != target - 1 {
                return Err(format!(
                    "the entries before {target} end at {}: the log has a gap",
                    before.fold.seq
                )
                .into());
            }
            let head = transit_log::fold(start, &entries, &record_hash)?;
            if let Some(q) = head.quarantined.iter().find(|q| q.seq == target) {
                return Err(format!(
                    "entry {target} is quarantined ({}): it changed nothing to revert",
                    q.reason
                )
                .into());
            }
            if header.seq != head.fold.seq + 1 {
                return Err(format!(
                    "the entry's seq is {}, and the log's head is {}",
                    header.seq, head.fold.seq
                )
                .into());
            }
            let entry = transit_log::read_entry(&log, &entries[at])?;
            let written: Vec<transit_log::Written> = parse(
                "written",
                written.unwrap_or(serde_json::Value::Array(Vec::new())),
            )?;
            let states =
                transit_log::revert_states(&before.fold, &head.fold, &entry, &record_hash)?;
            let changes = transit_log::revert_with(&states, &written, &record_hash)?;
            if changes.is_empty() {
                return Err(format!("entry {target} changed no record").into());
            }
            let n = changes.len();
            Ok(Response::Revert {
                entry: cap_abi::canonical_bytes(&transit_log::Entry::new(
                    header,
                    changes,
                    Some(target),
                ))?,
                changes: n,
                states,
            })
        }
        Request::CloseGap { header } => {
            let header: Header = parse("header", header)?;
            if header
                .intent
                .note
                .as_deref()
                .is_none_or(|n| n.trim().is_empty())
            {
                return Err(
                    "an entry that closes a gap names its cause (header.intent.note)".into(),
                );
            }
            Ok(Response::CloseGap {
                entry: cap_abi::canonical_bytes(&transit_log::Entry::new(
                    header,
                    Vec::new(),
                    None,
                ))?,
            })
        }
    }
}

pub fn dispatch_json(text: &str) -> String {
    cap_abi::answer(text, dispatch)
}

pub fn dispatch_bytes(op: u32, _bytes: &[u8]) -> String {
    serde_json::json!({ "error": { "message": format!("unknown bulk op {op}: the log is JSON (compose, fold)") } }).to_string()
}

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
