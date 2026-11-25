#![forbid(unsafe_code)]

use std::sync::Arc;

use cap_abi::kernel::kind;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use transit_network::{Network, NetworkEdit, NetworkError, NetworkWarning};

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "cap_author/")]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Request {
    Apply {
        network_hash: String,
        edits: Vec<NetworkEdit>,
        #[serde(default)]
        #[ts(optional)]
        work_rules_hash: Option<String>,
    },
    StampPick { network_hash: String, name: String, id: String },
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "cap_author/")]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Response {
    Apply {
        network_hash: String,
        network: Network,
        issues: Vec<NetworkError>,
        warnings: Vec<NetworkWarning>,
        created: Vec<transit_network::CreatedEntity>,
        touched: Vec<transit_network::TouchedEntity>,
    },
    StampPick {
        network_hash: String,
        network: Network,
        issues: Vec<NetworkError>,
        warnings: Vec<NetworkWarning>,
        stamped: bool,
    },
}

/// A network read one version ahead is refused here, because an edit writes
/// the next document.
fn network_arc(network_hash: &str) -> Result<Arc<Network>, cap_abi::Refusal> {
    kind::<transit_network::Network>().require_writable(network_hash)
}

pub fn dispatch(request: Request) -> Result<Response, cap_abi::Refusal> {
    match request {
        Request::Apply { network_hash, edits, work_rules_hash } => {
            let rules = work_rules_hash.as_deref().map(|h| kind::<transit_runs::WorkRules>().require(h)).transpose()?;
            let current = network_arc(&network_hash)?;
            let transit_network::Applied { network: next, created, touched } =
                transit_network::apply_edits_reporting(&current, &edits).map_err(|e| e.to_string())?;
            let issues = transit_network::validation_issues_under(&next, &transit_runs::required(rules.as_deref()));
            let warnings = transit_network::validation_warnings(&next);
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

pub fn dispatch_json(text: &str) -> String {
    cap_abi::answer(text, dispatch)
}

pub fn dispatch_bytes(op: u32, _bytes: &[u8]) -> String {
    cap_abi::outcome::<(), String>(Err(format!("unknown bulk op {op}: author has no bulk op of its own")))
}

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
