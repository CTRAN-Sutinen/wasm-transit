//! cap-network — the network-codec capability module.
//!
//! The second capability extracted from the MONOLITH (after
//! analysis): the codec that turns a GTFS feed into an authored
//! network document (the planner's bootstrap) and back (the publish
//! path). The domain code lives in the shared `transit-codec`
//! library; a planner's bundle ships this module, a rider's never
//! hears of it.
//!
//! Documents are content-addressed like everywhere on the bus:
//! * **feed** — the raw GTFS zip bytes; the content hash IS the
//!   document identity (the same formula cap-gtfs and cap-router
//!   return, so the host can load one zip into several modules and
//!   assert they agree);
//! * **network** — the authored document, registered under the hash
//!   of its canonical serialization; every op addresses it by hash,
//!   never by carrying the whole document per request.
//!
//! Only the planner persona authors networks — but this module's
//! write op (export) is the publish path: an authored document in, a
//! spec-shaped GTFS zip out, deterministic bytes so the produced
//! artifact has one identity everywhere.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use serde::{Deserialize, Serialize};
use cap_abi::content_hash;
use ts_rs::TS;
use transit_codec::{Boundary, Feed, ImportNote, ImportOutcome, TripIdMap};
use transit_core::FeedSummary;
use transit_network::Network;

/// The bulk-bytes op code (the only op that takes raw feed bytes).
pub const OP_LOAD_FEED: u32 = 0;

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "cap_network/")]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Request {
    /// Parse a GTFS zip from raw bytes; registers the feed under its
    /// content hash. Bulk-bytes op 0; the JSON form exists for hosts
    /// that already hold the bytes as numbers.
    LoadFeed { bytes: Vec<u8> },
    /// The planner's bootstrap: read a loaded feed into an authored
    /// network document — the agency's trips, by id, with their own
    /// times. Deterministic; what the model cannot hold yet is
    /// reported in the notes, never silent. The document registers
    /// itself with its boundary residue (what the feed's bytes carry
    /// that the document does not), and the response carries its hash
    /// so validate/export can chain without a reload.
    Import { feed_hash: String },
    /// Register an authored network document under its content hash
    /// (idempotent by hash). The authoring UI's entry point; every
    /// later op addresses the document by the returned hash.
    /// Decoded by `cap_abi::load_network_json` on either lane, never
    /// through the tagged envelope (whose buffered decode would cost
    /// ~8× the document in peak memory and a second decoder).
    #[serde(skip_deserializing)]
    LoadNetwork { network: Network },
    /// Every issue of a registered document — the authoring UI's
    /// feedback loop, the whole list, never thrown.
    Validate { network_hash: String },
    /// The publish path: write a registered document as a GTFS zip
    /// (deterministic bytes) — with its import's boundary when it is
    /// the imported document itself. Returns the zip (base64) under
    /// the content hash of those bytes — the same identity any other
    /// module returns for the same bytes, so the host can assert the
    /// handoff. Invalid documents are refused (validate first).
    Export { network_hash: String },
    /// The TRIP-ID MAP of a network: every trip, by its id (the
    /// agency's, for an imported trip), with its pattern trips and
    /// first start — the join rt data is keyed by.
    TripMap { network_hash: String },
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "cap_network/")]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Response {
    LoadFeed { feed_hash: String, summary: FeedSummary },
    Import { network: Network, boundary: Boundary, notes: Vec<ImportNote>, network_hash: String },
    LoadNetwork { network_hash: String },
    /// `issues` gate (invalid documents never export); `warnings`
    /// never gate — they name what validity lets through (trips with
    /// no vehicle, a pick with no service in its period).
    Validate { issues: Vec<transit_network::NetworkError>, warnings: Vec<transit_network::NetworkWarning> },
    Export { feed_hash: String, summary: FeedSummary, zip_b64: String },
    TripMap { map: TripIdMap },
}

/// Feeds by content hash of the zip bytes.
fn feeds() -> &'static Mutex<HashMap<String, Arc<Feed>>> {
    static REGISTRY: OnceLock<Mutex<HashMap<String, Arc<Feed>>>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

/// A feed's counts at a glance.
fn summary(feed: &Feed) -> FeedSummary {
    FeedSummary {
        agencies: feed.agency.len(),
        routes: feed.routes.len(),
        stops: feed.stops.len(),
        trips: feed.trips.len(),
        stop_times: feed.stop_times.len(),
    }
}

/// Authored network documents by content hash of their canonical
/// serialization — the module is the hash authority.
fn networks() -> &'static Mutex<HashMap<String, Arc<Network>>> {
    static REGISTRY: OnceLock<Mutex<HashMap<String, Arc<Network>>>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

