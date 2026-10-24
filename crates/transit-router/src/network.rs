//! Building the routing network from a parsed GTFS feed.
//!
//! RAPTOR does not work on individual trips — it boards *trip
//! patterns*: groups of trips that visit the same stop sequence in the
//! same order (e.g. every run of "route 14 inbound today"). Within one
//! pattern, trips are time-ordered, which turns "earliest trip I can
//! board at stop S after time T" into one binary search.
//!
//! The network is built for one service date: only trips whose service
//! runs that day are included (via transit-core's calendar), so the
//! per-pattern trip arrays contain exactly the boardable trips.

use std::collections::HashMap;

use chrono::NaiveDate;

use transit_core::Gtfs;

/// Default footpath radius in meters.
pub const WALK_RADIUS_M: f64 = 300.0;
/// Average walking speed (m/s) used to turn distances into durations.
pub const WALK_SPEED_MPS: f64 = 1.3;

/// One stop in the network. Footpaths are the precomputed walking
/// transfers to nearby stops.
#[derive(Debug, Clone)]
pub struct StopNode {
    pub stop_id: String,
    pub stop_name: String,
    pub footpaths: Vec<Footpath>,
}

/// A walking edge: "to" is a stop index in the network.
#[derive(Debug, Clone, Copy)]
pub struct Footpath {
    pub to: u32,
    pub walk_secs: u32,
}

/// A trip pattern: trips sharing the same ordered stop sequence.
#[derive(Debug, Clone)]
pub struct Pattern {
    /// Stop indices in visit order.
    pub stop_positions: Vec<u32>,
    /// Trips sorted by departure at the first position, for
    /// deterministic iteration. Note: later positions are NOT
    /// guaranteed monotonic (overtaking trips), so boarding searches
    /// must scan rather than binary-search (see [`crate::raptor`]).
    pub trips: Vec<PatternTrip>,
}

/// One trip of a pattern, with times aligned to `Pattern::stop_positions`.
///
/// `departures`/`arrivals` are seconds since service-day midnight.
/// Missing GTFS times are filled: departures forward-fill, arrivals
/// backward-fill, so every position has usable values.
#[derive(Debug, Clone)]
pub struct PatternTrip {
    pub trip_id: String,
    pub route_id: String,
    pub route_name: String,
    pub headsign: Option<String>,
    pub departures: Vec<u32>,
    pub arrivals: Vec<u32>,
}

/// The routing network for one feed on one date. Fields are public for
/// read-only use (tests, tooling); treat them as immutable.
#[derive(Debug, Clone)]
pub struct RouterNetwork {
    pub date: NaiveDate,
    pub stops: Vec<StopNode>,
    /// stop_id → index into `stops`.
    stop_index: HashMap<String, u32>,
    pub patterns: Vec<Pattern>,
    /// For each stop index: (pattern index, position) — every pattern
    /// serving that stop and at which position(s).
    pub patterns_at_stop: Vec<Vec<(u32, u32)>>,
}

impl RouterNetwork {
    pub fn stop_index(&self, stop_id: &str) -> Option<u32> {
        self.stop_index.get(stop_id).copied()
    }

