//! RAPTOR boards patterns, which are groups of trips that visit the same stop
//! sequence in the same order. The patterns group trips by their actual stop
//! sequence, so an edited trip that skips stops has its own pattern.

use std::collections::HashMap;

use transit_date::Date;

use transit_live::LiveDay;
use transit_network::{MaterializedTrip, Network};
use transit_streets::walking::WalkTable;

/// The straight-line transfer radius in meters, used when the network has no
/// walk table (D-32).
pub const WALK_RADIUS_M: f64 = 300.0;
pub const WALK_SPEED_MPS: f64 = 1.3;

#[derive(Debug, Clone)]
pub struct StopNode {
    pub stop_id: String,
    pub stop_name: String,
    pub footpaths: Vec<Footpath>,
}

#[derive(Debug, Clone, Copy)]
pub struct Footpath {
    pub to: u32,
    pub walk_secs: u32,
}

#[derive(Debug, Clone)]
pub struct Pattern {
    pub stop_positions: Vec<u32>,
    /// Trips sorted by departure at the first position. Overtaking trips make
    /// later positions non-monotonic, so a boarding search must scan and not
    /// binary-search (see [`crate::raptor`]).
    pub trips: Vec<PatternTrip>,
}

/// `departures` and `arrivals` are seconds since service-day midnight. A value
/// past 86400 is a trip after midnight.
#[derive(Debug, Clone)]
pub struct PatternTrip {
    pub trip: String,
    pub start_secs: u32,
    pub route_id: String,
    pub route_name: String,
    pub route_long_name: Option<String>,
    /// The sign per visit, run-length encoded as `(first position, headsign)`
    /// in position order, by `transit_network::visit_destination`. Read it with
    /// [`PatternTrip::headsign_at`].
    pub headsigns: Vec<(u32, String)>,
    pub departures: Vec<u32>,
    pub arrivals: Vec<u32>,
    pub delays: Vec<(Option<i32>, Option<i32>)>,
}

impl PatternTrip {
    pub fn headsign_at(&self, position: usize) -> &str {
        self.headsigns
            .iter()
            .rev()
            .find(|(from, _)| *from as usize <= position)
            .map(|(_, h)| h.as_str())
            .unwrap_or("")
    }

