//! cap-author — the authoring capability.
//!
//! One question: **what does this network become after these
//! edits?** A planner's change — retime a schedule, add a stop, fork
//! the plan into the next pick — is a list of typed edits
//! (`transit_network::NetworkEdit`) applied atomically to a
//! registered document. The answer is the NEXT document under its
//! content hash, registered so the next change chains by hash, with
//! its validation issues and warnings: the authoring loop in one op.
//! The last document is never touched; the host wholesale-replaces
//! its copy (JS owns the documents).
//!
//! The domain code (the edit vocabulary, the referential refusals)
//! lives in transit-network; this module is the seam. Same identity
//! as every capability: the module is the hash authority.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use ts_rs::TS;
use transit_network::{Network, NetworkEdit, NetworkError, NetworkWarning};

/// One operation call. Tagged like every capability's seam.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "cap_author/")]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Request {
    /// Register the NETWORK document under its content hash
    /// (idempotent) — the same identity every model-side capability
    /// returns.
    LoadNetwork { network: Network },
    /// Apply `edits`, in order and atomically, to the registered
    /// document: the next document, registered under its own hash.
    Apply { network_hash: String, edits: Vec<NetworkEdit> },
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "cap_author/")]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Response {
    LoadNetwork { network_hash: String },
    /// The next document and what it is: its hash (already
    /// registered — chain the next change on it), the issues that
    /// gate export, and the warnings that never gate.
    Apply {
        network_hash: String,
        network: Network,
        issues: Vec<NetworkError>,
        warnings: Vec<NetworkWarning>,
        /// What the change created, with the ids it was given (minted
        /// when the edit named none) — what an authoring host opens.
        created: Vec<transit_network::CreatedEntity>,
    },
}

fn networks() -> &'static Mutex<HashMap<String, Arc<Network>>> {
    static REGISTRY: OnceLock<Mutex<HashMap<String, Arc<Network>>>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Canonical serialization → content hash, registered (idempotent).
fn register(network: Network) -> Result<String, String> {
    let canonical =
        serde_json::to_string(&network).map_err(|e| format!("document does not serialize: {e}"))?;
    let hash = format!("sha256:{:x}", Sha256::digest(canonical.as_bytes()));
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
        Request::Apply { network_hash, edits } => {
            let current = network_arc(&network_hash)?;
            let (next, created) =
                transit_network::apply_edits_reporting(&current, &edits).map_err(|e| e.to_string())?;
            let issues = transit_network::validation_issues(&next);
            let warnings = transit_network::validation_warnings(&next);
            let network_hash = register(next.clone())?;
            Ok(Response::Apply { network_hash, network: next, issues, warnings, created })
        }
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

/// Authoring takes documents and edits inline — no bulk-bytes op
/// exists; the seam answers honestly instead of silently ignoring one.
pub fn dispatch_bytes(op: u32, _bytes: &[u8]) -> String {
    serde_json::json!({ "error": format!("unknown bulk op {op}: authoring takes its document and edits inline (load_network / apply)") })
        .to_string()
}

cap_abi::capability! {
    json: dispatch_json,
    bytes: dispatch_bytes,
}
