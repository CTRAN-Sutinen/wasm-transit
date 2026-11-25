//! `compact` skips a tombstone that another log's record names (D-57).
//! Edit records are judged against the network at `load_edits` and reported,
//! never rejected (cap-divert `reports`), so a second verdict here would be
//! two answers to one question. The module holds no state. The host decides
//! how the store writes the log's objects (docs/FORMATS.md).

#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "cap_sync/")]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Request {
    SyncEdits {
        client: Vec<transit_sync::EditRecord>,
        server: Vec<transit_sync::EditRecord>,
        #[ts(type = "number")]
        now_ms: u64,
    },
    ReadEdits {
        #[ts(type = "any")]
        document: serde_json::Value,
    },
    CompactEdits {
        records: Vec<transit_sync::EditRecord>,
        #[ts(type = "number")]
        now_ms: u64,
        #[ts(type = "number")]
        horizon_ms: u64,
    },
    LogRecords { document: transit_sync::EditDocument },
    LogDocument {
        #[ts(type = "Record<string, { version: number, value: unknown }>")]
        records: std::collections::BTreeMap<String, transit_log::Held>,
        #[serde(default)]
        #[ts(optional, type = "{ version: number, value: unknown }")]
        document: Option<transit_log::Held>,
    },
    Receive {
        #[ts(type = "{ checkpoint?: unknown, header: unknown, unread?: Array<{ seq: number, text: string }> }")]
        write: transit_log::Write,
        #[serde(default)]
        #[ts(type = "Record<string, unknown>")]
        documents: std::collections::BTreeMap<String, serde_json::Value>,
        #[ts(type = "Array<{ cap: string, op: string, args?: unknown }>")]
        ops: Vec<transit_log::OpCall>,
    },
    LogRevert {
        #[ts(type = "Array<{ kind: string, id: string, now?: { version: number, value: unknown }, prior?: { version: number, value: unknown } }>")]
        states: Vec<transit_log::RevertState>,
        #[ts(type = "number")]
        now_ms: u64,
    },
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "cap_sync/")]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Response {
    SyncEdits {
        records: Vec<transit_sync::EditRecord>,
        document: String,
        document_hash: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        #[ts(optional, as = "Option<Vec<String>>")]
        unknown_fields: Vec<String>,
    },
    ReadEdits {
        records: Vec<transit_sync::EditRecord>,
        dropped: usize,
        schema_version: u32,
        document: String,
        document_hash: String,
    },
    CompactEdits {
        records: Vec<transit_sync::EditRecord>,
        document: String,
        document_hash: String,
        dropped: usize,
    },
    LogRecords {
        #[ts(type = "Array<{ kind: string, version: number, records: Record<string, unknown> }>")]
        next: Vec<transit_log::KindRecords>,
    },
    LogDocument {
        document: String,
        document_hash: String,
    },
    Receive(transit_log::Received),
    LogRevert {
        #[ts(type = "Array<{ kind: string, id: string, version: number, after?: unknown }>")]
        written: Vec<transit_log::Written>,
    },
}

pub fn dispatch(request: Request) -> Result<Response, cap_abi::Refusal> {
    match request {
        Request::SyncEdits { client, server, now_ms } => {
            let unknown_fields = transit_sync::unknown_fields(&client);
            let client = transit_sync::clamp_timestamps(&client, now_ms);
            let records = transit_sync::submit(transit_sync::merge(&client, &server), now_ms);
            let (document, document_hash) = canonical(&records)?;
            Ok(Response::SyncEdits { records, document, document_hash, unknown_fields })
        }
        Request::ReadEdits { document } => {
            let (read, dropped) = transit_sync::read_document(&document)?;
            let (canonical, hash) = canonical(&read.records)?;
            Ok(Response::ReadEdits {
                records: read.records,
                dropped,
                schema_version: transit_sync::SCHEMA_VERSION,
                document: canonical,
                document_hash: hash,
            })
        }
        Request::CompactEdits { records, now_ms, horizon_ms } => {
            let (records, dropped) = transit_sync::compact(&records, now_ms, horizon_ms);
            let (document, document_hash) = canonical(&records)?;
            Ok(Response::CompactEdits { records, document, document_hash, dropped })
        }
        Request::LogRecords { document } => {
            let doc = serde_json::to_value(&document).map_err(|e| e.to_string())?;
            Ok(Response::LogRecords { next: transit_log::document_records(&doc, "records", RECORD_KIND, transit_sync::KIND, transit_sync::SCHEMA_VERSION)? })
        }
        Request::LogDocument { records, document } => {
            let template = serde_json::json!({"kind": transit_sync::KIND, "schema_version": transit_sync::SCHEMA_VERSION});
            let doc: transit_sync::EditDocument = transit_log::assemble(template, document.as_ref(), "records", &records, true)?;
            Ok(Response::LogDocument { document: cap_abi::canonical_bytes(&doc)?, document_hash: cap_abi::canonical_hash(&doc)? })
        }
        Request::Receive { write, documents, ops } => receive(write, documents, ops),
        Request::LogRevert { states, now_ms } => {
            let record = |held: &Option<transit_log::Held>, id: &str| -> Result<Option<transit_sync::EditRecord>, String> {
                match held {
                    None => Ok(None),
                    Some(h) if h.version != transit_sync::SCHEMA_VERSION => {
                        Err(format!("record {id}: {}", transit_version::refused(transit_sync::KIND, h.version, transit_sync::SCHEMA_VERSION)))
                    }
                    Some(h) => serde_json::from_value(h.value.clone()).map(Some).map_err(|e| format!("record {id}: {e}")),
                }
            };
            let mut written = Vec::new();
            for s in states.iter().filter(|s| s.kind == RECORD_KIND) {
                let back = transit_sync::revert_record(record(&s.prior, &s.id)?.as_ref(), record(&s.now, &s.id)?.as_ref(), now_ms);
                let after = back.map(|r| serde_json::to_value(r).map_err(|e| e.to_string())).transpose()?;
                written.push(transit_log::Written { kind: RECORD_KIND.into(), id: s.id.clone(), version: transit_sync::SCHEMA_VERSION, after });
            }
            Ok(Response::LogRevert { written })
        }
    }
}

