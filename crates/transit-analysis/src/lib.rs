//! Planner analysis: the questions a network planner asks, answered
//! from an authored document on one service date — at the network
//! level (how much service runs), the route level (span, frequency,
//! trips), and the stop level (what serves this corner, how often) —
//! plus population coverage against an imported layer of population
//! points ("what percent of the population does this network serve?
//! what percent with frequent service?").
//!
//! All of it is computed here, in the artifact, never in a host:
//! the same numbers everywhere is the point. Stats are best-effort
//! by design — a document mid-edit can carry broken services, and
//! analysis must still answer for the parts that work (validation
//! is the gate that blocks; this never is).

use std::collections::{BTreeMap, BTreeSet};

use chrono::NaiveDate;
use serde::Serialize;
use ts_rs::TS;

use transit_network::Network;

/// One stop's activity on a service date.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct StopStats {
    pub stop_id: String,
    pub name: String,
    /// Routes with at least one visit (count; ids live in the UI from
    /// the document itself — stats stay small).
    pub routes: u32,
    pub departures: u32,
    pub first_arrival_secs: Option<u32>,
    pub last_arrival_secs: Option<u32>,
    /// (last − first) / (departures − 1): the even-spacing headway the
    /// day's visits boil down to. Null below two visits.
    pub typical_headway_secs: Option<u32>,
    /// The stop's authored transfer-point flag.
    pub transfer: bool,
    /// Transfer points only: how many stops pool into the walking
    /// group (the stop, siblings sharing a parent, children, the
    /// parent — transfers happen across a street or down a block).
    pub transfer_group: Option<u32>,
    /// Transfer points only: ordered route pairs with at least one
    /// usable timed connection across the walking group on this
    /// date — an arrival of one route followed within the window
    /// (1–15 min, the walk folded into the bounds) by a departure of
    /// the other. Zero pairs means the flag is aspirational today.
    pub transfer_pairs: Option<u32>,
}

/// One route's activity on a service date.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct RouteStats {
    pub route_id: String,
    pub name: String,
    pub patterns: u32,
    /// Distinct stops across the route's patterns.
    pub stops: u32,
    /// Trips on the date (all of the route's patterns, all services).
    pub trips: u32,
    pub first_start_secs: Option<u32>,
    pub last_arrival_secs: Option<u32>,
    /// The route's headway today — the number a planner reads as
    /// "every N minutes": the MEDIAN gap between consecutive
    /// departures at the route's busiest stop (`headway_stop`).
    /// Departures only — a trip's final visit is an arrival — and a
    /// median, so a short-lived variant or a clustered pair of trips
    /// cannot stand in for the route. Null below two departures.
    pub typical_headway_secs: Option<u32>,
    /// Where the headway was measured: the stop with the most
    /// departures of this route today (ties by stop id).
    pub headway_stop: Option<String>,
    /// The route's revenue hours today: the sum of its trips'
    /// runtimes (arrival − start), the cost basis per route.
    pub revenue_hours: f64,
}

/// A service type's numbers on its representative day — the first
/// date its rule runs. Planners design service types (Weekday,
/// Saturday, …) and link them to days; this is the axis they scan,
/// independent of whichever calendar date the rest of the analysis
/// is pinned to.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct ServiceTypeSummary {
    pub service_type: String,
    pub name: String,
    /// The representative date these numbers were computed on.
    pub date: String,
    pub trips: u32,
    pub active_routes: u32,
    pub active_stops: u32,
    pub revenue_hours: f64,
}

/// The first date a rule runs on (deterministic), or None when the
/// rule runs on no date at all (an authored type with no days yet).
fn representative_date(rule: &transit_network::CalendarRule) -> Option<NaiveDate> {
    use chrono::Duration;
    for range in &rule.ranges {
        let mut date = range.start;
        while date <= range.end {
            if rule.runs_on(date) {
                return Some(date);
            }
            date += Duration::days(1);
        }
    }
    None
}

