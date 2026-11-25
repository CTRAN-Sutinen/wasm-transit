//! A write's base is a `cap_abi::Base`, which refuses a copy read ahead
//! read-only.

#![forbid(unsafe_code)]

use std::sync::Arc;

use cap_abi::kernel::kind;
use cap_abi::{Base, Current};
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use transit_dispatch::{CancelRow, DiversionDraft, DiversionRow, DraftView};
use transit_network::Network;
use transit_sync::EditDocument;

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "cap_divert/")]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Request {
    Draft { network_hash: String, draft: DiversionDraft },
    List {
        network_hash: String,
        #[ts(as = "EditDocument")]
        edits: Current<EditDocument>,
    },
    Reports {
        network_hash: String,
        #[ts(as = "EditDocument")]
        edits: Current<EditDocument>,
    },
    Propose {
        network_hash: String,
        draft: DiversionDraft,
        #[ts(as = "EditDocument")]
        edits: Base<EditDocument>,
        record_id: String,
        #[ts(type = "number")]
        now_ms: u64,
    },
    Cancel {
        network_hash: String,
        #[ts(as = "EditDocument")]
        edits: Base<EditDocument>,
        record_id: String,
        trip_id: String,
        #[serde(default)]
        start_secs: Option<u32>,
        date: String,
        #[ts(type = "number")]
        now_ms: u64,
    },
    Retire {
        #[ts(as = "EditDocument")]
        edits: Base<EditDocument>,
        record_id: String,
        #[ts(type = "number")]
        now_ms: u64,
    },
    Restore {
        #[ts(as = "EditDocument")]
        edits: Base<EditDocument>,
        record_id: String,
        #[ts(type = "number")]
        now_ms: u64,
    },
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "cap_divert/")]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Response {
    Draft { view: DraftView },
    List { rows: Vec<DiversionRow>, cancels: Vec<CancelRow> },
    Reports { reports: Vec<transit_sync::EditReport> },
    Propose { edits: EditDocument },
    Cancel { edits: EditDocument },
    Retire { edits: EditDocument },
    Restore { edits: EditDocument },
}

fn network_arc(network_hash: &str) -> Result<Arc<Network>, cap_abi::Refusal> {
    kind::<transit_network::Network>().require(network_hash)
}

pub fn dispatch(request: Request) -> Result<Response, cap_abi::Refusal> {
    let next = |records| EditDocument::new(records);
    match request {
        Request::Draft { network_hash, draft } => {
            let network = network_arc(&network_hash)?;
            Ok(Response::Draft { view: transit_dispatch::draft_view(&network, &draft) })
        }
        Request::List { network_hash, edits } => {
            let network = network_arc(&network_hash)?;
            Ok(Response::List {
                rows: transit_dispatch::diversion_rows(&network, &edits.records),
                cancels: transit_dispatch::cancel_rows(&network, &edits.records),
            })
        }
        Request::Reports { network_hash, edits } => {
            let network = network_arc(&network_hash)?;
            Ok(Response::Reports { reports: transit_dispatch::report_against_network(&network, &edits.records) })
        }
        Request::Propose { network_hash, draft, edits, record_id, now_ms } => {
            let network = network_arc(&network_hash)?;
            // The domain id is stable across re-proposals of one record.
            let id = edits
                .records
                .iter()
                .find(|r| r.id == record_id)
                .and_then(|r| r.diversion.as_ref())
                .map(|d| d.id.clone())
                .unwrap_or_else(|| format!("div-{}-{now_ms}", draft.route_id));
            let diversion = transit_dispatch::diversion_of(&network, &draft, &id)?;
            Ok(Response::Propose { edits: next(transit_sync::propose(&edits.records, &record_id, diversion, now_ms)) })
        }
        Request::Cancel { network_hash, edits, record_id, trip_id, start_secs, date, now_ms } => {
            let network = network_arc(&network_hash)?;
            // The domain id is stable across re-proposals of one record.
            let id = edits
                .records
                .iter()
                .find(|r| r.id == record_id)
                .and_then(|r| r.cancel.as_ref())
                .map(|c| c.id.clone())
                .unwrap_or_else(|| format!("cancel-{trip_id}-{now_ms}"));
            let cancel = transit_dispatch::cancel_of(&network, &id, &trip_id, start_secs, &date)?;
            Ok(Response::Cancel { edits: next(transit_sync::propose_cancel(&edits.records, &record_id, cancel, now_ms)) })
        }
        Request::Retire { edits, record_id, now_ms } => {
            Ok(Response::Retire { edits: next(transit_sync::set_retired(&edits.records, &record_id, true, now_ms)?) })
        }
        Request::Restore { edits, record_id, now_ms } => {
            Ok(Response::Restore { edits: next(transit_sync::set_retired(&edits.records, &record_id, false, now_ms)?) })
        }
    }
}

pub fn dispatch_json(text: &str) -> String {
    cap_abi::answer(text, dispatch)
}

pub fn dispatch_bytes(op: u32, _bytes: &[u8]) -> String {
    cap_abi::outcome::<(), String>(Err(format!("unknown bulk op {op}: divert has no bulk op of its own")))
}

#[cfg(feature = "examples")]
pub mod examples;

cap_abi::capability! {
    id: "divert",
    json: dispatch_json,
    bytes: dispatch_bytes,
    documents: [transit_network::Network],
    request: Request,
    consumes: &[("network", "network")],
    speaks: &[
        ("network", &[transit_network::SCHEMA_VERSION]),
        ("operator_edits", &[transit_sync::SCHEMA_VERSION]),
    ],
}
