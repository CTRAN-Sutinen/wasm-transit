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
//!
//! And the stop question asked from a POINT: the places nearest it,
//! a station's bays answering as the station — and from what a rider
//! TYPES: the stops a query finds, exact matches first, then a sign's
//! abbreviations, then typos (D-48, [`find_stops`]).

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::geometry::EntityKind;
use crate::materialize::materialize_service;
use crate::model::{Departs, Network};

/// One entity as a list shows it. `context` is what places it (a
/// pattern's route, a schedule's pattern and service type); `trips`
/// is what a schedule runs or a block chains.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
pub struct EntityRow {
    pub id: String,
    pub name: String,
    pub context: Option<String>,
    /// A designated transfer point (stops only).
    pub transfer: bool,
    pub trips: Option<u32>,
    /// A route's long name ("Lincoln") beside its short one (routes
    /// only; `None` for a route without one).
    #[serde(default)]
    pub long_name: Option<String>,
}

/// One service type's schedules on a route.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS, schemars::JsonSchema)]
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
pub struct RouteDetail {
    pub patterns: Vec<EntityRow>,
    pub stops: Vec<EntityRow>,
    pub pattern_trips_by_type: Vec<PatternTripsGroup>,
    pub directions: Vec<DirectionGroup>,
}

/// One direction's patterns on a route. `direction` is `None` for the
/// patterns placed in none (listed last, when there are any).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
pub struct DirectionGroup {
    pub direction: Option<String>,
    pub name: String,
    pub patterns: Vec<EntityRow>,
    /// The ways a rider can ride the direction, most trips first.
    #[serde(default)]
    pub variants: Vec<RouteVariant>,
}

/// One way a rider can ride a direction: the direction's patterns that
/// visit the same PLACES in the same order — a station's bays are one
/// place (the walking group its board departs across), so patterns
/// that differ only by the bay they use read as one. Any other
/// difference (a branch, a stop served only at some hours, a short
/// trip) is its own variant.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
pub struct RouteVariant {
    /// "First place → last place"; when another variant of the
    /// direction shares its ends, what sets it apart from the one with
    /// the most trips: "via" a place only it visits, else "skips" the
    /// first place it does not.
    pub name: String,
    /// Its patterns, by id.
    pub patterns: Vec<String>,
    /// Its places, in order.
    pub stops: Vec<VariantStop>,
    /// The trips its patterns' schedules run, every service type
    /// (instances, as the day expands them).
    pub trips: u32,
}

/// One place on a variant: the station (or the stop, when it is in no
/// station the network holds), and the stops its patterns use there
/// (a station's bays, in pattern order).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
pub struct VariantStop {
    pub place: String,
    pub name: String,
    pub stops: Vec<String>,
}

/// A timetable: one route, one direction, one service COMBINATION
/// (D-55: the service types that run together on the same dates).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
pub struct Timetable {
    pub route: String,
    pub route_name: String,
    /// The route's long name, when it has one.
    pub route_long_name: Option<String>,
    pub direction: Option<String>,
    pub direction_name: String,
    /// The combination this timetable pools: its id (members joined
    /// " + ") and its members — the service ids key trips, the
    /// combination is the day a rider gets.
    pub group: String,
    pub members: Vec<String>,
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

/// A baseline trip instance the proposal no longer runs. A trip the
/// proposal keeps under a new start is not removed — it reads
/// retimed (`TimetableTrip::retimed_from_secs`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
pub struct RemovedTrip {
    pub trip: String,
    pub pattern_trips: String,
    pub start_secs: u32,
    pub pattern: String,
}

/// One timetable column.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS, schemars::JsonSchema)]
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS, schemars::JsonSchema)]
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
pub struct TimetableTrip {
    pub trip: String,
    pub pattern_trips: String,
    /// The service key the trip runs under (its pattern trips').
    pub service_type: String,
    pub start_secs: u32,
    pub pattern: String,
    pub block: Option<String>,
    pub times: Vec<Option<u32>>,
    /// The trip's running time end to end: its last stop's arrival
    /// less its start — each trip's own, so a slower evening reads
    /// as it runs.
    pub run_secs: u32,
    /// Asked against a baseline: the baseline does not run this trip.
    pub added: bool,
    /// Asked against a baseline: the baseline runs this one-start trip
    /// (the same trip id) at another start — its start there. A moved
    /// trip is kept, not removed and added (QAF-64).
    pub retimed_from_secs: Option<u32>,
}

