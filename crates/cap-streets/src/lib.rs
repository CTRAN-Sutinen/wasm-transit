//! cap-streets — the street-graph capability module.
//!
//! One question: where along the streets? Snap a point to the nearest
//! street, route between two positions along the graph — so a traced
//! detour, or a pattern drawn the same way, is STREET GEOMETRY, never
//! freehand chords — and how long a rider walks between a network's
//! stops along them (the walk table the router plans transfers
//! over). The domain code lives in the shared
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
use transit_network::Network;
use transit_streets::walking::{WalkGraph, WalkTable, MAX_WALK_SECS, WALK_SPEED_MPS};
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
    /// Register the NETWORK document under its content hash — the
    /// same identity every model-side capability answers; the walk
    /// table reads its stops. Decoded by `cap_abi::load_network_json`
    /// on either lane (kernel bulk op 256).
    #[serde(skip_deserializing)]
    LoadNetwork { network: Network },
    /// How long a rider walks between the network's stops along the
    /// streets: the walk table (a document — the router plans
    /// transfers over it) and its hash. Walkable streets are every
    /// class but motorway, ramp and unknown, both ways along one-way
    /// streets; a walk is each stop's walk to its nearest walkable
    /// street plus the street distance, at `speed_mps` (default 1.3),
    /// rounded up to the second, kept within `max_walk_secs` (default
    /// 600).
    Walks {
        streets_hash: String,
        network_hash: String,
        #[serde(default)]
        speed_mps: Option<f64>,
        #[serde(default)]
        max_walk_secs: Option<u32>,
    },
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
    LoadNetwork { network_hash: String },
    /// The walk table and its hash (the router's `load_walks` answers
    /// the same).
    Walks { walks_hash: String, walks: WalkTable },
    Snap { position: Option<StreetPosition> },
    /// `length_m`: the path's length along the graph from `from` to
    /// `to` — null when no path exists.
    Route { points: Vec<StreetPoint>, length_m: Option<f64> },
}

/// A registered streets document and the graphs built over it on
/// first use: the street graph (snap, route) and the walking graph
/// (walks) — a host that only walks never builds the other.
struct Registered {
    streets: Streets,
    graph: OnceLock<StreetNetwork>,
    walking: OnceLock<WalkGraph>,
}

impl Registered {
    fn graph(&self) -> &StreetNetwork {
        self.graph.get_or_init(|| StreetNetwork::build(&self.streets))
    }

    fn walking(&self) -> &WalkGraph {
        self.walking.get_or_init(|| WalkGraph::new(&self.streets))
    }
}

/// Streets documents by their hash.
fn registry() -> &'static Mutex<HashMap<String, Arc<Registered>>> {
    static REGISTRY: OnceLock<Mutex<HashMap<String, Arc<Registered>>>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

/// A network's stops (id, lat, lon) by the network's hash — all the
/// walk table reads of it.
fn networks() -> &'static Mutex<HashMap<String, Arc<Vec<(String, f64, f64)>>>> {
    static REGISTRY: OnceLock<Mutex<HashMap<String, Arc<Vec<(String, f64, f64)>>>>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

fn registered(streets_hash: &str) -> Result<Arc<Registered>, String> {
    registry()
        .lock()
        .expect("streets registry lock")
        .get(streets_hash)
        .cloned()
        .ok_or_else(|| format!("unknown street network {streets_hash}: import or load_streets first"))
}

/// Register a canonical document: its hash, its way count, and its
/// graph's size (idempotent — the same streets are the same graph).
fn register(streets: Streets) -> Result<(String, Streets, u32, u32, u32), String> {
    let hash = cap_abi::canonical_hash(&streets)?;
    let ways = streets.ways.len() as u32;
    let entry = registry()
        .lock()
        .expect("streets registry lock")
        .entry(hash.clone())
        .or_insert_with(|| Arc::new(Registered { streets, graph: OnceLock::new(), walking: OnceLock::new() }))
        .clone();
    // The graph's size, counted off the document — registering builds
    // no graph.
    let segments = entry.streets.ways.iter().map(|w| w.points.len() as u32 - 1).sum();
    let nodes = entry.streets.ways.iter().flat_map(|w| &w.points).collect::<std::collections::HashSet<_>>().len() as u32;
    Ok((hash, entry.streets.clone(), ways, segments, nodes))
}

