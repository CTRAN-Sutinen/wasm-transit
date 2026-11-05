//! cap-divert — the diversion-authoring capability.
//!
//! One question: **what does this diversion amount to?** A
//! dispatcher's draft (route, pattern, anchors, the interior stops
//! the detour still serves, a traced path, added time, dates) is
//! answered over the NETWORK: the route's patterns and the default
//! one, each stop's role, the skips in effect, the alignment the
//! detour is drawn against, the traced path and its length, and what
//! keeps the draft from being a record — and the edit document's
//! records as the dispatcher's list names them, and what each record
//! amounts to on the network (its validation report: a record the
//! network cannot answer moves nothing). Proposing it, retiring a
//! record (the tombstone), restoring one, and reading a stored edit
//! document are the edit document's next states — the envelope
//! rules (rev, stamp, status) every writer follows.
//!
//! The host owns the documents (wholesale-replaced) and the clock
//! and ids (`now_ms`, `record_id` — hosts own time and randomness).
//! The domain code lives in transit-dispatch (the draft over the
//! network) and transit-sync (the record envelope); this module is
//! the seam. Converging two edit documents is cap-sync's question —
//! never this module's.

#![forbid(unsafe_code)]

use std::sync::Arc;

use cap_abi::kernel::NETWORK;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use transit_dispatch::{DiversionDraft, DiversionRow, DraftView};
use transit_network::Network;
use transit_sync::EditRecord;

/// One operation call. Tagged like every capability's seam.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "cap_divert/")]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Request {
    /// What the editor shows about a draft (total: problems, never an
    /// error, for a draft that names what the network lacks).
    Draft { network_hash: String, draft: DiversionDraft },
    /// The edit document's rows, names resolved over the network —
    /// what the dispatcher's list shows.
    List { network_hash: String, records: Vec<EditRecord> },
    /// What each record of the edit document amounts to on the
    /// network, in document order: whether it is valid against it (its
    /// route, pattern, and stops exist and are in order), what is wrong
    /// when it is not, and how many trip instances it touches. Invalid
    /// records are REPORTED, never rejected (the sync contract) — every
    /// day view simply finds nothing to patch for them.
    Reports { network_hash: String, records: Vec<EditRecord> },
    /// The edit document with the draft proposed as `record_id` (a
    /// new id appends at rev 1; an existing one is replaced, rev
    /// bumped). Errors name what keeps the draft from being a record.
    Propose {
        network_hash: String,
        draft: DiversionDraft,
        records: Vec<EditRecord>,
        record_id: String,
        #[ts(type = "number")]
        now_ms: u64,
    },
    /// The edit document with `record_id` retired — the tombstone.
    Retire {
        records: Vec<EditRecord>,
        record_id: String,
        #[ts(type = "number")]
        now_ms: u64,
    },
    /// The edit document with `record_id` back in force as a draft.
    Restore {
        records: Vec<EditRecord>,
        record_id: String,
        #[ts(type = "number")]
        now_ms: u64,
    },
    /// Read a stored edit document: the records valid under the
    /// current schema (serde defaults filled) and how many were not.
    Sanitize {
        #[ts(type = "unknown")]
        records: serde_json::Value,
    },
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "cap_divert/")]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Response {
    Draft { view: DraftView },
    List { rows: Vec<DiversionRow> },
    Reports { reports: Vec<transit_sync::EditReport> },
    Propose { records: Vec<EditRecord> },
    Retire { records: Vec<EditRecord> },
    Restore { records: Vec<EditRecord> },
    Sanitize { records: Vec<EditRecord>, dropped: u32 },
}

fn network_arc(network_hash: &str) -> Result<Arc<Network>, String> {
    NETWORK.require(network_hash)
}

/// The typed dispatch (native hosts call this directly). Failures
/// are `{"error": ...}`, never panics — a total seam.
pub fn dispatch(request: Request) -> Result<Response, String> {
    match request {
        Request::Draft { network_hash, draft } => {
            let network = network_arc(&network_hash)?;
            Ok(Response::Draft { view: transit_dispatch::draft_view(&network, &draft) })
        }
        Request::List { network_hash, records } => {
            let network = network_arc(&network_hash)?;
            Ok(Response::List { rows: transit_dispatch::diversion_rows(&network, &records) })
        }
        Request::Reports { network_hash, records } => {
            let network = network_arc(&network_hash)?;
            Ok(Response::Reports { reports: transit_dispatch::report_against_network(&network, &records) })
        }
        Request::Propose { network_hash, draft, records, record_id, now_ms } => {
            let network = network_arc(&network_hash)?;
            // The domain id is stable across re-proposals of one record.
            let id = records
                .iter()
                .find(|r| r.id == record_id)
                .map(|r| r.diversion.id.clone())
                .unwrap_or_else(|| format!("div-{}-{now_ms}", draft.route_id));
            let diversion = transit_dispatch::diversion_of(&network, &draft, &id)?;
            Ok(Response::Propose { records: transit_sync::propose(&records, &record_id, diversion, now_ms) })
        }
        Request::Retire { records, record_id, now_ms } => {
            Ok(Response::Retire { records: transit_sync::set_retired(&records, &record_id, true, now_ms)? })
        }
        Request::Restore { records, record_id, now_ms } => {
            Ok(Response::Restore { records: transit_sync::set_retired(&records, &record_id, false, now_ms)? })
        }
        Request::Sanitize { records } => {
            let (records, dropped) = transit_sync::sanitize(&records);
            Ok(Response::Sanitize { records, dropped: dropped as u32 })
        }
    }
}

/// The JSON seam: `{"op": ...}` in, `{"ok": ...}` / `{"error": ...}`
/// out — byte-compatible with every other capability module.
pub fn dispatch_json(text: &str) -> String {
    cap_abi::answer(text, dispatch)
}

/// Drafts and edit documents travel inline — no bulk op of its own;
/// the seam answers honestly instead of silently ignoring one.
pub fn dispatch_bytes(op: u32, _bytes: &[u8]) -> String {
    cap_abi::outcome::<()>(Err(format!("unknown bulk op {op}: divert has no bulk op of its own")))
}

cap_abi::capability! {
    id: "divert",
    json: dispatch_json,
    bytes: dispatch_bytes,
    documents: [NETWORK],
}
