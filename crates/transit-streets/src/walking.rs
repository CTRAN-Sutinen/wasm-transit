//! Walking along the streets: how long a rider walks between two
//! stops.
//!
//! The walking profile is strict: a rider walks every class but
//! motorway, ramp, and unknown, and walks a one-way street either way
//! (one-way restricts vehicles). A walk is a stop's walk to its
//! nearest walkable street (the graph's snap neighborhood), the street
//! distance, and the walk from the street to the other stop, at the
//! walking speed, rounded up to the second; a walk past the limit is
//! no walk, and a stop with no walkable street nearby walks nowhere.
//!
//! The walk table ([`WalkTable`]) is a document DERIVED from a streets
//! document and a network: the router plans its transfers over it
//! instead of straight lines.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::model::{StreetClass, Streets};
use crate::StreetNetwork;

/// The default walking speed, meters per second.
pub const WALK_SPEED_MPS: f64 = 1.3;
/// The default walk limit: ten minutes.
pub const MAX_WALK_SECS: u32 = 600;

/// Whether a rider walks a street of this class.
pub fn walkable(class: StreetClass) -> bool {
    !matches!(class, StreetClass::Motorway | StreetClass::Ramp | StreetClass::Unknown)
}

/// One walk: `from` to `to`, in seconds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Walk {
    pub from: String,
    pub to: String,
    pub walk_secs: u32,
}

/// Every walk between a network's stops within the limit, and what
/// it was derived from: the streets and network documents' hashes,
/// the speed, the limit. Walks in (from, to) order; symmetric.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct WalkTable {
    pub streets_hash: String,
    pub network_hash: String,
    pub speed_mps: f64,
    pub max_walk_secs: u32,
    pub walks: Vec<Walk>,
}

/// The graph a rider walks: the walkable streets, every one two-way.
pub struct WalkGraph(StreetNetwork);

impl WalkGraph {
    pub fn new(streets: &Streets) -> WalkGraph {
        let ways = streets
            .ways
            .iter()
            .filter(|w| walkable(w.class))
            .map(|w| crate::Way { oneway: false, ..w.clone() })
            .collect();
        WalkGraph(StreetNetwork::build(&Streets { ways }))
    }

    /// Every walk between `stops` (id, lat, lon) within `max_walk_secs`
    /// at `speed_mps`, in (from, to) order.
    pub fn walks(&self, stops: &[(String, f64, f64)], speed_mps: f64, max_walk_secs: u32) -> Vec<Walk> {
        let net = &self.0;
        let limit_m = max_walk_secs as f64 * speed_mps;
        let snapped: Vec<_> = stops.iter().map(|(_, lat, lon)| net.snap(*lat, *lon)).collect();
        let mut dist = vec![f64::INFINITY; net.node_count() as usize];
        let mut touched = Vec::new();
        let mut out = Vec::new();
        for (i, from) in snapped.iter().enumerate() {
            let Some(from) = *from else { continue };
            if from.dist_m > limit_m {
                continue;
            }
            net.reach(from, limit_m - from.dist_m, &mut dist, &mut touched);
            // Each pair once, both ways: a walk is symmetric by
            // construction, never two searches rounding apart.
            for (j, to) in snapped.iter().enumerate().skip(i + 1) {
                let Some(to) = *to else { continue };
                let meters = from.dist_m + net.meters_to(from, &dist, to) + to.dist_m;
                let secs = (meters / speed_mps).ceil();
                if secs <= max_walk_secs as f64 {
                    let walk_secs = secs as u32;
                    out.push(Walk { from: stops[i].0.clone(), to: stops[j].0.clone(), walk_secs });
                    out.push(Walk { from: stops[j].0.clone(), to: stops[i].0.clone(), walk_secs });
                }
            }
            for node in touched.drain(..) {
                dist[node as usize] = f64::INFINITY;
            }
        }
        out.sort_by(|a, b| (&a.from, &a.to).cmp(&(&b.from, &b.to)));
        out.dedup_by(|a, b| a.from == b.from && a.to == b.to);
        out
    }
}

/// [`WalkGraph::walks`] over a streets document, built once.
pub fn walks(streets: &Streets, stops: &[(String, f64, f64)], speed_mps: f64, max_walk_secs: u32) -> Vec<Walk> {
    WalkGraph::new(streets).walks(stops, speed_mps, max_walk_secs)
}
