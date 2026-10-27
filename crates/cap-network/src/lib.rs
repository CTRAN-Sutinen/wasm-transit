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
use sha2::{Digest, Sha256};
use ts_rs::TS;
use transit_codec::{gtfs_to_network, network_to_gtfs, ImportNote, ImportOutcome, TripIdMap};
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
    /// The planner's bootstrap: infer an authored network document
    /// from a loaded feed. Deliberately lossy, deterministic, and
    /// every resolution is reported in the notes — never silent. The
    /// inferred document registers itself (valid by construction —
    /// the codec's law), and the response carries its hash so
    /// validate/export can chain without a reload.
    Import { feed_hash: String },
    /// Register an authored network document under its content hash
    /// (idempotent by hash). The authoring UI's entry point; every
    /// later op addresses the document by the returned hash.
    LoadNetwork { network: Network },
    /// Every issue of a registered document — the authoring UI's
    /// feedback loop, the whole list, never thrown.
    Validate { network_hash: String },
    /// The publish path: materialize a registered document into a
    /// GTFS zip (deterministic bytes). Returns the zip (base64) under
    /// the content hash of those bytes — the same identity any other
    /// module returns for the same bytes, so the host can assert the
    /// handoff. Invalid documents are refused (validate first).
    Export { network_hash: String },
    /// The import's TRIP-ID MAP for a network: the derived trips
    /// ((schedule, start) — the model's identity) joined to the
    /// agency trip ids they came from. The join rt data and the
    /// source feed's ops are keyed by. `None` for a network that was
    /// not inferred from a feed (an authored document has no agency
    /// ids — the honest answer, never a silent empty map).
    TripMap { network_hash: String },
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "cap_network/")]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Response {
    LoadFeed { feed_hash: String, summary: FeedSummary },
    Import { network: Network, notes: Vec<ImportNote>, network_hash: String },
    LoadNetwork { network_hash: String },
    /// `issues` gate (invalid documents never export); `warnings`
    /// never gate — they name what validity lets through (trips with
    /// no vehicle, a pick with no service in its period).
    Validate { issues: Vec<transit_network::NetworkError>, warnings: Vec<transit_network::NetworkWarning> },
    Export { feed_hash: String, summary: FeedSummary, zip_b64: String },
    TripMap { map: Option<TripIdMap> },
}

/// Feeds by content hash of the zip bytes.
fn feeds() -> &'static Mutex<HashMap<String, Arc<transit_core::Gtfs>>> {
    static REGISTRY: OnceLock<Mutex<HashMap<String, Arc<transit_core::Gtfs>>>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Authored network documents by content hash of their canonical
/// serialization — the module is the hash authority.
fn networks() -> &'static Mutex<HashMap<String, Arc<Network>>> {
    static REGISTRY: OnceLock<Mutex<HashMap<String, Arc<Network>>>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

/// The import's trip-id maps, keyed by the network they belong to —
/// captured at import (the agency ids exist only in the feed; an
/// authored document has none).
fn trip_maps() -> &'static Mutex<HashMap<String, Arc<TripIdMap>>> {
    static REGISTRY: OnceLock<Mutex<HashMap<String, Arc<TripIdMap>>>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

fn content_hash(bytes: &[u8]) -> String {
    format!("sha256:{:x}", Sha256::digest(bytes))
}

fn network_hash(network: &Network) -> Result<String, String> {
    let canonical =
        serde_json::to_string(network).map_err(|e| format!("network does not serialize: {e}"))?;
    Ok(content_hash(canonical.as_bytes()))
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
            return Ok(Response::LoadFeed { feed_hash: hash, summary: feed.summary() });
        }
    }
    let gtfs = transit_core::Gtfs::from_zip_bytes(bytes)
        .map_err(|e| format!("feed does not parse: {e}"))?;
    let summary = gtfs.summary();
    feeds()
        .lock()
        .expect("feed registry lock")
        .insert(hash.clone(), Arc::new(gtfs));
    Ok(Response::LoadFeed { feed_hash: hash, summary })
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
            let ImportOutcome { network, notes, trip_map } = gtfs_to_network(&feed);
            // The inferred document registers itself: import's
            // answer is immediately addressable for validate and
            // export (valid by construction — the codec's law), and
            // its trip-id map rides along under the same hash.
            let network_hash = register_network(network.clone())?;
            trip_maps()
                .lock()
                .expect("trip-map registry lock")
                .insert(network_hash.clone(), Arc::new(trip_map));
            Ok(Response::Import { network, notes, network_hash })
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
            let gtfs = network_to_gtfs(&network)?;
            let summary = gtfs.summary();
            let bytes = transit_core::write::to_zip_bytes(&gtfs);
            // The produced feed registers under its own bytes' hash:
            // the exported artifact is a first-class feed document,
            // importable in this module and loadable in any other.
            let feed_hash = content_hash(&bytes);
            feeds()
                .lock()
                .expect("feed registry lock")
                .insert(feed_hash.clone(), Arc::new(gtfs));
            Ok(Response::Export { feed_hash, summary, zip_b64: transit_core::write::base64_encode(&bytes) })
        }
        Request::TripMap { network_hash } => {
            // Unknown network is a caller error; a known network
            // without a map (authored, or loaded directly) answers
            // None — the honest boundary answer.
            network_arc(&network_hash)?;
            let map = trip_maps()
                .lock()
                .expect("trip-map registry lock")
                .get(&network_hash)
                .cloned();
            Ok(Response::TripMap { map: map.map(|m| (*m).clone()) })
        }
    }
}

/// The JSON seam: `{"op": ...}` in, `{"ok": ...}` / `{"error": ...}`
/// out — byte-compatible with the monolith and every other
/// capability module.
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

/// The bulk-bytes seam: op 0 loads a feed.
pub fn dispatch_bytes(op: u32, bytes: &[u8]) -> String {
    let outcome = match op {
        OP_LOAD_FEED => match load_feed(bytes) {
            Ok(response) => serde_json::json!({ "ok": response }),
            Err(error) => serde_json::json!({ "error": error }),
        },
        other => serde_json::json!({ "error": format!("unknown bulk op {other}") }),
    };
    serde_json::to_string(&outcome).expect("dispatch outcomes serialize")
}

cap_abi::capability! {
    json: dispatch_json,
    bytes: dispatch_bytes,
}
