//! What exists, as the planner lists it: one row per entity of a
//! kind (narrowed the way a filter box's typing does), and a route's
//! composition — its patterns, the stops they visit, its schedules
//! grouped by service type with the trips each one runs — and a
//! stop's, the routes whose patterns visit it. The trip
//! count is every trip's instances — the same expansion materialize
//! runs, so a list never promises a trip the day does not have.
//!
//! And the planner's TIMETABLE: one route, one direction, one service
//! type — every trip of the direction's patterns (bay and short-turn
//! variants together), one row per trip, one column per stop in the
//! route's order. Rows are `materialize_service`'s trips, so the
//! timetable is the day the model runs, never a second derivation.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::geometry::EntityKind;
use crate::materialize::materialize_service;
use crate::model::{Departs, Network};

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
/// visit order, pattern by pattern), its pattern trips by service
/// type (the design axis), and its patterns by direction (what a
/// timetable is asked for).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct RouteDetail {
    pub patterns: Vec<EntityRow>,
    pub stops: Vec<EntityRow>,
    pub pattern_trips_by_type: Vec<PatternTripsGroup>,
    pub directions: Vec<DirectionGroup>,
}

/// One direction's patterns on a route. `direction` is `None` for the
/// patterns placed in none (listed last, when there are any).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct DirectionGroup {
    pub direction: Option<String>,
    pub name: String,
    pub patterns: Vec<EntityRow>,
}

/// A timetable: one route, one direction, one service type.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Timetable {
    pub route: String,
    pub route_name: String,
    pub direction: Option<String>,
    pub direction_name: String,
    pub service_type: String,
    pub service_type_name: String,
    /// The columns: every stop the direction's patterns visit, in
    /// route order (a variant's stops sit where it leaves and rejoins
    /// the others).
    pub stops: Vec<TimetableStop>,
    /// The direction's patterns under this service type, most trips
    /// first — the legend a row's `pattern` points into.
    pub patterns: Vec<TimetablePattern>,
    /// Every trip, by start (then pattern trips id).
    pub trips: Vec<TimetableTrip>,
    /// Asked against a baseline (`timetable_against`): the baseline's
    /// trips this timetable no longer runs, by start. Empty otherwise.
    pub removed: Vec<RemovedTrip>,
}

/// A baseline trip instance the proposal no longer runs (a retimed
/// trip is removed at its old start and added at its new one: a row
/// is (trip, start)).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct RemovedTrip {
    pub trip: String,
    pub pattern_trips: String,
    pub start_secs: u32,
    pub pattern: String,
}

/// One timetable column.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct TimetableStop {
    pub stop: String,
    pub name: String,
    /// A timepoint on some pattern visiting it (a public timetable's
    /// columns are its timepoints).
    pub timepoint: bool,
}

/// One pattern in a timetable, with the pattern trips that run it
/// under the service type.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct TimetablePattern {
    pub pattern: String,
    pub name: String,
    pub pattern_trips: Vec<String>,
    pub trips: u32,
    /// Some of its trips are frequency trips.
    pub headway: bool,
}

/// One trip instance: its trip's id, its pattern trips, start and
/// pattern, the block carrying it, and its departure at each column
/// (`None` where its pattern does not stop).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct TimetableTrip {
    pub trip: String,
    pub pattern_trips: String,
    pub start_secs: u32,
    pub pattern: String,
    pub block: Option<String>,
    pub times: Vec<Option<u32>>,
    /// Asked against a baseline: the baseline does not run this trip.
    pub added: bool,
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
                trips: Some(s.trips.iter().map(|t| t.departs.instances().len()).sum::<usize>() as u32),
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
    let named = network.routes[route].directions.iter().map(|(id, d)| (Some(id.clone()), d.name.clone()));
    let directions = named
        .chain(std::iter::once((None, "No direction".to_string())))
        .filter_map(|(direction, name)| {
            let patterns: Vec<EntityRow> = pattern_ids
                .iter()
                .filter(|pid| network.patterns[**pid].direction == direction)
                .map(|pid| pattern_row(network, pid))
                .collect();
            (direction.is_some() || !patterns.is_empty()).then_some(DirectionGroup { direction, name, patterns })
        })
        .collect();
    Some(RouteDetail {
        patterns: pattern_ids.iter().map(|pid| pattern_row(network, pid)).collect(),
        stops,
        pattern_trips_by_type,
        directions,
    })
}

