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
//! NETWORK document (the kernel's load_network, the same identity
//! every network consumer holds) and, for an imported network, its
//! `Boundary` (the published stop_sequence numbering rt updates
//! name), and `trip_progress` / `late_trips` / `live` answer over the
//! network's own trips — an rt trip id IS a model trip id.
//!
//! The rt payload is EPHEMERAL: snapshots are polled every few
//! seconds, never persisted; decode registers each snapshot under
//! its content hash (idempotent — the same bytes are the same
//! snapshot) so later ops can address it. The capability writes
//! nothing.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use serde::{Deserialize, Serialize};
use transit_codec::Boundary;
use transit_live::{LiveDay, LiveSummary, OnTimeWindow};
use transit_network::Network;
use transit_rt::{LateTrip, RtOverview, TripProgress};
use ts_rs::TS;

/// The bulk-bytes op code that decodes an rt payload (the runner's
/// `--load` default); the network arrives on the kernel's
/// load_network op.
pub const OP_DECODE: u32 = 0;

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "cap_rt/")]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Request {
    /// Decode a GTFS-rt protobuf payload (the bytes an agency serves,
    /// typically polled every 15–30 seconds) into the overview, and
    /// register the snapshot under its content hash (idempotent —
    /// the same bytes are the same snapshot) so the joins can
    /// address it. Bulk-bytes op 0.
    Decode { bytes: Vec<u8> },
    /// Register the NETWORK document under its content hash — the
    /// same identity every network consumer holds. Decoded by
    /// `cap_abi::load_network_json` on either lane.
    #[serde(skip_deserializing)]
    LoadNetwork { network: Network },
    /// Register an import's BOUNDARY (cap-network `import`'s
    /// `boundary`) under its content hash: the stop_sequence numbering
    /// the published feed — and so its rt updates — use. A network
    /// published from its own export needs none (1..n).
    LoadBoundary { boundary: Boundary },
    /// One trip's joined board: every scheduled stop of the instance
    /// the snapshot's update names, with the live delay and the
    /// PREDICTED time. Null when the snapshot has no update for the
    /// trip or the update names no instance of it.
    TripProgress {
        network_hash: String,
        #[serde(default)]
        #[ts(optional)]
        boundary_hash: Option<String>,
        rt_hash: String,
        trip_id: String,
    },
    /// The ranked "what's late right now" view: every trip whose
    /// worst stop delay reaches `min_delay_secs`, worst first, each
    /// carrying the vehicle's position when one names it (or another
    /// trip of its block) — and the DELAYED-BUT-DARK flag when none
    /// does. `vp_hash` names the VehiclePositions snapshot agencies
    /// serve as a SEPARATE feed (omitted for a combined feed).
    LateTrips {
        network_hash: String,
        rt_hash: String,
        #[serde(default)]
        vp_hash: Option<String>,
        min_delay_secs: i32,
    },
    /// THE LIVE DOCUMENT: one rt moment resolved into model
    /// vocabulary — delays per trip instance (trip, start) and stop
    /// (propagated downstream), vehicles on their trip instances,
    /// every status classified under `window` (recorded in the
    /// document), plus the summary a status panel reads (vehicles by
    /// status, the late list worst first — late trips when there are
    /// no positions), and the SERVICE ALERTS in force at the snapshot's
    /// moment, their informed entities resolved into routes, stops, and
    /// trip instances. `rt_hash` names trip updates, `vp_hash` vehicle
    /// positions, `alerts_hash` service alerts (alerts are gathered from
    /// every snapshot given); a combined feed passes only `rt_hash`. At
    /// least one.
    Live {
        network_hash: String,
        #[serde(default)]
        #[ts(optional)]
        boundary_hash: Option<String>,
        #[serde(default)]
        #[ts(optional)]
        rt_hash: Option<String>,
        #[serde(default)]
        #[ts(optional)]
        vp_hash: Option<String>,
        #[serde(default)]
        #[ts(optional)]
        alerts_hash: Option<String>,
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
    LoadNetwork { network_hash: String },
    LoadBoundary { boundary_hash: String },
    TripProgress { progress: Option<TripProgress> },
    LateTrips { trips: Vec<LateTrip> },
    Live { live_hash: String, live: LiveDay, summary: LiveSummary },
}

fn registry<T>() -> Mutex<HashMap<String, Arc<T>>> {
    Mutex::new(HashMap::new())
}

/// Networks by content hash (canonical serialization).
fn networks() -> &'static Mutex<HashMap<String, Arc<Network>>> {
    static REGISTRY: OnceLock<Mutex<HashMap<String, Arc<Network>>>> = OnceLock::new();
    REGISTRY.get_or_init(registry)
}

/// Boundaries by content hash (canonical serialization).
fn boundaries() -> &'static Mutex<HashMap<String, Arc<Boundary>>> {
    static REGISTRY: OnceLock<Mutex<HashMap<String, Arc<Boundary>>>> = OnceLock::new();
    REGISTRY.get_or_init(registry)
}

