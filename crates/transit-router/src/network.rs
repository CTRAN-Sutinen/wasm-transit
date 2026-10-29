//! Building the routing network from the network model's materialized
//! day.
//!
//! RAPTOR does not work on individual trips — it boards *trip
//! patterns*: groups of trips that visit the same stop sequence in the
//! same order (e.g. every run of "route 14 inbound today"). Within one
//! pattern, trips are time-ordered, which turns "earliest trip I can
//! board at stop S after time T" into one binary search.
//!
//! The network is built for one service date over the materialized
//! day (the model's own trips — materialized, and patched with the
//! day's edits when the caller composed an effective day). Grouping
//! is by ACTUAL stop sequence: an edited trip that skips stops no
//! longer shares its authored pattern's sequence and must not ride
//! the authored pattern's array.
//!
//! With a LIVE document, a trip instance rides at its PREDICTED
//! times where the document states them (see [`live_times`]), the
//! schedule elsewhere — the graph is the day as it is expected to
//! run, and each visit remembers how far its time moved.

use std::collections::HashMap;

use chrono::NaiveDate;

use transit_live::LiveDay;
use transit_network::{MaterializedTrip, Network};
use transit_streets::walking::WalkTable;

/// The straight-line footpath radius in meters — the transfers of a
/// network with no walk table ([`RouterNetwork::with_walks`]; D-32).
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
/// `departures`/`arrivals` are seconds since service-day midnight
/// (the model's anchor; past-86400 values are after-midnight trips,
/// not errors). Materialized times are complete — the model
/// interpolates non-timepoints at materialize time, so there is
/// nothing to fill here.
#[derive(Debug, Clone)]
pub struct PatternTrip {
    /// The trip instance: the network's trip and its start.
    pub trip: String,
    pub start_secs: u32,
    pub route_id: String,
    /// The route's label (short name, else long name).
    pub route_name: String,
    /// The route's long name, when it has one.
    pub route_long_name: Option<String>,
    /// The sign per visit, run-length encoded: `(first position,
    /// headsign)` in position order — `transit_network::
    /// visit_destination`, the one shared rule. Read with
    /// [`PatternTrip::headsign_at`].
    pub headsigns: Vec<(u32, String)>,
    /// The times the trip rides at: predicted where the live
    /// document states them, else the day's own.
    pub departures: Vec<u32>,
    pub arrivals: Vec<u32>,
    /// Per position, `(arrival, departure)` minus the day's own time
    /// — `None` where the time is the day's own (no prediction).
    /// Empty when the live document says nothing about the trip.
    pub delays: Vec<(Option<i32>, Option<i32>)>,
}

impl PatternTrip {
    /// The headsign the bus shows at `position`.
    pub fn headsign_at(&self, position: usize) -> &str {
        self.headsigns
            .iter()
            .rev()
            .find(|(from, _)| *from as usize <= position)
            .map(|(_, h)| h.as_str())
            .unwrap_or("")
    }

    /// `(arrival delay, departure delay)` at `position` — `None`
    /// where the time is the day's own.
    pub fn delay_at(&self, position: usize) -> (Option<i32>, Option<i32>) {
        self.delays.get(position).copied().unwrap_or((None, None))
    }
}

/// One visit's times as the live document predicts them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LiveVisit {
    pub arrival_secs: u32,
    pub departure_secs: u32,
    /// The document states this visit (a stated zero delay is still a
    /// prediction: "on time, as of now").
    pub stated: bool,
}

/// A trip instance's PREDICTED times per visit, or `None` when the live document states nothing about any
/// of its visits.
///
/// The live document keys a stop entry by trip instance, stop and
/// the published schedule's departure there; its delays are against
/// that published schedule (GTFS-rt's reading — propagation is
/// already applied in the document, and stops before the first
/// stated delay are absent: they stay on schedule). A visit matches
/// the entry at its stop whose scheduled departure is the visit's
/// own; failing that (a diverted trip's patched times moved), the
/// entry at its stop when the stop is visited once and stated once.
/// The predicted departure is the published departure plus the
/// departure delay (else the arrival delay); the predicted arrival
/// is the published arrival — the visit's dwell before it — plus the
/// arrival delay (else the departure delay). Times never run
/// backwards along the trip: a prediction is clamped to the time
/// before it.
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

/// The routing network for one network document on one date. Fields
/// are public for read-only use (tests, tooling); treat them as
/// immutable.
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
    /// The network with its transfers walking the streets: every
    /// footpath is the walk table's (`transit_streets::walking` — the
    /// stops' walks along the streets), the straight-line ones are
    /// dropped, and a walk naming a stop the network lacks is ignored.
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

    /// The service day's end: the latest arrival of any trip (past
    /// 86400 when the day runs after midnight), 0 on an empty day.
    pub fn day_end(&self) -> u32 {
        self.patterns
            .iter()
            .flat_map(|p| &p.trips)
            .filter_map(|t| t.arrivals.last().copied())
            .max()
            .unwrap_or(0)
    }

    /// The schedule-only graph: [`RouterNetwork::build`] with no live
    /// document.
    pub fn build_from_day(network: &Network, day: &[MaterializedTrip], date: NaiveDate) -> RouterNetwork {
        Self::build(network, day, date, None)
    }

    /// Build the network for `date` over a materialized day. The day
    /// is the caller's composition: plain `materialize` for a
    /// schedule-only day, `effective_day` for the day with the edits
    /// in force — the router never expands calendars or applies
    /// patches itself (Law 2: one composition, not per-module
    /// reimplementations). Trips whose stop sequence references
    /// unknown stops are skipped, never an error. With `live`, each
    /// trip instance rides at its predicted times ([`live_times`]).
    pub fn build(
        network: &Network,
        day: &[MaterializedTrip],
        date: NaiveDate,
        live: Option<&LiveDay>,
    ) -> RouterNetwork {
        // Stops first, from the network document (BTreeMap order —
        // deterministic): index every stop, remember coordinates for
        // the footpath graph.
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

        // Group trips into patterns keyed by their stop sequence —
        // the ACTUAL sequence (an edited trip that skips stops is its
        // own pattern), resolved to stop indices.
        let mut pattern_map: HashMap<Vec<u32>, Vec<PatternTrip>> = HashMap::new();
        for trip in day {
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
            // What the sign says at each visit — the shared rule,
            // stored as runs (a trip's sign rarely changes).
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
            // A visit's delay is how far its time moved — stated by
            // the document (zero included), or carried by the clamp.
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
        // Deterministic order (iteration order of HashMap varies).
        patterns.sort_by(|a, b| a.stop_positions.cmp(&b.stop_positions));

        // Inverted index: stop → patterns serving it.
        let mut patterns_at_stop: Vec<Vec<(u32, u32)>> = vec![Vec::new(); stops.len()];
        for (pi, pattern) in patterns.iter().enumerate() {
            for (pos, &stop) in pattern.stop_positions.iter().enumerate() {
                patterns_at_stop[stop as usize].push((pi as u32, pos as u32));
            }
        }

        // Footpaths: stop pairs within WALK_RADIUS_M in a straight
        // line, until a walk table replaces them (`with_walks`). A
        // simple uniform grid keeps this O(n) instead of O(n^2).
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