/// One route a service key carries: the pattern trips under the key
/// on that route, and the trips they run.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct ServiceKeyRoute {
    pub route_id: String,
    pub name: String,
    pub pattern_trips: Vec<String>,
    pub trips: u32,
}

/// One service key as authored: its name, its calendar rule (weekdays,
/// date ranges, exceptions — the rule the calendar edits change), and
/// what runs on it.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct ServiceKey {
    pub id: String,
    pub name: String,
    pub rule: crate::model::CalendarRule,
    /// Its trips (materialized: a frequency trip counts its instances).
    pub trips: u32,
    /// The routes it carries, id order; a route it runs nothing on is
    /// no row.
    pub routes: Vec<ServiceKeyRoute>,
}

/// The service keys, in id order: each one's calendar as authored and
/// the routes and trips it carries — what a planner edits when the
/// calendar changes, and what moves when trips change keys.
pub fn service_keys(network: &Network) -> Vec<ServiceKey> {
    network
        .service_types
        .iter()
        .map(|(id, st)| {
            let mut routes: std::collections::BTreeMap<&str, (Vec<String>, u32)> = Default::default();
            for (pid, pt) in &network.pattern_trips {
                if pt.service_type != *id {
                    continue;
                }
                let n = materialize_service(network, pid).map(|t| t.len() as u32).unwrap_or(0);
                if n == 0 {
                    continue;
                }
                let Some(route) = network.patterns.get(&pt.pattern).map(|p| p.route.as_str()) else { continue };
                let e = routes.entry(route).or_default();
                e.0.push(pid.clone());
                e.1 += n;
            }
            let routes: Vec<ServiceKeyRoute> = routes
                .into_iter()
                .map(|(r, (pattern_trips, trips))| ServiceKeyRoute {
                    route_id: r.to_string(),
                    name: network.routes.get(r).map(|x| x.name.clone()).unwrap_or_default(),
                    pattern_trips,
                    trips,
                })
                .collect();
            ServiceKey {
                id: id.clone(),
                name: st.name.clone(),
                rule: st.rule.clone(),
                trips: routes.iter().map(|r| r.trips).sum(),
                routes,
            }
        })
        .collect()
}

/// A stop's composition: the routes whose patterns visit it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
pub struct StopDetail {
    pub routes: Vec<EntityRow>,
}

fn row(id: &str, name: &str) -> EntityRow {
    EntityRow { id: id.to_string(), name: name.to_string(), context: None, transfer: false, trips: None, long_name: None }
}

/// A route's row: its label, and its long name beside it.
fn route_row(network: &Network, id: &str) -> EntityRow {
    match network.routes.get(id) {
        Some(r) => EntityRow { long_name: r.long_name.clone(), ..row(id, r.label()) },
        None => row(id, id),
    }
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
            context: Some(network.routes.get(&p.route).map_or(p.route.clone(), |r| r.label().to_string())),
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

/// Every entity of `kind`, in id order, whose id, name, and long name
/// (routes) together hold every whitespace-separated term of `query`
/// (case-insensitive; an empty query keeps all).
pub fn entity_rows(network: &Network, kind: EntityKind, query: &str) -> Vec<EntityRow> {
    let rows: Vec<EntityRow> = match kind {
        EntityKind::Stop => network.stops.keys().map(|id| stop_row(network, id)).collect(),
        EntityKind::Route => network.routes.keys().map(|id| route_row(network, id)).collect(),
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
            let haystack = format!("{} {} {}", r.id, r.name, r.long_name.as_deref().unwrap_or("")).to_lowercase();
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
            let ids: Vec<&String> =
                pattern_ids.iter().copied().filter(|pid| network.patterns[*pid].direction == direction).collect();
            let patterns: Vec<EntityRow> = ids.iter().map(|pid| pattern_row(network, pid)).collect();
            (direction.is_some() || !patterns.is_empty()).then(|| DirectionGroup {
                direction,
                name,
                patterns,
                variants: variants(network, &ids),
            })
        })
        .collect();
    Some(RouteDetail {
        patterns: pattern_ids.iter().map(|pid| pattern_row(network, pid)).collect(),
        stops,
        pattern_trips_by_type,
        directions,
    })
}

