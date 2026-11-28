use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;
#[cfg(feature = "bindings")]
use ts_rs::TS;

use transit_date::Date;

use crate::{
    Network, trip_distance_m, median_gap_secs, METERS_PER_MILE, ServiceTypeSummary, usable_transfer_pairs,
    service_type_summaries
};

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

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "bindings", derive(TS))]
#[cfg_attr(feature = "bindings", ts(export))]
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

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "bindings", derive(TS))]
#[cfg_attr(feature = "bindings", ts(export))]
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


pub(crate) fn stats_fold(net: &Network, include: impl Fn(&str) -> bool, date: Date) -> NetworkStats {
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


pub(crate) fn family_stats(net: &Network, group: &BTreeSet<String>) -> NetworkStats {
    stats_fold(net, |sid| group.contains(sid), Date::MIN)
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