/// One schedule of a service type, counted: the drill-down's
/// schedule list row.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct PatternTripsSummary {
    pub pattern_trips: String,
    pub pattern: String,
    pub pattern_name: String,
    pub trips: u32,
}

/// A service type's whole day: the planner's drill-down. Everything
/// that runs under the type on its representative day — routes,
/// stops, schedules — plus the type's own numbers.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct ServiceTypeDetail {
    pub service_type: String,
    pub name: String,
    /// The representative day this describes: the first date the
    /// type's rule runs. Planners design types, not calendar dates.
    pub date: String,
    pub trips: u32,
    pub revenue_hours: f64,
    pub transfer_stops: u32,
    /// Routes running under this type (active rows only).
    pub routes: Vec<RouteStats>,
    /// Stops served under this type (served rows only).
    pub stops: Vec<StopStats>,
    pub pattern_trips: Vec<PatternTripsSummary>,
}

/// The drill-down: one service type expanded on its representative
/// day. Errors when the type is unknown or runs on no date (an
/// authored type with no days yet).
pub fn service_type_detail(net: &Network, type_id: &str) -> Result<ServiceTypeDetail, String> {
    let service_type = net
        .service_types
        .get(type_id)
        .ok_or_else(|| format!("unknown service type {type_id:?}"))?;
    let date = representative_date(&service_type.rule)
        .ok_or_else(|| format!("service type {type_id:?} runs on no date"))?;
    let stats = day_stats(net, date, Some(type_id));

    let mut pattern_trips = Vec::new();
    for (id, entry) in &net.pattern_trips {
        if &entry.service_type != type_id {
            continue;
        }
        pattern_trips.push(PatternTripsSummary {
            pattern_trips: id.clone(),
            pattern: entry.pattern.clone(),
            pattern_name: net
                .patterns
                .get(&entry.pattern)
                .map(|p| p.name.clone())
                .unwrap_or_else(|| entry.pattern.clone()),
            trips: transit_network::trip_starts(&entry.departures).len() as u32,
        });
    }

    Ok(ServiceTypeDetail {
        service_type: type_id.to_string(),
        name: service_type.name.clone(),
        date: date.format("%Y-%m-%d").to_string(),
        trips: stats.trips,
        revenue_hours: stats.revenue_hours,
        transfer_stops: stats.transfer_stops,
        routes: stats.routes.into_iter().filter(|r| r.trips > 0).collect(),
        stops: stats.stops.into_iter().filter(|s| s.departures > 0).collect(),
        pattern_trips,
    })
}

/// The overview table: every service type's numbers, each on its own
/// representative day. This — not a calendar date — is the planner's
/// design axis.
pub fn service_type_overview(net: &Network) -> Vec<ServiceTypeSummary> {
    service_type_summaries(net)
}

/// One service type's population coverage on its representative day.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct TypeCoverage {
    pub service_type: String,
    pub name: String,
    /// The representative day the coverage was measured on.
    pub date: String,
    pub served_pct: f64,
    pub frequent_pct: f64,
    pub served_stops: u32,
    pub frequent_stops: u32,
}

/// Coverage per service type — "what share of the population does
/// WEEKDAY service cover? Saturday service?" — each measured on the
/// type's representative day. `window` judges frequency inside a
/// time window (see [`CoverageWindow`]); `None` is the whole-day
/// average.
pub fn type_coverage(
    net: &Network,
    layer: &PopLayer,
    radius_m: f64,
    frequent_secs: u32,
    min_departures: u32,
    window: Option<CoverageWindow>,
) -> Vec<TypeCoverage> {
    let mut out = Vec::with_capacity(net.service_types.len());
    for (type_id, service_type) in &net.service_types {
        let Some(date) = representative_date(&service_type.rule) else {
            continue; // a type with no running days covers nothing
        };
        let stats = coverage_for_type(net, date, layer, radius_m, frequent_secs, min_departures, window, Some(type_id.as_str()));
        out.push(TypeCoverage {
            service_type: type_id.clone(),
            name: service_type.name.clone(),
            date: date.format("%Y-%m-%d").to_string(),
            served_pct: stats.served_pct,
            frequent_pct: stats.frequent_pct,
            served_stops: stats.served_stops,
            frequent_stops: stats.frequent_stops,
        });
    }
    out
}

