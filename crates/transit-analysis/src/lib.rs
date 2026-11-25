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

use chrono::NaiveDate;
use serde::Serialize;
use ts_rs::TS;

use transit_network::Network;

mod coverage;
pub mod load;
pub use coverage::{
    coverage, coverage_compare, parse_layer, unserved_points, CoverageDelta, CoverageSide, CoverageStats,
    CoverageWindow, LayerField, LayerReading, Rings, UnservedPlaces, UnservedPoint, WeightedLayer, Weights,
};
use coverage::coverage_of_members;

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct StopStats {
    pub stop_id: String,
    pub name: String,
    pub routes: u32,
    pub departures: u32,
    pub first_arrival_secs: Option<u32>,
    pub last_arrival_secs: Option<u32>,
    pub typical_headway_secs: Option<u32>,
    pub transfer: bool,
    pub transfer_group: Option<u32>,
    /// Ordered route pairs with a usable timed connection. Zero means the flag
    /// is aspirational today.
    pub transfer_pairs: Option<u32>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct RouteStats {
    pub route_id: String,
    pub name: String,
    pub patterns: u32,
    pub stops: u32,
    pub trips: u32,
    pub first_start_secs: Option<u32>,
    pub last_arrival_secs: Option<u32>,
    /// Median gap between departures at the busiest stop (`headway_stop`). Null
    /// below four departures, because a median over fewer is a headway nobody
    /// runs.
    pub typical_headway_secs: Option<u32>,
    pub headway_stop: Option<String>,
    pub revenue_hours: f64,
    pub revenue_miles: f64,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct ServiceTypeSummary {
    pub service_type: String,
    pub name: String,
    pub trips: u32,
    pub active_routes: u32,
    pub active_stops: u32,
    pub revenue_hours: f64,
    pub revenue_miles: f64,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct PatternTripsSummary {
    pub pattern_trips: String,
    pub pattern: String,
    pub pattern_name: String,
    pub trips: u32,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
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
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
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

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
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

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct ServiceTypeComparison {
    pub group: String,
    pub members: Vec<String>,
    pub base: ComparedTotals,
    pub proposal: ComparedTotals,
    pub routes: Vec<RouteChange>,
    pub unchanged_routes: u32,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct ComparedTotals {
    /// 0 when the members never run together on this side.
    pub days: u32,
    pub trips: u32,
    pub revenue_hours: f64,
    pub revenue_miles: f64,
    pub routes: u32,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
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

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct ServiceGroup {
    #[serde(flatten)]
    #[ts(flatten)]
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

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct DayTypeGroup {
    pub id: String,
    pub members: Vec<String>,
    pub dates: Vec<String>,
    pub spans: Vec<transit_network::DateSpan>,
    pub weekdays: Vec<bool>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct DayType {
    /// A day type's regular group runs on more than half of a weekday's dates
    /// in the period. Otherwise the type takes every weekday it runs on.
    pub weekdays: Vec<bool>,
    pub groups: Vec<DayTypeGroup>,
}

/// An absent `from` or `through` is the first or last in-period date that any
/// combination holds. A combination with no date in the period is left out
/// (D-137).
pub fn day_types(net: &Network, from: Option<NaiveDate>, through: Option<NaiveDate>) -> Vec<DayType> {
    use chrono::Datelike;
    let mut groups: Vec<DayTypeGroup> = transit_network::service_combinations(net)
        .into_iter()
        .filter_map(|c| {
            let dates: Vec<NaiveDate> = c
                .dates
                .iter()
                .filter_map(|d| NaiveDate::parse_from_str(d, "%Y-%m-%d").ok())
                .filter(|d| from.map_or(true, |f| *d >= f) && through.map_or(true, |t| *d <= t))
                .collect();
            if dates.is_empty() {
                return None;
            }
            let mut weekdays = vec![false; 7];
            for d in &dates {
                weekdays[d.weekday().num_days_from_monday() as usize] = true;
            }
            Some(DayTypeGroup {
                id: c.id,
                members: c.members,
                spans: transit_network::date_spans(&dates),
                dates: dates.iter().map(|d| d.format("%Y-%m-%d").to_string()).collect(),
                weekdays,
            })
        })
        .collect();
    groups.sort_by(|a, b| {
        b.dates.len().cmp(&a.dates.len()).then_with(|| a.dates[0].cmp(&b.dates[0])).then_with(|| a.id.cmp(&b.id))
    });
    let first = from.or_else(|| groups.iter().filter_map(|g| g.dates.first()).min().and_then(|d| NaiveDate::parse_from_str(d, "%Y-%m-%d").ok()));
    let last = through.or_else(|| groups.iter().filter_map(|g| g.dates.last()).max().and_then(|d| NaiveDate::parse_from_str(d, "%Y-%m-%d").ok()));
    let mut period = [0u32; 7];
    if let (Some(mut d), Some(end)) = (first, last) {
        while d <= end {
            period[d.weekday().num_days_from_monday() as usize] += 1;
            d += chrono::Duration::days(1);
        }
    }
    let within = |a: &[bool], b: &[bool]| a.iter().zip(b).all(|(x, y)| !*x || *y);
    let mut types: Vec<DayType> = Vec::new();
    for g in groups {
        match types.iter_mut().find(|t| within(&g.weekdays, &t.weekdays)) {
            Some(t) => t.groups.push(g),
            None => {
                let mut held = [0u32; 7];
                for d in g.dates.iter().filter_map(|d| NaiveDate::parse_from_str(d, "%Y-%m-%d").ok()) {
                    held[d.weekday().num_days_from_monday() as usize] += 1;
                }
                let most: Vec<bool> = (0..7).map(|w| held[w] * 2 > period[w]).collect();
                let weekdays = if most.iter().any(|&x| x) { most } else { g.weekdays.clone() };
                types.push(DayType { weekdays, groups: vec![g] });
            }
        }
    }
    types
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct DayTypeGroupComparison {
    pub group: DayTypeGroup,
    pub comparison: Option<ServiceTypeComparison>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct DayTypeComparison {
    pub weekdays: Vec<bool>,
    pub groups: Vec<DayTypeGroupComparison>,
}

pub fn plan_compare(
    base: &Network,
    proposal: &Network,
    from: Option<NaiveDate>,
    through: Option<NaiveDate>,
) -> Vec<DayTypeComparison> {
    day_types(proposal, from, through)
        .into_iter()
        .map(|t| DayTypeComparison {
            weekdays: t.weekdays,
            groups: t
                .groups
                .into_iter()
                .map(|g| {
                    let comparison = service_type_compare(base, proposal, &g.members);
                    DayTypeGroupComparison { group: g, comparison }
                })
                .collect(),
        })
        .collect()
}

pub fn service_type_overview(net: &Network) -> Vec<ServiceTypeSummary> {
    service_type_summaries(net, None)
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct CombinationCoverage {
    pub group: String,
    pub members: Vec<String>,
    pub spans: Vec<transit_network::DateSpan>,
    pub served: f64,
    pub frequent: f64,
    pub served_pct: f64,
    pub frequent_pct: f64,
    pub served_stops: u32,
    pub frequent_stops: u32,
}

/// `window` judges frequency inside a time window ([`CoverageWindow`]). `None`
/// is the whole-day average.
pub fn type_coverage(
    net: &Network,
    weights: &Weights,
    members: &[String],
    radius_m: f64,
    frequent_secs: u32,
    min_departures: u32,
    window: Option<CoverageWindow>,
) -> Option<CombinationCoverage> {
    let combination = transit_network::service_combination(net, members)?;
    let stats = coverage_of_members(net, &combination.members, weights, radius_m, frequent_secs, min_departures, window);
    Some(CombinationCoverage {
        group: combination.id,
        members: combination.members,
        spans: combination.spans,
        served: stats.served,
        frequent: stats.frequent,
        served_pct: stats.served_pct,
        frequent_pct: stats.frequent_pct,
        served_stops: stats.served_stops,
        frequent_stops: stats.frequent_stops,
    })
}

/// With `running_on`, only the types whose calendar runs on that date are
/// listed.
fn service_type_summaries(net: &Network, running_on: Option<NaiveDate>) -> Vec<ServiceTypeSummary> {
    let mut out = Vec::with_capacity(net.service_types.len());
    for (type_id, service_type) in &net.service_types {
        if running_on.is_some_and(|date| !service_type.rule.runs_on(date)) {
            continue;
        }
        let mut trips = 0u32;
        let mut routes: BTreeSet<String> = BTreeSet::new();
        let mut stops: BTreeSet<String> = BTreeSet::new();
        let mut runtime_secs = 0u64;
        let mut meters = 0f64;
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
                    meters += trip_distance_m(net, &trip);
                }
            }
        }
        out.push(ServiceTypeSummary {
            service_type: type_id.clone(),
            name: service_type.name.clone(),
            trips,
            active_routes: routes.len() as u32,
            active_stops: stops.len() as u32,
            revenue_hours: runtime_secs as f64 / 3600.0,
            revenue_miles: meters / METERS_PER_MILE,
        });
    }
    out
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
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

/// Seconds a rider needs to move between an arrival and a departure.
const TRANSFER_WINDOW_MIN_SECS: u32 = 60;
/// Beyond 15 minutes the connection is not a timed transfer. The walk across a
/// street or down a block is folded into these bounds.
const TRANSFER_WINDOW_MAX_SECS: u32 = 900;

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

pub fn network_stats(net: &Network, date: NaiveDate) -> NetworkStats {
    day_stats(net, date, None)
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

fn stats_fold(net: &Network, include: impl Fn(&str) -> bool, date: NaiveDate) -> NetworkStats {
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
        date: date.format("%Y-%m-%d").to_string(),
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

fn day_stats(net: &Network, date: NaiveDate, only_type: Option<&str>) -> NetworkStats {
    let mut stats = stats_fold(net, |sid| {
        net.service_types
            .get(sid)
            .is_some_and(|st| st.rule.runs_on(date))
            && only_type.map_or(true, |t| sid == t)
    }, date);
    stats.service_types = service_type_summaries(net, Some(date));
    stats
}

fn family_stats(net: &Network, group: &BTreeSet<String>) -> NetworkStats {
    stats_fold(net, |sid| group.contains(sid), NaiveDate::MIN)
}


/// The hour is the service-day hour, 0..47. A gap belongs to the hour it starts
/// in.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct StopHourGaps {
    pub hour: u32,
    pub departures: u32,
    pub max_gap_secs: Option<u32>,
    pub median_gap_secs: Option<u32>,
}

/// Two routes that leave together make no gap, and `departures` counts both.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct StopGaps {
    pub stop_id: String,
    pub name: String,
    pub departures: u32,
    pub first_departure_secs: Option<u32>,
    pub last_departure_secs: Option<u32>,
    /// None below two distinct departures.
    pub max_gap_secs: Option<u32>,
    pub max_gap_after_secs: Option<u32>,
    pub hourly: Vec<StopHourGaps>,
}

pub fn stop_gaps(net: &Network, date: NaiveDate, stop_id: &str) -> StopGaps {
    let pooled: BTreeSet<&str> = std::iter::once(stop_id)
        .chain(
            net.stops
                .iter()
                .filter(|(_, s)| s.parent.as_deref() == Some(stop_id))
                .map(|(id, _)| id.as_str()),
        )
        .collect();
    let mut departures: Vec<u32> = Vec::new();
    for (service_id, service) in &net.pattern_trips {
        if !net
            .service_types
            .get(&service.service_type)
            .is_some_and(|st| st.rule.runs_on(date))
        {
            continue;
        }
        let Ok(materialized) = transit_network::materialize_service(net, service_id) else {
            continue;
        };
        for trip in materialized {
            for st in &trip.stop_times[..trip.stop_times.len() - 1] {
                if pooled.contains(st.stop.as_str()) {
                    departures.push(st.departure_secs);
                }
            }
        }
    }
    let distinct: BTreeSet<u32> = departures.iter().copied().collect();
    let spine: Vec<u32> = distinct.into_iter().collect();
    let mut max_gap = None;
    let mut max_after = None;
    let mut gaps_by_hour: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
    for w in spine.windows(2) {
        let gap = w[1] - w[0];
        if max_gap.map_or(true, |m| gap > m) {
            max_gap = Some(gap);
            max_after = Some(w[0]);
        }
        gaps_by_hour.entry(w[0] / 3600).or_default().push(gap);
    }
    let departures_by_hour: BTreeMap<u32, u32> =
        departures.iter().copied().fold(BTreeMap::new(), |mut m, d| {
            *m.entry(d / 3600).or_default() += 1;
            m
        });
    let mut hours: BTreeSet<u32> = departures_by_hour.keys().copied().collect();
    hours.extend(gaps_by_hour.keys().copied());
    let hourly = hours
        .into_iter()
        .map(|hour| {
            let mut gaps = gaps_by_hour.get(&hour).cloned().unwrap_or_default();
            gaps.sort_unstable();
            StopHourGaps {
                hour,
                departures: departures_by_hour.get(&hour).copied().unwrap_or(0),
                max_gap_secs: gaps.last().copied(),
                median_gap_secs: (gaps.len() >= 4).then(|| gaps[gaps.len() / 2]),
            }
        })
        .collect();
    StopGaps {
        stop_id: stop_id.to_string(),
        name: net.stops.get(stop_id).map(|s| s.name.clone()).unwrap_or_default(),
        departures: departures.len() as u32,
        first_departure_secs: spine.first().copied(),
        last_departure_secs: spine.last().copied(),
        max_gap_secs: max_gap,
        max_gap_after_secs: max_after,
        hourly,
    }
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct StopWait {
    pub stop_id: String,
    pub name: String,
    pub departures: u32,
    pub longest_wait_secs: u32,
    pub wait_from_secs: u32,
}

pub fn longest_waits(
    net: &Network,
    date: NaiveDate,
    window: Option<CoverageWindow>,
    limit: Option<u32>,
) -> Vec<StopWait> {
    let mut departures: BTreeMap<&str, Vec<u32>> = BTreeMap::new();
    for (service_id, service) in &net.pattern_trips {
        if !net.service_types.get(&service.service_type).is_some_and(|st| st.rule.runs_on(date)) {
            continue;
        }
        let Ok(materialized) = transit_network::materialize_service(net, service_id) else {
            continue;
        };
        for trip in materialized {
            for st in &trip.stop_times[..trip.stop_times.len() - 1] {
                if let Some((id, _)) = net.stops.get_key_value(&st.stop) {
                    departures.entry(id.as_str()).or_default().push(st.departure_secs);
                }
            }
        }
    }
    let mut rows: Vec<StopWait> = Vec::new();
    for (stop_id, deps) in departures {
        let spine: Vec<u32> = deps.iter().copied().collect::<BTreeSet<_>>().into_iter().collect();
        let (count, edges) = match window {
            None => (deps.len() as u32, spine),
            Some(w) => {
                let inside = |t: &u32| *t >= w.start_secs && *t <= w.end_secs;
                let mut edges = vec![w.start_secs];
                edges.extend(spine.iter().copied().filter(inside));
                edges.push(w.end_secs);
                (deps.iter().filter(|t| inside(t)).count() as u32, edges)
            }
        };
        let mut best: Option<(u32, u32)> = None;
        for e in edges.windows(2) {
            if best.map_or(true, |(g, _)| e[1] - e[0] > g) {
                best = Some((e[1] - e[0], e[0]));
            }
        }
        let Some((longest_wait_secs, wait_from_secs)) = best else { continue };
        rows.push(StopWait {
            stop_id: stop_id.to_string(),
            name: net.stops[stop_id].name.clone(),
            departures: count,
            longest_wait_secs,
            wait_from_secs,
        });
    }
    rows.sort_by(|a, b| b.longest_wait_secs.cmp(&a.longest_wait_secs).then_with(|| a.stop_id.cmp(&b.stop_id)));
    if let Some(n) = limit {
        rows.truncate(n as usize);
    }
    rows
}

/// The hour is the service-day hour, 0..47.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct HourlyTypeTrips {
    pub service_type: String,
    pub trips: u32,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct HourlyTrips {
    pub hour: u32,
    pub trips: u32,
    pub service_types: Vec<HourlyTypeTrips>,
}

pub fn hourly_trips(net: &Network, date: NaiveDate) -> Vec<HourlyTrips> {
    let mut by_hour: BTreeMap<u32, BTreeMap<String, u32>> = BTreeMap::new();
    for (service_id, service) in &net.pattern_trips {
        if !net
            .service_types
            .get(&service.service_type)
            .is_some_and(|st| st.rule.runs_on(date))
        {
            continue;
        }
        let Ok(materialized) = transit_network::materialize_service(net, service_id) else {
            continue;
        };
        for trip in materialized {
            *by_hour
                .entry(trip.start_secs / 3600)
                .or_default()
                .entry(service.service_type.clone())
                .or_default() += 1;
        }
    }
    by_hour
        .into_iter()
        .map(|(hour, types)| HourlyTrips {
            hour,
            trips: types.values().sum(),
            service_types: types
                .into_iter()
                .map(|(service_type, trips)| HourlyTypeTrips { service_type, trips })
                .collect(),
        })
        .collect()
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct RouteOverlap {
    pub a: String,
    pub b: String,
    pub shared_stops: u32,
    pub shared_segments: u32,
    pub stack_stop: Option<String>,
    pub stack_departures: u32,
}

pub fn route_overlap(net: &Network, date: NaiveDate) -> Vec<RouteOverlap> {
    struct RouteDay {
        stops: BTreeSet<String>,
        segs: BTreeSet<(String, String)>,
        deps: BTreeMap<String, u32>,
    }
    let mut routes: BTreeMap<String, RouteDay> = BTreeMap::new();
    for (service_id, service) in &net.pattern_trips {
        if !net
            .service_types
            .get(&service.service_type)
            .is_some_and(|st| st.rule.runs_on(date))
        {
            continue;
        }
        let Ok(materialized) = transit_network::materialize_service(net, service_id) else {
            continue;
        };
        for trip in materialized {
            let Some(pattern) = net.patterns.get(&trip.pattern) else { continue };
            let entry = routes.entry(pattern.route.clone()).or_insert(RouteDay {
                stops: BTreeSet::new(),
                segs: BTreeSet::new(),
                deps: BTreeMap::new(),
            });
            for st in &trip.stop_times {
                entry.stops.insert(st.stop.clone());
            }
            for w in pattern.stops.windows(2) {
                entry.segs.insert((w[0].stop.clone(), w[1].stop.clone()));
            }
            for st in &trip.stop_times[..trip.stop_times.len() - 1] {
                *entry.deps.entry(st.stop.clone()).or_default() += 1;
            }
        }
    }
    let ids: Vec<&String> = routes.keys().collect();
    let mut out = Vec::new();
    for i in 0..ids.len() {
        for j in (i + 1)..ids.len() {
            let (ra, rb) = (&routes[ids[i]], &routes[ids[j]]);
            let shared: BTreeSet<&String> = ra.stops.intersection(&rb.stops).collect();
            if shared.is_empty() {
                continue;
            }
            let shared_segments =
                ra.segs.intersection(&rb.segs).count() as u32;
            let (stack_stop, stack_departures) = shared
                .iter()
                .map(|stop| {
                    (
                        (*stop).clone(),
                        ra.deps.get(*stop).copied().unwrap_or(0)
                            + rb.deps.get(*stop).copied().unwrap_or(0),
                    )
                })
                .max_by(|x, y| x.1.cmp(&y.1).then_with(|| y.0.cmp(&x.0)))
                .unwrap();
            out.push(RouteOverlap {
                a: ids[i].clone(),
                b: ids[j].clone(),
                shared_stops: shared.len() as u32,
                shared_segments,
                stack_stop: Some(stack_stop),
                stack_departures,
            });
        }
    }
    out
}

/// A wait is a transfer when `min_secs <= wait <= max_secs`. The defaults are
/// the transfer-point bounds: 60 s and 900 s. A hub with widely separated bays
/// needs a wider window.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, serde::Deserialize, TS)]
#[ts(export)]
pub struct TransferWindow {
    pub min_secs: u32,
    pub max_secs: u32,
}

impl Default for TransferWindow {
    fn default() -> Self {
        TransferWindow { min_secs: TRANSFER_WINDOW_MIN_SECS, max_secs: TRANSFER_WINDOW_MAX_SECS }
    }
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct TimedTransfer {
    pub from_route: String,
    pub to_route: String,
    pub arrival_secs: u32,
    pub arrival_stop: String,
    pub departure_secs: u32,
    pub departure_stop: String,
    pub wait_secs: u32,
}

pub fn timed_transfers(
    net: &Network,
    date: NaiveDate,
    stop_id: &str,
    window: Option<TransferWindow>,
) -> Vec<TimedTransfer> {
    let window = window.unwrap_or_default();
    let (arrivals, departures) = hub_visits(net, date, stop_id);
    let mut out = Vec::new();
    for (from_route, arrival_stop, arrival_secs) in &arrivals {
        for (to_route, departure_stop, departure_secs) in &departures {
            let wait = departure_secs.saturating_sub(*arrival_secs);
            if from_route != to_route && window.min_secs <= wait && wait <= window.max_secs {
                out.push(TimedTransfer {
                    from_route: from_route.clone(),
                    to_route: to_route.clone(),
                    arrival_secs: *arrival_secs,
                    arrival_stop: arrival_stop.clone(),
                    departure_secs: *departure_secs,
                    departure_stop: departure_stop.clone(),
                    wait_secs: wait,
                });
            }
        }
    }
    out.sort_by(|x, y| {
        x.arrival_secs
            .cmp(&y.arrival_secs)
            .then_with(|| x.from_route.cmp(&y.from_route))
            .then_with(|| x.to_route.cmp(&y.to_route))
            .then_with(|| x.departure_secs.cmp(&y.departure_secs))
    });
    out
}

type Visits = Vec<(String, String, u32)>;
fn hub_visits(net: &Network, date: NaiveDate, stop_id: &str) -> (Visits, Visits) {
    let group = transit_network::walking_group(net, stop_id);
    let mut arrivals: Visits = Vec::new();
    let mut departures: Visits = Vec::new();
    for (service_id, service) in &net.pattern_trips {
        if !net
            .service_types
            .get(&service.service_type)
            .is_some_and(|st| st.rule.runs_on(date))
        {
            continue;
        }
        let Ok(materialized) = transit_network::materialize_service(net, service_id) else {
            continue;
        };
        for trip in materialized {
            let Some(route) = net.patterns.get(&trip.pattern).map(|p| p.route.clone()) else {
                continue;
            };
            for (i, st) in trip.stop_times.iter().enumerate() {
                if !group.contains_key(&st.stop) {
                    continue;
                }
                arrivals.push((route.clone(), st.stop.clone(), st.arrival_secs));
                if i + 1 < trip.stop_times.len() {
                    departures.push((route.clone(), st.stop.clone(), st.departure_secs));
                }
            }
        }
    }
    (arrivals, departures)
}

/// Seconds. The default is provisional (D-133).
pub const MISS_SECS: u32 = 300;

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct MissedConnection {
    pub from_route: String,
    pub to_route: String,
    pub arrival_secs: u32,
    pub arrival_stop: String,
    pub departure_secs: u32,
    pub departure_stop: String,
    pub missed_by_secs: u32,
    pub next_departure_secs: Option<u32>,
}

pub fn missed_connections(
    net: &Network,
    date: NaiveDate,
    stop_id: &str,
    window: Option<TransferWindow>,
    miss_secs: Option<u32>,
) -> Vec<MissedConnection> {
    let min = window.unwrap_or_default().min_secs as i64;
    let miss = miss_secs.unwrap_or(MISS_SECS) as i64;
    let (arrivals, departures) = hub_visits(net, date, stop_id);
    let mut out = Vec::new();
    for (from_route, arrival_stop, arrival_secs) in &arrivals {
        let a = *arrival_secs as i64;
        for (to_route, departure_stop, departure_secs) in &departures {
            let d = *departure_secs as i64;
            if from_route == to_route || d < a - miss || d >= a + min {
                continue;
            }
            let next_departure_secs = departures
                .iter()
                .filter(|(r, _, t)| r == to_route && (*t as i64) >= a + min)
                .map(|(_, _, t)| *t)
                .min();
            out.push(MissedConnection {
                from_route: from_route.clone(),
                to_route: to_route.clone(),
                arrival_secs: *arrival_secs,
                arrival_stop: arrival_stop.clone(),
                departure_secs: *departure_secs,
                departure_stop: departure_stop.clone(),
                missed_by_secs: (a + min - d) as u32,
                next_departure_secs,
            });
        }
    }
    out.sort_by(|x, y| {
        x.arrival_secs
            .cmp(&y.arrival_secs)
            .then_with(|| x.from_route.cmp(&y.from_route))
            .then_with(|| x.to_route.cmp(&y.to_route))
            .then_with(|| x.departure_secs.cmp(&y.departure_secs))
    });
    out
}


/// Provisional thresholds (D-136).
#[derive(Debug, Clone, Default, PartialEq, Serialize, serde::Deserialize, TS)]
#[ts(export)]
pub struct FindingParams {
    #[serde(default)]
    #[ts(optional)]
    pub hole_factor: Option<f64>,
    #[serde(default)]
    #[ts(optional)]
    pub wait_secs: Option<u32>,
    #[serde(default)]
    #[ts(optional)]
    pub miss_cost_secs: Option<u32>,
    #[serde(default)]
    #[ts(optional)]
    pub window: Option<CoverageWindow>,
    #[serde(default)]
    #[ts(optional)]
    pub limit: Option<u32>,
    #[serde(default)]
    #[ts(optional)]
    pub watch: Option<Vec<String>>,
}

const HOLE_FACTOR: f64 = 1.5;
const LONG_WAIT_SECS: u32 = 3600;
const MISS_COST_SECS: u32 = 900;

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct FrequencyHole {
    /// The key is "hole|<route>|<hour>", so the same hole in another version
    /// has the same key.
    pub key: String,
    pub route_id: String,
    pub name: String,
    pub hour: u32,
    pub longest_gap_secs: u32,
    pub around_gap_secs: u32,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct LongWait {
    /// The key is "wait|<stop>".
    pub key: String,
    pub stop_id: String,
    pub name: String,
    pub longest_wait_secs: u32,
    pub wait_from_secs: u32,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct CostlyMiss {
    /// The key is "miss|<hub>|<from route>|<to route>".
    pub key: String,
    pub hub_id: String,
    pub hub_name: String,
    pub from_route: String,
    pub to_route: String,
    pub misses: u32,
    pub worst_wait_secs: Option<u32>,
    pub stranded: u32,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct HoleRow {
    pub now: FrequencyHole,
    pub base: Option<FrequencyHole>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct WaitRow {
    pub now: LongWait,
    pub base: Option<LongWait>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct MissRow {
    pub now: CostlyMiss,
    pub base: Option<CostlyMiss>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AnyFinding {
    Hole(FrequencyHole),
    Wait(LongWait),
    Miss(CostlyMiss),
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct Watched {
    pub key: String,
    pub now: Option<AnyFinding>,
    pub base: Option<AnyFinding>,
}

#[derive(Debug, Clone, Default, Serialize, TS)]
#[ts(export)]
pub struct ResolvedFindings {
    pub holes: Vec<FrequencyHole>,
    pub waits: Vec<LongWait>,
    pub misses: Vec<CostlyMiss>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct NetworkFindings {
    pub group: String,
    /// Every date that the combination holds answers alike, so this is
    /// the first.
    pub date: String,
    pub holes: Vec<HoleRow>,
    pub waits: Vec<WaitRow>,
    pub misses: Vec<MissRow>,
    pub resolved: ResolvedFindings,
    pub watched: Vec<Watched>,
}

struct Found {
    group: String,
    date: String,
    holes: Vec<FrequencyHole>,
    waits: Vec<LongWait>,
    misses: Vec<CostlyMiss>,
}

impl Found {
    fn find(&self, key: &str) -> Option<AnyFinding> {
        self.holes
            .iter()
            .find(|f| f.key == key)
            .map(|f| AnyFinding::Hole(f.clone()))
            .or_else(|| self.waits.iter().find(|f| f.key == key).map(|f| AnyFinding::Wait(f.clone())))
            .or_else(|| self.misses.iter().find(|f| f.key == key).map(|f| AnyFinding::Miss(f.clone())))
    }
}

fn findings_of(net: &Network, members: &[String], p: &FindingParams) -> Option<Found> {
    let combination = transit_network::service_combination(net, members)?;
    let date_text = combination.dates.first()?.clone();
    let date = NaiveDate::parse_from_str(&date_text, "%Y-%m-%d").ok()?;
    let factor = p.hole_factor.unwrap_or(HOLE_FACTOR);
    let in_hour = |h: u32| p.window.map_or(true, |w| h * 3600 >= w.start_secs && h * 3600 < w.end_secs);
    let in_secs = |t: u32| p.window.map_or(true, |w| t >= w.start_secs && t <= w.end_secs);

    let mut holes = Vec::new();
    if let Some(grid) = route_frequency(net, &combination.members) {
        for row in grid.routes {
            let gaps: BTreeMap<u32, u32> =
                row.hours.iter().filter_map(|h| h.longest_gap_secs.map(|g| (h.hour, g))).collect();
            for (&h, &g) in &gaps {
                if h == 0 || !in_hour(h) {
                    continue;
                }
                let (Some(&a), Some(&b)) = (gaps.get(&(h - 1)), gaps.get(&(h + 1))) else { continue };
                let around = a.max(b);
                if g as f64 >= factor * around as f64 {
                    holes.push(FrequencyHole {
                        key: format!("hole|{}|{}", row.route_id, h),
                        route_id: row.route_id.clone(),
                        name: row.name.clone(),
                        hour: h,
                        longest_gap_secs: g,
                        around_gap_secs: around,
                    });
                }
            }
        }
    }
    holes.sort_by(|x, y| y.longest_gap_secs.cmp(&x.longest_gap_secs).then_with(|| x.key.cmp(&y.key)));

    let threshold = p.wait_secs.unwrap_or(LONG_WAIT_SECS);
    let waits: Vec<LongWait> = longest_waits(net, date, p.window, None)
        .into_iter()
        .filter(|w| w.longest_wait_secs >= threshold)
        .map(|w| LongWait {
            key: format!("wait|{}", w.stop_id),
            stop_id: w.stop_id,
            name: w.name,
            longest_wait_secs: w.longest_wait_secs,
            wait_from_secs: w.wait_from_secs,
        })
        .collect();

    let cost = p.miss_cost_secs.unwrap_or(MISS_COST_SECS);
    let mut folded: BTreeMap<(String, String, String), (u32, Option<u32>, u32)> = BTreeMap::new();
    for (hub, stop) in &net.stops {
        let station = net.stops.values().any(|c| c.parent.as_deref() == Some(hub.as_str()));
        if !station && !(stop.transfer && stop.parent.is_none()) {
            continue;
        }
        for m in missed_connections(net, date, hub, None, None) {
            if !in_secs(m.arrival_secs) {
                continue;
            }
            let wait = m.next_departure_secs.map(|t| t - m.arrival_secs);
            if wait.is_some_and(|w| w < cost) {
                continue;
            }
            let e = folded.entry((hub.clone(), m.from_route, m.to_route)).or_insert((0, None, 0));
            e.0 += 1;
            match wait {
                Some(w) => e.1 = Some(e.1.map_or(w, |x| x.max(w))),
                None => e.2 += 1,
            }
        }
    }
    let mut misses: Vec<CostlyMiss> = folded
        .into_iter()
        .map(|((hub, from_route, to_route), (misses, worst_wait_secs, stranded))| CostlyMiss {
            key: format!("miss|{hub}|{from_route}|{to_route}"),
            hub_name: net.stops.get(&hub).map(|s| s.name.clone()).unwrap_or_default(),
            hub_id: hub,
            from_route,
            to_route,
            misses,
            worst_wait_secs,
            stranded,
        })
        .collect();
    misses.sort_by(|x, y| y.misses.cmp(&x.misses).then_with(|| x.key.cmp(&y.key)));

    Some(Found { group: combination.id, date: date_text, holes, waits, misses })
}

pub fn network_findings(
    net: &Network,
    base: Option<&Network>,
    members: &[String],
    params: &FindingParams,
) -> Option<NetworkFindings> {
    let now = findings_of(net, members, params)?;
    let then = base.and_then(|b| findings_of(b, members, params));
    let watched: Vec<Watched> = params
        .watch
        .iter()
        .flatten()
        .map(|key| Watched {
            key: key.clone(),
            now: now.find(key),
            base: then.as_ref().and_then(|t| t.find(key)),
        })
        .collect();
    let cut = |n: usize| params.limit.map_or(n, |l| n.min(l as usize));
    let mut resolved = ResolvedFindings::default();
    let (mut base_holes, mut base_waits, mut base_misses): (BTreeMap<_, _>, BTreeMap<_, _>, BTreeMap<_, _>) =
        Default::default();
    if let Some(t) = then {
        let hole_keys: BTreeSet<&String> = now.holes.iter().map(|f| &f.key).collect();
        let wait_keys: BTreeSet<&String> = now.waits.iter().map(|f| &f.key).collect();
        let miss_keys: BTreeSet<&String> = now.misses.iter().map(|f| &f.key).collect();
        for f in t.holes {
            if hole_keys.contains(&f.key) { base_holes.insert(f.key.clone(), f); } else { resolved.holes.push(f); }
        }
        for f in t.waits {
            if wait_keys.contains(&f.key) { base_waits.insert(f.key.clone(), f); } else { resolved.waits.push(f); }
        }
        for f in t.misses {
            if miss_keys.contains(&f.key) { base_misses.insert(f.key.clone(), f); } else { resolved.misses.push(f); }
        }
    }
    resolved.holes.truncate(cut(resolved.holes.len()));
    resolved.waits.truncate(cut(resolved.waits.len()));
    resolved.misses.truncate(cut(resolved.misses.len()));
    let n = (cut(now.holes.len()), cut(now.waits.len()), cut(now.misses.len()));
    Some(NetworkFindings {
        group: now.group,
        date: now.date,
        holes: now.holes.into_iter().take(n.0).map(|f| HoleRow { base: base_holes.remove(&f.key), now: f }).collect(),
        waits: now.waits.into_iter().take(n.1).map(|f| WaitRow { base: base_waits.remove(&f.key), now: f }).collect(),
        misses: now.misses.into_iter().take(n.2).map(|f| MissRow { base: base_misses.remove(&f.key), now: f }).collect(),
        resolved,
        watched,
    })
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct RouteConnections {
    pub from_route: String,
    pub to_route: String,
    pub connections: u32,
    pub shortest_wait_secs: u32,
    pub longest_wait_secs: u32,
    pub median_wait_secs: Option<u32>,
}

pub fn hub_connections(
    net: &Network,
    date: NaiveDate,
    stop_id: &str,
    window: Option<TransferWindow>,
) -> Vec<RouteConnections> {
    let mut pairs: BTreeMap<(String, String), Vec<u32>> = BTreeMap::new();
    for t in timed_transfers(net, date, stop_id, window) {
        pairs.entry((t.from_route, t.to_route)).or_default().push(t.wait_secs);
    }
    pairs
        .into_iter()
        .map(|((from_route, to_route), mut waits)| {
            waits.sort_unstable();
            RouteConnections {
                from_route,
                to_route,
                connections: waits.len() as u32,
                shortest_wait_secs: waits[0],
                longest_wait_secs: waits[waits.len() - 1],
                median_wait_secs: (waits.len() >= 4).then(|| waits[waits.len() / 2]),
            }
        })
        .collect()
}


/// A target headway that varies through the day reads as the range it runs,
/// never rounded to a grid.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct RouteHourGaps {
    pub hour: u32,
    pub departures: u32,
    pub shortest_gap_secs: Option<u32>,
    pub longest_gap_secs: Option<u32>,
    pub median_gap_secs: Option<u32>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct RouteFrequencyRow {
    pub route_id: String,
    pub name: String,
    pub headway_stop: Option<String>,
    pub hours: Vec<RouteHourGaps>,
    pub stats: RouteStats,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct RouteFrequency {
    pub group: String,
    pub members: Vec<String>,
    pub routes: Vec<RouteFrequencyRow>,
}

pub fn route_frequency(net: &Network, members: &[String]) -> Option<RouteFrequency> {
    let combination = transit_network::service_combination(net, members)?;
    let group: BTreeSet<String> = combination.members.iter().cloned().collect();
    let mut stats: BTreeMap<String, RouteStats> =
        family_stats(net, &group).routes.into_iter().map(|r| (r.route_id.clone(), r)).collect();
    let mut departures: BTreeMap<String, BTreeMap<String, Vec<u32>>> = BTreeMap::new();
    for (id, entry) in &net.pattern_trips {
        if !group.contains(&entry.service_type) {
            continue;
        }
        let Ok(day) = transit_network::materialize_service(net, id) else {
            continue;
        };
        for trip in day {
            let Some(route) = net.patterns.get(&trip.pattern).map(|p| p.route.clone()) else {
                continue;
            };
            if trip.stop_times.is_empty() {
                continue;
            }
            for st in &trip.stop_times[..trip.stop_times.len() - 1] {
                departures
                    .entry(route.clone())
                    .or_default()
                    .entry(st.stop.clone())
                    .or_default()
                    .push(st.departure_secs);
            }
        }
    }
    let routes = departures
        .into_iter()
        .filter_map(|(route_id, stops)| {
            let (stop, times) = stops.into_iter().fold(None::<(String, Vec<u32>)>, |best, (stop, times)| match best {
                Some((_, ref b)) if b.len() >= times.len() => best,
                _ => Some((stop, times)),
            })?;
            let spine: Vec<u32> = times.iter().copied().collect::<BTreeSet<u32>>().into_iter().collect();
            let mut by_hour: BTreeMap<u32, (u32, Vec<u32>)> = BTreeMap::new();
            for t in &times {
                by_hour.entry(t / 3600).or_default().0 += 1;
            }
            for w in spine.windows(2) {
                by_hour.entry(w[0] / 3600).or_default().1.push(w[1] - w[0]);
            }
            let hours = by_hour
                .into_iter()
                .map(|(hour, (count, mut gaps))| {
                    gaps.sort_unstable();
                    RouteHourGaps {
                        hour,
                        departures: count,
                        shortest_gap_secs: gaps.first().copied(),
                        longest_gap_secs: gaps.last().copied(),
                        median_gap_secs: (gaps.len() >= 4).then(|| gaps[gaps.len() / 2]),
                    }
                })
                .collect();
            Some(RouteFrequencyRow {
                name: net.routes.get(&route_id).map_or(route_id.clone(), |r| r.label().to_string()),
                stats: stats.remove(&route_id)?,
                route_id,
                headway_stop: Some(stop),
                hours,
            })
        })
        .collect();
    Some(RouteFrequency {
        group: combination.id,
        members: combination.members,
        routes,
    })
}
