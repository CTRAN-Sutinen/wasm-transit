//! Planner analysis: the questions a network planner asks, answered
//! from an authored document on one service date — at the network
//! level (how much service runs), the route level (span, frequency,
//! trips), and the stop level (what serves this corner, how often) —
//! plus coverage against an imported weighted layer — people, jobs,
//! households, points or polygons ("what share of the jobs does this
//! network serve? with frequent service? what does a proposal gain
//! and lose?"; the `coverage` module).
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

mod coverage;
pub use coverage::{
    coverage, coverage_compare, parse_layer, unserved_points, CoverageDelta, CoverageSide, CoverageStats,
    CoverageWindow, LayerField, LayerReading, Rings, UnservedPlaces, UnservedPoint, WeightedLayer, Weights,
};
use coverage::coverage_of_members;

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
    /// cannot stand in for the route. Null below four departures at
    /// the headway stop: a 3-trip route has no "typical" gap, and
    /// the median would read as a headway nobody runs (QAF-55).
    pub typical_headway_secs: Option<u32>,
    /// Where the headway was measured: the stop with the most
    /// departures of this route today (ties by stop id).
    pub headway_stop: Option<String>,
    /// The route's revenue hours today: the sum of its trips'
    /// runtimes (arrival − start), the cost basis per route.
    pub revenue_hours: f64,
    /// The route's revenue miles today: the sum of its trips'
    /// pattern distances (last stop's distance − first's, the
    /// authored alignment length kept from the feed's
    /// shape_dist_traveled), the OTHER cost basis — every cost
    /// model is per hour AND per mile.
    pub revenue_miles: f64,
}

/// One service type's own numbers: its authored service (every trip
/// it keys), on no date — a date never stands in for a type (D-55).
/// The service a rider gets on a day is a COMBINATION of types
/// ([`service_groups`]).
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

/// The SERVICE GROUP's drill-down: a combination of service types
/// that run together (D-55), pooled over its members' authored
/// service. Routes and headways answer for the combination, not a
/// bare id (service ids key TRIPS, never routes — the 164 lesson),
/// and they ARE the service on every date the combination holds.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct ServiceTypeDetail {
    /// The combination: its id (members joined " + ") and members.
    pub group: String,
    pub members: Vec<String>,
    pub trips: u32,
    pub revenue_hours: f64,
    pub revenue_miles: f64,
    pub transfer_stops: u32,
    /// Routes running under this type (active rows only).
    pub routes: Vec<RouteStats>,
    /// Stops served under this type (served rows only).
    pub stops: Vec<StopStats>,
    pub pattern_trips: Vec<PatternTripsSummary>,
}

/// The drill-down: one service combination expanded. `None` for a
/// member set that never runs together on a date — lookups answer
/// empty for unknown ids; errors are for hash addresses (QAF-19's
/// convention).
pub fn service_type_detail(net: &Network, members: &[String]) -> Option<ServiceTypeDetail> {
    let combination = transit_network::service_combination(net, members)?;
    Some(pooled_detail(net, &combination.members))
}

/// The members' pooled authored service, whether or not they run
/// together in `net` (a baseline the combination is new to runs what
/// its members run there).
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

/// One pattern's schedule texture on the day: the headway
/// uniformity and clockface alignment of its starts, POOLED across
/// every schedule that runs the pattern that day — the same
/// pattern runs under several service ids (route 164's weekday
/// trips split across 1-WKDY and its overlays), and a texture read
/// per service id would slice the corridor into thin, wrong
/// headways. Gaps run between consecutive pooled starts (sorted);
/// the median carries the same four-start floor as every median
/// here (a median over fewer is a headway nobody runs, QAF-55).
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct PatternTexture {
    /// How many service ids feed the pattern that day — the split
    /// the pooling folded (1 = the pattern runs under one type).
    pub schedules: u32,
    pub route: String,
    pub pattern: String,
    pub trips: u32,
    pub min_gap_secs: Option<u32>,
    pub median_gap_secs: Option<u32>,
    pub max_gap_secs: Option<u32>,
    /// Starts on the quarter-hour grid (start % 900 == 0, the
    /// clockface convention) — a 15-minute headway run off-grid
    /// reads "every 15 minutes" but never on the quarter-hour.
    pub on_clockface: u32,
}