/// Per-type numbers, each on its own representative day.
fn service_type_summaries(net: &Network) -> Vec<ServiceTypeSummary> {
    let mut out = Vec::with_capacity(net.service_types.len());
    for (type_id, service_type) in &net.service_types {
        let Some(date) = representative_date(&service_type.rule) else {
            out.push(ServiceTypeSummary {
                service_type: type_id.clone(),
                name: service_type.name.clone(),
                date: String::new(),
                trips: 0,
                active_routes: 0,
                active_stops: 0,
                revenue_hours: 0.0,
            });
            continue;
        };
        let mut trips = 0u32;
        let mut routes: BTreeSet<String> = BTreeSet::new();
        let mut stops: BTreeSet<String> = BTreeSet::new();
        let mut runtime_secs = 0u64;
        for (pattern_trips_id, pattern_trips) in &net.pattern_trips {
            if &pattern_trips.service_type != type_id {
                continue;
            }
            let Ok(materialized) = transit_network::materialize_service(net, pattern_trips_id) else {
                continue;
            };
            for trip in materialized {
                if let Some(pattern) = net.patterns.get(&trip.pattern) {
                    routes.insert(pattern.route.clone());
                }
                if let Some(end) = trip.stop_times.last() {
                    trips += 1;
                    stops.extend(trip.stop_times.iter().map(|st| st.stop.clone()));
                    runtime_secs += end.arrival_secs.saturating_sub(trip.start_secs) as u64;
                }
            }
        }
        out.push(ServiceTypeSummary {
            service_type: type_id.clone(),
            name: service_type.name.clone(),
            date: date.format("%Y-%m-%d").to_string(),
            trips,
            active_routes: routes.len() as u32,
            active_stops: stops.len() as u32,
            revenue_hours: runtime_secs as f64 / 3600.0,
        });
    }
    out
}

/// The whole network on one service date.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct NetworkStats {
    pub date: String,
    pub trips: u32,
    pub active_routes: u32,
    pub active_stops: u32,
    pub first_start_secs: Option<u32>,
    pub last_arrival_secs: Option<u32>,
    /// Sum of trip runtimes (arrival − start), hours. Revenue hours;
    /// layovers are the runcut's business, not the schedule's.
    pub revenue_hours: f64,
    /// Transfer points active on the date (flagged stops whose
    /// walking group is served by at least two routes today).
    pub transfer_stops: u32,
    /// The design axis: every service type's numbers, each on its
    /// own representative day.
    pub service_types: Vec<ServiceTypeSummary>,
    /// Every route, sorted by id — zero-trip routes are analysis too.
    pub routes: Vec<RouteStats>,
    /// Every stop, sorted by id.
    pub stops: Vec<StopStats>,
}

#[derive(Debug, Clone, Default)]
struct StopActivity {
    departures: u32,
    routes: BTreeSet<String>,
    first: Option<u32>,
    last: Option<u32>,
}

/// A usable timed connection: an arrival followed by a departure at
/// least this far apart (a rider needs a minute to move) …
const TRANSFER_WINDOW_MIN_SECS: u32 = 60;
/// … and at most this close (beyond 15 minutes the connection is
/// not a timed transfer; the walk across the street or down the
/// block is folded into these bounds).
const TRANSFER_WINDOW_MAX_SECS: u32 = 900;