fn walks(streets_hash: &str, network_hash: &str, speed_mps: Option<f64>, max_walk_secs: Option<u32>) -> Result<Response, String> {
    let speed_mps = speed_mps.unwrap_or(WALK_SPEED_MPS);
    let max_walk_secs = max_walk_secs.unwrap_or(MAX_WALK_SECS);
    if !(speed_mps.is_finite() && speed_mps > 0.0 && speed_mps <= 10.0) {
        return Err(format!("speed_mps {speed_mps}: a walking speed is above 0 and at most 10 m/s"));
    }
    if max_walk_secs > 3_600 {
        return Err(format!("max_walk_secs {max_walk_secs}: a walk is at most an hour"));
    }
    let entry = registered(streets_hash)?;
    let stops = networks()
        .lock()
        .expect("network registry lock")
        .get(network_hash)
        .cloned()
        .ok_or_else(|| format!("unknown network {network_hash}: load_network first"))?;
    let table = WalkTable {
        streets_hash: streets_hash.to_string(),
        network_hash: network_hash.to_string(),
        speed_mps,
        max_walk_secs,
        walks: entry.walking().walks(&stops, speed_mps, max_walk_secs),
    };
    Ok(Response::Walks { walks_hash: cap_abi::canonical_hash(&table)?, walks: table })
}

fn import(bytes: &[u8]) -> Result<Response, String> {
    let streets = transit_streets::geojson::import(bytes).map_err(|e| format!("street layer: {}", e.0))?;
    let (streets_hash, streets, ways, segments, nodes) = register(streets)?;
    Ok(Response::Import { streets_hash, streets, ways, segments, nodes })
}

fn load_streets(streets: Streets) -> Result<Response, String> {
    streets.validate().map_err(|e| format!("streets document: {}", e.0))?;
    let (streets_hash, _, ways, segments, nodes) = register(streets)?;
    Ok(Response::LoadStreets { streets_hash, ways, segments, nodes })
}

/// The typed dispatch (native hosts call this directly). Failures
/// are `Err(msg)`, never panics — a total seam: garbage bytes are
/// an error outcome, not a trap.
pub fn dispatch(request: Request) -> Result<Response, String> {
    match request {
        Request::Import { bytes } => import(&bytes),
        Request::LoadStreets { streets } => load_streets(streets),
        Request::LoadNetwork { network } => {
            let hash = cap_abi::canonical_hash(&network)?;
            let stops = network.stops.iter().map(|(id, s)| (id.clone(), s.lat, s.lon)).collect();
            networks().lock().expect("network registry lock").entry(hash.clone()).or_insert_with(|| Arc::new(stops));
            Ok(Response::LoadNetwork { network_hash: hash })
        }
        Request::Walks { streets_hash, network_hash, speed_mps, max_walk_secs } => {
            walks(&streets_hash, &network_hash, speed_mps, max_walk_secs)
        }
        Request::Snap { streets_hash, lat, lon } => {
            let entry = registered(&streets_hash)?;
            Ok(Response::Snap { position: entry.graph().snap(lat, lon) })
        }
        Request::Route { streets_hash, from, to } => {
            let entry = registered(&streets_hash)?;
            Ok(match entry.graph().route_with_length(from, to) {
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

/// The bulk-bytes seam: op 0 imports a street layer, op 1 registers
/// a streets document's JSON, the kernel's op 256 a network's.
pub fn dispatch_bytes(op: u32, bytes: &[u8]) -> String {
    let result = match op {
        OP_IMPORT => import(bytes),
        OP_LOAD_STREETS => serde_json::from_slice::<Streets>(bytes)
            .map_err(|e| format!("streets document: {e}"))
            .and_then(load_streets),
        cap_abi::OP_LOAD_NETWORK => {
            cap_abi::parse_network(bytes).and_then(|network| dispatch(Request::LoadNetwork { network }))
        }
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
