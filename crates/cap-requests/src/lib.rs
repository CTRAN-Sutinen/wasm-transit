//! cap-requests — the stop requests capability.
//!
//! One question: **what work has been asked for at a stop, and where is
//! it?** The stop requests document (transit-requests; D-159) holds the
//! planning team's requests — a new stop, a reinstall, a relocation or
//! removal, a repair, a temporary stop — each a record by id tracking
//! the work order the agency's work order system opens for it.
//! `validate` answers what a write must pass: every refusal, naming its
//! request and problem, or the canonical document and its hash (the
//! module is the hash authority; a host stores exactly those bytes).
//! `requests_for` answers a stop's requests over a document the
//! kernel's `load_stop_requests` registered, and `list` the work across
//! every stop (by status and job, with every request counted by
//! status); `put` answers the next
//! document with one request placed by id, checked as a write is; and
//! `vocabulary` answers the lists a request's answers come from, and
//! `record_work_order` puts what the work order system answered on a
//! request (D-165). The
//! domain code lives in the shared `transit-requests` library; the
//! module writes nothing.

#![forbid(unsafe_code)]

use cap_abi::kernel::kind;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

pub use transit_requests::{RequestRefusal, StopRequest, StopRequests};

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "cap_requests/")]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Request {
    /// What a write must pass: every refusal (its request, its
    /// problem), or — when there is none — the canonical document and
    /// its hash.
    Validate {
        #[ts(as = "StopRequests")]
        requests: cap_abi::Base<StopRequests>,
    },
    /// A stop's requests, in id order.
    RequestsFor { requests_hash: String, stop: String },
    /// The work across every stop: every request in id order, only those
    /// at `statuses` and of `jobs` where given, with every request
    /// counted by status.
    List {
        requests_hash: String,
        #[serde(default)]
        #[ts(optional)]
        statuses: Option<Vec<String>>,
        #[serde(default)]
        #[ts(optional)]
        jobs: Option<Vec<String>>,
    },
    /// The document with `request` placed under `id` — new, or
    /// replacing the one held — then checked as a write is: every
    /// refusal, or the document registered and answered with its hash.
    /// A document absent (`requests_hash` omitted) is an empty one: the
    /// first request begins it.
    Put {
        #[serde(default)]
        #[ts(optional)]
        requests_hash: Option<String>,
        id: String,
        request: StopRequest,
        /// The write says the request is new (D-228): an id the document
        /// holds is refused (`id_taken`), and nothing is placed.
        #[serde(default)]
        #[ts(optional)]
        new: Option<bool>,
    },
    /// What to send the work order system for request `id` (D-228):
    /// `action` is `open` or `refresh`. The host makes the call, and
    /// `record_work_order` records the answer.
    WorkOrderCall { requests_hash: String, id: String, action: String },
    /// What the work order system answered, on request `id` (D-165): its
    /// reference on the request, a drafted or submitted one moved to
    /// ordered — then checked as a write is. An id the document does not
    /// hold is an error.
    RecordWorkOrder { requests_hash: String, id: String, work_order: transit_requests::WorkOrder },
    /// The lists a request's answers come from: its jobs, statuses, the
    /// sides of a street, the directions of travel, and the markings.
    Vocabulary {},
    /// The stop requests document as the stops log's records (D-197):
    /// one record of kind `stop_request` for each request.
    LogRecords { requests_hash: String },
    /// The stop requests document assembled from the stops log's
    /// `stop_request` records, registered and answered as its canonical
    /// bytes and hash. An older record comes forward (transit-requests'
    /// `version::record_forward`); a newer one is refused, naming both.
    LogDocument {
        #[ts(type = "Record<string, { version: number, value: unknown }>")]
        records: std::collections::BTreeMap<String, transit_log::Held>,
        /// The document's own record (absent: the empty document's).
        #[serde(default)]
        #[ts(optional, type = "{ version: number, value: unknown }")]
        document: Option<transit_log::Held>,
    },
}

