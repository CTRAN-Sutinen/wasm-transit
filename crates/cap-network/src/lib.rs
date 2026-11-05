//! cap-network — the network-codec capability module.
//!
//! The second capability extracted from the MONOLITH (after
//! analysis): the codec that turns a GTFS feed into an authored
//! network document (the planner's bootstrap) and back (the publish
//! path). The domain code lives in the shared `transit-codec`
//! library; a planner's bundle ships this module, a rider's never
//! hears of it.
//!
//! The documents it reads are the kernel's, registered by its loads
//! and content-addressed like everywhere on the bus:
//! * **feed** — the raw GTFS zip bytes; the content hash IS the
//!   document identity (the same in every module, so a host can load
//!   one zip anywhere and assert they agree);
//! * **network** — the authored document, registered under the hash
//!   of its canonical serialization; every op addresses it by hash,
//!   never by carrying the whole document per request;
//! * **boundary** — an import's residue (what the feed's bytes carry
//!   that the document does not); an export names it to write the
//!   imported numbering back.
//!
//! What it mints registers where it was minted: an import's network
//! and boundary, an export's feed — each answered by hash, so a host
//! chains on without sending them back in.
//!
//! Only the planner persona authors networks — but this module's
//! write op (export) is the publish path: an authored document in, a
//! spec-shaped GTFS zip out, deterministic bytes so the produced
//! artifact has one identity everywhere.

#![forbid(unsafe_code)]

use cap_abi::content_hash;
use cap_abi::kernel::{BOUNDARY, FEED, NETWORK};
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use transit_codec::{Boundary, FeedSummary, ImportNote, ImportOutcome};
use transit_network::Network;

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "cap_network/")]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Request {
    /// The planner's bootstrap: read a loaded feed into an authored
    /// network document — the agency's trips, by id, with their own
    /// times. Deterministic; what the model cannot hold yet is
    /// reported in the notes, never silent. The document registers
    /// itself, and so does its boundary residue (what the feed's bytes
    /// carry that the document does not): the response carries both
    /// hashes so validate/export can chain without a reload.
    Import { feed_hash: String },
    /// Every issue of a registered document — the authoring UI's
    /// feedback loop, the whole list, never thrown.
    Validate { network_hash: String },
    /// The publish path: write a registered document as a GTFS zip
    /// (deterministic bytes) — numbered by `boundary_hash` when it
    /// names the document's import's boundary (the feed round-trips),
    /// 1..n otherwise. Returns the zip (base64) under the content hash
    /// of those bytes — the same identity any other module returns for
    /// the same bytes, so the host can assert the handoff. Invalid
    /// documents are refused (validate first).
    Export {
        network_hash: String,
        #[serde(default)]
        #[ts(optional)]
        boundary_hash: Option<String>,
    },
    /// A registered document's canonical bytes (its serialization, the
    /// bytes its hash names) and its plan frame — what a host stores
    /// when it persists a network it was sent in any serialization
    /// (the server publishing a pick): the module is the hash
    /// authority, the host stores exactly these bytes and never reads
    /// them.
    Document { network_hash: String },
    /// What refuses a registered network as a published pick: its
    /// validation issues, and the warnings that refuse publication (a
    /// pick that runs no service in its period, D-76). The remaining
    /// warnings ride along and never refuse.
    Publishable { network_hash: String },
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "cap_network/")]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Response {
    Import { network: Network, boundary: Boundary, notes: Vec<ImportNote>, network_hash: String, boundary_hash: String },
    /// `issues` gate (invalid documents never export); `warnings`
    /// never gate — they name what validity lets through (trips with
    /// no vehicle, a pick with no service in its period).
    Validate { issues: Vec<transit_network::NetworkError>, warnings: Vec<transit_network::NetworkWarning> },
    Export { feed_hash: String, summary: FeedSummary, zip_b64: String },
    Document { document: String, document_hash: String, plan: transit_network::PlanMeta },
    Publishable {
        issues: Vec<transit_network::NetworkError>,
        refusals: Vec<transit_network::NetworkWarning>,
        warnings: Vec<transit_network::NetworkWarning>,
    },
}

/// The typed dispatch (native hosts call this directly). Failures
/// are `{"error": ...}`, never panics — a total seam.
pub fn dispatch(request: Request) -> Result<Response, String> {
    match request {
        Request::Import { feed_hash } => {
            let feed = FEED.require(&feed_hash)?;
            let ImportOutcome { network, boundary, notes } = transit_codec::import(&feed);
            // The documents register themselves: import's answer is
            // immediately addressable for validate and export (and,
            // in an assembly, for every capability that reads them).
            let (network_hash, _) = NETWORK.register(network.clone())?;
            let (boundary_hash, _) = BOUNDARY.register(boundary.clone())?;
            Ok(Response::Import { network, boundary, notes, network_hash, boundary_hash })
        }
        Request::Validate { network_hash } => {
            let network = NETWORK.require(&network_hash)?;
            Ok(Response::Validate {
                issues: transit_network::validation_issues(&network),
                warnings: transit_network::validation_warnings(&network),
            })
        }
        Request::Export { network_hash, boundary_hash } => {
            let network = NETWORK.require(&network_hash)?;
            let boundary = boundary_hash.as_deref().map(|h| BOUNDARY.require(h)).transpose()?;
            let feed = transit_codec::export(&network, boundary.as_deref().unwrap_or(&Boundary::default()))?;
            let counts = feed.summary();
            let bytes = feed.to_zip_bytes();
            // The produced feed registers under its own bytes' hash:
            // the exported artifact is a first-class feed document,
            // importable here and loadable in any other module.
            let (feed_hash, _) = FEED.register_bytes(&bytes, None)?;
            Ok(Response::Export { feed_hash, summary: counts, zip_b64: transit_codec::base64_encode(&bytes) })
        }
        Request::Document { network_hash } => {
            let network = NETWORK.require(&network_hash)?;
            let document = serde_json::to_string(&*network).map_err(|e| e.to_string())?;
            let document_hash = content_hash(document.as_bytes());
            if document_hash != network_hash {
                return Err(format!("registered under {network_hash} but serializes to {document_hash}"));
            }
            Ok(Response::Document { document, document_hash, plan: network.plan.clone() })
        }
        Request::Publishable { network_hash } => {
            let network = NETWORK.require(&network_hash)?;
            let refusals = transit_network::publish_refusals(&network);
            let warnings =
                transit_network::validation_warnings(&network).into_iter().filter(|w| !refusals.contains(w)).collect();
            Ok(Response::Publishable { issues: transit_network::validation_issues(&network), refusals, warnings })
        }
    }
}

/// The JSON seam: `{"op": ...}` in, `{"ok": ...}` / `{"error": ...}`
/// out — byte-compatible with the monolith and every other
/// capability module.
pub fn dispatch_json(text: &str) -> String {
    cap_abi::answer(text, dispatch)
}

/// The codec has no bulk op of its own (a feed's zip registers on the
/// kernel's `load_feed`); the seam answers honestly instead of
/// silently ignoring one.
pub fn dispatch_bytes(op: u32, _bytes: &[u8]) -> String {
    cap_abi::outcome::<()>(Err(format!("unknown bulk op {op}: the network codec has no bulk op of its own")))
}

cap_abi::capability! {
    id: "network",
    json: dispatch_json,
    bytes: dispatch_bytes,
    documents: [FEED, NETWORK, BOUNDARY],
}
