//! cap-streets — the street-graph capability module.
//!
//! The detour editor's need, atomically: a dispatcher traces a detour
//! with waypoints, and the trace must be STREET GEOMETRY — clicks
//! snap to the imported street network, the path between waypoints
//! routes along it. The domain code lives in the shared
//! `transit-streets` library (parse, snap, shortest path); the
//! monolith keeps no copy.
//!
//! The street network is reference data the HOST owns (an OSM
//! extract or an open-data layer as GeoJSON lines, re-imported per
//! session — never persisted). `load_streets` registers the bytes
//! under their content hash (idempotent — the same bytes are the
//! same graph), and every query addresses that hash. The module is
//! pure and stateless otherwise: it writes nothing.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use serde::{Deserialize, Serialize};
use ts_rs::TS;
use transit_streets::{StreetNetwork, StreetPoint, StreetPosition};

/// The bulk-bytes op code: 0 parses a street-network GeoJSON.
pub const OP_LOAD_STREETS: u32 = 0;

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "cap_streets/")]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Request {
    /// Parse a street-network GeoJSON (FeatureCollection or Feature
    /// of LineString/MultiLineString geometries) into the routing
    /// graph and register it under its content hash (idempotent —
    /// the same bytes are the same graph). Bulk-bytes op 0.
    LoadStreets { bytes: Vec<u8> },
    /// The nearest street position to (lat, lon), within the snap
    /// neighborhood — the graph address (segment, parameter) a
    /// detour waypoint commits to. Null when no street is nearby.
    Snap { streets_hash: String, lat: f64, lon: f64 },
    /// The street geometry between two positions: the shortest path
    /// along the graph, as points ending exactly at `to` and starting
    /// AFTER `from` (a detour chains legs: each leg's start is the
    /// previous leg's committed end — prepend `from` for a standalone
    /// trace). Empty when the positions sit on disconnected parts of
    /// the network — the caller falls back to a straight line.
    Route {
        streets_hash: String,
        from: StreetPosition,
        to: StreetPosition,
    },
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "cap_streets/")]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Response {
    LoadStreets { streets_hash: String, segments: u32, nodes: u32 },
    Snap { position: Option<StreetPosition> },
    /// `length_m`: the path's length along the graph from `from` to
    /// `to` — null when no path exists.
    Route { points: Vec<StreetPoint>, length_m: Option<f64> },
}

/// Street graphs by content hash of the GeoJSON bytes — the host
/// keeps the bytes, the module keeps the parsed graph.
fn graphs() -> &'static Mutex<HashMap<String, Arc<StreetNetwork>>> {
    static REGISTRY: OnceLock<Mutex<HashMap<String, Arc<StreetNetwork>>>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

fn content_hash(bytes: &[u8]) -> String {
    use sha2::Digest;
    format!("sha256:{:x}", sha2::Sha256::digest(bytes))
}

fn graph_arc(streets_hash: &str) -> Result<Arc<StreetNetwork>, String> {
    graphs()
        .lock()
        .expect("street graph registry lock")
        .get(streets_hash)
        .cloned()
        .ok_or_else(|| format!("unknown street network {streets_hash}: load_streets (bulk op 0) first"))
}

fn load_streets(bytes: &[u8]) -> Result<Response, String> {
    let hash = content_hash(bytes);
    {
        let guard = graphs().lock().expect("street graph registry lock");
        if let Some(net) = guard.get(&hash) {
            return Ok(Response::LoadStreets {
                streets_hash: hash,
                segments: net.segment_count(),
                nodes: net.node_count(),
            });
        }
    }
    let net = StreetNetwork::parse(bytes)
        .map_err(|e| format!("street network: {}", e.0))?;
    let segments = net.segment_count();
    let nodes = net.node_count();
    graphs()
        .lock()
        .expect("street graph registry lock")
        .insert(hash.clone(), Arc::new(net));
    Ok(Response::LoadStreets { streets_hash: hash, segments, nodes })
}

/// The typed dispatch (native hosts call this directly). Failures
/// are `Err(msg)`, never panics — a total seam: garbage bytes are
/// an error outcome, not a trap.
pub fn dispatch(request: Request) -> Result<Response, String> {
    match request {
        Request::LoadStreets { bytes } => load_streets(&bytes),
        Request::Snap { streets_hash, lat, lon } => {
            let net = graph_arc(&streets_hash)?;
            Ok(Response::Snap { position: net.snap(lat, lon) })
        }
        Request::Route { streets_hash, from, to } => {
            let net = graph_arc(&streets_hash)?;
            Ok(match net.route_with_length(from, to) {
                Some((points, length)) => Response::Route { points, length_m: Some(length) },
                None => Response::Route { points: vec![], length_m: None },
            })
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

/// The bulk-bytes seam: op 0 parses a street-network GeoJSON.
pub fn dispatch_bytes(op: u32, bytes: &[u8]) -> String {
    let outcome = match op {
        OP_LOAD_STREETS => match load_streets(bytes) {
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
