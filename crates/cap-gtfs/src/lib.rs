//! cap-gtfs — the GTFS capability module.
//!
//! One subject: the FEED document. Parse a GTFS static zip, hold
//! it in a registry keyed by the CONTENT HASH of the input bytes,
//! and answer the reads only the feed itself carries — its
//! identity (`feed_info.txt`), its stop names (the join hosts
//! otherwise do by hand), and the stop list for pickers. It has
//! no ops that author anything — "only planners write networks,
//! everything else imports" is enforced by the Request enum, not
//! by convention — and no day views: the day answers over the
//! NETWORK document (cap-board, cap-runcut, cap-calendar; the
//! bus's one-answer law — a duplicate feed-side answer is
//! deleted, not kept).
//!
//! The hash-keyed registry is the bus's core seam: capabilities
//! compose through documents addressed by content hash, so the
//! host (JS) can load the same bytes into any set of modules and
//! assert they agree on the identity — "same document, same
//! truth" is a checkable property, not a hope.
//!
//! The ABI is the same flat, import-free C ABI as every
//! capability (via `cap_abi::capability!`): a capability differs
//! in which ops its dispatch answers, never in how it is called.
//! The TS types that cross the boundary are generated into
//! `types/cap_gtfs/`.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use ts_rs::TS;
use transit_core::{FeedSummary, StopSummary};

/// The bulk-bytes op code (the only op that takes raw feed bytes).
pub const OP_LOAD_FEED: u32 = 0;

/// One operation call. Tagged like the monolith's seam: the serde form
/// is `{"op": "<name>", ...fields}`.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "cap_gtfs/")]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Request {
    /// Parse a GTFS zip from raw bytes; registers the feed under its
    /// content hash. Loading the same bytes twice is idempotent —
    /// same hash, same registry entry.
    LoadFeed { bytes: Vec<u8> },
    /// Stop list for UI pickers: the first `limit` stops by id.
    Stops { feed_hash: String, limit: u32 },
    /// Stop names by id — the join hosts otherwise do by hand when
    /// a view (like rt's) carries ids a human reads as names.
    /// Unknown ids are omitted (the caller asked for names, and
    /// silence is the honest answer for an id the feed does not
    /// carry).
    StopsByIds { feed_hash: String, ids: Vec<String> },
    /// `feed_info.txt` metadata (version, publisher) — the identity
    /// question: which feed is this?
    FeedInfo { feed_hash: String },
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "cap_gtfs/")]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Response {
    LoadFeed { feed_hash: String, summary: FeedSummary },
    Stops { stops: Vec<StopSummary> },
    StopsByIds { stops: Vec<StopSummary> },
    FeedInfo { info: Option<transit_core::FeedInfo> },
}

/// The registry: content hash -> parsed feed. One instance per module
/// (wasm modules never share state); the mutex only guards the map.
fn registry() -> &'static Mutex<HashMap<String, Arc<transit_core::Gtfs>>> {
    static REGISTRY: OnceLock<Mutex<HashMap<String, Arc<transit_core::Gtfs>>>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

/// "sha256:<hex>" — the manifest's document-hash format everywhere.
pub fn content_hash(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    format!("sha256:{:x}", digest)
}

fn with_feed<T>(
    feed_hash: &str,
    f: impl FnOnce(&transit_core::Gtfs) -> Result<T, String>,
) -> Result<T, String> {
    let guard = registry().lock().expect("registry lock");
    let feed = guard
        .get(feed_hash)
        .ok_or_else(|| format!("unknown feed {feed_hash}: load_feed first"))?;
    f(&feed)
}

/// Load a feed into the registry; idempotent by content hash.
fn load_feed(bytes: &[u8]) -> Result<Response, String> {
    let hash = content_hash(bytes);
    {
        let guard = registry().lock().expect("registry lock");
        if let Some(feed) = guard.get(&hash) {
            return Ok(Response::LoadFeed {
                feed_hash: hash,
                summary: feed.summary(),
            });
        }
    }
    let feed =
        transit_core::Gtfs::from_zip_bytes(bytes).map_err(|e| format!("parse: {e}"))?;
    let summary = feed.summary();
    registry()
        .lock()
        .expect("registry lock")
        .insert(hash.clone(), Arc::new(feed));
    Ok(Response::LoadFeed { feed_hash: hash, summary })
}

/// The typed dispatch (native hosts call this directly; the ABI shell
/// wraps it in JSON). Failures are `{"error": ...}`, never panics.
pub fn dispatch(request: Request) -> Result<Response, String> {
    match request {
        Request::LoadFeed { bytes } => load_feed(&bytes),
        Request::Stops { feed_hash, limit } => with_feed(&feed_hash, |feed| {
            Ok(Response::Stops {
                stops: feed.stop_summaries().into_iter().take(limit as usize).collect(),
            })
        }),
        Request::StopsByIds { feed_hash, ids } => with_feed(&feed_hash, |feed| {
            let mut out = Vec::with_capacity(ids.len());
            for id in &ids {
                if let Some(stop) = feed.stops.get(id) {
                    out.push(StopSummary {
                        stop_id: stop.stop_id.clone(),
                        name: stop.stop_name.clone(),
                        parent_station: stop.parent_station.clone(),
                    });
                }
            }
            Ok(Response::StopsByIds { stops: out })
        }),
        Request::FeedInfo { feed_hash } => with_feed(&feed_hash, |feed| {
            Ok(Response::FeedInfo { info: feed.feed_info.clone() })
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
