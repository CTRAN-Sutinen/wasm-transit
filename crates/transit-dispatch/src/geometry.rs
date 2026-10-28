//! Where service runs today: geometry over the EFFECTIVE day.
//!
//! A trip that still follows its pattern draws the pattern's
//! alignment; a diverted trip (stops skipped) no longer does, and
//! draws straight lines over its patched stops — exactly as the
//! router rides it. An itinerary's transit leg draws only the ridden
//! segment; a walk draws through the stops it passes. The map is
//! handed these lines; it computes none.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use transit_network::{authored_alignment, stop_point, stops_line, Coord, MaterializedTrip, Network};

fn day_trip<'a>(day: &'a [MaterializedTrip], pattern_trips: &str, start_secs: u32) -> Option<&'a MaterializedTrip> {
    day.iter().find(|t| t.pattern_trips == pattern_trips && t.start_secs == start_secs)
}

/// The trip's pattern alignment, when the effective trip still visits
/// exactly its pattern's stops and the pattern runs an alignment.
fn followed_path(network: &Network, trip: &MaterializedTrip) -> Option<Vec<Coord>> {
    let pattern_id = &network.pattern_trips.get(&trip.pattern_trips)?.pattern;
    let pattern = network.patterns.get(pattern_id)?;
    let follows = pattern.stops.len() == trip.stop_times.len()
        && pattern.stops.iter().zip(&trip.stop_times).all(|(ps, st)| ps.stop == st.stop);
    follows.then(|| authored_alignment(network, pattern_id)).flatten()
}

/// One derived trip's line in the effective day: its pattern's
/// authored alignment while it follows the pattern, else its stops
/// joined in order. Empty when the trip does not run that day.
pub fn trip_path(network: &Network, day: &[MaterializedTrip], pattern_trips: &str, start_secs: u32) -> Vec<Coord> {
    let Some(trip) = day_trip(day, pattern_trips, start_secs) else { return Vec::new() };
    followed_path(network, trip)
        .unwrap_or_else(|| stops_line(network, trip.stop_times.iter().map(|st| st.stop.as_str())))
}

/// The ridden part of a trip, from one of its stops to a later one:
/// the authored alignment sliced between the vertices nearest the two
/// stops (planar distance — exact at city scale) while the trip
/// follows its pattern; else its stops between the two; else a
/// straight line. Empty when either stop is unknown to the network.
pub fn trip_segment(
    network: &Network,
    day: &[MaterializedTrip],
    pattern_trips: &str,
    start_secs: u32,
    from_stop: &str,
    to_stop: &str,
) -> Vec<Coord> {
    let (Some(from), Some(to)) = (stop_point(network, from_stop), stop_point(network, to_stop)) else {
        return Vec::new();
    };
    if let Some(trip) = day_trip(day, pattern_trips, start_secs) {
        if let Some(path) = followed_path(network, trip) {
            let (i0, i1) = (nearest_vertex(&path, from), nearest_vertex(&path, to));
            let (lo, hi) = (i0.min(i1), i0.max(i1));
            if hi > lo {
                return path[lo..=hi].to_vec();
            }
        }
        let i = trip.stop_times.iter().position(|st| st.stop == from_stop);
        let j = trip.stop_times.iter().position(|st| st.stop == to_stop);
        if let (Some(i), Some(j)) = (i, j) {
            if i != j {
                let (lo, hi) = (i.min(j), i.max(j));
                let line = stops_line(network, trip.stop_times[lo..=hi].iter().map(|st| st.stop.as_str()));
                if !line.is_empty() {
                    return line;
                }
            }
        }
    }
    vec![from, to]
}

fn nearest_vertex(path: &[Coord], at: Coord) -> usize {
    let mut best = (0, f64::INFINITY);
    for (i, c) in path.iter().enumerate() {
        let d = (c.lat - at.lat).powi(2) + (c.lon - at.lon).powi(2);
        if d < best.1 {
            best = (i, d);
        }
    }
    best.0
}

/// One drawn leg of an itinerary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum LegKind {
    Transit,
    Walk,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct LegPath {
    pub kind: LegKind,
    pub path: Vec<Coord>,
}