/// The place a rider walks to for a stop: its station when the network
/// holds one, else the stop itself (`stops_near`'s rule).
fn place_of<'a>(network: &'a Network, stop: &'a str) -> &'a str {
    match network.stops.get(stop).and_then(|s| s.parent.as_deref()) {
        Some(p) if network.stops.contains_key(p) => p,
        _ => stop,
    }
}

/// The patterns grouped by the places they visit, in order — see
/// [`RouteVariant`].
fn variants(network: &Network, pattern_ids: &[&String]) -> Vec<RouteVariant> {
    let name_of = |id: &str| network.stops.get(id).map_or(id.to_string(), |s| s.name.clone());
    let mut groups: std::collections::BTreeMap<Vec<&str>, Vec<&String>> = Default::default();
    for pid in pattern_ids {
        let places = network.patterns[*pid].stops.iter().map(|ps| place_of(network, &ps.stop)).collect();
        groups.entry(places).or_default().push(pid);
    }
    let trips_of = |pid: &str| -> u32 {
        network
            .pattern_trips
            .values()
            .filter(|pt| pt.pattern == pid)
            .flat_map(|pt| pt.trips.iter())
            .map(|t| t.departs.instances().len() as u32)
            .sum()
    };
    let mut out: Vec<(Vec<&str>, RouteVariant)> = groups
        .into_iter()
        .map(|(places, pids)| {
            let stops = places
                .iter()
                .enumerate()
                .map(|(i, place)| {
                    let mut used: Vec<String> = Vec::new();
                    for pid in &pids {
                        let stop = &network.patterns[*pid].stops[i].stop;
                        if !used.contains(stop) {
                            used.push(stop.clone());
                        }
                    }
                    VariantStop { place: place.to_string(), name: name_of(place), stops: used }
                })
                .collect();
            let name = match (places.first(), places.last()) {
                (Some(a), Some(b)) if a != b => format!("{} → {}", name_of(a), name_of(b)),
                (Some(a), _) => name_of(a),
                _ => String::new(),
            };
            let variant = RouteVariant {
                name,
                patterns: pids.iter().map(|p| p.to_string()).collect(),
                stops,
                trips: pids.iter().map(|p| trips_of(p)).sum(),
            };
            (places, variant)
        })
        .collect();
    out.sort_by(|a, b| b.1.trips.cmp(&a.1.trips).then_with(|| a.1.patterns.cmp(&b.1.patterns)));

    // Variants sharing their ends are named by what sets each apart
    // from the one with the most trips.
    let bases: Vec<String> = out.iter().map(|(_, v)| v.name.clone()).collect();
    for i in 0..out.len() {
        let Some(main) = (0..i).find(|&j| bases[j] == bases[i]) else { continue };
        let (main_places, places) = (out[main].0.clone(), &out[i].0);
        let suffix = if let Some(only) = places.iter().find(|p| !main_places.contains(p)) {
            format!(" via {}", name_of(only))
        } else if let Some(skipped) = main_places.iter().find(|p| !places.contains(p)) {
            format!(", skips {}", name_of(skipped))
        } else {
            String::new()
        };
        out[i].1.name = format!("{}{suffix}", bases[i]);
    }
    out.into_iter().map(|(_, v)| v).collect()
}

/// The timetable of `route` in `direction` (`None`: its patterns in
/// no direction) under the service COMBINATION `members` (D-55): every
/// member's trips on the direction's patterns pooled — the types that
/// run together on its dates, the service ids keying trips, never
/// owning the route. `None` for an unknown route or direction, or a
/// member set that never runs together on a date. A pattern whose
/// trips cannot materialize (an invalid document mid-edit) contributes
/// none — validation names it.
pub fn timetable(network: &Network, route: &str, direction: Option<&str>, members: &[String]) -> Option<Timetable> {
    let combination = crate::service_combination(network, members)?;
    pooled_timetable(network, route, direction, &combination.members)
}

