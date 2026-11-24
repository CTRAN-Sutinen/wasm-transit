//! cap-streets: where along the streets? (OPS.md has the ops.)
//!
//! The seam over transit-streets: snap, route along the graph (so a traced
//! detour or a drawn pattern is street geometry), and walks between a
//! network's stops or from a point to the stops near it (D-26 for the bus
//! profile).
//!
//! * The streets document is the host's. The module is the hash
//!   authority: the hash is the canonical document's, so the same streets
//!   from another file are one hash, and `load_streets` refuses a
//!   document that is not canonical.
//! * The street graph and the walking graph are derived entries
//!   (`cap_abi::kernel::STREET_GRAPH`, `WALK_GRAPH`), each built on first
//!   use, so a host that only walks never builds the street graph.
//! * `route` answers empty across a gap in the graph, and the caller
//!   falls back to a straight line.

#![forbid(unsafe_code)]

use std::sync::Arc;

use cap_abi::kernel::{kind, STREET_GRAPH, WALK_GRAPH};
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
    /// Read a street layer into the streets document, and register it under
    /// its hash. The layer is a GeoJSON FeatureCollection or Feature of
    /// LineString/MultiLineString geometries. Lines that are not streets
    /// (railroads, water) are left out (D-30). Bulk-bytes op 0.
    Import { bytes: Vec<u8> },
    /// How long a rider walks between the network's stops along the streets.
    /// The answer is the walk table, a document that the router plans
    /// transfers over, and its hash. Walkable streets are every class but
    /// motorway, ramp and unknown. Walks run both ways along one-way
    /// streets. A walk is each stop's walk to its nearest walkable street
    /// plus the street distance, at `speed_mps` (default 1.3), rounded up to
    /// the second. It is kept within `max_walk_secs` (default 600).
    Walks {
        streets_hash: String,
        network_hash: String,
        #[serde(default)]
        speed_mps: Option<f64>,
        #[serde(default)]
        max_walk_secs: Option<u32>,
    },
    /// The walks from a point (a tapped spot, the rider's location) to the
    /// network's stops within the limit. These are the walks the walk table
    /// would hold for a stop at the point. Each walk has the path the rider
    /// walks, and the walks come nearest first. When `to` is given, the
    /// answer also has the walk from the point straight to it. The router
    /// plans a trip from the point over these walks. The answer is empty
    /// when no walkable street is near the point.
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
    /// neighborhood. This is the graph address (segment, parameter) that a
    /// trace's waypoint commits to. Null when no street is nearby.
    Snap { streets_hash: String, lat: f64, lon: f64 },
    /// The street geometry between two positions: the shortest path along
    /// the graph. The points end exactly at `to`. They start after `from`.
    /// A trace chains legs, so each leg starts at the previous leg's
    /// committed end. For a standalone trace, prepend `from`. The answer is
    /// empty when the positions sit on disconnected parts of the network.
    /// The caller then falls back to a straight line. The answer names the
    /// streets the path follows. For the bus profile (D-26), a bus is drawn
    /// down any street and is never refused. The answer also names the
    /// streets of a class in `confirm`, for the planner to confirm (default:
    /// unknown, path, steps, track).
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
    /// The document the host holds, its hash, and its size.
    /// The streets as their canonical bytes exactly (RFC 8785, D-169):
    /// a host serves them under `streets_hash` as they stand.
    Import {
        streets_hash: String,
        #[ts(as = "Streets")]
        streets: cap_abi::Canonical<Streets>,
        ways: u32,
        segments: u32,
        nodes: u32,
    },
    /// The walk table and its hash (the router's `load_walks` answers
    /// the same).
    /// The walk table as its canonical bytes exactly (RFC 8785, D-169).
    Walks {
        walks_hash: String,
        #[ts(as = "WalkTable")]
        walks: cap_abi::Canonical<WalkTable>,
    },
    /// `direct`: the walk to `to` (null when none was asked, or none
    /// is within the limit).
    Access { walks: Vec<PointWalk>, direct: Option<DirectWalk> },
    Snap { position: Option<StreetPosition> },
    /// `length_m`: the path's length along the graph from `from` to
    /// `to`. It is null when no path exists. `streets`: the streets the
    /// path follows, in order (class, name, meters along it). `to_confirm`:
    /// those of them whose class the profile asks the planner to confirm.
    /// They are drawn all the same.
    Route { points: Vec<StreetPoint>, length_m: Option<f64>, streets: Vec<StreetUse>, to_confirm: Vec<StreetUse> },
}

/// The street graph (snap, route) over a registered streets document,
/// built on first use and held as a derived entry.
fn graph(streets_hash: &str) -> Result<Arc<StreetNetwork>, String> {
    let streets = kind::<transit_streets::Streets>().require(streets_hash)?;
    cap_abi::kernel::derive(&STREET_GRAPH, streets_hash, &[streets_hash], || Ok(StreetNetwork::build(&streets)))
}

/// The walking graph over a registered streets document, built on
/// first use and held as a derived entry. A host that only walks never
/// builds the street graph.
fn walking(streets_hash: &str) -> Result<Arc<WalkGraph>, String> {
    let streets = kind::<transit_streets::Streets>().require(streets_hash)?;
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

/// A registered network's stops (id, lat, lon). A walk reads only these.
fn network_stops(network_hash: &str) -> Result<Vec<(String, f64, f64)>, String> {
    Ok(kind::<transit_network::Network>().require(network_hash)?.stops.iter().map(|(id, s)| (id.clone(), s.lat, s.lon)).collect())
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
    Ok(Response::Walks { walks_hash: cap_abi::canonical_hash(&table)?, walks: cap_abi::Canonical(table) })
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
    // The document registers where it is minted. Every query, and in an
    // assembly every capability, addresses it by hash.
    let (ways, segments, nodes) = streets.counts();
    let (streets_hash, streets) = kind::<transit_streets::Streets>().register(streets)?;
    Ok(Response::Import { streets_hash, streets: cap_abi::Canonical((*streets).clone()), ways, segments, nodes })
}

/// The typed dispatch. Native hosts call this directly. Failures are
/// `Err(msg)`. It never panics: a total seam. Garbage bytes are an error
/// outcome, not a trap.
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

/// The JSON seam: `{"op": ...}` in, `{"ok": ...}` or `{"error": ...}` out.
/// The bytes match every other capability module.
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

/// The worked examples. The generator runs them. No module build turns
/// the feature on.
#[cfg(feature = "examples")]
pub mod examples;

cap_abi::capability! {
    id: "streets",
    json: dispatch_json,
    bytes: dispatch_bytes,
    documents: [transit_streets::Streets, transit_network::Network],
    request: Request,
    consumes: &[("network", "network")],
    speaks: &[("streets", &[1]), ("network", &[transit_network::SCHEMA_VERSION]), ("walks", &[1])],
}