/// The import's boundary residue, keyed by the network it belongs
/// to — captured at import (an edited or authored document exports
/// with the default boundary).
fn boundaries() -> &'static Mutex<HashMap<String, Arc<Boundary>>> {
    static REGISTRY: OnceLock<Mutex<HashMap<String, Arc<Boundary>>>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

fn network_hash(network: &Network) -> Result<String, String> {
    cap_abi::canonical_hash(network)
}

fn register_network(network: Network) -> Result<String, String> {
    let hash = network_hash(&network)?;
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

fn load_feed(bytes: &[u8]) -> Result<Response, String> {
    let hash = content_hash(bytes);
    {
        let guard = feeds().lock().expect("feed registry lock");
        if let Some(feed) = guard.get(&hash) {
            return Ok(Response::LoadFeed { feed_hash: hash, summary: summary(feed) });
        }
    }
    let feed = Feed::from_zip_bytes(bytes).map_err(|e| format!("feed does not parse: {e}"))?;
    let counts = summary(&feed);
    feeds()
        .lock()
        .expect("feed registry lock")
        .insert(hash.clone(), Arc::new(feed));
    Ok(Response::LoadFeed { feed_hash: hash, summary: counts })
}

/// The typed dispatch (native hosts call this directly). Failures
/// are `{"error": ...}`, never panics — a total seam.
pub fn dispatch(request: Request) -> Result<Response, String> {
    match request {
        Request::LoadFeed { bytes } => load_feed(&bytes),
        Request::Import { feed_hash } => {
            let feed = feeds()
                .lock()
                .expect("feed registry lock")
                .get(&feed_hash)
                .cloned()
                .ok_or_else(|| format!("unknown feed {feed_hash}: load_feed first"))?;
            let ImportOutcome { network, boundary, notes } = transit_codec::import(&feed);
            // The document registers itself: import's answer is
            // immediately addressable for validate and export, and its
            // boundary rides along under the same hash.
            let network_hash = register_network(network.clone())?;
            boundaries()
                .lock()
                .expect("boundary registry lock")
                .insert(network_hash.clone(), Arc::new(boundary.clone()));
            Ok(Response::Import { network, boundary, notes, network_hash })
        }
        Request::LoadNetwork { network } => {
            let hash = register_network(network)?;
            Ok(Response::LoadNetwork { network_hash: hash })
        }
        Request::Validate { network_hash } => {
            let network = network_arc(&network_hash)?;
            Ok(Response::Validate {
                issues: transit_network::validation_issues(&network),
                warnings: transit_network::validation_warnings(&network),
            })
        }
        Request::Export { network_hash } => {
            let network = network_arc(&network_hash)?;
            let boundary = boundaries().lock().expect("boundary registry lock").get(&network_hash).cloned();
            let feed = transit_codec::export(&network, boundary.as_deref().unwrap_or(&Boundary::default()))?;
            let counts = summary(&feed);
            let bytes = feed.to_zip_bytes();
            // The produced feed registers under its own bytes' hash:
            // the exported artifact is a first-class feed document,
            // importable in this module and loadable in any other.
            let feed_hash = content_hash(&bytes);
            feeds()
                .lock()
                .expect("feed registry lock")
                .insert(feed_hash.clone(), Arc::new(feed));
            Ok(Response::Export { feed_hash, summary: counts, zip_b64: transit_core::write::base64_encode(&bytes) })
        }
        Request::TripMap { network_hash } => {
            let network = network_arc(&network_hash)?;
            Ok(Response::TripMap { map: transit_codec::trip_map(&network) })
        }
    }
}

/// The JSON seam: `{"op": ...}` in, `{"ok": ...}` / `{"error": ...}`
/// out — byte-compatible with the monolith and every other
/// capability module.
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

/// The bulk-bytes seam: op 0 loads a feed.
pub fn dispatch_bytes(op: u32, bytes: &[u8]) -> String {
    let outcome = match op {
        OP_LOAD_FEED => match load_feed(bytes) {
            Ok(response) => serde_json::json!({ "ok": response }),
            Err(error) => serde_json::json!({ "error": error }),
        },
        cap_abi::OP_LOAD_NETWORK => {
            return cap_abi::outcome(
                cap_abi::parse_network(bytes).and_then(|network| dispatch(Request::LoadNetwork { network })),
            )
        }
        other => serde_json::json!({ "error": format!("unknown bulk op {other}") }),
    };
    serde_json::to_string(&outcome).expect("dispatch outcomes serialize")
}

cap_abi::capability! {
    json: dispatch_json,
    bytes: dispatch_bytes,
}