/// The timetable over `members`' trips, whether or not they run
/// together in this network (a baseline the combination is new to
/// runs what its members run there).
fn pooled_timetable(network: &Network, route: &str, direction: Option<&str>, members: &[String]) -> Option<Timetable> {
    let r = network.routes.get(route)?;
    let direction_name = match direction {
        Some(d) => r.directions.get(d)?.name.clone(),
        None => "No direction".to_string(),
    };
    let mut members: Vec<String> = members.to_vec();
    members.sort();
    members.dedup();

    // The direction's patterns under the combination, with their
    // trips — every member's schedules pool (service ids key trips).
    let family: std::collections::BTreeSet<&str> = members.iter().map(String::as_str).collect();
    let mut by_pattern: std::collections::BTreeMap<&str, (Vec<String>, Vec<crate::MaterializedTrip>, bool)> =
        Default::default();
    for (id, pt) in &network.pattern_trips {
        let Some(p) = network.patterns.get(&pt.pattern) else { continue };
        if !family.contains(pt.service_type.as_str()) || p.route != route || p.direction.as_deref() != direction {
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
                service_type: network.pattern_trips.get(&trip.pattern_trips).map(|pt| pt.service_type.clone()).unwrap_or_default(),
                start_secs: trip.start_secs,
                pattern: trip.pattern.clone(),
                run_secs: trip.stop_times.last().map_or(0, |st| st.arrival_secs.saturating_sub(trip.start_secs)),
                times,
                added: false,
                retimed_from_secs: None,
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
        route_name: r.label().to_string(),
        route_long_name: r.long_name.clone(),
        direction: direction.map(str::to_string),
        direction_name,
        group: members.join(" + "),
        members,
        stops,
        patterns,
        trips,
        removed: Vec::new(),
    })
}

/// The proposal's timetable marked against a baseline. A trip's id
/// is its identity: a one-start trip both run at different starts is
/// RETIMED (it carries its baseline start), one only the proposal runs
/// is `added`, and one only the baseline runs is `removed`. A
/// frequency trip's instances share its id, so each instance is
/// matched by (trip, start) — kept, added, or removed. The baseline
/// side runs what the combination's members run there (a baseline
/// without this route, direction, or members runs none of them).
pub fn timetable_against(
    network: &Network,
    base: &Network,
    route: &str,
    direction: Option<&str>,
    members: &[String],
) -> Option<Timetable> {
    use std::collections::{BTreeMap, BTreeSet};
    let mut t = timetable(network, route, direction, members)?;
    let before = pooled_timetable(base, route, direction, &t.members).map(|b| b.trips).unwrap_or_default();
    // A trip id with exactly one instance is a one-start trip.
    let singles = |trips: &[TimetableTrip]| -> BTreeMap<String, u32> {
        let mut seen: BTreeMap<String, Option<u32>> = BTreeMap::new();
        for x in trips {
            seen.entry(x.trip.clone()).and_modify(|s| *s = None).or_insert(Some(x.start_secs));
        }
        seen.into_iter().filter_map(|(id, s)| Some((id, s?))).collect()
    };
    let (was_single, now_single) = (singles(&before), singles(&t.trips));
    let kept = |id: &str| was_single.contains_key(id) && now_single.contains_key(id);
    let key = |trip: &str, s: u32| (trip.to_string(), s);
    let now: BTreeSet<_> = t.trips.iter().map(|x| key(&x.trip, x.start_secs)).collect();
    let was: BTreeSet<_> = before.iter().map(|x| key(&x.trip, x.start_secs)).collect();
    for trip in &mut t.trips {
        if kept(&trip.trip) {
            let old = was_single[&trip.trip];
            trip.retimed_from_secs = (old != trip.start_secs).then_some(old);
        } else {
            trip.added = !was.contains(&key(&trip.trip, trip.start_secs));
        }
    }
    t.removed = before
        .into_iter()
        .filter(|x| !kept(&x.trip) && !now.contains(&key(&x.trip, x.start_secs)))
        .map(|x| RemovedTrip { trip: x.trip, pattern_trips: x.pattern_trips, start_secs: x.start_secs, pattern: x.pattern })
        .collect();
    Some(t)
}

/// The routes serving a stop, in id order; `None` for an unknown
/// stop. The board's rule: a stop with service of its own answers
/// its own routes and never pools, and a stop no pattern visits (a
/// station pin whose patterns visit only its bays) answers what its
/// WALKING GROUP answers — the same group the station board departs
/// across.
pub fn stop_detail(network: &Network, stop: &str) -> Option<StopDetail> {
    network.stops.get(stop)?;
    let index = crate::validate::stop_route_index(network);
    let routes = match index.get(stop) {
        Some(own) if !own.is_empty() => own.clone(),
        _ => crate::validate::routes_meeting_at(network, &index, stop),
    };
    let routes = routes
        .into_iter()
        .map(|rid| route_row(network, &rid))
        .collect();
    Some(StopDetail { routes })
}