/// The SERVICE FAMILY: the base plus every service type that rides
/// along with it — the SAME WEEKLY PATTERN (a rider runs the same
/// KIND of days: the overlays are Mon–Fri like their 1-WKDY), and
/// never a day the base does not run (calendar subset, Added
/// exceptions included). A Tuesday-only or Mon+Wed service is its
/// own family however few days it runs. The agency's ids say the
/// family (1-WKDY/30, /31), the model's calendars prove it; service
/// ids key TRIPS, and the trips of one family are ONE service.
/// Empty for an unknown base.
pub fn service_family(network: &Network, base_id: &str) -> std::collections::BTreeSet<String> {
    let Some(base) = network.service_types.get(base_id) else {
        return Default::default();
    };
    let mut members = std::collections::BTreeSet::new();
    for (id, other) in &network.service_types {
        if other.rule.days != base.rule.days {
            continue;
        }
        let rides_along = other
            .rule
            .ranges
            .iter()
            .flat_map(|r| {
                let mut out = Vec::new();
                let mut d = r.start;
                while d <= r.end {
                    out.push(d);
                    d += chrono::Duration::days(1);
                }
                out
            })
            .chain(
                other
                    .rule
                    .exceptions
                    .iter()
                    .filter(|e| e.state == crate::DayState::Added)
                    .map(|e| e.date),
            )
            .all(|d| !other.rule.runs_on(d) || base.rule.runs_on(d));
        if rides_along {
            members.insert(id.clone());
        }
    }
    members
}

/// The timetable of `route` in `direction` (`None`: its patterns in
/// no direction) under `service_type` — the base's SERVICE FAMILY
/// pooled: every member's trips on the direction's patterns, the
/// service ids keying trips, never owning the route. `None` for an
/// unknown route, direction or service type. A pattern whose trips
/// cannot materialize (an invalid document mid-edit) contributes
/// none — validation names it.
pub fn timetable(network: &Network, route: &str, direction: Option<&str>, service_type: &str) -> Option<Timetable> {
    let r = network.routes.get(route)?;
    let direction_name = match direction {
        Some(d) => r.directions.get(d)?.name.clone(),
        None => "No direction".to_string(),
    };
    let service = network.service_types.get(service_type)?;

    // The direction's patterns under the base's FAMILY, with their
    // trips — every member's schedules pool (service ids key trips).
    let family = service_family(network, service_type);
    let mut by_pattern: std::collections::BTreeMap<&str, (Vec<String>, Vec<crate::MaterializedTrip>, bool)> =
        Default::default();
    for (id, pt) in &network.pattern_trips {
        let Some(p) = network.patterns.get(&pt.pattern) else { continue };
        if !family.contains(&pt.service_type) || p.route != route || p.direction.as_deref() != direction {
            continue;
        }
        let entry = by_pattern.entry(pt.pattern.as_str()).or_default();
        entry.0.push(id.clone());
        entry.1.extend(materialize_service(network, id).unwrap_or_default());
        entry.2 |= pt.trips.iter().any(|t| matches!(t.departs, Departs::Every { .. }));
    }
    let mut order: Vec<&str> = by_pattern.keys().copied().collect();
    order.sort_by(|a, b| by_pattern[b].1.len().cmp(&by_pattern[a].1.len()).then(a.cmp(b)));

    // Columns: the busiest pattern's stops, then each other pattern's
    // stops slotted in after the last stop it shares (greedy, so the
    // columns are a supersequence of every pattern's stops).
    let mut columns: Vec<&str> = Vec::new();
    for pid in &order {
        let mut at: Option<usize> = None;
        for ps in &network.patterns[*pid].stops {
            let from = at.map_or(0, |i| i + 1);
            at = Some(match columns[from..].iter().position(|c| *c == ps.stop) {
                Some(i) => from + i,
                None => {
                    columns.insert(from, ps.stop.as_str());
                    from
                }
            });
        }
    }
    let embed = |pid: &str| -> Vec<usize> {
        let mut next = 0;
        network.patterns[pid]
            .stops
            .iter()
            .map(|ps| {
                let i = next + columns[next..].iter().position(|c| *c == ps.stop).expect("columns embed every pattern");
                next = i + 1;
                i
            })
            .collect()
    };

    let block_of: std::collections::HashMap<&str, &str> = network
        .blocks
        .iter()
        .flat_map(|(bid, b)| b.trips.iter().map(move |t| (t.trip.as_str(), bid.as_str())))
        .collect();
    let mut trips: Vec<TimetableTrip> = Vec::new();
    for pid in &order {
        let slots = embed(pid);
        for trip in &by_pattern[pid].1 {
            let mut times = vec![None; columns.len()];
            for (st, &col) in trip.stop_times.iter().zip(&slots) {
                times[col] = Some(st.departure_secs);
            }
            trips.push(TimetableTrip {
                block: block_of.get(trip.trip.as_str()).map(|b| b.to_string()),
                trip: trip.trip.clone(),
                pattern_trips: trip.pattern_trips.clone(),
                start_secs: trip.start_secs,
                pattern: trip.pattern.clone(),
                times,
                added: false,
            });
        }
    }
    trips.sort_by(|a, b| {
        a.start_secs
            .cmp(&b.start_secs)
            .then_with(|| a.pattern_trips.cmp(&b.pattern_trips))
            .then_with(|| a.trip.cmp(&b.trip))
    });

    let stops = columns
        .iter()
        .map(|stop| TimetableStop {
            stop: stop.to_string(),
            name: network.stops.get(*stop).map_or(stop.to_string(), |s| s.name.clone()),
            timepoint: order
                .iter()
                .any(|pid| network.patterns[*pid].stops.iter().any(|ps| ps.stop == *stop && ps.timepoint)),
        })
        .collect();
    let patterns = order
        .iter()
        .map(|pid| {
            let (ids, ts, headway) = &by_pattern[pid];
            TimetablePattern {
                pattern: pid.to_string(),
                name: network.patterns[*pid].name.clone(),
                pattern_trips: ids.clone(),
                trips: ts.len() as u32,
                headway: *headway,
            }
        })
        .collect();
    Some(Timetable {
        route: route.to_string(),
        route_name: r.name.clone(),
        direction: direction.map(str::to_string),
        direction_name,
        service_type: service_type.to_string(),
        service_type_name: service.name.clone(),
        stops,
        patterns,
        trips,
        removed: Vec::new(),
    })
}

