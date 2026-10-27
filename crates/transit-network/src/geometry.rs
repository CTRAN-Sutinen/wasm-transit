//! Where things are: the network's own geometry answers.
//!
//! The model's contract for drawing a pattern is its authored path,
//! or — "None means draw stop-to-stop lines" — its stops joined in
//! order. Every view that draws a pattern (the diversion editor's
//! ghost, a planner's drill-down, a trip that still follows its
//! pattern) asks here, so every host draws the same line.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::model::{Coord, Network};

/// The pattern's alignment: its authored path when it has two points
/// or more, else its stops joined in order with consecutive repeats
/// dropped. Empty when that leaves fewer than two points (or the
/// pattern is unknown).
pub fn pattern_alignment(network: &Network, pattern: &str) -> Vec<Coord> {
    let Some(p) = network.patterns.get(pattern) else { return Vec::new() };
    if let Some(path) = p.path.as_ref().filter(|path| path.len() >= 2) {
        return path.clone();
    }
    stops_line(network, p.stops.iter().map(|ps| ps.stop.as_str()))
}

/// The pattern's stops, in order (stops the network lacks skipped).
pub fn pattern_stop_points(network: &Network, pattern: &str) -> Vec<Coord> {
    network
        .patterns
        .get(pattern)
        .map(|p| p.stops.iter().filter_map(|ps| stop_point(network, &ps.stop)).collect())
        .unwrap_or_default()
}

/// One stop's coordinates.
pub fn stop_point(network: &Network, stop: &str) -> Option<Coord> {
    network.stops.get(stop).map(|s| Coord { lat: s.lat, lon: s.lon })
}

/// Stops joined in order, consecutive repeats dropped; empty below
/// two points (a line needs two).
pub fn stops_line<'a>(network: &Network, stops: impl IntoIterator<Item = &'a str>) -> Vec<Coord> {
    let mut out: Vec<Coord> = Vec::new();
    for stop in stops {
        let Some(c) = stop_point(network, stop) else { continue };
        if out.last().is_some_and(|l| l.lat == c.lat && l.lon == c.lon) {
            continue;
        }
        out.push(c);
    }
    if out.len() < 2 {
        out.clear();
    }
    out
}

/// What a drill-down can frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum EntityKind {
    Stop,
    Route,
    Pattern,
    Schedule,
    ServiceType,
    Block,
}

/// An entity on the map: the lines it runs and the stops it serves.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct EntityFocus {
    pub lines: Vec<Vec<Coord>>,
    pub points: Vec<Coord>,
}

/// Where an entity is: a stop is its point; a pattern (and a schedule,
/// through its pattern) its alignment and stops; a route every one of
/// its patterns; a service type every pattern scheduled under it; a
/// block every pattern its trips run. None for an unknown id.
pub fn entity_focus(network: &Network, kind: EntityKind, id: &str) -> Option<EntityFocus> {
    let patterns: Vec<&str> = match kind {
        EntityKind::Stop => {
            return stop_point(network, id).map(|c| EntityFocus { lines: vec![], points: vec![c] });
        }
        EntityKind::Pattern => network.patterns.get_key_value(id).map(|(k, _)| vec![k.as_str()])?,
        EntityKind::Schedule => vec![network.schedules.get(id)?.pattern.as_str()],
        EntityKind::Route => {
            network.routes.get(id)?;
            network.patterns.iter().filter(|(_, p)| p.route == id).map(|(k, _)| k.as_str()).collect()
        }
        EntityKind::ServiceType => {
            network.service_types.get(id)?;
            let mut ids: Vec<&str> =
                network.schedules.values().filter(|s| s.service_type == id).map(|s| s.pattern.as_str()).collect();
            ids.sort();
            ids.dedup();
            ids
        }
        EntityKind::Block => {
            let block = network.blocks.get(id)?;
            let mut ids: Vec<&str> = block
                .trips
                .iter()
                .filter_map(|t| network.schedules.get(&t.schedule).map(|s| s.pattern.as_str()))
                .collect();
            ids.sort();
            ids.dedup();
            ids
        }
    };
    let mut focus = EntityFocus::default();
    for p in patterns {
        let line = pattern_alignment(network, p);
        if !line.is_empty() {
            focus.lines.push(line);
        }
        focus.points.extend(pattern_stop_points(network, p));
    }
    Some(focus)
}
