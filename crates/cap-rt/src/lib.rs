//! cap-rt — the realtime capability module.
//!
//! The third capability extracted from the MONOLITH (after analysis
//! and the network codec): decode a GTFS-rt protobuf payload into
//! the overview view (delays, vehicles) a dispatcher or rider reads.
//! The domain code lives in the shared `transit-rt` library — the
//! monolith keeps no copy; every host (web, runner, a future server
//! leg) decodes through the same bytes.
//!
//! It also owns the JOIN the dispatcher actually asks for: load the
//! same feed document cap-gtfs serves (content hash, asserted at the
//! host), and `trip_progress` / `late_trips` answer one trip's
//! predicted times and the ranked "what's late now" view — the
//! static schedule and the live delays in one response, so no host
//! has to hand-join them (the founding sessions did, twice).
//!
//! The rt payload is EPHEMERAL: snapshots are polled every few
//! seconds, never persisted; decode registers each snapshot under
//! its content hash (idempotent — the same bytes are the same
//! snapshot) so later ops can address it. The capability writes
//! nothing.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use serde::{Deserialize, Serialize};
use ts_rs::TS;
use transit_codec::TripIdMap;
use transit_live::{LiveDay, LiveSummary, OnTimeWindow};
use transit_rt::{LateTrip, RtOverview, TripProgress};

/// The bulk-bytes op codes: 0 decodes an rt payload (the runner's
/// `--load` default), 1 loads a GTFS zip (the same feed document
/// identity cap-gtfs returns — the host asserts they agree).
pub const OP_DECODE: u32 = 0;
pub const OP_LOAD_FEED: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "cap_rt/")]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Request {
    /// Decode a GTFS-rt protobuf payload (the bytes an agency serves,
    /// typically polled every 15–30 seconds) into the overview, and
    /// register the snapshot under its content hash (idempotent —
    /// the same bytes are the same snapshot) so `trip_progress` /
    /// `late_trips` can address it. Bulk-bytes op 0.
    Decode { bytes: Vec<u8> },
    /// Register the STATIC feed the schedule joins come from — the
    /// same GTFS zip bytes cap-gtfs loads, the same content-hash
    /// identity (assert equality at the host). Bulk-bytes op 1.
    LoadFeed { bytes: Vec<u8> },
    /// One trip's joined board: every scheduled stop with the live
    /// delay and the PREDICTED time — the dispatcher's most-asked
    /// question in one op. The trip must exist in the feed; the rt
    /// snapshot may or may not carry it (stops without updates
    /// answer from the schedule alone).
    TripProgress {
        feed_hash: String,
        rt_hash: String,
        trip_id: String,
    },
    /// The ranked "what's late right now" view: every trip whose
    /// worst stop delay reaches `min_delay_secs`, worst first, each
    /// carrying the vehicle's position when one names it — and the
    /// DELAYED-BUT-DARK flag when none does. `vp_hash` names the
    /// VehiclePositions snapshot agencies serve as a SEPARATE feed
    /// (omitted when the delays snapshot is a combined feed).
    LateTrips {
        feed_hash: String,
        rt_hash: String,
        #[serde(default)]
        vp_hash: Option<String>,
        min_delay_secs: i32,
    },
    /// Register the import's TRIP-ID MAP (cap-network `trip_map`)
    /// under its content hash — the join from agency trip ids to
    /// derived trips that `live` resolves through.
    LoadTripMap { trip_map: TripIdMap },
    /// THE LIVE DOCUMENT: one rt moment resolved into model
    /// vocabulary — delays per derived trip and stop (propagated
    /// downstream), vehicles on their derived trips, every status
    /// classified under `window` (recorded in the document), plus the
    /// summary a status panel reads (vehicles by status, the late
    /// list worst first — late trips when there are no positions).
    /// `rt_hash` names trip updates, `vp_hash` vehicle positions;
    /// a combined feed passes only `rt_hash`. At least one.
    Live {
        feed_hash: String,
        #[serde(default)]
        #[ts(optional)]
        rt_hash: Option<String>,
        #[serde(default)]
        #[ts(optional)]
        vp_hash: Option<String>,
        trip_map_hash: String,
        #[serde(default)]
        #[ts(optional)]
        window: Option<OnTimeWindow>,
        /// Late-list length (default 15).
        #[serde(default)]
        #[ts(optional)]
        late_limit: Option<u32>,
    },
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "cap_rt/")]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Response {
    Decode { overview: RtOverview, rt_hash: String },
    LoadFeed { feed_hash: String, summary: transit_core::FeedSummary },
    TripProgress { progress: Option<TripProgress> },
    LateTrips { trips: Vec<LateTrip> },
    LoadTripMap { trip_map_hash: String },
    Live { live_hash: String, live: LiveDay, summary: LiveSummary },
}

/// Trip-id maps by canonical content hash.
fn trip_maps() -> &'static Mutex<HashMap<String, Arc<TripIdMap>>> {
    static REGISTRY: OnceLock<Mutex<HashMap<String, Arc<TripIdMap>>>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

fn canonical_hash<T: Serialize>(value: &T) -> Result<String, String> {
    let canonical = serde_json::to_string(value).map_err(|e| format!("document does not serialize: {e}"))?;
    Ok(content_hash(canonical.as_bytes()))
}

