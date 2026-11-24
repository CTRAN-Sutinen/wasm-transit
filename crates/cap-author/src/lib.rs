//! cap-author: what does this network become after these edits? (OPS.md
//! has the ops.)
//!
//! The seam over transit-network's edits. `apply` takes typed edits,
//! atomically. It answers the next document under its hash. That document
//! is registered, so the next change chains by hash. The answer also carries
//! its issues and warnings. The last document is never touched. Every host,
//! the service-design UI included, sends edits and adopts the answer. With
//! `work_rules_hash`, a block's waits are judged under the work rules (D-9).
//!
//! * A redraw names its reach, one pattern or every pattern on the shared
//!   alignment, and answers what it touched, so a fix for one variant
//!   never moves the others silently (D-11).
//! * A timetable gesture changes only the one service type it names,
//!   because a combination's members are separate authored types; the UI
//!   warns when others run with it (D-8).
//! * A fork with a period end carries each weekly type still running in
//!   the calendar's last week to that end. Otherwise a fork runs nothing
//!   until each type is extended (13 edits on C-TRAN). Then the fork's
//!   calendar is cut to its period. Every type stays (D-41, D-104, D-112,
//!   D-138).

#![forbid(unsafe_code)]

use std::sync::Arc;

use cap_abi::kernel::kind;
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
    /// Stamp an imported feed as a pick (D-140, D-228): `set_plan` under
    /// `id` and `name`, effective from the feed's first service date. A
    /// document that is a pick already, or runs no service, comes back
    /// as it was.
    StampPick { network_hash: String, name: String, id: String },
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "cap_author/")]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Response {
    /// The next document and what it is. It holds the hash, which is
    /// already registered. Chain the next change on it. It also holds the
    /// issues that gate export, and the warnings that never gate.
    Apply {
        network_hash: String,
        network: Network,
        issues: Vec<NetworkError>,
        warnings: Vec<NetworkWarning>,
        /// What the change created, with the ids it was given. The ids are
        /// minted when the edit names none. An authoring host opens these.
        created: Vec<transit_network::CreatedEntity>,
        /// What the change altered beyond what it created: the patterns a
        /// `draw_alignment` redrew (D-11). Nothing moves silently.
        touched: Vec<transit_network::TouchedEntity>,
    },
    /// The document, stamped or as it was (`stamped`), registered and
    /// checked as `apply` checks.
    StampPick {
        network_hash: String,
        network: Network,
        issues: Vec<NetworkError>,
        warnings: Vec<NetworkWarning>,
        stamped: bool,
    },
}

/// The network an edit applies to: the base of a write, so a network
/// read one version ahead through a read-only step is refused (D-186).
fn network_arc(network_hash: &str) -> Result<Arc<Network>, String> {
    kind::<transit_network::Network>().require_writable(network_hash)
}

/// The typed dispatch. Native hosts call this directly. Failures are
/// `{"error": ...}`. It never panics: a total seam.
pub fn dispatch(request: Request) -> Result<Response, String> {
    match request {
        Request::Apply { network_hash, edits, work_rules_hash } => {
            let rules = work_rules_hash.as_deref().map(|h| kind::<transit_runs::WorkRules>().require(h)).transpose()?;
            let current = network_arc(&network_hash)?;
            let transit_network::Applied { network: next, created, touched } =
                transit_network::apply_edits_reporting(&current, &edits).map_err(|e| e.to_string())?;
            let issues = transit_network::validation_issues_under(&next, &transit_runs::required(rules.as_deref()));
            let warnings = transit_network::validation_warnings(&next);
            // The next document registers where it is minted. The next
            // change chains on its hash, and so does every reader in an
            // assembly.
            let (network_hash, next) = kind::<transit_network::Network>().register(next)?;
            Ok(Response::Apply { network_hash, network: (*next).clone(), issues, warnings, created, touched })
        }
        Request::StampPick { network_hash, name, id } => {
            let current = network_arc(&network_hash)?;
            let edits = transit_network::stamp_as_pick(&current, &name, &id);
            let stamped = !edits.is_empty();
            let next = transit_network::apply_edits(&current, &edits).map_err(|e| e.to_string())?;
            let issues = transit_network::validation_issues_under(&next, &transit_runs::required(None));
            let warnings = transit_network::validation_warnings(&next);
            let (network_hash, next) = kind::<transit_network::Network>().register(next)?;
            Ok(Response::StampPick { network_hash, network: (*next).clone(), issues, warnings, stamped })
        }
    }
}

/// The JSON seam: `{"op": ...}` in, `{"ok": ...}` or `{"error": ...}` out.
/// The bytes match every other capability module.
pub fn dispatch_json(text: &str) -> String {
    cap_abi::answer(text, dispatch)
}

/// Authoring takes edits inline. It has no bulk op of its own. The seam
/// answers with an error instead of silently ignoring the bulk bytes.
pub fn dispatch_bytes(op: u32, _bytes: &[u8]) -> String {
    cap_abi::outcome::<()>(Err(format!("unknown bulk op {op}: author has no bulk op of its own")))
}

/// The worked examples. The generator runs them. No module build turns
/// the feature on.
#[cfg(feature = "examples")]
pub mod examples;

cap_abi::capability! {
    id: "author",
    json: dispatch_json,
    bytes: dispatch_bytes,
    documents: [transit_network::Network, transit_runs::WorkRules],
    request: Request,
    consumes: &[("network", "network")],
    speaks: &[("network", &[transit_network::SCHEMA_VERSION]), ("work_rules", &[1])],
}
