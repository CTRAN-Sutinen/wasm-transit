//! cap-router — the trip-planning capability module.
//!
//! A PURE-FUNCTION capability in the capability-bus PoC: it holds no
//! domain state of its own beyond a feed registry keyed by content
//! hash (the same document currency the gtfs capability produces),
//! builds the RAPTOR network lazily per service date, and answers
//! earliest-arrival queries. It cannot author anything; it cannot
//! even list departures — a rider whose bundle stops at "know when
//! the bus comes" ships gtfs and never downloads this module's
//! planning code, and a planner whose bundle adds it pays for it
//! once, in a separately cached, separately hashed module.
//!
//! Composition happens in the host: JS loads the same feed bytes
//! here and into cap-gtfs, asserts the returned content hashes are
//! equal, and the two capabilities agree on the document's identity
//! without ever sharing memory. The ABI is the monolith's flat C
//! ABI (via `cap_abi::capability!`); TS types land in
//! `types/cap_router/`.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use ts_rs::TS;
use transit_core::FeedSummary;

/// The bulk-bytes op code.
pub const OP_LOAD_FEED: u32 = 0;

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "cap_router/")]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Request {
    /// Parse a GTFS zip from raw bytes; registers the feed under its
    /// content hash (idempotent — same bytes, same hash).
    LoadFeed { bytes: Vec<u8> },
    /// Earliest-arrival plan from `from` to `to`, leaving at
    /// `depart_secs` on `date`. `None` in the response means
    /// unreachable within `max_legs`.
    Plan {
        feed_hash: String,
        from: String,
        to: String,
        date: String,
        depart_secs: u32,
        max_legs: u32,
    },
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "cap_router/")]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Response {
    LoadFeed { feed_hash: String, summary: FeedSummary },
    Plan { itinerary: Option<transit_router::Itinerary> },
}

/// One registry entry: the parsed feed plus RAPTOR networks built
/// lazily per service date (the router graph is date-dependent —
/// active services decide which trips exist).
struct RouterEntry {
    feed: Arc<transit_core::Gtfs>,
    networks: HashMap<NaiveDate, Arc<transit_router::RouterNetwork>>,
}

fn registry() -> &'static Mutex<HashMap<String, RouterEntry>> {
    static REGISTRY: OnceLock<Mutex<HashMap<String, RouterEntry>>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

/// "sha256:<hex>" — the same content-hash format as the gtfs
/// capability and the manifest.
fn content_hash(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    format!("sha256:{:x}", digest)
}

fn parse_date(date: &str) -> Result<NaiveDate, String> {
    NaiveDate::parse_from_str(date, "%Y-%m-%d")
        .map_err(|e| format!("invalid date {date:?}: {e}"))
}

fn load_feed(bytes: &[u8]) -> Result<Response, String> {
    let hash = content_hash(bytes);
    {
        let guard = registry().lock().expect("registry lock");
        if let Some(entry) = guard.get(&hash) {
            return Ok(Response::LoadFeed {
                feed_hash: hash,
                summary: entry.feed.summary(),
            });
        }
    }
    let feed =
        transit_core::Gtfs::from_zip_bytes(bytes).map_err(|e| format!("parse: {e}"))?;
    let summary = feed.summary();
    registry().lock().expect("registry lock").insert(
        hash.clone(),
        RouterEntry { feed: Arc::new(feed), networks: HashMap::new() },
    );
    Ok(Response::LoadFeed { feed_hash: hash, summary })
}

/// The network for (feed, date), built on first use and cached.
fn network(feed_hash: &str, date: NaiveDate) -> Result<Arc<transit_router::RouterNetwork>, String> {
    let mut guard = registry().lock().expect("registry lock");
    let entry = guard
        .get_mut(feed_hash)
        .ok_or_else(|| format!("unknown feed {feed_hash}: load_feed first"))?;
    let net = entry
        .networks
        .entry(date)
        .or_insert_with(|| Arc::new(transit_router::RouterNetwork::build(&entry.feed, date)))
        .clone();
    Ok(net)
}

/// The typed dispatch (native hosts call this directly).
pub fn dispatch(request: Request) -> Result<Response, String> {
    match request {
        Request::LoadFeed { bytes } => load_feed(&bytes),
        Request::Plan { feed_hash, from, to, date, depart_secs, max_legs } => {
            let parsed = parse_date(&date)?;
            let net = network(&feed_hash, parsed)?;
            let legs = max_legs.max(1) as usize;
            let itinerary = transit_router::earliest_arrival(&net, &from, &to, depart_secs, legs)
                .map_err(|e| e.to_string())?;
            Ok(Response::Plan { itinerary })
        }
    }
}

/// The JSON seam: `{"op": ...}` in, `{"ok": ...}` /
/// `{"error": ...}` out — byte-compatible with the monolith and with
/// every other capability module.
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

/// The bulk-bytes seam: op 0 loads a feed zip.
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
