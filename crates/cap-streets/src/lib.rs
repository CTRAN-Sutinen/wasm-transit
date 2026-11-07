//! cap-streets — the street-graph capability module.
//!
//! One question: where along the streets? Snap a point to the nearest
//! street, route between two positions along the graph — so a traced
//! detour, or a pattern drawn the same way, is STREET GEOMETRY, never
//! freehand chords — and how long a rider walks along them: between
//! a network's stops (the walk table the router plans transfers
//! over), and from any point to the stops near it (the ends of a trip
//! from anywhere). A route names the streets it follows, and the bus
//! profile's streets to confirm (D-26). The domain code lives in the shared
//! `transit-streets` library (the streets document, its import, the
//! graph).
//!
//! The streets document is the HOST's, like every document. `import`
//! reads a street layer (GeoJSON lines) into the canonical document,
//! registers it, and answers it with its hash; the kernel's
//! `load_streets` registers a document the host already holds. THE
//! MODULE IS THE HASH AUTHORITY: the hash is the canonical document's,
//! so the same streets from another file are the same hash. Every
//! query addresses it. The graphs over a document (the street graph
//! for snap and route, the walking graph for walks) are derived
//! entries, built on first use (`cap_abi::kernel::STREET_GRAPH`,
//! `WALK_GRAPH`). The module writes nothing.

#![forbid(unsafe_code)]

use std::sync::Arc;

use cap_abi::kernel::{NETWORK, STREETS, STREET_GRAPH, WALK_GRAPH};
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use transit_streets::walking::{DirectWalk, PointWalk, WalkGraph, WalkTable, MAX_WALK_SECS, WALK_SPEED_MPS};
use transit_streets::{StreetClass, StreetNetwork, StreetPoint, StreetPosition, StreetUse, Streets};

/// The bulk-bytes op code that imports a street layer (GeoJSON); a
/// streets document registers on the kernel's `load_streets`.
pub const OP_IMPORT: u32 = 0;

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "cap_streets/")]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Request {
    /// Read a street layer — a GeoJSON FeatureCollection or Feature of
    /// LineString/MultiLineString geometries — into the streets
    /// document, and register it under its hash. Lines that are not
    /// streets (railroads, water) are left out (D-30). Bulk-bytes op 0.
    Import { bytes: Vec<u8> },
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
    /// The walks from a point (a tapped spot, the rider's location) to
    /// the network's stops within the limit, as the walk table walks —
    /// a stop at the point would have these walks — each with the path
    /// the rider walks, nearest first; and, when `to` is given, the
    /// walk from the point straight to it. The router plans a trip
    /// from the point over them (a place's walks). Nothing when no
    /// walkable street is near the point.
    Access {
        streets_hash: String,
        network_hash: String,
        point: StreetPoint,
        #[serde(default)]
        to: Option<StreetPoint>,
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
    /// the network — the caller falls back to a straight line. The
    /// answer names the streets the path follows, and — the bus
    /// profile (D-26): a bus is drawn down any street, never refused —
    /// the ones of a class in `confirm` for the planner to confirm
    /// (default: unknown, path, steps, track).
    Route {
        streets_hash: String,
        from: StreetPosition,
        to: StreetPosition,
        #[serde(default)]
        #[ts(optional)]
        confirm: Option<Vec<StreetClass>>,
    },
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "cap_streets/")]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Response {
    /// The document the host now holds, its hash, and its size.
    Import { streets_hash: String, streets: Streets, ways: u32, segments: u32, nodes: u32 },
    /// The walk table and its hash (the router's `load_walks` answers
    /// the same).
    Walks { walks_hash: String, walks: WalkTable },
    /// `direct`: the walk to `to` (null when none was asked, or none
    /// is within the limit).
    Access { walks: Vec<PointWalk>, direct: Option<DirectWalk> },
    Snap { position: Option<StreetPosition> },
    /// `length_m`: the path's length along the graph from `from` to
    /// `to` — null when no path exists. `streets`: the streets the path
    /// follows, in order (class, name, meters along it); `to_confirm`:
    /// those of them whose class the profile asks the planner to
    /// confirm — drawn all the same.
    Route { points: Vec<StreetPoint>, length_m: Option<f64>, streets: Vec<StreetUse>, to_confirm: Vec<StreetUse> },
}

/// The street graph (snap, route) over a registered streets document,
/// built on first use and held as a derived entry.
fn graph(streets_hash: &str) -> Result<Arc<StreetNetwork>, String> {
    let streets = STREETS.require(streets_hash)?;
    cap_abi::kernel::derive(&STREET_GRAPH, streets_hash, &[streets_hash], || Ok(StreetNetwork::build(&streets)))
}

/// The walking graph over a registered streets document, built on
/// first use and held as a derived entry — a host that only walks
/// never builds the street graph.
fn walking(streets_hash: &str) -> Result<Arc<WalkGraph>, String> {
    let streets = STREETS.require(streets_hash)?;
    cap_abi::kernel::derive(&WALK_GRAPH, streets_hash, &[streets_hash], || Ok(WalkGraph::new(&streets)))
}