/// The proposal's timetable marked against a baseline: each trip the
/// baseline does not run is `added`, and the baseline's trips it no
/// longer runs are `removed`. A baseline without this route,
/// direction, or service type runs none of them.
pub fn timetable_against(
    network: &Network,
    base: &Network,
    route: &str,
    direction: Option<&str>,
    service_type: &str,
) -> Option<Timetable> {
    let mut t = timetable(network, route, direction, service_type)?;
    let before = timetable(base, route, direction, service_type).map(|b| b.trips).unwrap_or_default();
    let key = |trip: &str, s: u32| (trip.to_string(), s);
    let now: std::collections::BTreeSet<_> = t.trips.iter().map(|x| key(&x.trip, x.start_secs)).collect();
    let was: std::collections::BTreeSet<_> = before.iter().map(|x| key(&x.trip, x.start_secs)).collect();
    for trip in &mut t.trips {
        trip.added = !was.contains(&key(&trip.trip, trip.start_secs));
    }
    t.removed = before
        .into_iter()
        .filter(|x| !now.contains(&key(&x.trip, x.start_secs)))
        .map(|x| RemovedTrip { trip: x.trip, pattern_trips: x.pattern_trips, start_secs: x.start_secs, pattern: x.pattern })
        .collect();
    Some(t)
}

/// The routes serving a stop, in id order; `None` for an unknown
/// stop. A station answers what its bays answer: routes pool
/// across the stop's WALKING GROUP, the same group the station
/// board departs across — a parent station whose patterns visit
/// only its bays still answers every route that serves it.
pub fn stop_detail(network: &Network, stop: &str) -> Option<StopDetail> {
    network.stops.get(stop)?;
    let index = crate::validate::stop_route_index(network);
    let routes = crate::validate::routes_meeting_at(network, &index, stop)
        .into_iter()
        .map(|rid| row(rid.as_str(), network.routes.get(rid.as_str()).map_or(rid.as_str(), |r| r.name.as_str())))
        .collect();
    Some(StopDetail { routes })
}
