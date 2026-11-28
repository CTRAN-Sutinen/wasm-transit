use std::collections::BTreeSet;

use serde::Serialize;
#[cfg(feature = "bindings")]
use ts_rs::TS;
use transit_date::Date;
use crate::{
    METERS_PER_MILE, ServiceTypeSummary, coverage_of_members, CoverageWindow,
    Weights, trip_distance_m, service_type_compare, Network, ServiceTypeComparison,
    stats_fold, NetworkStats
};


#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "bindings", derive(TS))]
#[cfg_attr(feature = "bindings", ts(export))]
pub struct DayTypeGroup {
    pub id: String,
    pub members: Vec<String>,
    pub dates: Vec<String>,
    pub spans: Vec<transit_network::DateSpan>,
    pub weekdays: Vec<bool>,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "bindings", derive(TS))]
#[cfg_attr(feature = "bindings", ts(export))]
pub struct DayType {
    /// A day type's regular group runs on more than half of a weekday's dates
    /// in the period. Otherwise the type takes every weekday it runs on.
    pub weekdays: Vec<bool>,
    pub groups: Vec<DayTypeGroup>,
}

/// An absent `from` or `through` is the first or last in-period date that any
/// combination holds. A combination with no date in the period is left out
/// (D-137).
pub fn day_types(net: &Network, from: Option<Date>, through: Option<Date>) -> Vec<DayType> {
    
    let mut groups: Vec<DayTypeGroup> = transit_network::service_combinations(net)
        .into_iter()
        .filter_map(|c| {
            let dates: Vec<Date> = c
                .dates
                .iter()
                .filter_map(|d| Date::parse(d).ok())
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
                dates: dates.iter().map(|d| d.to_string()).collect(),
                weekdays,
            })
        })
        .collect();
    groups.sort_by(|a, b| {
        b.dates.len().cmp(&a.dates.len()).then_with(|| a.dates[0].cmp(&b.dates[0])).then_with(|| a.id.cmp(&b.id))
    });
    let first = from.or_else(|| groups.iter().filter_map(|g| g.dates.first()).min().and_then(|d| Date::parse(d).ok()));
    let last = through.or_else(|| groups.iter().filter_map(|g| g.dates.last()).max().and_then(|d| Date::parse(d).ok()));
    let mut period = [0u32; 7];
    if let (Some(mut d), Some(end)) = (first, last) {
        while d <= end {
            period[d.weekday().num_days_from_monday() as usize] += 1;
            d += transit_date::Days::new(1);
        }
    }
    let within = |a: &[bool], b: &[bool]| a.iter().zip(b).all(|(x, y)| !*x || *y);
    let mut types: Vec<DayType> = Vec::new();
    for g in groups {
        match types.iter_mut().find(|t| within(&g.weekdays, &t.weekdays)) {
            Some(t) => t.groups.push(g),
            None => {
                let mut held = [0u32; 7];
                for d in g.dates.iter().filter_map(|d| Date::parse(d).ok()) {
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

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "bindings", derive(TS))]
#[cfg_attr(feature = "bindings", ts(export))]
pub struct DayTypeGroupComparison {
    pub group: DayTypeGroup,
    pub comparison: Option<ServiceTypeComparison>,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "bindings", derive(TS))]
#[cfg_attr(feature = "bindings", ts(export))]
pub struct DayTypeComparison {
    pub weekdays: Vec<bool>,
    pub groups: Vec<DayTypeGroupComparison>,
}

pub fn plan_compare(
    base: &Network,
    proposal: &Network,
    from: Option<Date>,
    through: Option<Date>,
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

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "bindings", derive(TS))]
#[cfg_attr(feature = "bindings", ts(export))]
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
fn service_type_summaries(net: &Network, running_on: Option<Date>) -> Vec<ServiceTypeSummary> {
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


pub fn network_stats(net: &Network, date: Date) -> NetworkStats {
    day_stats(net, date, None)
}

fn day_stats(net: &Network, date: Date, only_type: Option<&str>) -> NetworkStats {
    let mut stats = stats_fold(net, |sid| {
        net.service_types
            .get(sid)
            .is_some_and(|st| st.rule.runs_on(date))
            && only_type.map_or(true, |t| sid == t)
    }, date);
    stats.service_types = service_type_summaries(net, Some(date));
    stats
}