/// The walking speed and limit, defaulted and checked.
fn pace(speed_mps: Option<f64>, max_walk_secs: Option<u32>) -> Result<(f64, u32), String> {
    let speed_mps = speed_mps.unwrap_or(WALK_SPEED_MPS);
    let max_walk_secs = max_walk_secs.unwrap_or(MAX_WALK_SECS);
    if !(speed_mps.is_finite() && speed_mps > 0.0 && speed_mps <= 10.0) {
        return Err(format!("speed_mps {speed_mps}: a walking speed is above 0 and at most 10 m/s"));
    }
    if max_walk_secs > 3_600 {
        return Err(format!("max_walk_secs {max_walk_secs}: a walk is at most an hour"));
    }
    Ok((speed_mps, max_walk_secs))
}

/// A registered network's stops (id, lat, lon) — all a walk reads of it.
fn network_stops(network_hash: &str) -> Result<Vec<(String, f64, f64)>, String> {
    Ok(NETWORK.require(network_hash)?.stops.iter().map(|(id, s)| (id.clone(), s.lat, s.lon)).collect())
}

fn walks(streets_hash: &str, network_hash: &str, speed_mps: Option<f64>, max_walk_secs: Option<u32>) -> Result<Response, String> {
    let (speed_mps, max_walk_secs) = pace(speed_mps, max_walk_secs)?;
    let walking = walking(streets_hash)?;
    let stops = network_stops(network_hash)?;
    let table = WalkTable { kind: Default::default(), schema_version: Default::default(),
        streets_hash: streets_hash.to_string(),
        network_hash: network_hash.to_string(),
        speed_mps,
        max_walk_secs,
        walks: walking.walks(&stops, speed_mps, max_walk_secs),
    };
    Ok(Response::Walks { walks_hash: cap_abi::canonical_hash(&table)?, walks: table })
}

fn access(
    streets_hash: &str,
    network_hash: &str,
    point: StreetPoint,
    to: Option<StreetPoint>,
    speed_mps: Option<f64>,
    max_walk_secs: Option<u32>,
) -> Result<Response, String> {
    let (speed_mps, max_walk_secs) = pace(speed_mps, max_walk_secs)?;
    for p in std::iter::once(&point).chain(&to) {
        if !(p.lat.is_finite() && p.lon.is_finite() && p.lat.abs() <= 90.0 && p.lon.abs() <= 180.0) {
            return Err(format!("point ({}, {}) is not on the globe", p.lat, p.lon));
        }
    }
    let walking = walking(streets_hash)?;
    let stops = network_stops(network_hash)?;
    let walks = walking.walks_from(point.lat, point.lon, &stops, speed_mps, max_walk_secs);
    let direct = to.and_then(|to| {
        let target = [(String::new(), to.lat, to.lon)];
        let walk = walking.walks_from(point.lat, point.lon, &target, speed_mps, max_walk_secs).pop()?;
        Some(DirectWalk { walk_secs: walk.walk_secs, path: walk.path })
    });
    Ok(Response::Access { walks, direct })
}

fn import(bytes: &[u8]) -> Result<Response, String> {
    let streets = transit_streets::geojson::import(bytes).map_err(|e| format!("street layer: {}", e.0))?;
    // The document registers where it was minted: every query (and,
    // in an assembly, every capability) addresses it by hash.
    let (ways, segments, nodes) = streets.counts();
    let (streets_hash, streets) = STREETS.register(streets)?;
    Ok(Response::Import { streets_hash, streets: (*streets).clone(), ways, segments, nodes })
}

/// The typed dispatch (native hosts call this directly). Failures
/// are `Err(msg)`, never panics — a total seam: garbage bytes are
/// an error outcome, not a trap.
pub fn dispatch(request: Request) -> Result<Response, String> {
    match request {
        Request::Import { bytes } => import(&bytes),
        Request::Walks { streets_hash, network_hash, speed_mps, max_walk_secs } => {
            walks(&streets_hash, &network_hash, speed_mps, max_walk_secs)
        }
        Request::Access { streets_hash, network_hash, point, to, speed_mps, max_walk_secs } => {
            access(&streets_hash, &network_hash, point, to, speed_mps, max_walk_secs)
        }
        Request::Snap { streets_hash, lat, lon } => {
            Ok(Response::Snap { position: graph(&streets_hash)?.snap(lat, lon) })
        }
        Request::Route { streets_hash, from, to, confirm } => {
            let confirm = confirm.unwrap_or_else(|| transit_streets::bus::CONFIRM.to_vec());
            Ok(match graph(&streets_hash)?.route_along(from, to) {
                Some(route) => {
                    let to_confirm = route.streets.iter().filter(|s| confirm.contains(&s.class)).cloned().collect();
                    Response::Route { points: route.points, length_m: Some(route.length_m), streets: route.streets, to_confirm }
                }
                None => Response::Route { points: vec![], length_m: None, streets: vec![], to_confirm: vec![] },
            })
        }
    }
}

/// The JSON seam: `{"op": ...}` in, `{"ok": ...}` / `{"error": ...}`
/// out — byte-compatible with the monolith and every other
/// capability module.
pub fn dispatch_json(text: &str) -> String {
    cap_abi::answer(text, dispatch)
}

/// The bulk-bytes seam: op 0 imports a street layer.
pub fn dispatch_bytes(op: u32, bytes: &[u8]) -> String {
    cap_abi::outcome(match op {
        OP_IMPORT => import(bytes),
        other => Err(format!("unknown bulk op {other}")),
    })
}

cap_abi::capability! {
    id: "streets",
    json: dispatch_json,
    bytes: dispatch_bytes,
    documents: [STREETS, NETWORK],
}