/// The SERVICE GROUP's schedule texture — the planner's unit (a
/// planner designs "weekday service", not a service id): a
/// combination of the types that run together (D-55), one row per
/// pattern (sorted by id) with the members' POOLED starts. Keying by
/// service id would slice the corridor into thin, wrong headways
/// (route 164's weekday trips split across 1-WKDY and its overlays).
/// `None` for a member set that never runs together — the convention
/// of service_type_detail (QAF-19).
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
    // The combination: the types that run together on its dates.
    let group: BTreeSet<String> = combination.members.iter().cloned().collect();
    // The scope: every pattern the group's schedules run.
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
        // The combination's POOLED starts: every schedule its members
        // run on this pattern — they all run on each of its dates.
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
        // Two service ids running the pattern at the same second is
        // real service (trips counts both), but no gap — the spine
        // the gaps read is the distinct starts (the stop_gaps
        // convention).
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

/// A proposal against its baseline under one service combination
/// (D-55): both sides' pooled numbers over its members, and the
/// routes whose numbers changed (route id order) — a route only one
/// side runs has `None` on the other. What a planner reads first when
/// asking "what does this change do?".
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct ServiceTypeComparison {
    pub group: String,
    pub members: Vec<String>,
    pub base: ComparedTotals,
    pub proposal: ComparedTotals,
    pub routes: Vec<RouteChange>,
    /// Routes running on both sides with the same numbers.
    pub unchanged_routes: u32,
}

/// One side's totals under the combination.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct ComparedTotals {
    /// The dates the combination holds on this side (0: its members
    /// never run together here — a baseline the combination is new
    /// to still counts what its members run).
    pub days: u32,
    pub trips: u32,
    pub revenue_hours: f64,
    pub revenue_miles: f64,
    pub routes: u32,
}

/// One route whose numbers differ between the baseline and the
/// proposal.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct RouteChange {
    pub route_id: String,
    pub name: String,
    pub base: Option<RouteStats>,
    pub proposal: Option<RouteStats>,
}

/// The comparison: `None` when the members never run together in the
/// proposal — unknown ids answer empty, never an error (QAF-19's
/// convention; errors are for hash addresses). The baseline side
/// pools the same members, whether or not they run together there.
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

/// One SERVICE GROUP: a combination of service types that run
/// together on the same dates (D-55), shown with the dates on which
/// it holds, and its members' POOLED authored service — which IS the
/// service on every one of those dates (no date stands in for it).
/// Members that never share a day are never pooled: C-TRAN's weekday
/// is 1-WKDY + 1-WKDY/30 in one period and 1-WKDY + 1-WKDY/31 in the
/// next, two rows.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct ServiceGroup {
    /// The combination: id (members joined " + "), members, the dates
    /// it holds on and their spans — the drill-down's address is its
    /// members.
    #[serde(flatten)]
    #[ts(flatten)]
    pub combination: transit_network::ServiceCombination,
    pub trips: u32,
    pub active_routes: u32,
    pub active_stops: u32,
    pub revenue_hours: f64,
    pub revenue_miles: f64,
}

