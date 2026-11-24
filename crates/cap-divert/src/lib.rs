//! cap-divert: what does this operator edit amount to over the network?
//! (OPS.md has the ops.)
//!
//! An operator edit is a diversion or a whole-trip cancel (D-45, D-102).
//! The seam over transit-dispatch (the draft over the network) and
//! transit-sync (the record envelope).
//!
//! * A record the network cannot answer is reported, never rejected, and
//!   moves nothing in a day view.
//! * Every op takes the edit document whole and answers the next one
//!   tagged (D-115), so a host persists or posts it as given. Another
//!   version is refused naming both (`cap_abi::Current`); a write's base
//!   is a `cap_abi::Base`, which refuses a copy read ahead read-only
//!   (D-186).
//! * The host owns the clock and ids (`now_ms`, `record_id`).
//! * Converging two edit documents is cap-sync's question, which the
//!   server hosts without a network.

#![forbid(unsafe_code)]

use std::sync::Arc;

use cap_abi::kernel::kind;
use cap_abi::{Base, Current};
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use transit_dispatch::{CancelRow, DiversionDraft, DiversionRow, DraftView};
use transit_network::Network;
use transit_sync::EditDocument;

/// One operation call. Tagged like every capability's seam.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "cap_divert/")]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Request {
    /// What the editor shows about a draft (total: problems, never an
    /// error, for a draft that names what the network lacks).
    Draft { network_hash: String, draft: DiversionDraft },
    /// The edit document's rows, with names resolved over the network. The
    /// list shows the diversions and the trip cancels, each in document
    /// order.
    List {
        network_hash: String,
        #[ts(as = "EditDocument")]
        edits: Current<EditDocument>,
    },
    /// What each record amounts to on the network, in document order:
    /// whether it is valid (its route, pattern, and stops exist and are in
    /// order), what is wrong when it is not, and how many trip instances it
    /// touches. Invalid records are reported, never rejected (the sync
    /// contract). Every day view simply finds nothing to patch for them.
    Reports {
        network_hash: String,
        #[ts(as = "EditDocument")]
        edits: Current<EditDocument>,
    },
    /// The edit document with the draft proposed as `record_id` (a
    /// new id appends at rev 1; an existing one is replaced, rev
    /// bumped). Errors name what keeps the draft from being a record.
    Propose {
        network_hash: String,
        draft: DiversionDraft,
        #[ts(as = "EditDocument")]
        edits: Base<EditDocument>,
        record_id: String,
        #[ts(type = "number")]
        now_ms: u64,
    },
    /// The edit document with trip `trip_id` cancelled on `date` as
    /// `record_id` (D-45): one instance (`start_secs`) or, without it,
    /// every instance the trip runs that day. A new id appends at rev
    /// 1; an existing one is replaced, rev bumped. Errors name what
    /// keeps it from being a record: an unknown trip, an instance it
    /// never runs, a date its service does not run.
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
    /// The edit document with `record_id` retired. This is the tombstone.
    Retire {
        #[ts(as = "EditDocument")]
        edits: Base<EditDocument>,
        record_id: String,
        #[ts(type = "number")]
        now_ms: u64,
    },
    /// The edit document with `record_id` back in force as a draft.
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

fn network_arc(network_hash: &str) -> Result<Arc<Network>, String> {
    kind::<transit_network::Network>().require(network_hash)
}

/// The typed dispatch. Native hosts call this directly. Failures are
/// `{"error": ...}`. It never panics: a total seam.
pub fn dispatch(request: Request) -> Result<Response, String> {
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

/// The JSON seam: `{"op": ...}` in, `{"ok": ...}` or `{"error": ...}` out.
/// The bytes match every other capability module.
pub fn dispatch_json(text: &str) -> String {
    cap_abi::answer(text, dispatch)
}

/// Drafts and edit documents travel inline. There is no bulk op of its own.
/// The seam answers with an error instead of silently ignoring the bulk bytes.
pub fn dispatch_bytes(op: u32, _bytes: &[u8]) -> String {
    cap_abi::outcome::<()>(Err(format!("unknown bulk op {op}: divert has no bulk op of its own")))
}

/// The worked examples. The generator runs them. No module build turns
/// the feature on.
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
