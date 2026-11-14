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

use cap_abi::kernel::STOP_REQUESTS;
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
    Validate { requests: StopRequests },
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
    },
    /// What the work order system answered, on request `id` (D-165): its
    /// reference on the request, a drafted or submitted one moved to
    /// ordered — then checked as a write is. An id the document does not
    /// hold is an error.
    RecordWorkOrder { requests_hash: String, id: String, work_order: transit_requests::WorkOrder },
    /// The lists a request's answers come from: its jobs, statuses, the
    /// sides of a street, the directions of travel, and the markings.
    Vocabulary {},
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
    Validate { refusals: Vec<RequestRefusal>, document: Option<String>, document_hash: Option<String> },
    RequestsFor { requests: Vec<IdRequest> },
    List { requests: Vec<IdRequest>, counts: Vec<transit_requests::RequestCount> },
    Put { refusals: Vec<RequestRefusal>, requests_hash: Option<String>, requests: Option<StopRequests> },
    RecordWorkOrder { refusals: Vec<RequestRefusal>, requests_hash: Option<String>, requests: Option<StopRequests> },
    Vocabulary { jobs: Vec<String>, statuses: Vec<String>, sides: Vec<String>, directions: Vec<String>, markings: Vec<String> },
}

fn strings(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| s.to_string()).collect()
}

/// The typed dispatch (native hosts call this directly). Failures
/// are `{"error": ...}`, never panics — a total seam.
pub fn dispatch(request: Request) -> Result<Response, String> {
    match request {
        Request::Validate { requests } => {
            use transit_version::Versioned;
            cap_abi::speaks(StopRequests::KIND, requests.schema_version(), StopRequests::READS)?;
            Ok(match requests.checked() {
                Ok(requests) => {
                    let document = serde_json::to_string(&requests).map_err(|e| e.to_string())?;
                    let document_hash = cap_abi::content_hash(document.as_bytes());
                    Response::Validate { refusals: vec![], document: Some(document), document_hash: Some(document_hash) }
                }
                Err(refusals) => Response::Validate { refusals, document: None, document_hash: None },
            })
        }
        Request::RequestsFor { requests_hash, stop } => {
            let held = STOP_REQUESTS.require(&requests_hash)?;
            Ok(Response::RequestsFor {
                requests: held.for_stop(&stop).into_iter().map(|(id, r)| IdRequest { id: id.to_string(), request: r.clone() }).collect(),
            })
        }
        Request::List { requests_hash, statuses, jobs } => {
            let held = STOP_REQUESTS.require(&requests_hash)?;
            Ok(Response::List {
                requests: held
                    .list(statuses.as_deref(), jobs.as_deref())
                    .into_iter()
                    .map(|(id, r)| IdRequest { id: id.to_string(), request: r.clone() })
                    .collect(),
                counts: held.counts(),
            })
        }
        Request::Put { requests_hash, id, request } => {
            let next = match requests_hash {
                Some(h) => STOP_REQUESTS.require(&h)?.put(&id, request),
                None => StopRequests::default().put(&id, request),
            };
            Ok(match next.checked() {
                Ok(next) => {
                    let (requests_hash, held) = STOP_REQUESTS.register(next)?;
                    Response::Put { refusals: vec![], requests_hash: Some(requests_hash), requests: Some((*held).clone()) }
                }
                Err(refusals) => Response::Put { refusals, requests_hash: None, requests: None },
            })
        }
        Request::RecordWorkOrder { requests_hash, id, work_order } => {
            let next = STOP_REQUESTS.require(&requests_hash)?.record_work_order(&id, work_order).ok_or_else(|| format!("no request {id:?} in the document"))?;
            Ok(match next.checked() {
                Ok(next) => {
                    let (requests_hash, held) = STOP_REQUESTS.register(next)?;
                    Response::RecordWorkOrder { refusals: vec![], requests_hash: Some(requests_hash), requests: Some((*held).clone()) }
                }
                Err(refusals) => Response::RecordWorkOrder { refusals, requests_hash: None, requests: None },
            })
        }
        Request::Vocabulary {} => Ok(Response::Vocabulary {
            jobs: strings(&transit_requests::JOBS),
            statuses: strings(&transit_requests::STATUSES),
            sides: strings(&transit_requests::SIDES),
            directions: strings(&transit_requests::DIRECTIONS),
            markings: strings(&transit_requests::MARKINGS),
        }),
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

cap_abi::capability! {
    id: "requests",
    json: dispatch_json,
    bytes: dispatch_bytes,
    documents: [STOP_REQUESTS],
}