/// Ordered route pairs with at least one usable timed connection:
/// `from`-route arrival, then a `to`-route departure inside the
/// window. Times are sorted arrival lists per route.
fn usable_transfer_pairs(pooled: &BTreeMap<&str, Vec<u32>>) -> u32 {
    let usable = |from: &[u32], to: &[u32]| {
        from.iter().any(|&a| {
            let i = to.partition_point(|&b| b < a + TRANSFER_WINDOW_MIN_SECS);
            i < to.len() && to[i] <= a + TRANSFER_WINDOW_MAX_SECS
        })
    };
    let routes: Vec<&&str> = pooled.keys().collect();
    let mut pairs = 0u32;
    for &from in &routes {
        for &to in &routes {
            if from != to && usable(&pooled[*from], &pooled[*to]) {
                pairs += 1;
            }
        }
    }
    pairs
}

impl StopActivity {
    fn visit(&mut self, route: &str, arrival: u32) {
        self.departures += 1;
        self.routes.insert(route.to_string());
        self.first = Some(self.first.map_or(arrival, |f| f.min(arrival)));
        self.last = Some(self.last.map_or(arrival, |l| l.max(arrival)));
    }

    fn typical_headway_secs(&self) -> Option<u32> {
        match (self.first, self.last) {
            (Some(f), Some(l)) if self.departures >= 2 => Some((l - f) / (self.departures - 1)),
            _ => None,
        }
    }
}

/// The median gap between consecutive times (sorted here); None
/// below two times. A median, not the span average: a busy hour and
/// a quiet evening each count for what they are.
fn median_gap_secs(times: &[u32]) -> Option<u32> {
    let mut sorted = times.to_vec();
    sorted.sort_unstable();
    let mut gaps: Vec<u32> = sorted.windows(2).map(|w| w[1] - w[0]).collect();
    if gaps.is_empty() {
        return None;
    }
    gaps.sort_unstable();
    Some(gaps[gaps.len() / 2])
}

/// Network stats for one service date. Best-effort: services that
/// fail to materialize (a document mid-edit) are skipped, not fatal.
pub fn network_stats(net: &Network, date: NaiveDate) -> NetworkStats {
    day_stats(net, date, None)
}

