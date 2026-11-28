use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use transit_date::Date;
#[cfg(feature = "bindings")]
use ts_rs::TS;

use crate::{
    CoverageWindow, Network, StopWait, TRANSFER_WINDOW_MAX_SECS, TRANSFER_WINDOW_MIN_SECS,
};

pub fn longest_waits(
    net: &Network,
    date: Date,
    window: Option<CoverageWindow>,
    limit: Option<u32>,
) -> Vec<StopWait> {
    let mut departures: BTreeMap<&str, Vec<u32>> = BTreeMap::new();
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
                if let Some((id, _)) = net.stops.get_key_value(&st.stop) {
                    departures
                        .entry(id.as_str())
                        .or_default()
                        .push(st.departure_secs);
                }
            }
        }
    }
    let mut rows: Vec<StopWait> = Vec::new();
    for (stop_id, deps) in departures {
        let spine: Vec<u32> = deps
            .iter()
            .copied()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
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
        let Some((longest_wait_secs, wait_from_secs)) = best else {
            continue;
        };
        rows.push(StopWait {
            stop_id: stop_id.to_string(),
            name: net.stops[stop_id].name.clone(),
            departures: count,
            longest_wait_secs,
            wait_from_secs,
        });
    }
    rows.sort_by(|a, b| {
        b.longest_wait_secs
            .cmp(&a.longest_wait_secs)
            .then_with(|| a.stop_id.cmp(&b.stop_id))
    });
    if let Some(n) = limit {
        rows.truncate(n as usize);
    }
    rows
}

/// The hour is the service-day hour, 0..47.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "bindings", derive(TS))]
#[cfg_attr(feature = "bindings", ts(export))]
pub struct HourlyTypeTrips {
    pub service_type: String,
    pub trips: u32,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "bindings", derive(TS))]
#[cfg_attr(feature = "bindings", ts(export))]
pub struct HourlyTrips {
    pub hour: u32,
    pub trips: u32,
    pub service_types: Vec<HourlyTypeTrips>,
}

pub fn hourly_trips(net: &Network, date: Date) -> Vec<HourlyTrips> {
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
                .map(|(service_type, trips)| HourlyTypeTrips {
                    service_type,
                    trips,
                })
                .collect(),
        })
        .collect()
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "bindings", derive(TS))]
#[cfg_attr(feature = "bindings", ts(export))]
pub struct RouteOverlap {
    pub a: String,
    pub b: String,
    pub shared_stops: u32,
    pub shared_segments: u32,
    pub stack_stop: Option<String>,
    pub stack_departures: u32,
}

pub fn route_overlap(net: &Network, date: Date) -> Vec<RouteOverlap> {
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
            let Some(pattern) = net.patterns.get(&trip.pattern) else {
                continue;
            };
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
            let shared_segments = ra.segs.intersection(&rb.segs).count() as u32;
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
#[derive(Debug, Clone, Copy, PartialEq, Serialize, serde::Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS))]
#[cfg_attr(feature = "bindings", ts(export))]
pub struct TransferWindow {
    pub min_secs: u32,
    pub max_secs: u32,
}

impl Default for TransferWindow {
    fn default() -> Self {
        TransferWindow {
            min_secs: TRANSFER_WINDOW_MIN_SECS,
            max_secs: TRANSFER_WINDOW_MAX_SECS,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "bindings", derive(TS))]
#[cfg_attr(feature = "bindings", ts(export))]
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
    date: Date,
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
fn hub_visits(net: &Network, date: Date, stop_id: &str) -> (Visits, Visits) {
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

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "bindings", derive(TS))]
#[cfg_attr(feature = "bindings", ts(export))]
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
    date: Date,
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