/// The service groups: one row per combination
/// (`transit_network::service_combinations`, most days first), each
/// with its members' pooled numbers. This — not the bare service id,
/// and not a calendar date — is the axis a planner designs.
pub fn service_groups(net: &Network) -> Vec<ServiceGroup> {
    transit_network::service_combinations(net)
        .into_iter()
        .map(|combination| {
            let members: BTreeSet<&str> = combination.members.iter().map(String::as_str).collect();
            // The combination's pooled authored service: every
            // member's schedules, their trips, routes, stops, runtime
            // and miles.
            let mut trips = 0u32;
            let mut routes: BTreeSet<String> = BTreeSet::new();
            let mut stops: BTreeSet<String> = BTreeSet::new();
            let mut runtime_secs = 0u64;
            let mut meters = 0f64;
            for (schedule_id, entry) in &net.pattern_trips {
                if !members.contains(entry.service_type.as_str()) {
                    continue;
                }
                // A mid-edit document can carry services that fail to
                // materialize; analysis skips them (validation reports
                // them).
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

/// The overview table: every service type's own numbers (its authored
/// service, on no date). The planner's design axis is the combinations
/// ([`service_groups`]); this is the per-id read under them.
pub fn service_type_overview(net: &Network) -> Vec<ServiceTypeSummary> {
    service_type_summaries(net, None)
}

/// One service COMBINATION's coverage of a weighted layer (D-55): the
/// service types that run together, pooled over their authored service
/// with no date standing in, and the dates it holds on beside it.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct CombinationCoverage {
    /// The combination's name: its members joined " + ".
    pub group: String,
    /// The service types that run together, sorted.
    pub members: Vec<String>,
    /// The dates it holds on, folded into spans.
    pub spans: Vec<transit_network::DateSpan>,
    /// The layer weight within the radius of a stop it serves, and of
    /// a stop it serves frequently.
    pub served: f64,
    pub frequent: f64,
    pub served_pct: f64,
    pub frequent_pct: f64,
    pub served_stops: u32,
    pub frequent_stops: u32,
}

/// A combination's coverage — "what share of the jobs (people,
/// households: the layer's weight) does WEEKDAY service cover?" — over
/// the members' pooled authored service: on any date the combination
/// holds exactly its members run, so this is what each of those dates
/// covers. `window` judges frequency inside a time window (see
/// [`CoverageWindow`]); `None` is the whole-day average. None when the
/// members never run together.
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

/// Per-type numbers: each type's authored service. With
/// `running_on`, only the types whose calendar runs on that date
/// are listed — the date-scoped read (QAF-2: the field used to
/// answer every type on any date); `None` lists every type (the
/// overview's axis scan).
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
    /// Sum of trip pattern distances, miles (last stop's distance −
    /// first's per trip) — the per-mile cost basis that pairs with
    /// revenue_hours; average operating speed follows from the two.
    pub revenue_miles: f64,
    /// Transfer points active on the date (flagged stops whose
    /// walking group is served by at least two routes today).
    pub transfer_stops: u32,
    /// The design axis, date-scoped: the service types whose
    /// calendar RUNS on this date, each with its own authored
    /// service's numbers (QAF-2 — the field used to list every type
    /// on any date).
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
/// below FOUR times — a median over fewer than three gaps is not a
/// "typical" anything, and a 3-trip route reading "every 664
/// minutes" is a headway nobody runs (QAF-55). A median, not the
/// span average: a busy hour and a quiet evening each count for
/// what they are.
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

/// Network stats for one service date. Best-effort: services that
/// fail to materialize (a document mid-edit) are skipped, not fatal.
pub fn network_stats(net: &Network, date: NaiveDate) -> NetworkStats {
    day_stats(net, date, None)
}

/// Meters per mile — the per-mile cost basis is reported in miles.
const METERS_PER_MILE: f64 = 1609.344;

/// One trip's pattern distance in meters: the last stop's distance
/// minus the first's — the authored alignment length the import
/// keeps from the feed's shape_dist_traveled (QAF-56). Zero for a
/// pattern with no distances: an authored document's honest zero.
fn trip_distance_m(net: &Network, trip: &transit_network::MaterializedTrip) -> f64 {
    let Some(pattern) = net.patterns.get(&trip.pattern) else {
        return 0.0;
    };
    match (pattern.stops.first(), pattern.stops.last()) {
        (Some(first), Some(last)) => (last.distance_m - first.distance_m).max(0.0),
        _ => 0.0,
    }
}

/// The stats engine's fold: one pass over the schedules the
/// `include` filter names. The date-scoped views filter by
/// calendar (a type running that date); the COMBINATION view pools
/// its members' authored service with no date filter — on every date
/// the combination holds exactly its members run, so the pooled
/// starts ARE that day's service (D-55).
fn stats_fold(net: &Network, include: impl Fn(&str) -> bool, date: NaiveDate) -> NetworkStats {
    // Per-pattern trip starts on the date: per-direction headways.
    let mut pattern_starts: BTreeMap<String, Vec<u32>> = BTreeMap::new();
    // Per-route accumulators.
    let mut route_trips: BTreeMap<String, u32> = BTreeMap::new();
    let mut route_first: BTreeMap<String, u32> = BTreeMap::new();
    let mut route_last: BTreeMap<String, u32> = BTreeMap::new();
    let mut route_stop_sets: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut route_runtime: BTreeMap<String, u64> = BTreeMap::new();
    // Per-route pattern distance, meters (revenue miles' numerator).
    let mut route_meters: BTreeMap<String, f64> = BTreeMap::new();
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
    let mut meters = 0f64;

    for (service_id, service) in &net.pattern_trips {
        if !include(&service.service_type) {
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
        revenue_miles: meters / METERS_PER_MILE,
        transfer_stops,
        // The wrappers decide whether the design axis rides along
        // (the network view wants it; the family drill-down pools
        // its own numbers).
        service_types: Vec::new(),
        routes,
        stops,
    }
}

/// The date-scoped view: every schedule whose service type runs on
/// the date (optionally one type's alone) — the day-shape read.
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

/// The COMBINATION view: the members' authored service, pooled with
/// no date filter — on every date a combination holds, exactly its
/// members run, so the pooled trips ARE that day's service (the laws
/// pin the equality with `network_stats` on each of its dates). The
/// fold's `date` is unused here (the detail reports no date).
fn family_stats(net: &Network, group: &BTreeSet<String>) -> NetworkStats {
    stats_fold(net, |sid| group.contains(sid), NaiveDate::MIN)
}

// ---------------------------------------------------------------------
// One stop's day: the gap view
// ---------------------------------------------------------------------

/// One hour of a stop's service day. The hour is the service-day
/// hour (0..47 — after-midnight departures are legitimate, never
/// errors); a gap belongs to the hour it STARTS in, so a 23:50
/// departure followed by a 00:30 one is hour 23's gap.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct StopHourGaps {
    pub hour: u32,
    /// Every trip departing in the hour — two routes leaving
    /// together is real service and counts twice.
    pub departures: u32,
    /// The largest gap starting in the hour (between consecutive
    /// distinct departure times).
    pub max_gap_secs: Option<u32>,
    /// The hour's median gap, with the same floor as every median
    /// here: None below four gaps (QAF-55 — a median over fewer is
    /// not a "typical" anything).
    pub median_gap_secs: Option<u32>,
}

/// One stop's day folded into gaps — the service-quality read of
/// "how long until the next bus?". Gaps run between consecutive
/// DISTINCT departure times (two routes leaving together is no gap
/// at all); `departures` still counts every trip. An unknown stop,
/// or one nothing serves on the date, answers zero / None / empty —
/// never an error (an id lookup).
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct StopGaps {
    pub stop_id: String,
    pub name: String,
    pub departures: u32,
    /// The day's first and last departure at the stop.
    pub first_departure_secs: Option<u32>,
    pub last_departure_secs: Option<u32>,
    /// The day's largest gap and the departure it follows. None
    /// below two distinct departures — a one-departure day has no
    /// gap to read.
    pub max_gap_secs: Option<u32>,
    pub max_gap_after_secs: Option<u32>,
    /// Every hour that departs or gaps, sorted.
    pub hourly: Vec<StopHourGaps>,
}

/// The stop's day as gaps: departures on the service date folded
/// into the max gap, the departure it follows, and the per-hour
/// profile (departures, max and median gap starting in the hour).
/// Final-stop visits are arrivals, not departures — they never
/// count (the same line the route headway reads). A station's waits
/// are its bays': asked for a stop other stops name as parent, the
/// departures of those stops are pooled with its own (nothing
/// departs from a GTFS station itself); a bay or a plain
/// stop reads itself alone.
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
        // A mid-edit document can carry services that fail to
        // materialize; analysis skips them (validation reports them).
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
    // Distinct departure times, sorted — the gap spine.
    let distinct: BTreeSet<u32> = departures.iter().copied().collect();
    let spine: Vec<u32> = distinct.into_iter().collect();
    let mut max_gap = None;
    let mut max_after = None;
    // The hour's gaps (starting in the hour), for the per-hour rows.
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

/// One stop in the network's longest-waits ranking.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct StopWait {
    pub stop_id: String,
    pub name: String,
    /// Departures inside the window (the whole day without one).
    pub departures: u32,
    /// The stop's longest wait for the next departure, and when it
    /// starts (a departure, or the window's start).
    pub longest_wait_secs: u32,
    pub wait_from_secs: u32,
}

/// Where riders wait longest (service-design question 8): every stop
/// that departs on the date, ranked by its longest wait — longest
/// first, then by stop id — and cut to `limit`. Without a window the
/// wait is the largest gap between distinct departure times (a stop
/// with fewer than two has none and is no row). With one it is
/// coverage's reading of a window ([`CoverageWindow`]): from the
/// window's start to the first departure in it, between departures,
/// and from the last to the window's end; a stop that departs that
/// day but not inside the window waits the whole window. Each stop
/// reads alone — a station's bays rank one by one, the boarding
/// places riders stand at. Departures only (a final visit is an
/// arrival); every route at the stop pools, as a rider sees it.
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
        // The first largest gap along the edges.
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

/// One service hour of one service type's day: how many trips
/// START in the hour. The hour is the service-day hour (0..47 —
/// after-midnight starts are legitimate, never errors).
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct HourlyTypeTrips {
    pub service_type: String,
    pub trips: u32,
}

/// One service hour of the day: every running service type's trip
/// count, sorted by type id. Only hours with at least one trip are
/// rows — the empty hours are the span's edges, not holes.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct HourlyTrips {
    pub hour: u32,
    pub trips: u32,
    pub service_types: Vec<HourlyTypeTrips>,
}

/// The day's service profile: trips per hour, per service type,
/// bucketed by trip start — the planner's first texture question
/// (where the service is thick, where it tapers). The materialized
/// day on the date, nothing else.
pub fn hourly_trips(net: &Network, date: NaiveDate) -> Vec<HourlyTrips> {
    // hour → (service type → trips), sorted structures throughout.
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

/// Two routes that meet: the corridor-overlap read. Stop sharing is
/// where they meet; segment sharing (consecutive stops in the same
/// order) is where they run together — the corridor. The stack is
/// where the two routes' service piles up: the shared stop with
/// the most combined departures on the date.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct RouteOverlap {
    /// The pair, route ids sorted (a < b).
    pub a: String,
    pub b: String,
    /// Stops both routes serve on the date.
    pub shared_stops: u32,
    /// Consecutive stop pairs both routes run, in the same order —
    /// the shared segments (the corridor proper).
    pub shared_segments: u32,
    /// The shared stop where the two routes' departures stack
    /// highest, and the combined departures there on the date.
    pub stack_stop: Option<String>,
    pub stack_departures: u32,
}

/// Which routes share stops or segments, and how much service
/// stacks there — the corridor question (the Vine Green/Red read):
/// every pair of routes that shares at least one stop on the date,
/// with the shared-stop count, the shared-segment count (same-order
/// consecutive pairs), and the busiest shared stop by combined
/// departures. Pairs sorted by (a, b).
pub fn route_overlap(net: &Network, date: NaiveDate) -> Vec<RouteOverlap> {
    // Per route: served stops, ordered segments, departures per stop.
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

/// The hub connection window a caller brings: the wait between an
/// arrival and a departure counts as a transfer when
/// min_secs <= wait <= max_secs. Defaults (omitted) are the
/// transfer-point convention: a rider needs a minute to move
/// (60 s), and beyond 15 minutes it is not a timed transfer — the
/// walk across the street or down the block is folded into those
/// bounds. A hub with widely separated bays gets a wider window.
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

/// One usable timed connection at a hub: an arrival of one route
/// followed by a departure of another within the connection window
/// (the caller's, else the 60–900 s transfer-point convention).
/// Stops are the CONCRETE bays — a connection crosses the hub's
/// walking group (across a street, down a block).
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

/// The hub's timed-transfer view: every arrival→departure pair
/// across routes within the connection window (the caller's, else
/// the 60–900 s convention the transfer-point pairs use), pooled
/// over the stop's walking group — the station board's next
/// question ("who can make what here?"), not just pooled
/// departures. Sorted by arrival time, then routes. An unknown stop
/// answers empty, never an error.
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

/// A hub's day as (route, stop, time) visits, pooled over the stop's
/// walking group: arrivals are every visit; departures are every
/// visit but the trip's last (final stops are arrivals).
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

/// How far before an arrival a departure still counts as just
/// missed, when the caller names no `miss_secs`: five minutes (the
/// provisional answer, D-133).
pub const MISS_SECS: u32 = 300;

/// One near miss at a hub: an arrival of one route and a departure of
/// another that leaves before the rider can make it.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct MissedConnection {
    pub from_route: String,
    pub to_route: String,
    pub arrival_secs: u32,
    pub arrival_stop: String,
    pub departure_secs: u32,
    pub departure_stop: String,
    /// How much later the departure would have had to leave for the
    /// rider to make it (the window's minimum after the arrival).
    pub missed_by_secs: u32,
    /// The route's first departure from the hub the rider can make,
    /// or none that day.
    pub next_departure_secs: Option<u32>,
}

/// The hub's just-missed connections (service-design question 9):
/// over the stop's walking group, every arrival of one route and
/// departure of another where the departure leaves from `miss_secs`
/// (default [`MISS_SECS`]) before the arrival up to, not including,
/// the connection window's minimum after it — the rider sees it go.
/// No near miss is a timed transfer. Each names the route's next
/// departure the rider can make, so a frequent route's misses read as
/// the short waits they are. Sorted as `timed_transfers` sorts. An
/// unknown stop answers empty, never an error.
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

/// One route pair's timed connections at a hub: how many the day
/// makes, the shortest and longest wait, and the median wait with
/// the floor every median here carries (None below four — QAF-55).
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

/// The hub as a route-pair matrix: `timed_transfers` folded by
/// (from route, to route) — who connects to whom here, how often,
/// and how long the wait runs. Pairs in route order; a pair with no
/// connection is no row. An unknown stop answers empty.
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

// ---------------------------------------------------------------------
// The frequency grid: each route's day by hour
// ---------------------------------------------------------------------

/// One hour of a route's day at its headway stop. A gap belongs to
/// the hour it starts in, between consecutive distinct departure
/// times; the gaps are the trips' own — a target headway adjusted
/// through the day for traffic, layover and reliefs reads as the
/// range it runs, never rounded to a grid.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct RouteHourGaps {
    /// The service-day hour (0..47).
    pub hour: u32,
    /// Every departure in the hour at the headway stop.
    pub departures: u32,
    pub shortest_gap_secs: Option<u32>,
    pub longest_gap_secs: Option<u32>,
    /// None below four gaps (QAF-55's floor).
    pub median_gap_secs: Option<u32>,
}