/// One request and its id.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "cap_requests/")]
pub struct IdRequest {
    pub id: String,
    pub request: StopRequest,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "cap_requests/")]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Response {
    Validate {
        refusals: Vec<RequestRefusal>,
        document: Option<String>,
        document_hash: Option<String>,
        /// The fields the document carries that this build does not
        /// know (D-167): kept in it, and flagged for a person, never
        /// refused. Absent when every field is known.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        #[ts(optional, as = "Option<Vec<String>>")]
        unknown_fields: Vec<String>,
    },
    RequestsFor { requests: Vec<IdRequest> },
    List { requests: Vec<IdRequest>, counts: Vec<transit_requests::RequestCount> },
    Put { refusals: Vec<RequestRefusal>, requests_hash: Option<String>, requests: Option<StopRequests> },
    RecordWorkOrder { refusals: Vec<RequestRefusal>, requests_hash: Option<String>, requests: Option<StopRequests> },
    WorkOrderCall { call: transit_requests::WorkOrderCall },
    Vocabulary { jobs: Vec<String>, statuses: Vec<String>, open_statuses: Vec<String>, sides: Vec<String>, directions: Vec<String>, markings: Vec<String> },
    LogRecords {
        /// The records of each kind, as the log capability's `compose`
        /// takes them: the collection's records, and the document's own
        /// record (`document`), which keeps the fields outside the
        /// collection.
        #[ts(type = "Array<{ kind: string, version: number, records: Record<string, unknown> }>")]
        next: Vec<transit_log::KindRecords>,
    },
    LogDocument {
        /// The document's hash (`requests_hash`).
        document_hash: String,
        /// The document's canonical bytes, which a host serves under
        /// `document_hash`.
        document: String,
    },
}

fn strings(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| s.to_string()).collect()
}

