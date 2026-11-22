//! cap-notes: the notes capability.
//!
//! One question: **what has the agency noted about this?** The notes
//! document (transit-notes; D-214) holds the agency's internal notes. A
//! note names a trip instance, an edit record, a service alert, or a
//! stop. Anyone reads the notes. They never go to riders.
//!
//! - `validate` answers what a write must pass: every refusal, naming
//!   its note and problem, or the canonical document and its hash. The
//!   module is the hash authority; a host stores exactly those bytes.
//! - `write` answers the next document with one note written or edited,
//!   stamped with the writer and the time that the host gives.
//! - `on` answers a target's notes, oldest first.
//! - `between` answers the notes written in a time span, newest first.
//! - `log_records` and `log_document` turn the document into the notes
//!   log's records and back.
//!
//! The domain code lives in the shared `transit-notes` library. The
//! module writes nothing.

#![forbid(unsafe_code)]

use cap_abi::kernel::NOTES;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

pub use transit_notes::{Note, NoteRefusal, NoteRow, NoteTarget, Notes};

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "cap_notes/")]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Request {
    /// What a write must pass: every refusal, or, when there is none,
    /// the canonical document and its hash.
    Validate {
        #[ts(as = "Notes")]
        notes: cap_abi::Base<Notes>,
    },
    /// The document with note `id` written by `by` at `at` (Unix
    /// seconds): a new note, or an edit of the held one's text. The host
    /// gives the verified writer and its clock. A document absent
    /// (`notes_hash` omitted) is an empty one.
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
    /// The notes about `target`, oldest first. A document absent is an
    /// empty one.
    On {
        #[serde(default)]
        #[ts(optional)]
        notes_hash: Option<String>,
        target: NoteTarget,
    },
    /// The notes written in `[from, to)` (Unix seconds), newest first. A
    /// document absent is an empty one.
    Between {
        #[serde(default)]
        #[ts(optional)]
        notes_hash: Option<String>,
        #[ts(type = "number")]
        from: u64,
        #[ts(type = "number")]
        to: u64,
    },
    /// The document as the notes log's records (D-197): a `note` record
    /// for each note, and the document's own record. A document left out
    /// gives no records.
    LogRecords {
        #[serde(default)]
        #[ts(optional)]
        notes_hash: Option<String>,
    },
    /// A write to the notes log, whole (D-228): each op of the write's
    /// intent over the document at the head (`heads.notes`, registered
    /// by the host; absent: the empty document), each note stamped with
    /// the writer and the clock of the entry's header, then the entry
    /// that records what changed. The ops are `write` (`{cap: "notes",
    /// op: "write", args: {id, target, text}}`). A refused op answers its
    /// refusals, and an op that does not run answers why (`failed`): no
    /// entry either way.
    Receive {
        #[ts(type = "{ checkpoint?: unknown, header: unknown, unread?: Array<{ seq: number, text: string }> }")]
        write: transit_log::Write,
        #[serde(default)]
        #[ts(type = "Record<string, string>")]
        heads: std::collections::BTreeMap<String, String>,
        #[ts(type = "Array<{ cap: string, op: string, args?: unknown }>")]
        ops: Vec<transit_log::OpCall>,
    },
    /// The notes document assembled from the notes log's records,
    /// registered and answered as its canonical bytes and hash. A record
    /// in another version is refused, naming both.
    LogDocument {
        #[ts(type = "Record<string, { version: number, value: unknown }>")]
        records: std::collections::BTreeMap<String, transit_log::Held>,
        /// The document's own record (absent: the empty document's).
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
        /// The fields the document carries that this build does not
        /// know (D-167): kept in it, and flagged for a person, never
        /// refused. Absent when every field is known.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        #[ts(optional, as = "Option<Vec<String>>")]
        unknown_fields: Vec<String>,
    },
    Write { refusals: Vec<NoteRefusal>, notes_hash: Option<String>, notes: Option<Notes> },
    On { notes: Vec<NoteRow> },
    Between { notes: Vec<NoteRow> },
    LogRecords {
        /// The records of each kind, as the log capability's `compose`
        /// takes them.
        #[ts(type = "Array<{ kind: string, version: number, records: Record<string, unknown> }>")]
        next: Vec<transit_log::KindRecords>,
    },
    /// What the write adds to the notes log (`heads.notes`: the
    /// document after it).
    Receive(transit_log::Received),
    LogDocument {
        /// The document's hash (`notes_hash`).
        document_hash: String,
        /// The document's canonical bytes, which a host serves under
        /// `document_hash`.
        document: String,
    },
}

