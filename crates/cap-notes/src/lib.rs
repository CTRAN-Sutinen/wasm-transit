#![forbid(unsafe_code)]

use cap_abi::kernel::kind;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

pub use transit_notes::{Note, NoteRefusal, NoteRow, NoteTarget, Notes};

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "cap_notes/")]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Request {
    Validate {
        #[ts(as = "Notes")]
        notes: cap_abi::Base<Notes>,
    },
    Write {
        #[serde(default)]
        #[ts(optional)]
        notes_hash: Option<String>,
        id: String,
        target: NoteTarget,
        text: String,
        by: String,
        #[ts(type = "number")]
        at: u64,
    },
    On {
        #[serde(default)]
        #[ts(optional)]
        notes_hash: Option<String>,
        target: NoteTarget,
    },
    Between {
        #[serde(default)]
        #[ts(optional)]
        notes_hash: Option<String>,
        #[ts(type = "number")]
        from: u64,
        #[ts(type = "number")]
        to: u64,
    },
    LogRecords {
        #[serde(default)]
        #[ts(optional)]
        notes_hash: Option<String>,
    },
    Receive {
        #[ts(type = "{ checkpoint?: unknown, header: unknown, unread?: Array<{ seq: number, text: string }> }")]
        write: transit_log::Write,
        #[serde(default)]
        #[ts(type = "Record<string, string>")]
        heads: std::collections::BTreeMap<String, String>,
        #[ts(type = "Array<{ cap: string, op: string, args?: unknown }>")]
        ops: Vec<transit_log::OpCall>,
    },
    LogDocument {
        #[ts(type = "Record<string, { version: number, value: unknown }>")]
        records: std::collections::BTreeMap<String, transit_log::Held>,
        #[serde(default)]
        #[ts(optional, type = "{ version: number, value: unknown }")]
        document: Option<transit_log::Held>,
    },
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "cap_notes/")]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Response {
    Validate {
        refusals: Vec<NoteRefusal>,
        document: Option<String>,
        document_hash: Option<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        #[ts(optional, as = "Option<Vec<String>>")]
        unknown_fields: Vec<String>,
    },
    Write { refusals: Vec<NoteRefusal>, notes_hash: Option<String>, notes: Option<Notes> },
    On { notes: Vec<NoteRow> },
    Between { notes: Vec<NoteRow> },
    LogRecords {
        #[ts(type = "Array<{ kind: string, version: number, records: Record<string, unknown> }>")]
        next: Vec<transit_log::KindRecords>,
    },
    Receive(transit_log::Received),
    LogDocument {
        document_hash: String,
        document: String,
    },
}

fn held(hash: Option<&str>) -> Result<std::sync::Arc<Notes>, cap_abi::Refusal> {
    match hash {
        Some(h) => kind::<transit_notes::Notes>().require(h),
        None => Ok(std::sync::Arc::new(Notes::default())),
    }
}