/// The stats engine: one service day, optionally scoped to a single
/// service type (the drill-down view — the type's own representative
/// day), in which case only that type's schedules count.
fn day_stats(net: &Network, date: NaiveDate, only_type: Option<&str>) -> NetworkStats {
    // Per-pattern trip starts on the date: per-direction headways.
    let mut pattern_starts: BTreeMap<String, Vec<u32>> = BTreeMap::new();
    // Per-route accumulators.
    let mut route_trips: BTreeMap<String, u32> = BTreeMap::new();
    let mut route_first: BTreeMap<String, u32> = BTreeMap::new();
    let mut route_last: BTreeMap<String, u32> = BTreeMap::new();
    let mut route_stop_sets: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut route_runtime: BTreeMap<String, u64> = BTreeMap::new();
    // (route, stop) → departure times on the date (final visits are
    // arrivals and never count): where the route's headway is read.
    let mut route_departures: BTreeMap<(String, String), Vec<u32>> = BTreeMap::new();
    // Per-stop accumulators.
    let mut stop_activity: BTreeMap<String, StopActivity> = BTreeMap::new();
    // (stop, route) → arrival times on the date: the pooled view a
    // transfer point's timed connections are measured against.
    let mut times_at: BTreeMap<(String, String), Vec<u32>> = BTreeMap::new();

    let mut trips = 0u32;
    let mut first_start: Option<u32> = None;
    let mut last_arrival: Option<u32> = None;
    let mut runtime_secs = 0u64;

    for (service_id, service) in &net.pattern_trips {
        let runs = net
            .service_types
            .get(&service.service_type)
            .is_some_and(|st| st.rule.runs_on(date))
            && only_type.map_or(true, |t| service.service_type == t);
        if !runs {
            continue;
        }
        // A mid-edit document can carry services that fail to
        // materialize; analysis skips them (validation reports them).
        let Ok(materialized) = transit_network::materialize_service(net, service_id) else {
            continue;
        };
        for trip in materialized {
            let Some(route) = net.patterns.get(&trip.pattern).map(|p| p.route.clone()) else {
                continue;
            };
            let Some(end) = trip.stop_times.last() else { continue };
            trips += 1;
            *route_trips.entry(route.to_string()).or_default() += 1;
            route_first
                .entry(route.to_string())
                .and_modify(|f| *f = (*f).min(trip.start_secs))
                .or_insert(trip.start_secs);
            route_last
                .entry(route.to_string())
                .and_modify(|l| *l = (*l).max(end.arrival_secs))
                .or_insert(end.arrival_secs);
            first_start = Some(first_start.map_or(trip.start_secs, |f| f.min(trip.start_secs)));
            last_arrival = Some(last_arrival.map_or(end.arrival_secs, |l| l.max(end.arrival_secs)));
            let runtime = (end.arrival_secs.saturating_sub(trip.start_secs)) as u64;
            runtime_secs += runtime;
            *route_runtime.entry(route.clone()).or_default() += runtime;
            for st in &trip.stop_times[..trip.stop_times.len() - 1] {
                route_departures
                    .entry((route.clone(), st.stop.clone()))
                    .or_default()
                    .push(st.departure_secs);
            }
            pattern_starts
                .entry(trip.pattern.clone())
                .or_default()
                .push(trip.start_secs);
            if let Some(pattern_stops) = net.patterns.get(&trip.pattern) {
                route_stop_sets
                    .entry(route.clone())
                    .or_default()
                    .extend(pattern_stops.stops.iter().map(|ps| ps.stop.clone()));
            }
            for st in &trip.stop_times {
                stop_activity
                    .entry(st.stop.clone())
                    .or_default()
                    .visit(&route, st.arrival_secs);
                times_at
                    .entry((st.stop.clone(), route.clone()))
                    .or_default()
                    .push(st.arrival_secs);
            }
        }
    }

    let mut routes = Vec::with_capacity(net.routes.len());
    for (route_id, route) in &net.routes {
        let route_trips = *route_trips.get(route_id).unwrap_or(&0);
        // The busiest stop (most departures; the first by id on a
        // tie — BTreeMap order) and its median departure gap.
        let busiest = route_departures
            .iter()
            .filter(|((r, _), _)| r == route_id)
            .fold(None::<(&String, &Vec<u32>)>, |best, ((_, stop), times)| match best {
                Some((_, b)) if b.len() >= times.len() => best,
                _ => Some((stop, times)),
            });
        let typical = busiest.and_then(|(_, times)| median_gap_secs(times));
        routes.push(RouteStats {
            route_id: route_id.clone(),
            name: route.name.clone(),
            patterns: pattern_starts
                .iter()
                .filter(|(pid, _)| {
                    net.patterns.get(*pid).is_some_and(|p| &p.route == route_id)
                })
                .count() as u32,
            stops: route_stop_sets
                .get(route_id)
                .map(|s| s.len())
                .unwrap_or(0) as u32,
            trips: route_trips,
            first_start_secs: route_first.get(route_id).copied(),
            last_arrival_secs: route_last.get(route_id).copied(),
            typical_headway_secs: typical,
            headway_stop: busiest.map(|(stop, _)| stop.clone()),
            revenue_hours: *route_runtime.get(route_id).unwrap_or(&0) as f64 / 3600.0,
        });
    }

    let mut stops = Vec::with_capacity(net.stops.len());
    let mut transfer_stops = 0u32;
    for (stop_id, stop) in &net.stops {
        let activity = stop_activity.get(stop_id);
        // Transfer points: the flag is a place, not a pin — the
        // group and its timed connections pool across the walking
        // group (across a street, down a block).
        let (transfer_group, transfer_pairs) = if stop.transfer {
            let group = transit_network::walking_group(net, stop_id);
            let mut pooled: BTreeMap<&str, Vec<u32>> = BTreeMap::new();
            for gid in group.keys() {
                for ((sid, rid), times) in &times_at {
                    if sid == gid {
                        pooled.entry(rid.as_str()).or_default().extend_from_slice(times);
                    }
                }
            }
            if pooled.len() >= 2 {
                transfer_stops += 1;
            }
            for times in pooled.values_mut() {
                times.sort_unstable();
            }
            (
                Some(group.len() as u32),
                Some(usable_transfer_pairs(&pooled)),
            )
        } else {
            (None, None)
        };
        stops.push(StopStats {
            stop_id: stop_id.clone(),
            name: stop.name.clone(),
            routes: activity.map(|a| a.routes.len()).unwrap_or(0) as u32,
            departures: activity.map(|a| a.departures).unwrap_or(0),
            first_arrival_secs: activity.and_then(|a| a.first),
            last_arrival_secs: activity.and_then(|a| a.last),
            typical_headway_secs: activity.and_then(|a| a.typical_headway_secs()),
            transfer: stop.transfer,
            transfer_group,
            transfer_pairs,
        });
    }

    NetworkStats {
        date: date.format("%Y-%m-%d").to_string(),
        trips,
        active_routes: route_trips.len() as u32,
        active_stops: stop_activity.len() as u32,
        first_start_secs: first_start,
        last_arrival_secs: last_arrival,
        revenue_hours: runtime_secs as f64 / 3600.0,
        transfer_stops,
        service_types: service_type_summaries(net),
        routes,
        stops,
    }
}

