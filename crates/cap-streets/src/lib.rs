//! cap-streets — the street-graph capability module.
//!
//! One question: where along the streets? Snap a point to the nearest
//! street, route between two positions along the graph — so a traced
//! detour, or a pattern drawn the same way, is STREET GEOMETRY, never
//! freehand chords. The domain code lives in the shared
//! `transit-streets` library (the streets document, its import, the
//! graph).
//!
//! The streets document is the HOST's, like every document. `import`
//! reads a street layer (GeoJSON lines) into the canonical document
//! and answers it with its hash; `load_streets` registers a document
//! the host already holds. THE MODULE IS THE HASH AUTHORITY: the hash
//! is the canonical document's, so the same streets from another file
//! are the same hash. Every query addresses it. The module writes
//! nothing.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use serde::{Deserialize, Serialize};
use ts_rs::TS;
use transit_streets::{StreetNetwork, StreetPoint, StreetPosition, Streets};

/// The bulk-bytes op codes: 0 imports a street layer (GeoJSON), 1
/// registers a streets document (its JSON).
pub const OP_IMPORT: u32 = 0;
pub const OP_LOAD_STREETS: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "cap_streets/")]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Request {
    /// Read a street layer — a GeoJSON FeatureCollection or Feature of
    /// LineString/MultiLineString geometries — into the streets
    /// document, and register it under its hash. Lines that are not
    /// streets (railroads, water) are left out (D-30). Bulk-bytes op 0.
    Import { bytes: Vec<u8> },
    /// Register a streets document the host holds (refused unless
    /// canonical, so it hashes as its import did). Bulk-bytes op 1
    /// takes its JSON.
    LoadStreets { streets: Streets },
    /// The nearest street position to (lat, lon), within the snap
    /// neighborhood — the graph address (segment, parameter) a
    /// trace's waypoint commits to. Null when no street is nearby.
    Snap { streets_hash: String, lat: f64, lon: f64 },
    /// The street geometry between two positions: the shortest path
    /// along the graph, as points ending exactly at `to` and starting
    /// AFTER `from` (a trace chains legs: each leg's start is the
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
    /// The document the host now holds, its hash, and its size.
    Import { streets_hash: String, streets: Streets, ways: u32, segments: u32, nodes: u32 },
    LoadStreets { streets_hash: String, ways: u32, segments: u32, nodes: u32 },
    Snap { position: Option<StreetPosition> },
    /// `length_m`: the path's length along the graph from `from` to
    /// `to` — null when no path exists.
    Route { points: Vec<StreetPoint>, length_m: Option<f64> },
}

/// Street graphs by the hash of their document.
fn graphs() -> &'static Mutex<HashMap<String, Arc<StreetNetwork>>> {
    static REGISTRY: OnceLock<Mutex<HashMap<String, Arc<StreetNetwork>>>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

fn graph_arc(streets_hash: &str) -> Result<Arc<StreetNetwork>, String> {
    graphs()
        .lock()
        .expect("street graph registry lock")
        .get(streets_hash)
        .cloned()
        .ok_or_else(|| format!("unknown street network {streets_hash}: import or load_streets first"))
}

/// Register a canonical document: its hash, its way count, and its
/// graph's size (idempotent — the same streets are the same graph).
fn register(streets: &Streets) -> Result<(String, u32, u32, u32), String> {
    let hash = cap_abi::canonical_hash(streets)?;
    let ways = streets.ways.len() as u32;
    let mut guard = graphs().lock().expect("street graph registry lock");
    let net = guard.entry(hash.clone()).or_insert_with(|| Arc::new(StreetNetwork::build(streets)));
    Ok((hash, ways, net.segment_count(), net.node_count()))
}

fn import(bytes: &[u8]) -> Result<Response, String> {
    let streets = transit_streets::geojson::import(bytes).map_err(|e| format!("street layer: {}", e.0))?;
    let (streets_hash, ways, segments, nodes) = register(&streets)?;
    Ok(Response::Import { streets_hash, streets, ways, segments, nodes })
}

fn load_streets(streets: Streets) -> Result<Response, String> {
    streets.validate().map_err(|e| format!("streets document: {}", e.0))?;
    let (streets_hash, ways, segments, nodes) = register(&streets)?;
    Ok(Response::LoadStreets { streets_hash, ways, segments, nodes })
}

/// The typed dispatch (native hosts call this directly). Failures
/// are `Err(msg)`, never panics — a total seam: garbage bytes are
/// an error outcome, not a trap.
pub fn dispatch(request: Request) -> Result<Response, String> {
    match request {
        Request::Import { bytes } => import(&bytes),
        Request::LoadStreets { streets } => load_streets(streets),
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

/// The bulk-bytes seam: op 0 imports a street layer, op 1 registers
/// a streets document's JSON.
pub fn dispatch_bytes(op: u32, bytes: &[u8]) -> String {
    let result = match op {
        OP_IMPORT => import(bytes),
        OP_LOAD_STREETS => serde_json::from_slice::<Streets>(bytes)
            .map_err(|e| format!("streets document: {e}"))
            .and_then(load_streets),
        other => Err(format!("unknown bulk op {other}")),
    };
    let outcome = match result {
        Ok(response) => serde_json::json!({ "ok": response }),
        Err(error) => serde_json::json!({ "error": error }),
    };
    serde_json::to_string(&outcome).expect("dispatch outcomes serialize")
}

cap_abi::capability! {
    json: dispatch_json,
    bytes: dispatch_bytes,
}