/// The typed dispatch (native hosts call this directly). Failures
/// are `{"error": ...}`, never panics — a total seam.
pub fn dispatch(request: Request) -> Result<Response, String> {
    match request {
        Request::Validate { requests } => {
            // A received document: the base of a write (cap_abi::Base).
            Ok(match requests.0.checked() {
                Ok(requests) => {
                    let document = cap_abi::canonical_bytes(&requests)?;
                    let document_hash = cap_abi::content_hash(document.as_bytes());
                    Response::Validate { refusals: vec![], document: Some(document), document_hash: Some(document_hash), unknown_fields: cap_abi::request_unknown_fields() }
                }
                Err(refusals) => Response::Validate { refusals, document: None, document_hash: None, unknown_fields: cap_abi::request_unknown_fields() },
            })
        }
        Request::RequestsFor { requests_hash, stop } => {
            let held = kind::<transit_requests::StopRequests>().require(&requests_hash)?;
            Ok(Response::RequestsFor {
                requests: held.for_stop(&stop).into_iter().map(|(id, r)| IdRequest { id: id.to_string(), request: r.clone() }).collect(),
            })
        }
        Request::List { requests_hash, statuses, jobs } => {
            let held = kind::<transit_requests::StopRequests>().require(&requests_hash)?;
            Ok(Response::List {
                requests: held
                    .list(statuses.as_deref(), jobs.as_deref())
                    .into_iter()
                    .map(|(id, r)| IdRequest { id: id.to_string(), request: r.clone() })
                    .collect(),
                counts: held.counts(),
            })
        }
        Request::Put { requests_hash, id, request, new } => {
            let held = match requests_hash {
                Some(h) => (*kind::<transit_requests::StopRequests>().require_writable(&h)?).clone(),
                None => StopRequests::default(),
            };
            if let Some(taken) = new.unwrap_or(false).then(|| held.refuse_taken(&id)).flatten() {
                return Ok(Response::Put { refusals: vec![taken], requests_hash: None, requests: None });
            }
            let next = held.put(&id, request);
            Ok(match next.checked() {
                Ok(next) => {
                    let (requests_hash, held) = kind::<transit_requests::StopRequests>().register(next)?;
                    Response::Put { refusals: vec![], requests_hash: Some(requests_hash), requests: Some((*held).clone()) }
                }
                Err(refusals) => Response::Put { refusals, requests_hash: None, requests: None },
            })
        }
        Request::WorkOrderCall { requests_hash, id, action } => {
            let call = kind::<transit_requests::StopRequests>().require(&requests_hash)?.work_order_call(&id, &action)?;
            Ok(Response::WorkOrderCall { call })
        }
        Request::RecordWorkOrder { requests_hash, id, work_order } => {
            let next = kind::<transit_requests::StopRequests>().require_writable(&requests_hash)?.record_work_order(&id, work_order).ok_or_else(|| format!("no request {id:?} in the document"))?;
            Ok(match next.checked() {
                Ok(next) => {
                    let (requests_hash, held) = kind::<transit_requests::StopRequests>().register(next)?;
                    Response::RecordWorkOrder { refusals: vec![], requests_hash: Some(requests_hash), requests: Some((*held).clone()) }
                }
                Err(refusals) => Response::RecordWorkOrder { refusals, requests_hash: None, requests: None },
            })
        }
        Request::Vocabulary {} => Ok(Response::Vocabulary {
            jobs: strings(&transit_requests::JOBS),
            statuses: strings(&transit_requests::STATUSES),
            open_statuses: strings(&transit_requests::OPEN_STATUSES),
            sides: strings(&transit_requests::SIDES),
            directions: strings(&transit_requests::DIRECTIONS),
            markings: strings(&transit_requests::MARKINGS),
        }),
        Request::LogRecords { requests_hash } => {
            let held = kind::<transit_requests::StopRequests>().require(&requests_hash)?;
            let doc = serde_json::to_value(&*held).map_err(|e| e.to_string())?;
            Ok(Response::LogRecords { next: transit_log::document_records(&doc, "requests", "stop_request", "stop_requests", transit_requests::SCHEMA_VERSION)? })
        }
        Request::LogDocument { records, document } => {
            // The stops log keeps each record in the version it was
            // written in: bring each one forward.
            let version = transit_requests::SCHEMA_VERSION;
            let forward = |kind: &str, held: &transit_log::Held| -> Result<transit_log::Held, String> {
                Ok(transit_log::Held { version, value: transit_requests::version::record_forward(kind, held.value.clone(), held.version)? })
            };
            let records = records.iter().map(|(id, held)| Ok((id.clone(), forward("stop_request", held)?))).collect::<Result<_, String>>()?;
            let document = document.as_ref().map(|held| forward("stop_requests", held)).transpose()?;
            let template = serde_json::json!({"kind": "stop_requests", "schema_version": version});
            let requests: StopRequests = transit_log::assemble(template, document.as_ref(), "requests", &records, false)?;
            let document = cap_abi::canonical_bytes(&requests)?;
            let (document_hash, _) = kind::<transit_requests::StopRequests>().register(requests)?;
            Ok(Response::LogDocument { document_hash, document })
        }
    }
}

/// The JSON seam: `{"op": ...}` in, `{"ok": ...}` / `{"error": ...}`
/// out — byte-compatible with every other capability module.
pub fn dispatch_json(text: &str) -> String {
    cap_abi::answer(text, dispatch)
}

/// The requests take their questions inline; a document registers on
/// the kernel's `load_stop_requests` (bulk op 271).
pub fn dispatch_bytes(op: u32, _bytes: &[u8]) -> String {
    serde_json::json!({ "error": format!("unknown bulk op {op}: a stop requests document registers on the kernel's load_stop_requests") }).to_string()
}

/// The worked examples, which the generator runs (no module build turns
/// the feature on).
#[cfg(feature = "examples")]
pub mod examples;

cap_abi::capability! {
    id: "requests",
    json: dispatch_json,
    bytes: dispatch_bytes,
    documents: [transit_requests::StopRequests],
    request: Request,
    speaks: &[("stop_requests", &[transit_requests::SCHEMA_VERSION])],
}
