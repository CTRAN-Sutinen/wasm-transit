//! `publishable` and cap-plans' `pick` run one check (transit-network
//! `publish_check`), and `document`'s bytes are the bytes a pick stores.

#![forbid(unsafe_code)]

use cap_abi::content_hash;
use cap_abi::kernel::kind;
use serde::{Deserialize, Serialize};
#[cfg(feature = "bindings")]
use ts_rs::TS;
use transit_codec::{Boundary, FeedSummary, ImportNote, ImportOutcome};
use transit_network::Network;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS))]
#[cfg_attr(feature = "bindings", ts(export, export_to = "cap_network/"))]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Request {
    Import { feed_hash: String },
    Validate { network_hash: String },
    Export {
        network_hash: String,
        #[serde(default)]
        #[cfg_attr(feature = "bindings", ts(optional))]
        boundary_hash: Option<String>,
    },
    Document { network_hash: String },
    Publishable { network_hash: String },
    StoredPlans {
        #[cfg_attr(feature = "bindings", ts(type = "unknown"))]
        document: serde_json::Value,
    },
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "bindings", derive(TS))]
#[cfg_attr(feature = "bindings", ts(export, export_to = "cap_network/"))]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Response {
    Import {
        #[cfg_attr(feature = "bindings", ts(as = "Network"))]
        network: cap_abi::Canonical<Network>,
        #[cfg_attr(feature = "bindings", ts(as = "Boundary"))]
        boundary: cap_abi::Canonical<Boundary>,
        notes: Vec<ImportNote>,
        network_hash: String,
        boundary_hash: String,
        timezone: Option<String>,
        first_service_date: Option<String>,
    },
    Validate { issues: Vec<transit_network::NetworkError>, warnings: Vec<transit_network::NetworkWarning> },
    Export { feed_hash: String, summary: FeedSummary, zip_b64: String },
    Document { document: String, document_hash: String, plan: transit_network::PlanMeta, timezone: Option<String> },
    Publishable {
        issues: Vec<transit_network::NetworkError>,
        refusals: Vec<transit_network::NetworkWarning>,
        warnings: Vec<transit_network::NetworkWarning>,
    },
    StoredPlans {
        #[cfg_attr(feature = "bindings", ts(type = "Record<string, unknown>"))]
        plans: std::collections::BTreeMap<String, serde_json::Value>,
    },
}

pub fn dispatch(request: Request) -> Result<Response, cap_abi::Refusal> {
    match request {
        Request::Import { feed_hash } => {
            let feed = kind::<transit_codec::Feed>().require(&feed_hash)?;
            let ImportOutcome { network, boundary, notes } = transit_codec::import(&feed);
            let (network_hash, _) = kind::<transit_network::Network>().register(network.clone())?;
            let (boundary_hash, _) = kind::<transit_codec::Boundary>().register(boundary.clone())?;
            let timezone = network.timezone().map(str::to_string);
            let first_service_date = transit_network::first_service_date(&network).map(|d| d.to_string());
            Ok(Response::Import {
                network: cap_abi::Canonical(network),
                boundary: cap_abi::Canonical(boundary),
                notes,
                network_hash,
                boundary_hash,
                timezone,
                first_service_date,
            })
        }
        Request::Validate { network_hash } => {
            let network = kind::<transit_network::Network>().require(&network_hash)?;
            Ok(Response::Validate {
                issues: transit_network::validation_issues(&network),
                warnings: transit_network::validation_warnings(&network),
            })
        }
        Request::Export { network_hash, boundary_hash } => {
            let network = kind::<transit_network::Network>().require(&network_hash)?;
            let boundary = boundary_hash.as_deref().map(|h| kind::<transit_codec::Boundary>().require(h)).transpose()?;
            let feed = transit_codec::export(&network, boundary.as_deref().unwrap_or(&Boundary::default()))?;
            let counts = feed.summary();
            let bytes = feed.to_zip_bytes();
            let (feed_hash, _) = kind::<transit_codec::Feed>().register_bytes(&bytes)?;
            Ok(Response::Export { feed_hash, summary: counts, zip_b64: transit_codec::base64_encode(&bytes) })
        }
        Request::Document { network_hash } => {
            // A host stores this document, so the read is a write. A network
            // read one version ahead answers in this build's version. A
            // read-only one is refused.
            let network = kind::<transit_network::Network>().require_writable(&network_hash)?;
            let document = cap_abi::canonical_bytes(&*network)?;
            let document_hash = content_hash(document.as_bytes());
            if document_hash != network_hash && kind::<transit_network::Network>().read_ahead(&network_hash).is_none() {
                return Err(format!("registered under {network_hash} but serializes to {document_hash}").into());
            }
            Ok(Response::Document {
                document,
                document_hash,
                plan: network.plan.clone(),
                timezone: network.timezone().map(str::to_string),
            })
        }
        Request::Publishable { network_hash } => {
            let network = kind::<transit_network::Network>().require(&network_hash)?;
            let transit_network::PublishCheck { issues, refusals, warnings } = transit_network::publish_check(&network);
            Ok(Response::Publishable { issues, refusals, warnings })
        }
        Request::StoredPlans { document } => Ok(Response::StoredPlans { plans: transit_network::plan_set::stored_plans(document)? }),
    }
}

pub fn dispatch_json(text: &str) -> String {
    cap_abi::answer(text, dispatch)
}

pub fn dispatch_bytes(op: u32, _bytes: &[u8]) -> String {
    cap_abi::outcome::<(), String>(Err(format!("unknown bulk op {op}: the network codec has no bulk op of its own")))
}

#[cfg(feature = "examples")]
pub mod examples;

cap_abi::capability! {
    id: "network",
    json: dispatch_json,
    bytes: dispatch_bytes,
    documents: [transit_codec::Feed, transit_network::Network, transit_codec::Boundary],
    request: Request,
    speaks: &[("feed", &[1]), ("network", &[transit_network::SCHEMA_VERSION]), ("boundary", &[1])],
}