    /// Build the network for `date`. Trips not running that day (per
    /// the calendar) are excluded; so are trips whose stop times are
    /// unusable (no first departure / last arrival) or that reference
    /// unknown stops.
    pub fn build(gtfs: &Gtfs, date: NaiveDate) -> RouterNetwork {
        // Stops first: index every stop, remember coordinates for the
        // footpath graph.
        let mut stops: Vec<StopNode> = Vec::with_capacity(gtfs.stops.len());
        let mut stop_index: HashMap<String, u32> = HashMap::with_capacity(gtfs.stops.len());
        let mut coords: Vec<Option<(f64, f64)>> = Vec::with_capacity(gtfs.stops.len());
        for stop in gtfs.stops.values() {
            let idx = stops.len() as u32;
            stops.push(StopNode {
                stop_id: stop.stop_id.clone(),
                stop_name: stop.stop_name.clone(),
                footpaths: Vec::new(),
            });
            coords.push(match (stop.stop_lat, stop.stop_lon) {
                (Some(lat), Some(lon)) => Some((lat, lon)),
                _ => None,
            });
            stop_index.insert(stop.stop_id.clone(), idx);
        }

        // Group trips into patterns keyed by their stop sequence.
        let mut pattern_map: HashMap<Vec<u32>, Vec<PatternTrip>> = HashMap::new();
        for trip in gtfs.trips.values() {
            if !gtfs.services.is_active(&trip.service_id, date) {
                continue;
            }
            let Some(stop_times) = gtfs.trip_stop_times(&trip.trip_id) else {
                continue;
            };
            if stop_times.len() < 2 {
                continue;
            }

            // Resolve stop indices for the whole sequence; bail on
            // unknown stops.
            let mut positions: Vec<u32> = Vec::with_capacity(stop_times.len());
            let mut all_known = true;
            for st in &stop_times {
                match stop_index.get(&st.stop_id) {
                    Some(&i) => positions.push(i),
                    None => {
                        all_known = false;
                        break;
                    }
                }
            }
            if !all_known {
                continue;
            }

            let n = stop_times.len();
            // Fill missing times: departures forward, arrivals backward.
            let mut departures = vec![u32::MAX; n];
            let mut prev = u32::MAX;
            for (i, st) in stop_times.iter().enumerate() {
                let dep = st.departure_time.or(st.arrival_time).unwrap_or(prev);
                departures[i] = dep;
                prev = dep;
            }
            let mut arrivals = vec![u32::MAX; n];
            let mut next = u32::MAX;
            for (i, st) in stop_times.iter().enumerate().rev() {
                let arr = st.arrival_time.or(st.departure_time).unwrap_or(next);
                arrivals[i] = arr;
                next = arr;
            }
            // First departure and last arrival are the minimum needed to
            // ride a trip meaningfully.
            if departures[0] == u32::MAX || arrivals[n - 1] == u32::MAX {
                continue;
            }

            let route_name = gtfs
                .routes
                .get(&trip.route_id)
                .map(|r| r.display_name())
                .unwrap_or_else(|| trip.route_id.clone());

            pattern_map.entry(positions).or_default().push(PatternTrip {
                trip_id: trip.trip_id.clone(),
                route_id: trip.route_id.clone(),
                route_name,
                headsign: trip.trip_headsign.clone(),
                departures,
                arrivals,
            });
        }

        let mut patterns: Vec<Pattern> = Vec::with_capacity(pattern_map.len());
        for (positions, mut trips) in pattern_map {
            trips.sort_by_key(|t| t.departures[0]);
            patterns.push(Pattern { stop_positions: positions, trips });
        }
        // Deterministic order (iteration order of HashMap varies).
        patterns.sort_by(|a, b| a.stop_positions.cmp(&b.stop_positions));

        // Inverted index: stop → patterns serving it.
        let mut patterns_at_stop: Vec<Vec<(u32, u32)>> = vec![Vec::new(); stops.len()];
        for (pi, pattern) in patterns.iter().enumerate() {
            for (pos, &stop) in pattern.stop_positions.iter().enumerate() {
                patterns_at_stop[stop as usize].push((pi as u32, pos as u32));
            }
        }

        // Footpaths: stop pairs within WALK_RADIUS_M. A simple uniform
        // grid keeps this O(n) instead of O(n^2).
        let footpaths = build_footpaths(&coords);
        for (i, edges) in footpaths.into_iter().enumerate() {
            stops[i].footpaths = edges;
        }

        RouterNetwork { date, stops, stop_index, patterns, patterns_at_stop }
    }
}

/// Straight-line walking distance in meters.
fn haversine_m(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let (lat1, lon1, lat2, lon2) = (lat1.to_radians(), lon1.to_radians(), lat2.to_radians(), lon2.to_radians());
    let dlat = lat2 - lat1;
    let dlon = lon2 - lon1;
    let a = (dlat / 2.0).sin().powi(2) + lat1.cos() * lat2.cos() * (dlon / 2.0).sin().powi(2);
    6_371_000.0 * 2.0 * a.sqrt().atan()
}

/// Grid cell ~0.005° (~400-550 m): a 3x3 neighborhood covers the
/// walk radius.
const CELL: f64 = 0.005;

fn build_footpaths(coords: &[Option<(f64, f64)>]) -> Vec<Vec<Footpath>> {
    let mut grid: HashMap<(i64, i64), Vec<usize>> = HashMap::new();
    for (i, c) in coords.iter().enumerate() {
        if let Some((lat, lon)) = c {
            let key = ((lat / CELL).floor() as i64, (lon / CELL).floor() as i64);
            grid.entry(key).or_default().push(i);
        }
    }

    let mut result: Vec<Vec<Footpath>> = vec![Vec::new(); coords.len()];
    for i in 0..coords.len() {
        let Some((lat1, lon1)) = coords[i] else { continue };
        let (cy, cx) = ((lat1 / CELL).floor() as i64, (lon1 / CELL).floor() as i64);
        for dy in -1..=1 {
            for dx in -1..=1 {
                let Some(candidates) = grid.get(&(cy + dy, cx + dx)) else { continue };
                for &j in candidates {
                    if i == j {
                        continue;
                    }
                    let Some((lat2, lon2)) = coords[j] else { continue };
                    let dist = haversine_m(lat1, lon1, lat2, lon2);
                    if dist <= WALK_RADIUS_M {
                        result[i].push(Footpath {
                            to: j as u32,
                            walk_secs: (dist / WALK_SPEED_MPS).ceil() as u32,
                        });
                    }
                }
            }
        }
    }
    result
}