/// One place near a point, as the directory's stop question answers
/// it from a location: the stop's identity, name, point and
/// walking-group structure (the same fields a `stops` row carries),
/// and how far it is.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
pub struct NearbyStop {
    pub stop_id: String,
    pub name: String,
    pub lat: f64,
    pub lon: f64,
    /// The parent station: `None` for every place but a bay whose
    /// station the network does not hold (a bay of a held station
    /// answers as that station).
    pub parent: Option<String>,
    /// A designated transfer point.
    pub transfer: bool,
    /// Great-circle meters from the point to the place's nearest
    /// member (the station's pin or any of its bays).
    pub distance_m: f64,
    /// Seconds on foot from the point to the place's nearest member,
    /// when the question named the point's walks and one reaches it.
    pub walk_secs: Option<u32>,
}

/// Great-circle distance in meters (haversine, mean earth radius).
fn haversine_m(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let (p1, p2) = (lat1.to_radians(), lat2.to_radians());
    let (dp, dl) = ((lat2 - lat1).to_radians(), (lon2 - lon1).to_radians());
    let a = (dp / 2.0).sin().powi(2) + p1.cos() * p2.cos() * (dl / 2.0).sin().powi(2);
    2.0 * 6_371_000.0 * a.sqrt().asin()
}

/// The places nearest a point, nearest first (ties by id), at most
/// `limit`, and — given `radius_m` — none farther than it. A PLACE is
/// what a rider walks to: a station with its bays answers once, as
/// the station (the walking group its board departs across), at the
/// distance of its nearest member; a stop with no station held by the
/// network is its own place.
pub fn stops_near(network: &Network, lat: f64, lon: f64, limit: usize, radius_m: Option<f64>) -> Vec<NearbyStop> {
    stops_near_walked(network, lat, lon, limit, radius_m, &[])
}

/// [`stops_near`] with the point's walks to stops (`(stop id,
/// walk_secs)`, cap-streets `access` along the streets): a place's
/// walk is the shortest to any of its members, and the places a walk
/// reaches come first, by walk (then distance, then id); the rest
/// follow by distance (D-34). The walks reorder the places and
/// nothing else.
pub fn stops_near_walked(
    network: &Network,
    lat: f64,
    lon: f64,
    limit: usize,
    radius_m: Option<f64>,
    walks: &[(String, u32)],
) -> Vec<NearbyStop> {
    let place_of = |id: &str| -> Option<&str> {
        let (id, s) = network.stops.get_key_value(id)?;
        Some(match s.parent.as_deref() {
            Some(p) if network.stops.contains_key(p) => p,
            _ => id.as_str(),
        })
    };
    let mut nearest: std::collections::BTreeMap<&str, (f64, Option<u32>)> = Default::default();
    for (id, s) in &network.stops {
        let place = place_of(id).expect("a held stop");
        let d = haversine_m(lat, lon, s.lat, s.lon);
        let e = nearest.entry(place).or_insert((f64::INFINITY, None));
        e.0 = e.0.min(d);
    }
    for (id, secs) in walks {
        let Some(place) = place_of(id) else { continue };
        if let Some(e) = nearest.get_mut(place) {
            e.1 = Some(e.1.map_or(*secs, |w| w.min(*secs)));
        }
    }
    let mut rows: Vec<(&str, (f64, Option<u32>))> =
        nearest.into_iter().filter(|(_, (d, _))| radius_m.is_none_or(|r| *d <= r)).collect();
    rows.sort_by(|(a_id, (a_d, a_w)), (b_id, (b_d, b_w))| {
        (a_w.is_none(), a_w.unwrap_or(0))
            .cmp(&(b_w.is_none(), b_w.unwrap_or(0)))
            .then(a_d.total_cmp(b_d))
            .then_with(|| a_id.cmp(b_id))
    });
    rows.into_iter()
        .take(limit)
        .map(|(id, (distance_m, walk_secs))| {
            let s = &network.stops[id];
            NearbyStop {
                stop_id: id.to_string(),
                name: s.name.clone(),
                lat: s.lat,
                lon: s.lon,
                parent: s.parent.clone(),
                transfer: s.transfer,
                distance_m,
                walk_secs,
            }
        })
        .collect()
}