/// One route's row of the grid: where its headway is read (the stop
/// with the most departures, lowest id on a tie — `RouteStats`'
/// `headway_stop`), that stop's hours, and the route's numbers under
/// the group (`service_type_detail`'s row for it) — one row the grid
/// reads whole.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct RouteFrequencyRow {
    pub route_id: String,
    pub name: String,
    pub headway_stop: Option<String>,
    pub hours: Vec<RouteHourGaps>,
    pub stats: RouteStats,
}

/// The SERVICE GROUP's frequency grid: every route that departs
/// under the combination (route id order), hour by hour.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct RouteFrequency {
    pub group: String,
    pub members: Vec<String>,
    pub routes: Vec<RouteFrequencyRow>,
}

/// How often each route comes, hour by hour, under a service
/// combination — the members `service_type_detail` pools, read at each
/// route's headway stop. `None` for a member set that never runs
/// together (QAF-19).
pub fn route_frequency(net: &Network, members: &[String]) -> Option<RouteFrequency> {
    let combination = transit_network::service_combination(net, members)?;
    let group: BTreeSet<String> = combination.members.iter().cloned().collect();
    let mut stats: BTreeMap<String, RouteStats> =
        family_stats(net, &group).routes.into_iter().map(|r| (r.route_id.clone(), r)).collect();
    // route → stop → departures (final visits are arrivals).
    let mut departures: BTreeMap<String, BTreeMap<String, Vec<u32>>> = BTreeMap::new();
    for (id, entry) in &net.pattern_trips {
        if !group.contains(&entry.service_type) {
            continue;
        }
        // A mid-edit document can carry services that fail to
        // materialize; analysis skips them (validation reports them).
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
            // The busiest stop: most departures, the first by id on a
            // tie (BTreeMap order) — the stats fold's rule.
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
