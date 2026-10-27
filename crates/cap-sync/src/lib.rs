//! cap-sync — the sync capability.
//!
//! One question: **what is the converged state of this edit
//! document pair?** The sync protocol is deliberately LWW per record
//! id plus tombstones (the terminal authority is the agency
//! scheduling system: propose → review → accept), and its one step
//! — merge the client document with the server document, transition
//! drafts to submitted — is what this module answers. The host owns
//! both documents (wholesale-replaced, JS/store-owned) and the
//! clock (`now_ms` is the only route; hosts own time). transit-sync
//! is the domain library — the semilattice merge is property-tested
//! there; this module is the seam.
//!
//! Validation is NOT here: edit records validate against the
//! NETWORK at load_edits — every model-side module's registration
//! answer (reported, never rejected — the sync contract). A second
//! answer here would be two answers to one question on the bus.
//! The module is stateless: no registries, no persistence —
//! document storage is the host's, and the sync-persistence
//! design (blobs + pointers, pointer CAS) is the host discipline's,
//! never a module's.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// One operation call. Tagged like every capability's seam.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "cap_sync/")]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Request {
    /// Merge the client's edit document with the server's (LWW per
    /// record id — `transit_sync::merge`, the property-tested
    /// semilattice), then transition drafts to submitted (the
    /// submit bumps rev and stamps `now_ms`). The answer is the
    /// converged document; the client adopts it wholesale.
    SyncEdits {
        client: Vec<transit_sync::EditRecord>,
        server: Vec<transit_sync::EditRecord>,
        #[ts(type = "number")]
        now_ms: u64,
    },
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "cap_sync/")]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Response {
    SyncEdits { records: Vec<transit_sync::EditRecord> },
}

/// The typed dispatch (native hosts call this directly). Failures
/// are `{"error": ...}`, never panics — a total seam.
pub fn dispatch(request: Request) -> Result<Response, String> {
    match request {
        Request::SyncEdits { client, server, now_ms } => Ok(Response::SyncEdits {
            records: transit_sync::submit(transit_sync::merge(&client, &server), now_ms),
        }),
    }
}

/// The JSON seam: `{"op": ...}` in, `{"ok": ...}` / `{"error": ...}`
/// out — byte-compatible with every other capability module.
pub fn dispatch_json(text: &str) -> String {
    let outcome = match serde_json::from_str::<Request>(text) {
        Ok(request) => match dispatch(request) {
            Ok(response) => serde_json::json!({ "ok": response }),
            Err(error) => serde_json::json!({ "error": error }),
        },
        Err(error) => serde_json::json!({ "error": format!("invalid request: {error}") }),
    };
    serde_json::to_string(&outcome).expect("dispatch outcomes serialize")
}

/// The edit document is a JSON document — no bulk-bytes op exists;
/// the seam answers honestly instead of silently ignoring one.
pub fn dispatch_bytes(op: u32, _bytes: &[u8]) -> String {
    serde_json::json!({ "error": format!("unknown bulk op {op}: the edit document is JSON (sync_edits)") })
        .to_string()
}

cap_abi::capability! {
    json: dispatch_json,
    bytes: dispatch_bytes,
}
