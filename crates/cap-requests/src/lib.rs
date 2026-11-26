//! cap-requests wraps transit-requests. The stops log holds the
//! requests, and cap-stops' `receive` writes them.

#![forbid(unsafe_code)]

use cap_abi::kernel::kind;
use serde::{Deserialize, Serialize};
#[cfg(feature = "bindings")]
use ts_rs::TS;

pub use transit_requests::{RequestRefusal, StopRequest, StopRequests};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS))]
#[cfg_attr(feature = "bindings", ts(export, export_to = "cap_requests/"))]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Request {
    Validate {
        #[cfg_attr(feature = "bindings", ts(as = "StopRequests"))]
        requests: cap_abi::Base<StopRequests>,
    },
    RequestsFor { requests_hash: String, stop: String },
    List {
        requests_hash: String,
        #[serde(default)]
        #[cfg_attr(feature = "bindings", ts(optional))]
        statuses: Option<Vec<String>>,
        #[serde(default)]
        #[cfg_attr(feature = "bindings", ts(optional))]
        jobs: Option<Vec<String>>,
    },
    Put {
        #[serde(default)]
        #[cfg_attr(feature = "bindings", ts(optional))]
        requests_hash: Option<String>,
        id: String,
        request: StopRequest,
        #[serde(default)]
        #[cfg_attr(feature = "bindings", ts(optional))]
        new: Option<bool>,
    },
    WorkOrderCall { requests_hash: String, id: String, action: String },
    RecordWorkOrder { requests_hash: String, id: String, work_order: transit_requests::WorkOrder },
    Vocabulary {},
    LogRecords { requests_hash: String },
    LogDocument {
        #[cfg_attr(feature = "bindings", ts(type = "Record<string, { version: number, value: unknown }>"))]
        records: std::collections::BTreeMap<String, transit_log::Held>,
        #[serde(default)]
        #[cfg_attr(feature = "bindings", ts(optional, type = "{ version: number, value: unknown }"))]
        document: Option<transit_log::Held>,
    },
}
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "bindings", derive(TS))]
#[cfg_attr(feature = "bindings", ts(export, export_to = "cap_requests/"))]
pub struct IdRequest {
    pub id: String,
    pub request: StopRequest,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "bindings", derive(TS))]
#[cfg_attr(feature = "bindings", ts(export, export_to = "cap_requests/"))]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Response {
    Validate {
        refusals: Vec<RequestRefusal>,
        document: Option<String>,
        document_hash: Option<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        #[cfg_attr(feature = "bindings", ts(optional, as = "Option<Vec<String>>"))]
        unknown_fields: Vec<String>,
    },
    RequestsFor { requests: Vec<IdRequest> },
    List { requests: Vec<IdRequest>, counts: Vec<transit_requests::RequestCount> },
    Put { refusals: Vec<RequestRefusal>, requests_hash: Option<String>, requests: Option<StopRequests> },
    RecordWorkOrder { refusals: Vec<RequestRefusal>, requests_hash: Option<String>, requests: Option<StopRequests> },
    WorkOrderCall { call: transit_requests::WorkOrderCall },
    Vocabulary { jobs: Vec<String>, statuses: Vec<String>, open_statuses: Vec<String>, sides: Vec<String>, directions: Vec<String>, markings: Vec<String> },
    LogRecords {
        #[cfg_attr(feature = "bindings", ts(type = "Array<{ kind: string, version: number, records: Record<string, unknown> }>"))]
        next: Vec<transit_log::KindRecords>,
    },
    LogDocument {
        document_hash: String,
        document: String,
    },
}

fn strings(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| s.to_string()).collect()
}

pub fn dispatch(request: Request) -> Result<Response, cap_abi::Refusal> {
    match request {
        Request::Validate { requests } => {
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

pub fn dispatch_json(text: &str) -> String {
    cap_abi::answer(text, dispatch)
}

/// A document registers on the kernel's `load_stop_requests` (271), so this
/// crate has no bytes ops.
pub fn dispatch_bytes(op: u32, _bytes: &[u8]) -> String {
    serde_json::json!({ "error": { "message": format!("unknown bulk op {op}: a stop requests document registers on the kernel's load_stop_requests") } }).to_string()
}

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
