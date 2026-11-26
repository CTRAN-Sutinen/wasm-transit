//! The street graph and the walking graph are separate cached entries
//! (`STREET_GRAPH`, `WALK_GRAPH`), each built on first use, so a host that
//! only walks never builds the street graph.

#![forbid(unsafe_code)]

use std::sync::Arc;

use cap_abi::kernel::{kind, STREET_GRAPH, WALK_GRAPH};
use serde::{Deserialize, Serialize};
#[cfg(feature = "bindings")]
use ts_rs::TS;
use transit_streets::walking::{DirectWalk, PointWalk, WalkGraph, WalkTable, MAX_WALK_SECS, WALK_SPEED_MPS};
use transit_streets::{StreetClass, StreetNetwork, StreetPoint, StreetPosition, StreetUse, Streets};

pub const OP_IMPORT: u32 = 0;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS))]
#[cfg_attr(feature = "bindings", ts(export, export_to = "cap_streets/"))]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Request {
    Import { bytes: Vec<u8> },
    Walks {
        streets_hash: String,
        network_hash: String,
        #[serde(default)]
        speed_mps: Option<f64>,
        #[serde(default)]
        max_walk_secs: Option<u32>,
    },
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
    Snap { streets_hash: String, lat: f64, lon: f64 },
    Route {
        streets_hash: String,
        from: StreetPosition,
        to: StreetPosition,
        #[serde(default)]
        #[cfg_attr(feature = "bindings", ts(optional))]
        confirm: Option<Vec<StreetClass>>,
    },
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "bindings", derive(TS))]
#[cfg_attr(feature = "bindings", ts(export, export_to = "cap_streets/"))]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Response {
    Import {
        streets_hash: String,
        #[cfg_attr(feature = "bindings", ts(as = "Streets"))]
        streets: cap_abi::Canonical<Streets>,
        ways: u32,
        segments: u32,
        nodes: u32,
    },
    Walks {
        walks_hash: String,
        #[cfg_attr(feature = "bindings", ts(as = "WalkTable"))]
        walks: cap_abi::Canonical<WalkTable>,
    },
    Access { walks: Vec<PointWalk>, direct: Option<DirectWalk> },
    Snap { position: Option<StreetPosition> },
    Route { points: Vec<StreetPoint>, length_m: Option<f64>, streets: Vec<StreetUse>, to_confirm: Vec<StreetUse> },
}

fn graph(streets_hash: &str) -> Result<Arc<StreetNetwork>, cap_abi::Refusal> {
    let streets = kind::<transit_streets::Streets>().require(streets_hash)?;
    cap_abi::kernel::derive(&STREET_GRAPH, streets_hash, &[streets_hash], || Ok::<_, String>(StreetNetwork::build(&streets)))
}

fn walking(streets_hash: &str) -> Result<Arc<WalkGraph>, cap_abi::Refusal> {
    let streets = kind::<transit_streets::Streets>().require(streets_hash)?;
    cap_abi::kernel::derive(&WALK_GRAPH, streets_hash, &[streets_hash], || Ok::<_, String>(WalkGraph::new(&streets)))
}

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

fn network_stops(network_hash: &str) -> Result<Vec<(String, f64, f64)>, cap_abi::Refusal> {
    Ok(kind::<transit_network::Network>().require(network_hash)?.stops.iter().map(|(id, s)| (id.clone(), s.lat, s.lon)).collect())
}

fn walks(streets_hash: &str, network_hash: &str, speed_mps: Option<f64>, max_walk_secs: Option<u32>) -> Result<Response, cap_abi::Refusal> {
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
) -> Result<Response, cap_abi::Refusal> {
    let (speed_mps, max_walk_secs) = pace(speed_mps, max_walk_secs)?;
    for p in std::iter::once(&point).chain(&to) {
        if !(p.lat.is_finite() && p.lon.is_finite() && p.lat.abs() <= 90.0 && p.lon.abs() <= 180.0) {
            return Err(format!("point ({}, {}) is not on the globe", p.lat, p.lon).into());
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

fn import(bytes: &[u8]) -> Result<Response, cap_abi::Refusal> {
    let streets = transit_streets::geojson::import(bytes).map_err(|e| format!("street layer: {}", e.0))?;
    let (ways, segments, nodes) = streets.counts();
    let (streets_hash, streets) = kind::<transit_streets::Streets>().register(streets)?;
    Ok(Response::Import { streets_hash, streets: cap_abi::Canonical((*streets).clone()), ways, segments, nodes })
}

pub fn dispatch(request: Request) -> Result<Response, cap_abi::Refusal> {
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

pub fn dispatch_json(text: &str) -> String {
    cap_abi::answer(text, dispatch)
}

pub fn dispatch_bytes(op: u32, bytes: &[u8]) -> String {
    cap_abi::outcome(match op {
        OP_IMPORT => import(bytes),
        other => Err(format!("unknown bulk op {other}").into()),
    })
}

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
