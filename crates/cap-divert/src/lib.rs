//! cap-divert — the diversion-authoring capability.
//!
//! One question: **what does this diversion amount to?** A
//! dispatcher's draft (route, pattern, anchors, the interior stops
//! the detour still serves, a traced path, added time, dates) is
//! answered over the NETWORK: the route's patterns and the default
//! one, each stop's role, the skips in effect, the alignment the
//! detour is drawn against, the traced path and its length, and what
//! keeps the draft from being a record — and the edit document's
//! records as the dispatcher's list names them. Proposing it, retiring a
//! record (the tombstone), restoring one, and reading a stored edit
//! document are the edit document's next states — the envelope
//! rules (rev, stamp, status) every writer follows.
//!
//! The host owns the documents (wholesale-replaced) and the clock
//! and ids (`now_ms`, `record_id` — hosts own time and randomness).
//! The domain code lives in transit-dispatch (the draft over the
//! network) and transit-sync (the record envelope); this module is
//! the seam. Converging two edit documents is cap-sync's question,
//! validating a record against the network every day view's
//! load_edits — never this module's.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

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
    /// Register the NETWORK document under its content hash
    /// (idempotent) — the same identity every model-side capability
    /// returns.
    /// Decoded by `cap_abi::load_network_json` on either lane, never
    /// through the tagged envelope (whose buffered decode would cost
    /// ~8× the document in peak memory and a second decoder).
    #[serde(skip_deserializing)]
    LoadNetwork { network: Network },
    /// What the editor shows about a draft (total: problems, never an
    /// error, for a draft that names what the network lacks).
    Draft { network_hash: String, draft: DiversionDraft },
    /// The edit document's rows, names resolved over the network —
    /// what the dispatcher's list shows.
    List { network_hash: String, records: Vec<EditRecord> },
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
    LoadNetwork { network_hash: String },
    Draft { view: DraftView },
    List { rows: Vec<DiversionRow> },
    Propose { records: Vec<EditRecord> },
    Retire { records: Vec<EditRecord> },
    Restore { records: Vec<EditRecord> },
    Sanitize { records: Vec<EditRecord>, dropped: u32 },
}

fn networks() -> &'static Mutex<HashMap<String, Arc<Network>>> {
    static REGISTRY: OnceLock<Mutex<HashMap<String, Arc<Network>>>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Canonical serialization → content hash, registered (idempotent).
fn register(network: Network) -> Result<String, String> {
    let hash = cap_abi::canonical_hash(&network)?;
    networks()
        .lock()
        .expect("network registry lock")
        .entry(hash.clone())
        .or_insert_with(|| Arc::new(network));
    Ok(hash)
}

fn network_arc(network_hash: &str) -> Result<Arc<Network>, String> {
    networks()
        .lock()
        .expect("network registry lock")
        .get(network_hash)
        .cloned()
        .ok_or_else(|| format!("unknown network {network_hash}: load_network first"))
}

/// The typed dispatch (native hosts call this directly). Failures
/// are `{"error": ...}`, never panics — a total seam.
pub fn dispatch(request: Request) -> Result<Response, String> {
    match request {
        Request::LoadNetwork { network } => Ok(Response::LoadNetwork { network_hash: register(network)? }),
        Request::Draft { network_hash, draft } => {
            let network = network_arc(&network_hash)?;
            Ok(Response::Draft { view: transit_dispatch::draft_view(&network, &draft) })
        }
        Request::List { network_hash, records } => {
            let network = network_arc(&network_hash)?;
            Ok(Response::List { rows: transit_dispatch::diversion_rows(&network, &records) })
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
    if let Some(answer) = cap_abi::load_network_json(text, |doc| dispatch_bytes(cap_abi::OP_LOAD_NETWORK, doc)) {
        return answer;
    }
    let outcome = match serde_json::from_str::<Request>(text) {
        Ok(request) => match dispatch(request) {
            Ok(response) => serde_json::json!({ "ok": response }),
            Err(error) => serde_json::json!({ "error": error }),
        },
        Err(error) => serde_json::json!({ "error": format!("invalid request: {error}") }),
    };
    serde_json::to_string(&outcome).expect("dispatch outcomes serialize")
}

/// Drafts and edit documents are JSON — no bulk-bytes op exists; the
/// seam answers honestly instead of silently ignoring one.
pub fn dispatch_bytes(op: u32, bytes: &[u8]) -> String {
    match op {
        cap_abi::OP_LOAD_NETWORK => cap_abi::outcome(
            cap_abi::parse_network(bytes).and_then(|network| dispatch(Request::LoadNetwork { network })),
        ),
        _ => serde_json::json!({ "error": format!("unknown bulk op {op}: diversion authoring answers the kernel's load_network bulk op; drafts and records travel inline") }).to_string(),
    }
}

cap_abi::capability! {
    json: dispatch_json,
    bytes: dispatch_bytes,
}