/// Decoded rt snapshots by content hash of the payload bytes —
/// ephemeral documents, addressable while the session lives.
fn snapshots() -> &'static Mutex<HashMap<String, Arc<RtOverview>>> {
    static REGISTRY: OnceLock<Mutex<HashMap<String, Arc<RtOverview>>>> = OnceLock::new();
    REGISTRY.get_or_init(registry)
}

fn content_hash(bytes: &[u8]) -> String {
    use sha2::Digest;
    format!("sha256:{:x}", sha2::Sha256::digest(bytes))
}

fn lookup<T>(registry: &Mutex<HashMap<String, Arc<T>>>, hash: &str, what: &str, how: &str) -> Result<Arc<T>, String> {
    registry
        .lock()
        .expect("registry lock")
        .get(hash)
        .cloned()
        .ok_or_else(|| format!("unknown {what} {hash}: {how} first"))
}

fn network_arc(hash: &str) -> Result<Arc<Network>, String> {
    lookup(networks(), hash, "network", "load_network")
}

/// The boundary a join pairs rt sequences by: the registered one, or
/// 1..n numbering.
fn boundary_arc(hash: Option<&str>) -> Result<Arc<Boundary>, String> {
    match hash {
        Some(hash) => lookup(boundaries(), hash, "boundary", "load_boundary"),
        None => Ok(Arc::new(Boundary::default())),
    }
}

fn snapshot_arc(rt_hash: &str) -> Result<Arc<RtOverview>, String> {
    lookup(snapshots(), rt_hash, "rt snapshot", "decode (bulk op 0)")
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

/// The typed dispatch (native hosts call this directly). Failures
/// are `{"error": ...}`, never panics — a total seam: garbage bytes
/// are an error outcome, not a trap.
pub fn dispatch(request: Request) -> Result<Response, String> {
    match request {
        Request::Decode { bytes } => decode(&bytes),
        Request::LoadNetwork { network } => {
            let hash = cap_abi::canonical_hash(&network)?;
            networks().lock().expect("network registry lock").entry(hash.clone()).or_insert_with(|| Arc::new(network));
            Ok(Response::LoadNetwork { network_hash: hash })
        }
        Request::LoadBoundary { boundary } => {
            let hash = cap_abi::canonical_hash(&boundary)?;
            boundaries().lock().expect("boundary registry lock").entry(hash.clone()).or_insert_with(|| Arc::new(boundary));
            Ok(Response::LoadBoundary { boundary_hash: hash })
        }
        Request::TripProgress { network_hash, boundary_hash, rt_hash, trip_id } => {
            let network = network_arc(&network_hash)?;
            let boundary = boundary_arc(boundary_hash.as_deref())?;
            let snapshot = snapshot_arc(&rt_hash)?;
            Ok(Response::TripProgress { progress: transit_rt::trip_progress(&network, &boundary, &snapshot, &trip_id) })
        }
        Request::LateTrips { network_hash, rt_hash, vp_hash, min_delay_secs } => {
            let network = network_arc(&network_hash)?;
            let snapshot = snapshot_arc(&rt_hash)?;
            let positions = vp_hash.as_deref().map(snapshot_arc).transpose()?;
            Ok(Response::LateTrips {
                trips: transit_rt::late_trips(&network, &snapshot, positions.as_deref(), min_delay_secs),
            })
        }
        Request::Live { network_hash, boundary_hash, rt_hash, vp_hash, alerts_hash, window, late_limit } => {
            if rt_hash.is_none() && vp_hash.is_none() && alerts_hash.is_none() {
                return Err(
                    "live needs a snapshot: rt_hash (trip updates), vp_hash (vehicle positions), and/or alerts_hash (service alerts)".into(),
                );
            }
            let network = network_arc(&network_hash)?;
            let boundary = boundary_arc(boundary_hash.as_deref())?;
            let delays = rt_hash.as_deref().map(snapshot_arc).transpose()?;
            let positions = vp_hash.as_deref().map(snapshot_arc).transpose()?;
            let alerts = alerts_hash.as_deref().map(snapshot_arc).transpose()?;
            let live = transit_rt::live_day(
                &network,
                &boundary,
                delays.as_deref(),
                positions.as_deref(),
                alerts.as_deref(),
                window.unwrap_or_default(),
            );
            let summary = transit_live::summarize(&live, late_limit.unwrap_or(15) as usize);
            Ok(Response::Live { live_hash: cap_abi::canonical_hash(&live)?, live, summary })
        }
    }
}

/// The JSON seam: `{"op": ...}` in, `{"ok": ...}` / `{"error": ...}`
/// out — byte-compatible with every other capability module.
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

/// The bulk-bytes seam: op 0 decodes a payload; the kernel's
/// load_network registers a network.
pub fn dispatch_bytes(op: u32, bytes: &[u8]) -> String {
    match op {
        OP_DECODE => cap_abi::outcome(decode(bytes)),
        cap_abi::OP_LOAD_NETWORK => cap_abi::outcome(
            cap_abi::parse_network(bytes).and_then(|network| dispatch(Request::LoadNetwork { network })),
        ),
        other => serde_json::json!({ "error": format!("unknown bulk op {other}") }).to_string(),
    }
}

cap_abi::capability! {
    json: dispatch_json,
    bytes: dispatch_bytes,
}