// ---------------------------------------------------------------------
// Population coverage
// ---------------------------------------------------------------------

/// One population point: a census block centroid with its headcount.
/// Point features keep coverage honest and cheap — polygon layers
/// would need point-in-polygon against every block, and the numbers
/// barely move for block-sized polygons.
#[derive(Debug, Clone)]
pub struct PopPoint {
    pub lat: f64,
    pub lon: f64,
    pub pop: f64,
}

/// An imported population layer: GeoJSON points plus the property
/// name the headcounts came from.
#[derive(Debug, Clone)]
pub struct PopLayer {
    pub pop_field: String,
    pub points: Vec<PopPoint>,
}

/// Property names tried, in order, when the caller does not name the
/// population field (common open-data spellings).
const POP_FIELD_CANDIDATES: [&str; 8] = [
    "POP", "POPULATION", "TOTAL_POP", "TOTAL_POPULATION", "POP10", "POP20", "pop", "population",
];

/// Parse a population layer from GeoJSON. Point features only;
/// features without the population property (or with a non-numeric
/// one) are skipped. `pop_field` picks the property; `None` tries the
/// common candidates and fails if none of them holds a number.
pub fn parse_pop_layer(bytes: &[u8], pop_field: Option<&str>) -> Result<PopLayer, String> {
    let value: serde_json::Value =
        serde_json::from_slice(bytes).map_err(|e| format!("population layer: {e}"))?;
    let features = value
        .get("features")
        .and_then(|f| f.as_array())
        .ok_or("population layer: not a FeatureCollection (no features array)")?;

    let points_of = |field: &str| -> Vec<PopPoint> {
        features
            .iter()
            .filter_map(|feature| {
                let coords = feature
                    .get("geometry")?
                    .get("coordinates")?
                    .as_array()?;
                // GeoJSON is [lon, lat].
                let lon = coords.first()?.as_f64()?;
                let lat = coords.get(1)?.as_f64()?;
                let pop = feature
                    .get("properties")?
                    .get(field)?
                    .as_f64()
                    .or_else(|| {
                        feature
                            .get("properties")?
                            .get(field)?
                            .as_str()
                            .and_then(|s| s.trim().parse::<f64>().ok())
                    })?;
                Some(PopPoint { lat, lon, pop })
            })
            .collect()
    };

    let (pop_field, points) = match pop_field {
        Some(field) => (field.to_string(), points_of(field)),
        None => {
            let mut found = None;
            for candidate in POP_FIELD_CANDIDATES {
                let points = points_of(candidate);
                if !points.is_empty() {
                    found = Some((candidate.to_string(), points));
                    break;
                }
            }
            found.ok_or(
                "population layer: no population property found (tried \
                 POP, POPULATION, TOTAL_POP, TOTAL_POPULATION, POP10, POP20); \
                 name it explicitly",
            )?
        }
    };
    if points.is_empty() {
        return Err(format!(
            "population layer: no usable point features with property {pop_field:?}"
        ));
    }
    Ok(PopLayer { pop_field, points })
}

