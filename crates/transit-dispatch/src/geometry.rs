//! Where service runs today: geometry over the effective day.
//!
//! A trip that still follows its pattern draws the pattern's alignment.
//! A diverted trip draws the alignment to the diversion's first kept stop, then
//! the diversion's traced detour, then the alignment from its last kept stop.
//! A skip-only diversion (no detour) leaves the alignment as it is.
//! With no alignment, its patched stops are joined in order, with the detour
//! between the anchors.
//! An itinerary's transit leg draws only the ridden segment of that line. A
//! walk draws through the stops it passes.
//! The map is handed these lines. It computes none.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use transit_network::{authored_alignment, stop_point, stops_line, Coord, MaterializedTrip, Network};
use transit_sync::Diversion;

fn day_trip<'a>(day: &'a [MaterializedTrip], trip: &str, start_secs: u32) -> Option<&'a MaterializedTrip> {
    day.iter().find(|t| t.trip == trip && t.start_secs == start_secs)
}

/// The trip's pattern alignment. It applies when the effective trip still
/// visits exactly its pattern's stops, and the pattern runs an alignment.
fn followed_path(network: &Network, trip: &MaterializedTrip) -> Option<Vec<Coord>> {
    let pattern_id = &network.pattern_trips.get(&trip.pattern_trips)?.pattern;
    let pattern = network.patterns.get(pattern_id)?;
    let follows = pattern.stops.len() == trip.stop_times.len()
        && pattern.stops.iter().zip(&trip.stop_times).all(|(ps, st)| ps.stop == st.stop);
    follows.then(|| authored_alignment(network, pattern_id)).flatten()
}

/// The diversions in force that reroute this patched trip: its route's
/// diversions whose kept stops it visits in order (the day's patch applied
/// them).
fn rerouting<'a>(trip: &MaterializedTrip, diversions: &'a [Diversion]) -> Vec<&'a Diversion> {
    diversions
        .iter()
        .filter(|d| d.route_id == trip.route)
        .filter(|d| {
            let p0 = trip.stop_times.iter().position(|st| st.stop == d.first_kept_stop);
            p0.is_some_and(|p0| trip.stop_times[p0 + 1..].iter().any(|st| st.stop == d.last_kept_stop))
        })
        .collect()
}

/// Splice each traced detour into a line: the line to the vertex nearest the
/// first kept stop, then the detour, then the line from the vertex nearest
/// the last kept stop (searched after the first). A diversion without a
/// detour leaves the line as it is.
fn spliced(network: &Network, mut path: Vec<Coord>, diversions: &[&Diversion]) -> Vec<Coord> {
    for d in diversions {
        let Some(points) = d.detour.as_ref().filter(|p| !p.is_empty()) else { continue };
        let (Some(first), Some(last)) = (stop_point(network, &d.first_kept_stop), stop_point(network, &d.last_kept_stop))
        else {
            continue;
        };
        if path.is_empty() {
            continue;
        }
        let i0 = nearest_vertex(&path, first);
        let i1 = i0 + nearest_vertex(&path[i0..], last);
        let mut next = path[..=i0].to_vec();
        next.extend(points.iter().map(|p| Coord { lat: p.lat, lon: p.lon }));
        next.extend_from_slice(&path[i1..]);
        path = next;
    }
    path
}

/// The line a trip instance runs: a trip on its pattern gets the alignment; a
/// diverted trip gets it with its detours spliced in, or, with no alignment,
/// its patched stops joined around the detours. `None` when the trip has no
/// alignment and no diversion.
fn drawn_path(network: &Network, trip: &MaterializedTrip, diversions: &[Diversion]) -> Option<Vec<Coord>> {
    let rerouted = rerouting(trip, diversions);
    if rerouted.is_empty() {
        return followed_path(network, trip);
    }
    let pattern = &network.pattern_trips.get(&trip.pattern_trips)?.pattern;
    let base = authored_alignment(network, pattern)
        .unwrap_or_else(|| stops_line(network, trip.stop_times.iter().map(|st| st.stop.as_str())));
    Some(spliced(network, base, &rerouted))
}

/// One trip instance's line in the effective day, given the diversions in
/// force: the pattern's authored alignment with each diversion that reroutes
/// it spliced in at the first and last kept stops. The result is empty when
/// the trip does not run that day.
pub fn trip_path(
    network: &Network,
    day: &[MaterializedTrip],
    diversions: &[Diversion],
    trip: &str,
    start_secs: u32,
) -> Vec<Coord> {
    let Some(trip) = day_trip(day, trip, start_secs) else { return Vec::new() };
    drawn_path(network, trip, diversions)
        .unwrap_or_else(|| stops_line(network, trip.stop_times.iter().map(|st| st.stop.as_str())))
}

/// The ridden part of a trip, from one of its stops to a later one: the
/// trip's line ([`trip_path`]) sliced between the vertices nearest the two
/// stops. Planar distance is exact at city scale. If the trip does not visit
/// both stops, the result is its stops between the two; if neither works, a
/// straight line. The result is empty when either stop is unknown to the
/// network.
pub fn trip_segment(
    network: &Network,
    day: &[MaterializedTrip],
    diversions: &[Diversion],
    trip: &str,
    start_secs: u32,
    from_stop: &str,
    to_stop: &str,
) -> Vec<Coord> {
    let (Some(from), Some(to)) = (stop_point(network, from_stop), stop_point(network, to_stop)) else {
        return Vec::new();
    };
    if let Some(trip) = day_trip(day, trip, start_secs) {
        let visits = |stop: &str| trip.stop_times.iter().any(|st| st.stop == stop);
        if let Some(path) = drawn_path(network, trip, diversions).filter(|_| visits(from_stop) && visits(to_stop)) {
            let i0 = nearest_vertex(&path, from);
            let i1 = i0 + nearest_vertex(&path[i0..], to);
            if i1 > i0 {
                return path[i0..=i1].to_vec();
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