fn trip_map_arc(hash: &str) -> Result<Arc<TripIdMap>, String> {
    trip_maps()
        .lock()
        .expect("trip map registry lock")
        .get(hash)
        .cloned()
        .ok_or_else(|| format!("unknown trip map {hash}: load_trip_map first"))
}

/// Static feeds by content hash of the zip bytes (the SAME identity
/// cap-gtfs returns — one zip, one truth).
fn feeds() -> &'static Mutex<HashMap<String, Arc<transit_core::Gtfs>>> {
    static REGISTRY: OnceLock<Mutex<HashMap<String, Arc<transit_core::Gtfs>>>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Decoded rt snapshots by content hash of the payload bytes —
/// ephemeral documents, addressable while the session lives.
fn snapshots() -> &'static Mutex<HashMap<String, Arc<RtOverview>>> {
    static REGISTRY: OnceLock<Mutex<HashMap<String, Arc<RtOverview>>>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

fn content_hash(bytes: &[u8]) -> String {
    use sha2::Digest;
    format!("sha256:{:x}", sha2::Sha256::digest(bytes))
}

fn feed_arc(feed_hash: &str) -> Result<Arc<transit_core::Gtfs>, String> {
    feeds()
        .lock()
        .expect("feed registry lock")
        .get(feed_hash)
        .cloned()
        .ok_or_else(|| format!("unknown feed {feed_hash}: load_feed (bulk op 1) first"))
}

fn snapshot_arc(rt_hash: &str) -> Result<Arc<RtOverview>, String> {
    snapshots()
        .lock()
        .expect("snapshot registry lock")
        .get(rt_hash)
        .cloned()
        .ok_or_else(|| format!("unknown rt snapshot {rt_hash}: decode (bulk op 0) first"))
}

fn decode(bytes: &[u8]) -> Result<Response, String> {
    let hash = content_hash(bytes);
    {
        let guard = snapshots().lock().expect("snapshot registry lock");
        if let Some(overview) = guard.get(&hash) {
            return Ok(Response::Decode { overview: (**overview).clone(), rt_hash: hash });
        }
    }
    let overview = transit_rt::decode_overview(bytes)?;
    snapshots()
        .lock()
        .expect("snapshot registry lock")
        .insert(hash.clone(), Arc::new(overview.clone()));
    Ok(Response::Decode { overview, rt_hash: hash })
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
/// are `{"error": ...}`, never panics — a total seam: garbage bytes
/// are an error outcome, not a trap.
pub fn dispatch(request: Request) -> Result<Response, String> {
    match request {
        Request::Decode { bytes } => decode(&bytes),
        Request::LoadFeed { bytes } => load_feed(&bytes),
        Request::TripProgress { feed_hash, rt_hash, trip_id } => {
            let feed = feed_arc(&feed_hash)?;
            let snapshot = snapshot_arc(&rt_hash)?;
            Ok(Response::TripProgress {
                progress: transit_rt::trip_progress(&feed, &snapshot, &trip_id),
            })
        }
        Request::LateTrips { feed_hash, rt_hash, vp_hash, min_delay_secs } => {
            let feed = feed_arc(&feed_hash)?;
            let snapshot = snapshot_arc(&rt_hash)?;
            let positions = match &vp_hash {
                Some(hash) => Some(snapshot_arc(hash)?),
                None => None,
            };
            Ok(Response::LateTrips {
                trips: transit_rt::late_trips(&feed, &snapshot, positions.as_deref(), min_delay_secs),
            })
        }
        Request::LoadTripMap { trip_map } => {
            let hash = canonical_hash(&trip_map)?;
            trip_maps().lock().expect("trip map registry lock").entry(hash.clone()).or_insert_with(|| Arc::new(trip_map));
            Ok(Response::LoadTripMap { trip_map_hash: hash })
        }
        Request::Live { feed_hash, rt_hash, vp_hash, trip_map_hash, window, late_limit } => {
            if rt_hash.is_none() && vp_hash.is_none() {
                return Err("live needs a snapshot: rt_hash (trip updates) and/or vp_hash (vehicle positions)".into());
            }
            let feed = feed_arc(&feed_hash)?;
            let map = trip_map_arc(&trip_map_hash)?;
            let delays = rt_hash.as_deref().map(snapshot_arc).transpose()?;
            let positions = vp_hash.as_deref().map(snapshot_arc).transpose()?;
            let live = transit_rt::live_day(&feed, delays.as_deref(), positions.as_deref(), &map, window.unwrap_or_default());
            let summary = transit_live::summarize(&live, late_limit.unwrap_or(15) as usize);
            Ok(Response::Live { live_hash: canonical_hash(&live)?, live, summary })
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

/// The bulk-bytes seam: op 0 decodes a payload, op 1 loads a feed.
pub fn dispatch_bytes(op: u32, bytes: &[u8]) -> String {
    let outcome = match op {
        OP_DECODE => match decode(bytes) {
            Ok(response) => serde_json::json!({ "ok": response }),
            Err(error) => serde_json::json!({ "error": error }),
        },
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