fn held(hash: Option<&str>) -> Result<std::sync::Arc<Notes>, String> {
    match hash {
        Some(h) => NOTES.require(h),
        None => Ok(std::sync::Arc::new(Notes::default())),
    }
}

/// The typed dispatch (native hosts call this directly). Failures are
/// `{"error": ...}`, never panics: a total seam.
pub fn dispatch(request: Request) -> Result<Response, String> {
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
                Some(h) => NOTES.require_writable(&h)?,
                None => std::sync::Arc::new(Notes::default()),
            };
            Ok(match base.write(&id, target, &text, &by, at) {
                Ok(next) => {
                    let (hash, held) = NOTES.register(next)?;
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
                let doc = serde_json::to_value(&*NOTES.require(&h)?).map_err(|e| e.to_string())?;
                next.extend(transit_log::document_records(&doc, "notes", "note", "notes", transit_notes::SCHEMA_VERSION)?);
            }
            Ok(Response::LogRecords { next })
        }
        Request::Receive { write, heads, ops } => receive(write, heads, ops),
        Request::LogDocument { records, document } => {
            let template = serde_json::json!({"kind": "notes", "schema_version": transit_notes::SCHEMA_VERSION});
            let notes: Notes = transit_log::assemble(template, document.as_ref(), "notes", &records, false)?;
            let bytes = cap_abi::canonical_bytes(&notes)?;
            let (document_hash, _) = NOTES.register(notes)?;
            Ok(Response::LogDocument { document_hash, document: bytes })
        }
    }
}

/// A write's ops over the head, then its entry (`Request::Receive`).
/// An op of a write to the notes log, as a page sends it: never who
/// wrote it or when, which the entry's header gives.
#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
enum Intent {
    Write { id: String, target: NoteTarget, text: String },
}

fn receive(write: transit_log::Write, heads: std::collections::BTreeMap<String, String>, ops: Vec<transit_log::OpCall>) -> Result<Response, String> {
    let (by, at) = write.stamp();
    if let Some(seq) = write.landed()? {
        return Ok(Response::Receive(transit_log::Received::landed(seq)));
    }
    let mut hash = heads.get("notes").cloned();
    let refused = |op, refusals: Vec<NoteRefusal>| -> Result<Response, String> {
        Ok(Response::Receive(transit_log::Received::refused(op, serde_json::to_value(refusals).map_err(|e| e.to_string())?)))
    };
    let failed = |op, error| Ok(Response::Receive(transit_log::Received::failed(op, error)));
    for (i, call) in ops.iter().enumerate() {
        if call.cap != "notes" {
            return failed(i, format!("{} {} is not a write to the notes log (notes: write)", call.cap, call.op));
        }
        let Intent::Write { id, target, text } = match call.request::<Intent>() {
            Ok(intent) => intent,
            Err(e) => return failed(i, e),
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

/// The JSON seam: `{"op": ...}` in, `{"ok": ...}` / `{"error": ...}`
/// out, byte-compatible with every other capability module.
pub fn dispatch_json(text: &str) -> String {
    cap_abi::answer(text, dispatch)
}

/// The notes take their questions inline; a document registers on the
/// kernel's `load_notes` (bulk op 274).
pub fn dispatch_bytes(op: u32, _bytes: &[u8]) -> String {
    serde_json::json!({ "error": format!("unknown bulk op {op}: a notes document registers on the kernel's load_notes") }).to_string()
}

/// The worked examples, which the generator runs (no module build turns
/// the feature on).
#[cfg(feature = "examples")]
pub mod examples;

cap_abi::capability! {
    id: "notes",
    json: dispatch_json,
    bytes: dispatch_bytes,
    documents: [NOTES],
}
