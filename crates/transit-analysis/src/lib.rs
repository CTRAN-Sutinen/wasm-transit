//! Every answer is computed here, so every host gets the same numbers. A
//! document in the middle of an edit can carry broken services. Analysis
//! answers for the parts that work, and validation does the gating.
//!
//! - A service group is a combination of the types that run on the same dates.
//!   Its numbers pool only over those dates, and no representative day stands
//!   in.
//! - Frequencies use the trips' own gaps. "Frequent" counts departures only, so
//!   a terminal where nobody boards is never frequent
//!   (`a_terminal_nobody_boards_at_is_never_frequent`). Counting arrivals
//!   would call a terminal frequent that no rider can leave from.
//! - Coverage takes any layer with any numeric weight and a planner's label
//!   (D-97). A polygon's weight spreads over a lattice (D-98), and a
//!   comparison names its two sides (D-99).
//! - Worklist findings are keyed, so a plan reads each as new, moved, resolved,
//!   or watched (D-136; D-133, D-134, D-137).
//! - A transfer point is the feed's flag, never inferred.

use std::collections::{BTreeMap, BTreeSet};

use transit_date::Date;
use serde::Serialize;
#[cfg(feature = "bindings")]
use ts_rs::TS;

use transit_network::Network;

mod stats;
pub use stats::{RouteStats, StopStats}; 
mod findings;
pub use findings::{
    FindingParams, FrequencyHole, LongWait, CostlyMiss, HoleRow, WaitRow, route_frequency,
    hub_connections, network_findings
};
mod coverage;
pub mod load;
pub use coverage::{
    coverage, coverage_compare, parse_layer, unserved_points, CoverageDelta, CoverageSide, CoverageStats,
    CoverageWindow, LayerField, LayerReading, Rings, UnservedPlaces, UnservedPoint, WeightedLayer, Weights,
};
use coverage::coverage_of_members;

mod calendar;
pub use calendar::{
    network_stats, day_types, DayTypeGroup, plan_compare, type_coverage, service_type_overview
};

mod stops;
pub use stops::{StopHourGaps, StopGaps, StopWait, stop_gaps};

mod transfers;
pub use transfers::{
    longest_waits, HourlyTrips, HourlyTypeTrips, MissedConnection, missed_connections,
    timed_transfers, TransferWindow, hourly_trips, route_overlap,
};

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "bindings", derive(TS))]
#[cfg_attr(feature = "bindings", ts(export))]
pub struct ServiceTypeSummary {
    pub service_type: String,
    pub name: String,
    pub trips: u32,
    pub active_routes: u32,
    pub active_stops: u32,
    pub revenue_hours: f64,
    pub revenue_miles: f64,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "bindings", derive(TS))]
#[cfg_attr(feature = "bindings", ts(export))]
pub struct PatternTripsSummary {
    pub pattern_trips: String,
    pub pattern: String,
    pub pattern_name: String,
    pub trips: u32,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "bindings", derive(TS))]
#[cfg_attr(feature = "bindings", ts(export))]
pub struct ServiceTypeDetail {
    pub group: String,
    pub members: Vec<String>,
    pub trips: u32,
    pub revenue_hours: f64,
    pub revenue_miles: f64,
    pub transfer_stops: u32,
    pub routes: Vec<RouteStats>,
    pub stops: Vec<StopStats>,
    pub pattern_trips: Vec<PatternTripsSummary>,
}

/// `None` when the members never run together on a date.
pub fn service_type_detail(net: &Network, members: &[String]) -> Option<ServiceTypeDetail> {
    let combination = transit_network::service_combination(net, members)?;
    Some(pooled_detail(net, &combination.members))
}

/// A baseline that the combination is new to still runs what its members run
/// there.
fn pooled_detail(net: &Network, members: &[String]) -> ServiceTypeDetail {
    let group: BTreeSet<String> = members.iter().cloned().collect();
    let stats = family_stats(net, &group);

    let mut pattern_trips = Vec::new();
    for (id, entry) in &net.pattern_trips {
        if !group.contains(&entry.service_type) {
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
            trips: entry.trips.iter().map(|t| t.departs.instances().len()).sum::<usize>() as u32,
        });
    }

    ServiceTypeDetail {
        group: group.iter().cloned().collect::<Vec<_>>().join(" + "),
        members: group.into_iter().collect(),
        trips: stats.trips,
        revenue_hours: stats.revenue_hours,
        revenue_miles: stats.revenue_miles,
        transfer_stops: stats.transfer_stops,
        routes: stats.routes.into_iter().filter(|r| r.trips > 0).collect(),
        stops: stats.stops.into_iter().filter(|s| s.departures > 0).collect(),
        pattern_trips,
    }
}