pub fn dispatch(request: Request) -> Result<Response, cap_abi::Refusal> {
    match request {
        Request::Validate { notes } => Ok(match notes.0.checked() {
            Ok(notes) => {
                let document = cap_abi::canonical_bytes(&notes)?;
                let document_hash = cap_abi::content_hash(document.as_bytes());
                Response::Validate { refusals: vec![], document: Some(document), document_hash: Some(document_hash), unknown_fields: cap_abi::request_unknown_fields() }
            }
            Err(refusals) => Response::Validate { refusals, document: None, document_hash: None, unknown_fields: cap_abi::request_unknown_fields() },
        }),
        Request::Write { notes_hash, id, target, text, by, at } => {
            let base = match notes_hash {
                Some(h) => kind::<transit_notes::Notes>().require_writable(&h)?,
                None => std::sync::Arc::new(Notes::default()),
            };
            Ok(match base.write(&id, target, &text, &by, at) {
                Ok(next) => {
                    let (hash, held) = kind::<transit_notes::Notes>().register(next)?;
                    Response::Write { refusals: vec![], notes_hash: Some(hash), notes: Some((*held).clone()) }
                }
                Err(refusals) => Response::Write { refusals, notes_hash: None, notes: None },
            })
        }
        Request::On { notes_hash, target } => Ok(Response::On { notes: held(notes_hash.as_deref())?.on(&target) }),
        Request::Between { notes_hash, from, to } => Ok(Response::Between { notes: held(notes_hash.as_deref())?.between(from, to) }),
        Request::LogRecords { notes_hash } => {
            let mut next = Vec::new();
            if let Some(h) = notes_hash {
                let doc = serde_json::to_value(&*kind::<transit_notes::Notes>().require(&h)?).map_err(|e| e.to_string())?;
                next.extend(transit_log::document_records(&doc, "notes", "note", "notes", transit_notes::SCHEMA_VERSION)?);
            }
            Ok(Response::LogRecords { next })
        }
        Request::Receive { write, heads, ops } => receive(write, heads, ops),
        Request::LogDocument { records, document } => {
            let template = serde_json::json!({"kind": "notes", "schema_version": transit_notes::SCHEMA_VERSION});
            let notes: Notes = transit_log::assemble(template, document.as_ref(), "notes", &records, false)?;
            let bytes = cap_abi::canonical_bytes(&notes)?;
            let (document_hash, _) = kind::<transit_notes::Notes>().register(notes)?;
            Ok(Response::LogDocument { document_hash, document: bytes })
        }
    }
}

/// An op names neither who wrote it nor when. The entry's header gives both.
#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
enum Intent {
    Write { id: String, target: NoteTarget, text: String },
}

fn receive(write: transit_log::Write, heads: std::collections::BTreeMap<String, String>, ops: Vec<transit_log::OpCall>) -> Result<Response, cap_abi::Refusal> {
    let (by, at) = write.stamp();
    if let Some(seq) = write.landed()? {
        return Ok(Response::Receive(transit_log::Received::landed(seq)));
    }
    let mut hash = heads.get("notes").cloned();
    let refused = |op, refusals: Vec<NoteRefusal>| -> Result<Response, cap_abi::Refusal> {
        Ok(Response::Receive(transit_log::Received::refused(op, serde_json::to_value(refusals).map_err(|e| e.to_string())?)))
    };
    let failed = |op, error: cap_abi::Refusal| Ok(Response::Receive(transit_log::Received::failed(op, error)));
    for (i, call) in ops.iter().enumerate() {
        if call.cap != "notes" {
            return failed(i, format!("{} {} is not a write to the notes log (notes: write)", call.cap, call.op).into());
        }
        let Intent::Write { id, target, text } = match call.request::<Intent>() {
            Ok(intent) => intent,
            Err(e) => return failed(i, e.into()),
        };
        let written = match dispatch(Request::Write { notes_hash: hash.clone(), id, target, text, by: by.to_string(), at }) {
            Ok(written) => written,
            Err(e) => return failed(i, e),
        };
        match written {
            Response::Write { notes_hash: Some(next), .. } => hash = Some(next),
            Response::Write { refusals, .. } => return refused(i, refusals),
            _ => unreachable!("write answers write"),
        }
    }
    let Response::LogRecords { next } = dispatch(Request::LogRecords { notes_hash: hash.clone() })? else { unreachable!("log_records answers log_records") };
    let mut received = transit_log::receive(&write, &next, &cap_abi::record_hash, &|e| cap_abi::canonical_bytes(e))?;
    received.heads.extend(hash.map(|h| ("notes".to_string(), h)));
    Ok(Response::Receive(received))
}

pub fn dispatch_json(text: &str) -> String {
    cap_abi::answer(text, dispatch)
}

pub fn dispatch_bytes(op: u32, _bytes: &[u8]) -> String {
    serde_json::json!({ "error": { "message": format!("unknown bulk op {op}: a notes document registers on the kernel's load_notes") } }).to_string()
}

#[cfg(feature = "examples")]
pub mod examples;

cap_abi::capability! {
    id: "notes",
    json: dispatch_json,
    bytes: dispatch_bytes,
    documents: [transit_notes::Notes],
    request: Request,
    speaks: &[("notes", &[transit_notes::SCHEMA_VERSION])],
}
