//! What exists, as the planner lists it: one row per entity of a
//! kind (narrowed the way a filter box's typing does), and a route's
//! composition — its patterns, the stops they visit, its schedules
//! grouped by service type with the trips each one runs — and a
//! stop's, the routes whose patterns visit it. The trip
//! count is `trip_starts` — the same expansion materialize runs, so
//! a list never promises a trip the day does not have.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::geometry::EntityKind;
use crate::materialize::trip_starts;
use crate::model::Network;

/// One entity as a list shows it. `context` is what places it (a
/// pattern's route, a schedule's pattern and service type); `trips`
/// is what a schedule runs or a block chains.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct EntityRow {
    pub id: String,
    pub name: String,
    pub context: Option<String>,
    /// A designated transfer point (stops only).
    pub transfer: bool,
    pub trips: Option<u32>,
}

/// One service type's schedules on a route.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PatternTripsGroup {
    pub service_type: String,
    pub name: String,
    pub pattern_trips: Vec<EntityRow>,
}

/// A route's composition: its patterns, the stops they visit (first
/// visit order, pattern by pattern), and its schedules by service
/// type (the design axis).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct RouteDetail {
    pub patterns: Vec<EntityRow>,
    pub stops: Vec<EntityRow>,
    pub pattern_trips_by_type: Vec<PatternTripsGroup>,
}

/// A stop's composition: the routes whose patterns visit it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct StopDetail {
    pub routes: Vec<EntityRow>,
}

fn row(id: &str, name: &str) -> EntityRow {
    EntityRow { id: id.to_string(), name: name.to_string(), context: None, transfer: false, trips: None }
}

fn stop_row(network: &Network, id: &str) -> EntityRow {
    match network.stops.get(id) {
        Some(s) => EntityRow { transfer: s.transfer, ..row(id, &s.name) },
        None => row(id, id),
    }
}

fn pattern_row(network: &Network, id: &str) -> EntityRow {
    match network.patterns.get(id) {
        Some(p) => EntityRow {
            context: Some(network.routes.get(&p.route).map_or(p.route.clone(), |r| r.name.clone())),
            ..row(id, &p.name)
        },
        None => row(id, id),
    }
}

fn schedule_row(network: &Network, id: &str) -> EntityRow {
    match network.pattern_trips.get(id) {
        Some(s) => {
            let pattern = network.patterns.get(&s.pattern).map_or(s.pattern.as_str(), |p| p.name.as_str());
            let service = network
                .service_types
                .get(&s.service_type)
                .map_or(s.service_type.as_str(), |t| t.name.as_str());
            EntityRow {
                context: Some(format!("{pattern} · {service}")),
                trips: Some(trip_starts(&s.departures).len() as u32),
                ..row(id, id)
            }
        }
        None => row(id, id),
    }
}

/// Every entity of `kind`, in id order, whose name and id together
/// hold every whitespace-separated term of `query` (case-insensitive;
/// an empty query keeps all).
pub fn entity_rows(network: &Network, kind: EntityKind, query: &str) -> Vec<EntityRow> {
    let rows: Vec<EntityRow> = match kind {
        EntityKind::Stop => network.stops.keys().map(|id| stop_row(network, id)).collect(),
        EntityKind::Route => network.routes.iter().map(|(id, r)| row(id, &r.name)).collect(),
        EntityKind::Pattern => network.patterns.keys().map(|id| pattern_row(network, id)).collect(),
        EntityKind::PatternTrips => network.pattern_trips.keys().map(|id| schedule_row(network, id)).collect(),
        EntityKind::ServiceType => network.service_types.iter().map(|(id, t)| row(id, &t.name)).collect(),
        EntityKind::Block => network
            .blocks
            .iter()
            .map(|(id, b)| EntityRow { trips: Some(b.trips.len() as u32), ..row(id, id) })
            .collect(),
    };
    let terms: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
    rows.into_iter()
        .filter(|r| {
            let haystack = format!("{} {}", r.id, r.name).to_lowercase();
            terms.iter().all(|t| haystack.contains(t.as_str()))
        })
        .collect()
}

/// The route's composition; `None` for an unknown route.
pub fn route_detail(network: &Network, route: &str) -> Option<RouteDetail> {
    network.routes.get(route)?;
    let pattern_ids: Vec<&String> =
        network.patterns.iter().filter(|(_, p)| p.route == route).map(|(id, _)| id).collect();
    let mut seen = std::collections::BTreeSet::new();
    let stops = pattern_ids
        .iter()
        .flat_map(|pid| network.patterns[*pid].stops.iter())
        .filter(|ps| seen.insert(ps.stop.as_str()))
        .map(|ps| stop_row(network, &ps.stop))
        .collect();
    let mut groups: std::collections::BTreeMap<&str, Vec<EntityRow>> = Default::default();
    for (id, s) in &network.pattern_trips {
        if pattern_ids.contains(&&s.pattern) {
            groups.entry(s.service_type.as_str()).or_default().push(schedule_row(network, id));
        }
    }
    let pattern_trips_by_type = groups
        .into_iter()
        .map(|(tid, pattern_trips)| PatternTripsGroup {
            service_type: tid.to_string(),
            name: network.service_types.get(tid).map_or(tid.to_string(), |t| t.name.clone()),
            pattern_trips,
        })
        .collect();
    Some(RouteDetail {
        patterns: pattern_ids.iter().map(|pid| pattern_row(network, pid)).collect(),
        stops,
        pattern_trips_by_type,
    })
}

/// The routes serving a stop, in id order; `None` for an unknown stop.
pub fn stop_detail(network: &Network, stop: &str) -> Option<StopDetail> {
    network.stops.get(stop)?;
    let serving: std::collections::BTreeSet<&str> = network
        .patterns
        .values()
        .filter(|p| p.stops.iter().any(|ps| ps.stop == stop))
        .map(|p| p.route.as_str())
        .collect();
    let routes = serving
        .into_iter()
        .map(|rid| row(rid, network.routes.get(rid).map_or(rid, |r| r.name.as_str())))
        .collect();
    Some(StopDetail { routes })
}