/// Starts pool across every schedule that runs the pattern, because one pattern
/// runs under several service ids and a per-id read gives thin, wrong headways.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "bindings", derive(TS))]
#[cfg_attr(feature = "bindings", ts(export))]
pub struct PatternTexture {
    pub schedules: u32,
    pub route: String,
    pub pattern: String,
    pub trips: u32,
    pub min_gap_secs: Option<u32>,
    pub median_gap_secs: Option<u32>,
    pub max_gap_secs: Option<u32>,
    pub on_clockface: u32,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "bindings", derive(TS))]
#[cfg_attr(feature = "bindings", ts(export))]
pub struct ScheduleTexture {
    pub group: String,
    pub members: Vec<String>,
    pub trips: u32,
    pub on_clockface: u32,
    pub pattern_trips: Vec<PatternTexture>,
}

pub fn schedule_texture(net: &Network, members: &[String]) -> Option<ScheduleTexture> {
    let combination = transit_network::service_combination(net, members)?;
    let group: BTreeSet<String> = combination.members.iter().cloned().collect();
    let mut patterns: BTreeSet<String> = BTreeSet::new();
    for entry in net.pattern_trips.values() {
        if group.contains(&entry.service_type) {
            patterns.insert(entry.pattern.clone());
        }
    }
    let mut rows = Vec::new();
    let mut trips = 0u32;
    let mut on_clockface = 0u32;
    for pattern in &patterns {
        let mut starts: Vec<u32> = Vec::new();
        let mut schedules = 0u32;
        for entry in net.pattern_trips.values() {
            if &entry.pattern != pattern {
                continue;
            }
            if !group.contains(&entry.service_type) {
                continue;
            }
            schedules += 1;
            starts.extend(entry.trips.iter().flat_map(|t| t.departs.instances()));
        }
        starts.sort_unstable();
        // Two service ids that start at the same second count as two trips but
        // make no gap.
        let trips_on_pattern = starts.len() as u32;
        starts.dedup();
        let gaps: Vec<u32> = starts.windows(2).map(|w| w[1] - w[0]).collect();
        let row_on = starts.iter().filter(|&&s| s % 900 == 0).count() as u32;
        trips += trips_on_pattern;
        on_clockface += row_on;
        rows.push(PatternTexture {
            schedules,
            route: net
                .patterns
                .get(pattern)
                .map(|p| p.route.clone())
                .unwrap_or_default(),
            pattern: pattern.clone(),
            trips: trips_on_pattern,
            min_gap_secs: gaps.iter().copied().min(),
            median_gap_secs: median_gap_secs(&starts),
            max_gap_secs: gaps.iter().copied().max(),
            on_clockface: row_on,
        });
    }
    Some(ScheduleTexture {
        group: combination.id,
        members: combination.members,
        trips,
        on_clockface,
        pattern_trips: rows,
    })
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "bindings", derive(TS))]
#[cfg_attr(feature = "bindings", ts(export))]
pub struct ServiceTypeComparison {
    pub group: String,
    pub members: Vec<String>,
    pub base: ComparedTotals,
    pub proposal: ComparedTotals,
    pub routes: Vec<RouteChange>,
    pub unchanged_routes: u32,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "bindings", derive(TS))]
#[cfg_attr(feature = "bindings", ts(export))]
pub struct ComparedTotals {
    /// 0 when the members never run together on this side.
    pub days: u32,
    pub trips: u32,
    pub revenue_hours: f64,
    pub revenue_miles: f64,
    pub routes: u32,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "bindings", derive(TS))]
#[cfg_attr(feature = "bindings", ts(export))]
pub struct RouteChange {
    pub route_id: String,
    pub name: String,
    pub base: Option<RouteStats>,
    pub proposal: Option<RouteStats>,
}

