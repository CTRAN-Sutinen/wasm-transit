//! cap-author — the authoring capability.
//!
//! One question: **what does this network become after these
//! edits?** A planner's change — retime a schedule, add a stop, fork
//! the plan into the next pick — is a list of typed edits
//! (`transit_network::NetworkEdit`) applied atomically to a
//! registered document (the kernel's). The answer is the NEXT
//! document under its content hash, registered so the next change
//! chains by hash, with
//! its validation issues and warnings: the authoring loop in one op.
//! The last document is never touched; the host wholesale-replaces
//! its copy (JS owns the documents).
//!
//! The domain code (the edit vocabulary, the referential refusals)
//! lives in transit-network; this module is the seam. With a
//! `work_rules_hash`, a block's waits are judged against the recovery
//! the work rules require (D-9: the rules win; without them, the
//! smallest authored layover at each terminal). Same identity
//! as every capability: the module is the hash authority.

#![forbid(unsafe_code)]

use std::sync::Arc;

use cap_abi::kernel::{NETWORK, WORK_RULES};
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use transit_network::{Network, NetworkEdit, NetworkError, NetworkWarning};

/// One operation call. Tagged like every capability's seam.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "cap_author/")]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Request {
    /// Apply `edits`, in order and atomically, to the registered
    /// document: the next document, registered under its own hash.
    Apply {
        network_hash: String,
        edits: Vec<NetworkEdit>,
        /// The work rules whose recovery requirement judges each wait
        /// (D-9); without them, each terminal's floor does.
        #[serde(default)]
        #[ts(optional)]
        work_rules_hash: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "cap_author/")]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Response {
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
        /// What the change altered beyond what it created: the
        /// patterns a `draw_alignment` redrew (D-11 — nothing moves
        /// silently).
        touched: Vec<transit_network::TouchedEntity>,
    },
}

fn network_arc(network_hash: &str) -> Result<Arc<Network>, String> {
    NETWORK.require(network_hash)
}

/// The typed dispatch (native hosts call this directly). Failures
/// are `{"error": ...}`, never panics — a total seam.
pub fn dispatch(request: Request) -> Result<Response, String> {
    match request {
        Request::Apply { network_hash, edits, work_rules_hash } => {
            let rules = work_rules_hash.as_deref().map(|h| WORK_RULES.require(h)).transpose()?;
            let current = network_arc(&network_hash)?;
            let transit_network::Applied { network: next, created, touched } =
                transit_network::apply_edits_reporting(&current, &edits).map_err(|e| e.to_string())?;
            let issues = transit_network::validation_issues_under(&next, &transit_runs::required(rules.as_deref()));
            let warnings = transit_network::validation_warnings(&next);
            // The next document registers where it was minted: the next
            // change (and, in an assembly, every reader) chains on its hash.
            let (network_hash, next) = NETWORK.register(next)?;
            Ok(Response::Apply { network_hash, network: (*next).clone(), issues, warnings, created, touched })
        }
    }
}

/// The JSON seam: `{"op": ...}` in, `{"ok": ...}` / `{"error": ...}`
/// out — byte-compatible with every other capability module.
pub fn dispatch_json(text: &str) -> String {
    cap_abi::answer(text, dispatch)
}

/// Authoring takes edits inline — no bulk op of its own; the seam
/// answers honestly instead of silently ignoring one.
pub fn dispatch_bytes(op: u32, _bytes: &[u8]) -> String {
    cap_abi::outcome::<()>(Err(format!("unknown bulk op {op}: author has no bulk op of its own")))
}

cap_abi::capability! {
    id: "author",
    json: dispatch_json,
    bytes: dispatch_bytes,
    documents: [NETWORK, WORK_RULES],
}