    pub fn delay_at(&self, position: usize) -> (Option<i32>, Option<i32>) {
        self.delays.get(position).copied().unwrap_or((None, None))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LiveVisit {
    pub arrival_secs: u32,
    pub departure_secs: u32,
    /// The document states this visit. A stated zero delay is a prediction
    /// ("on time, as of now").
    pub stated: bool,
}

/// The live document keys a stop entry by trip instance, stop, and the
/// published departure there. Its delays are against that published schedule,
/// as GTFS-rt reads them. The document has already propagated delays.
///
/// A visit matches the entry at its stop whose scheduled departure is the
/// visit's own. A diverted trip's patched times may have moved. Then the
/// entry at the stop matches when the trip visits the stop once and the
/// document states it once. A prediction is clamped to the time before it, so
/// times never run backwards.
pub fn live_times(trip: &MaterializedTrip, live: &LiveDay) -> Option<Vec<LiveVisit>> {
    let entry = live.trips.iter().find(|t| t.trip == trip.trip && t.start_secs == trip.start_secs)?;
    if entry.stops.is_empty() {
        return None;
    }
    let mut out = Vec::with_capacity(trip.stop_times.len());
    let mut floor = 0u32;
    for st in &trip.stop_times {
        let at_stop = || entry.stops.iter().filter(|s| s.stop == st.stop);
        let stated = at_stop().find(|s| s.scheduled_departure == Some(st.departure_secs)).or_else(|| {
            let visits = trip.stop_times.iter().filter(|v| v.stop == st.stop).count();
            (visits == 1 && at_stop().count() == 1).then(|| at_stop().next()).flatten()
        });
        let (mut arr, mut dep) = (st.arrival_secs, st.departure_secs);
        let mut is_stated = false;
        if let Some(s) = stated {
            let published_dep = s.scheduled_departure.unwrap_or(st.departure_secs) as i64;
            let published_arr = published_dep - (st.departure_secs - st.arrival_secs) as i64;
            let dep_delay = s.departure_delay.or(s.arrival_delay);
            let arr_delay = s.arrival_delay.or(s.departure_delay);
            if let Some(d) = arr_delay {
                arr = (published_arr + d as i64).max(0) as u32;
            }
            if let Some(d) = dep_delay {
                dep = (published_dep + d as i64).max(0) as u32;
            }
            is_stated = dep_delay.is_some();
        }
        arr = arr.max(floor);
        dep = dep.max(arr);
        floor = dep;
        out.push(LiveVisit { arrival_secs: arr, departure_secs: dep, stated: is_stated });
    }
    Some(out)
}

#[derive(Debug, Clone)]
pub struct RouterNetwork {
    pub date: Date,
    pub stops: Vec<StopNode>,
    stop_index: HashMap<String, u32>,
    pub patterns: Vec<Pattern>,
    pub patterns_at_stop: Vec<Vec<(u32, u32)>>,
}

impl RouterNetwork {
    pub fn with_walks(mut self, walks: &WalkTable) -> RouterNetwork {
        for stop in &mut self.stops {
            stop.footpaths.clear();
        }
        for walk in &walks.walks {
            if let (Some(from), Some(to)) = (self.stop_index(&walk.from), self.stop_index(&walk.to)) {
                self.stops[from as usize].footpaths.push(Footpath { to, walk_secs: walk.walk_secs });
            }
        }
        self
    }

    pub fn stop_index(&self, stop_id: &str) -> Option<u32> {
        self.stop_index.get(stop_id).copied()
    }

    pub fn day_end(&self) -> u32 {
        self.patterns
            .iter()
            .flat_map(|p| &p.trips)
            .filter_map(|t| t.arrivals.last().copied())
            .max()
            .unwrap_or(0)
    }

    pub fn build_from_day(network: &Network, day: &[MaterializedTrip], date: Date) -> RouterNetwork {
        Self::build(network, day, date, None)
    }

    /// The caller passes the day with the edits in force, because the router
    /// applies no patches. It skips a trip that names an unknown stop.
    pub fn build(
        network: &Network,
        day: &[MaterializedTrip],
        date: Date,
        live: Option<&LiveDay>,
    ) -> RouterNetwork {
        let mut stops: Vec<StopNode> = Vec::with_capacity(network.stops.len());
        let mut stop_index: HashMap<String, u32> = HashMap::with_capacity(network.stops.len());
        let mut coords: Vec<Option<(f64, f64)>> = Vec::with_capacity(network.stops.len());
        for (stop_id, stop) in &network.stops {
            let idx = stops.len() as u32;
            stops.push(StopNode {
                stop_id: stop_id.clone(),
                stop_name: stop.name.clone(),
                footpaths: Vec::new(),
            });
            coords.push(Some((stop.lat, stop.lon)));
            stop_index.insert(stop_id.clone(), idx);
        }

        let mut pattern_map: HashMap<Vec<u32>, Vec<PatternTrip>> = HashMap::new();
        for trip in day {
            // What the live document says does not happen is not in the planned
            // world. A cancelled instance is gone, and a skipped visit
            // is not a stop of the trip.
            let kept;
            let trip = match live {
                Some(l) if trip.stop_times.iter().any(|st| !l.runs(&trip.trip, trip.start_secs, &st.stop, st.departure_secs)) => {
                    let mut t = trip.clone();
                    t.stop_times.retain(|st| l.runs(&trip.trip, trip.start_secs, &st.stop, st.departure_secs));
                    kept = t;
                    &kept
                }
                _ => trip,
            };
            if trip.stop_times.len() < 2 {
                continue;
            }
            let mut positions: Vec<u32> = Vec::with_capacity(trip.stop_times.len());
            let mut all_known = true;
            for st in &trip.stop_times {
                match stop_index.get(&st.stop) {
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

            let route = network.routes.get(&trip.route);
            let route_name = route.map(|r| r.label().to_string()).unwrap_or_else(|| trip.route.clone());
            let route_long_name = route.and_then(|r| r.long_name.clone()).filter(|n| !n.is_empty());
            let mut headsigns: Vec<(u32, String)> = Vec::new();
            for v in 0..trip.stop_times.len() {
                let sign = transit_network::visit_destination(network, trip, v);
                if headsigns.last().is_none_or(|(_, h)| *h != sign) {
                    headsigns.push((v as u32, sign));
                }
            }

            let own: Vec<LiveVisit> = trip
                .stop_times
                .iter()
                .map(|st| LiveVisit { arrival_secs: st.arrival_secs, departure_secs: st.departure_secs, stated: false })
                .collect();
            let predicted = live.and_then(|l| live_times(trip, l));
            let times = predicted.as_deref().unwrap_or(&own);
            let delays = match &predicted {
                None => Vec::new(),
                Some(p) => p
                    .iter()
                    .zip(&own)
                    .map(|(p, o)| {
                        let moved = |pt: u32, ot: u32| (p.stated || pt != ot).then(|| pt as i32 - ot as i32);
                        (moved(p.arrival_secs, o.arrival_secs), moved(p.departure_secs, o.departure_secs))
                    })
                    .collect(),
            };

            pattern_map.entry(positions).or_default().push(PatternTrip {
                trip: trip.trip.clone(),
                start_secs: trip.start_secs,
                route_id: trip.route.clone(),
                route_name,
                route_long_name,
                headsigns,
                departures: times.iter().map(|t| t.departure_secs).collect(),
                arrivals: times.iter().map(|t| t.arrival_secs).collect(),
                delays,
            });
        }

        let mut patterns: Vec<Pattern> = Vec::with_capacity(pattern_map.len());
        for (positions, mut trips) in pattern_map {
            trips.sort_by_key(|t| t.departures[0]);
            patterns.push(Pattern { stop_positions: positions, trips });
        }
        patterns.sort_by(|a, b| a.stop_positions.cmp(&b.stop_positions));

        let mut patterns_at_stop: Vec<Vec<(u32, u32)>> = vec![Vec::new(); stops.len()];
        for (pi, pattern) in patterns.iter().enumerate() {
            for (pos, &stop) in pattern.stop_positions.iter().enumerate() {
                patterns_at_stop[stop as usize].push((pi as u32, pos as u32));
            }
        }

        // Straight-line footpaths join stops within WALK_RADIUS_M until a walk
        // table replaces them (`with_walks`). A uniform grid keeps this O(n).
        let footpaths = build_footpaths(&coords);
        for (i, edges) in footpaths.into_iter().enumerate() {
            stops[i].footpaths = edges;
        }

        RouterNetwork { date, stops, stop_index, patterns, patterns_at_stop }
    }
}

/// A grid cell of about 0.005° (400-550 m). A 3x3 neighborhood covers the walk
/// radius.
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
                    let dist = transit_geo::distance_m(lat1, lon1, lat2, lon2);
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