/// The baseline side pools the same members, whether or not they run together
/// there.
pub fn service_type_compare(base: &Network, proposal: &Network, members: &[String]) -> Option<ServiceTypeComparison> {
    let combination = transit_network::service_combination(proposal, members)?;
    let p = pooled_detail(proposal, &combination.members);
    let b = pooled_detail(base, &combination.members);
    let base_days = transit_network::service_combination(base, &combination.members).map_or(0, |c| c.dates.len() as u32);
    let proposal_days = combination.dates.len() as u32;
    let same = |x: &RouteStats, y: &RouteStats| {
        (x.name.as_str(), x.patterns, x.stops, x.trips, x.first_start_secs, x.last_arrival_secs, x.typical_headway_secs)
            == (y.name.as_str(), y.patterns, y.stops, y.trips, y.first_start_secs, y.last_arrival_secs, y.typical_headway_secs)
            && (x.revenue_hours - y.revenue_hours).abs() < 1e-9
    };
    let mut rows: BTreeMap<String, (Option<RouteStats>, Option<RouteStats>)> = BTreeMap::new();
    for r in &b.routes {
        rows.entry(r.route_id.clone()).or_default().0 = Some(r.clone());
    }
    for r in &p.routes {
        rows.entry(r.route_id.clone()).or_default().1 = Some(r.clone());
    }
    let mut unchanged_routes = 0;
    let mut routes = Vec::new();
    for (route_id, (base, proposal)) in rows {
        if let (Some(x), Some(y)) = (&base, &proposal) {
            if same(x, y) {
                unchanged_routes += 1;
                continue;
            }
        }
        let name = proposal.as_ref().or(base.as_ref()).map(|r| r.name.clone()).unwrap_or_default();
        routes.push(RouteChange { route_id, name, base, proposal });
    }
    let totals = |d: &ServiceTypeDetail, days: u32| ComparedTotals {
        days,
        trips: d.trips,
        revenue_hours: d.revenue_hours,
        revenue_miles: d.revenue_miles,
        routes: d.routes.len() as u32,
    };
    Some(ServiceTypeComparison {
        group: combination.id,
        members: combination.members,
        base: totals(&b, base_days),
        proposal: totals(&p, proposal_days),
        routes,
        unchanged_routes,
    })
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "bindings", derive(TS))]
#[cfg_attr(feature = "bindings", ts(export))]
pub struct ServiceGroup {
    #[serde(flatten)]
    #[cfg_attr(feature = "bindings", ts(flatten))]
    pub combination: transit_network::ServiceCombination,
    pub trips: u32,
    pub active_routes: u32,
    pub active_stops: u32,
    pub revenue_hours: f64,
    pub revenue_miles: f64,
}

pub fn service_groups(net: &Network) -> Vec<ServiceGroup> {
    transit_network::service_combinations(net)
        .into_iter()
        .map(|combination| {
            let members: BTreeSet<&str> = combination.members.iter().map(String::as_str).collect();
            let mut trips = 0u32;
            let mut routes: BTreeSet<String> = BTreeSet::new();
            let mut stops: BTreeSet<String> = BTreeSet::new();
            let mut runtime_secs = 0u64;
            let mut meters = 0f64;
            for (schedule_id, entry) in &net.pattern_trips {
                if !members.contains(entry.service_type.as_str()) {
                    continue;
                }
                let Ok(materialized) = transit_network::materialize_service(net, schedule_id) else {
                    continue;
                };
                for trip in materialized {
                    if let Some(p) = net.patterns.get(&trip.pattern) {
                        routes.insert(p.route.clone());
                    }
                    trips += 1;
                    stops.extend(trip.stop_times.iter().map(|st| st.stop.clone()));
                    if let Some(end) = trip.stop_times.last() {
                        runtime_secs += end.arrival_secs.saturating_sub(trip.start_secs) as u64;
                    }
                    meters += trip_distance_m(net, &trip);
                }
            }
            ServiceGroup {
                combination,
                trips,
                active_routes: routes.len() as u32,
                active_stops: stops.len() as u32,
                revenue_hours: runtime_secs as f64 / 3600.0,
                revenue_miles: meters / METERS_PER_MILE,
            }
        })
        .collect()
}



#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "bindings", derive(TS))]
#[cfg_attr(feature = "bindings", ts(export))]
pub struct NetworkStats {
    pub date: String,
    pub trips: u32,
    pub active_routes: u32,
    pub active_stops: u32,
    pub first_start_secs: Option<u32>,
    pub last_arrival_secs: Option<u32>,
    pub revenue_hours: f64,
    pub revenue_miles: f64,
    pub transfer_stops: u32,
    pub service_types: Vec<ServiceTypeSummary>,
    pub routes: Vec<RouteStats>,
    pub stops: Vec<StopStats>,
}