/// How a typed query answers a stop's name (D-48; thresholds D-91),
/// best first: the order a picker lists matches in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum NameMatch {
    /// Every term is a substring of the name or the id.
    Exact,
    /// At least one term is answered by an abbreviation on the sign
    /// ("Van" for "vancouver", "St" for "street"), the rest exactly.
    Abbreviation,
    /// At least one term is answered only within a typo of a name word.
    Typo,
}

/// How `query` answers a stop named `name` with id `id`: each
/// whitespace-separated term, case-insensitive, must be answered —
/// as a substring of the name or the id (exact); by a name word that
/// abbreviates it (a word of three letters or more that begins the
/// term, or a word that is a subsequence of the term sharing its first
/// and last letter); or within a typo of a name word or of a word's
/// start (one edit — a letter substituted, added, dropped, or two
/// neighbours swapped — from four letters, two from eight; never
/// under four). The worst term's answer is the stop's; `None` when a
/// term has none. An empty query answers every stop exactly.
pub fn name_match(query: &str, name: &str, id: &str) -> Option<NameMatch> {
    let haystack = format!("{name} {id}").to_lowercase();
    let words: Vec<Vec<char>> = name
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(|w| w.chars().collect())
        .collect();
    let mut worst = NameMatch::Exact;
    for term in query.split_whitespace().map(str::to_lowercase) {
        let answer = if haystack.contains(term.as_str()) {
            NameMatch::Exact
        } else {
            let t: Vec<char> = term.chars().collect();
            if words.iter().any(|w| abbreviates(w, &t)) {
                NameMatch::Abbreviation
            } else if words.iter().any(|w| within_typo(&t, w)) {
                NameMatch::Typo
            } else {
                return None;
            }
        };
        worst = worst.max(answer);
    }
    Some(worst)
}

/// The stops `query` finds ([`name_match`]), best match first, then by
/// name, then id — the pickers' order; no query, every stop by name.
pub fn find_stops(network: &Network, query: &str) -> Vec<String> {
    let mut found: Vec<(NameMatch, &str, &String)> = network
        .stops
        .iter()
        .filter_map(|(id, s)| name_match(query, &s.name, id).map(|m| (m, s.name.as_str(), id)))
        .collect();
    found.sort();
    found.into_iter().map(|(_, _, id)| id.clone()).collect()
}

/// A sign's word `w` abbreviates the typed term `t`.
fn abbreviates(w: &[char], t: &[char]) -> bool {
    if w.len() >= t.len() || w.len() < 2 {
        return false;
    }
    if w.len() >= 3 && t.starts_with(w) {
        return true;
    }
    if w[0] != t[0] || w.last() != t.last() {
        return false;
    }
    let mut rest = t.iter();
    w.iter().all(|c| rest.any(|x| x == c))
}

/// The typed term `t` is within a typo of the word `w` or of its start.
fn within_typo(t: &[char], w: &[char]) -> bool {
    let budget = match t.len() {
        0..=3 => return false,
        4..=7 => 1,
        _ => 2,
    };
    let lo = t.len().saturating_sub(budget).max(1);
    let hi = (t.len() + budget).min(w.len());
    (lo..=hi).any(|k| edits(t, &w[..k]) <= budget)
}

/// The optimal-string-alignment distance: substitutions, insertions,
/// deletions, and adjacent transpositions.
fn edits(a: &[char], b: &[char]) -> usize {
    let (n, m) = (a.len(), b.len());
    let mut d = vec![vec![0usize; m + 1]; n + 1];
    for (i, row) in d.iter_mut().enumerate() {
        row[0] = i;
    }
    for j in 0..=m {
        d[0][j] = j;
    }
    for i in 1..=n {
        for j in 1..=m {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            d[i][j] = (d[i - 1][j] + 1).min(d[i][j - 1] + 1).min(d[i - 1][j - 1] + cost);
            if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                d[i][j] = d[i][j].min(d[i - 2][j - 2] + 1);
            }
        }
    }
    d[n][m]
}