/// "Frequent" inside a time window — the way planners state it:
/// "every 15 minutes, 07:00–19:00". A stop is frequent in the window
/// when no rider waits longer than `frequent_secs` anywhere in it:
/// the gap from the window's start to the first departure, every gap
/// between departures, and the gap from the last departure to the
/// window's end all fit. Departures only (a trip's final visit is an
/// arrival); every route at the stop pools, as a rider sees it.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, serde::Deserialize, TS)]
#[ts(export)]
pub struct CoverageWindow {
    pub start_secs: u32,
    pub end_secs: u32,
}

impl CoverageWindow {
    /// The longest wait inside the window over sorted departures; None
    /// when nothing departs inside it.
    fn worst_wait_secs(&self, sorted: &[u32]) -> Option<u32> {
        let inside: Vec<u32> = sorted
            .iter()
            .copied()
            .filter(|t| (self.start_secs..=self.end_secs).contains(t))
            .collect();
        let first = *inside.first()?;
        let last = *inside.last()?;
        let between = inside.windows(2).map(|w| w[1] - w[0]).max().unwrap_or(0);
        Some((first - self.start_secs).max(between).max(self.end_secs - last))
    }
}

/// Coverage answers for one service date: what share of the layer's
/// population is within `radius_m` of a stop with service, and what
/// share is near *frequent* service (headway at or under
/// `frequent_secs`, with at least `min_departures` visits so a
/// 2-trip pocket cannot read as frequent).
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct CoverageStats {
    pub date: String,
    pub pop_field: String,
    pub radius_m: f64,
    pub frequent_secs: u32,
    pub min_departures: u32,
    /// The window frequency was judged in; null = the whole-day
    /// average headway.
    pub window: Option<CoverageWindow>,
    pub total_pop: f64,
    pub served_pop: f64,
    pub frequent_pop: f64,
    pub served_pct: f64,
    pub frequent_pct: f64,
    pub served_stops: u32,
    pub frequent_stops: u32,
}

/// Great-circle distance in meters (haversine, mean earth radius).
fn distance_m(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let (p1, p2) = (lat1.to_radians(), lat2.to_radians());
    let dlat = p2 - p1;
    let dlon = (lon2 - lon1).to_radians();
    let a = (dlat / 2.0).sin().powi(2) + p1.cos() * p2.cos() * (dlon / 2.0).sin().powi(2);
    6_371_000.0 * 2.0 * a.sqrt().asin()
}

pub fn coverage(
    net: &Network,
    date: NaiveDate,
    layer: &PopLayer,
    radius_m: f64,
    frequent_secs: u32,
    min_departures: u32,
    window: Option<CoverageWindow>,
) -> CoverageStats {
    coverage_for_type(net, date, layer, radius_m, frequent_secs, min_departures, window, None)
}

