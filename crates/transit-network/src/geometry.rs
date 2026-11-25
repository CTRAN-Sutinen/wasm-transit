//! Every view that draws a pattern calls [`pattern_alignment`], so every
//! host draws the same line.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::model::{Coord, Network};

pub fn pattern_alignment(network: &Network, pattern: &str) -> Vec<Coord> {
    let Some(p) = network.patterns.get(pattern) else { return Vec::new() };
    authored_alignment(network, pattern)
        .unwrap_or_else(|| stops_line(network, p.stops.iter().map(|ps| ps.stop.as_str())))
}

pub fn authored_alignment(network: &Network, pattern: &str) -> Option<Vec<Coord>> {
    let id = network.patterns.get(pattern)?.alignment.as_ref()?;
    let points = &network.alignments.get(id)?.points;
    (points.len() >= 2).then(|| points.iter().map(|p| Coord { lat: p.lat, lon: p.lon }).collect())
}

pub fn pattern_stop_points(network: &Network, pattern: &str) -> Vec<Coord> {
    network
        .patterns
        .get(pattern)
        .map(|p| p.stops.iter().filter_map(|ps| stop_point(network, &ps.stop)).collect())
        .unwrap_or_default()
}

pub fn stop_point(network: &Network, stop: &str) -> Option<Coord> {
    network.stops.get(stop).map(|s| Coord { lat: s.lat, lon: s.lon })
}

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum EntityKind {
    Stop,
    Route,
    Pattern,
    PatternTrips,
    ServiceType,
    Block,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
pub struct EntityFocus {
    pub lines: Vec<Vec<Coord>>,
    pub points: Vec<Coord>,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct RouteLines {
    pub route_id: String,
    pub name: String,
    pub color: Option<String>,
    pub lines: Vec<Vec<Coord>>,
}

/// `members` names service types. When it is given, only the patterns that
/// run trips under one of them draw. [`entity_focus`] frames the same lines.
pub fn route_lines(network: &Network, members: Option<&[String]>) -> Vec<RouteLines> {
    let runs = |pattern: &str| {
        members.map_or(true, |m| {
            network
                .pattern_trips
                .values()
                .any(|pt| pt.pattern == pattern && !pt.trips.is_empty() && m.contains(&pt.service_type))
        })
    };
    let mut out = Vec::new();
    for (route_id, route) in &network.routes {
        let mut lines: Vec<Vec<Coord>> = Vec::new();
        for (pattern_id, pattern) in &network.patterns {
            if pattern.route != *route_id || !runs(pattern_id) {
                continue;
            }
            let line = pattern_alignment(network, pattern_id);
            if !line.is_empty() && !lines.contains(&line) {
                lines.push(line);
            }
        }
        if !lines.is_empty() {
            out.push(RouteLines { route_id: route_id.clone(), name: route.name.clone(), color: route.color.clone(), lines });
        }
    }
    out
}

pub fn entity_focus(network: &Network, kind: EntityKind, id: &str) -> Option<EntityFocus> {
    let patterns: Vec<&str> = match kind {
        EntityKind::Stop => {
            return stop_point(network, id).map(|c| EntityFocus { lines: vec![], points: vec![c] });
        }
        EntityKind::Pattern => network.patterns.get_key_value(id).map(|(k, _)| vec![k.as_str()])?,
        EntityKind::PatternTrips => vec![network.pattern_trips.get(id)?.pattern.as_str()],
        EntityKind::Route => {
            network.routes.get(id)?;
            network.patterns.iter().filter(|(_, p)| p.route == id).map(|(k, _)| k.as_str()).collect()
        }
        EntityKind::ServiceType => {
            network.service_types.get(id)?;
            let mut ids: Vec<&str> =
                network.pattern_trips.values().filter(|s| s.service_type == id).map(|s| s.pattern.as_str()).collect();
            ids.sort();
            ids.dedup();
            ids
        }
        EntityKind::Block => {
            let block = network.blocks.get(id)?;
            let mut ids: Vec<&str> = block
                .trips
                .iter()
                .filter_map(|t| network.trip(&t.trip).map(|(_, pt, _)| pt.pattern.as_str()))
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
