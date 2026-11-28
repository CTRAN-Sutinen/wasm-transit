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

use serde::Serialize;
#[cfg(feature = "bindings")]
use ts_rs::TS;

use transit_network::Network;

mod stats;
pub use stats::{RouteStats, StopStats, NetworkStats, network_stats};
use stats::family_stats;

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
    day_types, DayTypeGroup, plan_compare, type_coverage, service_type_overview
};
use calendar::service_type_summaries;

mod stops;
pub use stops::{StopHourGaps, StopGaps, StopWait, stop_gaps};

mod transfers;
pub use transfers::{
    longest_waits, HourlyTrips, HourlyTypeTrips, MissedConnection, missed_connections,
    timed_transfers, TransferWindow, hourly_trips, route_overlap,
};

mod detail;
pub use detail::{ServiceTypeDetail, service_type_detail};
use detail::pooled_detail;

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