#[derive(Debug, Clone, Default)]
struct StopActivity {
    departures: u32,
    routes: BTreeSet<String>,
    first: Option<u32>,
    last: Option<u32>,
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

/// None below four times, because a median over fewer than three gaps is a
/// headway nobody runs. The median, not the span average, keeps a busy hour and
/// a quiet evening in proportion.
fn median_gap_secs(times: &[u32]) -> Option<u32> {
    if times.len() < 4 {
        return None;
    }
    let mut sorted = times.to_vec();
    sorted.sort_unstable();
    let mut gaps: Vec<u32> = sorted.windows(2).map(|w| w[1] - w[0]).collect();
    if gaps.is_empty() {
        return None;
    }
    gaps.sort_unstable();
    Some(gaps[gaps.len() / 2])
}




const METERS_PER_MILE: f64 = 1609.344;

/// Zero for a pattern with no distances.
fn trip_distance_m(net: &Network, trip: &transit_network::MaterializedTrip) -> f64 {
    let Some(pattern) = net.patterns.get(&trip.pattern) else {
        return 0.0;
    };
    match (pattern.stops.first(), pattern.stops.last()) {
        (Some(first), Some(last)) => (last.distance_m - first.distance_m).max(0.0),
        _ => 0.0,
    }
}

fn stats_fold(net: &Network, include: impl Fn(&str) -> bool, date: Date) -> NetworkStats {
    let mut pattern_starts: BTreeMap<String, Vec<u32>> = BTreeMap::new();
    let mut route_trips: BTreeMap<String, u32> = BTreeMap::new();
    let mut route_first: BTreeMap<String, u32> = BTreeMap::new();
    let mut route_last: BTreeMap<String, u32> = BTreeMap::new();
    let mut route_stop_sets: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut route_runtime: BTreeMap<String, u64> = BTreeMap::new();
    let mut route_meters: BTreeMap<String, f64> = BTreeMap::new();
    let mut route_departures: BTreeMap<(String, String), Vec<u32>> = BTreeMap::new();
    let mut stop_activity: BTreeMap<String, StopActivity> = BTreeMap::new();
    let mut times_at: BTreeMap<(String, String), Vec<u32>> = BTreeMap::new();

    let mut trips = 0u32;
    let mut first_start: Option<u32> = None;
    let mut last_arrival: Option<u32> = None;
    let mut runtime_secs = 0u64;
    let mut meters = 0f64;

    for (service_id, service) in &net.pattern_trips {
        if !include(&service.service_type) {
            continue;
        }
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
            let distance = trip_distance_m(net, &trip);
            meters += distance;
            *route_meters.entry(route.clone()).or_default() += distance;
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
            name: route.label().to_string(),
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
            revenue_miles: *route_meters.get(route_id).unwrap_or(&0.0) / METERS_PER_MILE,
        });
    }

    let mut stops = Vec::with_capacity(net.stops.len());
    let mut transfer_stops = 0u32;
    for (stop_id, stop) in &net.stops {
        let activity = stop_activity.get(stop_id);
        // The flag marks a place, so the group and its connections pool across
        // the walking group.
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
        date: date.to_string(),
        trips,
        active_routes: route_trips.len() as u32,
        active_stops: stop_activity.len() as u32,
        first_start_secs: first_start,
        last_arrival_secs: last_arrival,
        revenue_hours: runtime_secs as f64 / 3600.0,
        revenue_miles: meters / METERS_PER_MILE,
        transfer_stops,
        service_types: Vec::new(),
        routes,
        stops,
    }
}


fn family_stats(net: &Network, group: &BTreeSet<String>) -> NetworkStats {
    stats_fold(net, |sid| group.contains(sid), Date::MIN)
}


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

/// Seconds a rider needs to move between an arrival and a departure.
const TRANSFER_WINDOW_MIN_SECS: u32 = 60;
/// Beyond 15 minutes the connection is not a timed transfer. The walk across a
/// street or down a block is folded into these bounds.
const TRANSFER_WINDOW_MAX_SECS: u32 = 900;