#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
enum Intent {
    Sync {
        client: serde_json::Value,
        #[serde(default)]
        from: Option<String>,
    },
    Compact {
        horizon_ms: u64,
    },
    Migrate {},
}

/// Brings an older document forward first. `read_document` refuses it.
fn read(doc: &serde_json::Value) -> Result<(Vec<transit_sync::EditRecord>, usize), cap_abi::Refusal> {
    let older = transit_sync::version::schema_version_of(doc)? < transit_sync::SCHEMA_VERSION;
    let doc = if older { transit_sync::version::migrate_to(doc.clone(), transit_sync::SCHEMA_VERSION)?.0 } else { doc.clone() };
    let (read, dropped) = transit_sync::read_document(&doc)?;
    Ok((read.records, dropped))
}

fn receive(write: transit_log::Write, documents: std::collections::BTreeMap<String, serde_json::Value>, ops: Vec<transit_log::OpCall>) -> Result<Response, cap_abi::Refusal> {
    let answer = |received| Ok(Response::Receive(received));
    if let Some(seq) = write.landed()? {
        return answer(transit_log::Received::landed(seq));
    }
    let now_ms = write.header.at_ms;
    let head = documents.get(transit_sync::KIND).filter(|v| !v.is_null());
    let mut records = match head {
        Some(doc) => read(doc)?.0,
        None => Vec::new(),
    };
    let head_hash = head.map(|_| canonical(&records)).transpose()?.map(|(_, h)| h);
    let mut wrote = false;
    let mut unknown_fields = Vec::new();
    for (i, call) in ops.iter().enumerate() {
        if call.cap != "sync" {
            return answer(transit_log::Received::failed(i, format!("{} {} is not a write to the disruptions log (sync: sync, compact, migrate)", call.cap, call.op)));
        }
        let intent = match call.request::<Intent>() {
            Ok(intent) => intent,
            Err(e) => return answer(transit_log::Received::failed(i, e)),
        };
        match intent {
            Intent::Sync { client, from } => {
                // An empty log has read nothing, so `from` may be empty.
                let empty_log = write.header.seq == 1;
                if let Some(from) = from {
                    if from != head_hash.clone().unwrap_or_default() && !(empty_log && from.is_empty()) {
                        return answer(transit_log::Received::moved(head_hash));
                    }
                }
                let client = match read(&client) {
                    Ok((client, _)) => client,
                    Err(e) => return answer(transit_log::Received::failed(i, e)),
                };
                unknown_fields.extend(transit_sync::unknown_fields(&client));
                let client = transit_sync::clamp_timestamps(&client, now_ms);
                records = transit_sync::submit(transit_sync::merge(&client, &records), now_ms);
            }
            Intent::Compact { horizon_ms } => {
                let keep = match (&write.references, write.now.as_ref().or(write.read.as_ref())) {
                    (Some(kinds), Some(now)) => transit_log::named(kinds, now, RECORD_KIND),
                    _ => Default::default(),
                };
                records = transit_sync::compact_keeping(&records, now_ms, horizon_ms, &keep).0
            }
            Intent::Migrate {} if records.is_empty() => continue,
            Intent::Migrate {} => {}
        }
        wrote = true;
    }
    let mut next = Vec::new();
    let mut heads = std::collections::BTreeMap::new();
    if wrote {
        let document = transit_sync::EditDocument::new(records.clone());
        let doc = serde_json::to_value(&document).map_err(|e| e.to_string())?;
        next = transit_log::document_records(&doc, "records", RECORD_KIND, transit_sync::KIND, transit_sync::SCHEMA_VERSION)?;
        heads.insert(transit_sync::KIND.to_string(), canonical(&records)?.1);
    }
    let mut received = transit_log::receive(&write, &next, &cap_abi::record_hash, &|e| cap_abi::canonical_bytes(e))?;
    received.heads = heads;
    unknown_fields.sort();
    unknown_fields.dedup();
    received.unknown_fields = unknown_fields;
    answer(received)
}

pub const RECORD_KIND: &str = "edit_record";

fn canonical(records: &[transit_sync::EditRecord]) -> Result<(String, String), String> {
    let document = transit_sync::EditDocument::new(records.to_vec());
    Ok((cap_abi::canonical_bytes(&document)?, cap_abi::canonical_hash(&document)?))
}

pub fn dispatch_json(text: &str) -> String {
    cap_abi::answer(text, dispatch)
}

pub fn dispatch_bytes(op: u32, _bytes: &[u8]) -> String {
    serde_json::json!({ "error": { "message": format!("unknown bulk op {op}: the edit document is JSON (sync_edits)") } })
        .to_string()
}

#[cfg(feature = "examples")]
pub mod examples;

cap_abi::capability! {
    id: "sync",
    json: dispatch_json,
    bytes: dispatch_bytes,
    documents: [],
    request: Request,
    speaks: &[("operator_edits", &[transit_sync::SCHEMA_VERSION])],
}