fn coverage_for_type(
    net: &Network,
    date: NaiveDate,
    layer: &PopLayer,
    radius_m: f64,
    frequent_secs: u32,
    min_departures: u32,
    window: Option<CoverageWindow>,
    only_type: Option<&str>,
) -> CoverageStats {
    // Which stops have service today, and which are frequent. Visits
    // aggregate across every service and route calling at the stop —
    // two routes at 30 minutes each make one 15-minute stop.
    let mut activity: BTreeMap<String, StopActivity> = BTreeMap::new();
    // Departure times per stop (final visits excluded) — what the
    // windowed frequency test reads.
    let mut departures_at: BTreeMap<String, Vec<u32>> = BTreeMap::new();
    for (service_id, service) in &net.pattern_trips {
        let runs = net
            .service_types
            .get(&service.service_type)
            .is_some_and(|st| st.rule.runs_on(date))
            && only_type.map_or(true, |t| service.service_type == t);
        if !runs {
            continue;
        }
        let Ok(materialized) = transit_network::materialize_service(net, service_id) else {
            continue;
        };
        for trip in &materialized {
            let Some(route) = net.patterns.get(&trip.pattern).map(|p| p.route.clone()) else {
                continue;
            };
            for st in &trip.stop_times {
                activity
                    .entry(st.stop.clone())
                    .or_default()
                    .visit(&route, st.arrival_secs);
            }
            for st in trip.stop_times.iter().take(trip.stop_times.len().saturating_sub(1)) {
                departures_at.entry(st.stop.clone()).or_default().push(st.departure_secs);
            }
        }
    }
    for times in departures_at.values_mut() {
        times.sort_unstable();
    }
    let mut served: Vec<(f64, f64)> = Vec::new();
    let mut frequent: Vec<(f64, f64)> = Vec::new();
    for (stop_id, act) in &activity {
        if act.departures == 0 {
            continue;
        }
        let Some(stop) = net.stops.get(stop_id) else { continue };
        served.push((stop.lat, stop.lon));
        let is_frequent = match window {
            // Inside the window: the worst wait fits, with enough
            // departures that a lone pair cannot pass.
            Some(window) => {
                let departs = departures_at.get(stop_id).map(Vec::as_slice).unwrap_or(&[]);
                let inside = departs.iter().filter(|t| (window.start_secs..=window.end_secs).contains(t)).count();
                inside as u32 >= min_departures
                    && window.worst_wait_secs(departs).is_some_and(|w| w <= frequent_secs)
            }
            None => {
                act.departures >= min_departures
                    && act.typical_headway_secs().is_some_and(|h| h <= frequent_secs)
            }
        };
        if is_frequent {
            frequent.push((stop.lat, stop.lon));
        }
    }
    let served_stops = served.len() as u32;
    let frequent_stops = frequent.len() as u32;

    // Population within radius of a served stop / a frequent stop.
    // A cheap bounding check first: lat/lon degrees are bounded apart.
    let deg_lat = radius_m / 111_320.0;
    let mut total_pop = 0.0;
    let mut served_pop = 0.0;
    let mut frequent_pop = 0.0;
    for point in &layer.points {
        total_pop += point.pop;
        let near = |targets: &[(f64, f64)]| {
            targets.iter().any(|(lat, lon)| {
                (lat - point.lat).abs() <= deg_lat
                    && distance_m(point.lat, point.lon, *lat, *lon) <= radius_m
            })
        };
        if near(&served) {
            served_pop += point.pop;
            if near(&frequent) {
                frequent_pop += point.pop;
            }
        }
    }

    let pct = |part: f64| if total_pop > 0.0 { 100.0 * part / total_pop } else { 0.0 };
    CoverageStats {
        date: date.format("%Y-%m-%d").to_string(),
        pop_field: layer.pop_field.clone(),
        radius_m,
        frequent_secs,
        min_departures,
        window,
        total_pop,
        served_pop,
        frequent_pop,
        served_pct: pct(served_pop),
        frequent_pct: pct(frequent_pop),
        served_stops,
        frequent_stops,
    }
}
