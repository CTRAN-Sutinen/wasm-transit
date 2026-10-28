//! The boundary resolution: one rt moment (trip updates and/or
//! vehicle positions) plus the network → THE live document in model
//! vocabulary. An rt trip id IS a model trip id; each update resolves
//! HERE, once, to the trip instance it names, and downstream readers
//! key by (trip, start). Service alerts resolve here too: the ones in
//! force at the snapshot's moment, their informed entities named as
//! the network's routes, stops, and trip instances.

use std::collections::HashMap;

use transit_codec::Boundary;
use transit_live::{classify, AlertPeriod, AlertScope, LiveAlert, LiveDay, LiveStop, LiveTrip, LiveVehicle, OnTimeWindow};
use transit_network::Network;

use crate::overview::{AlertView, InformedView, RtOverview, TripUpdateView};
use crate::progress::{progress_of, resolve_instance};

/// The worst delay an update states anywhere (arrival or departure)
/// — `None` when it states none. The same headline `late_trips` and
/// `trip_progress` rank by.
fn stated_worst(update: &TripUpdateView) -> Option<i32> {
    update.stop_updates.iter().filter_map(|s| s.arrival_delay.max(s.departure_delay)).max()
}

/// Resolve a snapshot. `delays` carries trip updates, `positions`
/// vehicles, `alerts` service alerts; agencies that serve one combined
/// feed pass it as `delays` and `None`s (its own vehicles and alerts
/// are used — alerts are gathered from every snapshot given, one per
/// entity id). `boundary` is the published numbering the updates'
/// stop_sequences use. Every status is classified under `window`,
/// which the document records.
pub fn live_day(
    network: &Network,
    boundary: &Boundary,
    delays: Option<&RtOverview>,
    positions: Option<&RtOverview>,
    alerts: Option<&RtOverview>,
    window: OnTimeWindow,
) -> LiveDay {
    let route_name = |route: &Option<String>| route.as_deref().and_then(|r| network.routes.get(r)).map(|r| r.label().to_string());
    let mut trips = Vec::new();
    let mut unmatched_trips = 0u32;
    let mut worst_by_instance: HashMap<(String, u32), Option<i32>> = HashMap::new();
    if let Some(delays) = delays {
        for update in &delays.trips {
            let instance = update
                .trip_id
                .as_deref()
                .and_then(|t| resolve_instance(network, t, update.start_time.as_deref()));
            let Some(instance) = instance else {
                unmatched_trips += 1;
                continue;
            };
            let progress = progress_of(network, boundary, update, &instance);
            let worst = stated_worst(update);
            let route = update.route_id.clone().or_else(|| progress.route_id.clone());
            worst_by_instance.insert((instance.trip.clone(), instance.start_secs), worst);
            trips.push(LiveTrip {
                trip: instance.trip.clone(),
                start_secs: instance.start_secs,
                route_name: route_name(&route),
                route,
                worst_delay_secs: worst,
                status: classify(worst, &window),
                stops: progress
                    .stops
                    .into_iter()
                    .filter(|s| s.arrival_delay.is_some() || s.departure_delay.is_some())
                    .map(|s| LiveStop {
                        stop: s.stop_id,
                        scheduled_departure: s.scheduled_departure,
                        arrival_delay: s.arrival_delay,
                        departure_delay: s.departure_delay,
                        predicted_departure: s.predicted_departure,
                    })
                    .collect(),
            });
        }
    }

    let source = positions.or(delays);
    let vehicles = source
        .map(|snapshot| {
            snapshot
                .vehicle_positions
                .iter()
                .map(|v| {
                    let instance = v
                        .trip_id
                        .as_deref()
                        .and_then(|t| resolve_instance(network, t, v.start_time.as_deref()));
                    let key = instance.as_ref().map(|i| (i.trip.clone(), i.start_secs));
                    let delay = key.as_ref().and_then(|k| worst_by_instance.get(k).copied().flatten());
                    let route = v.route_id.clone().or_else(|| instance.as_ref().map(|i| i.route.clone()));
                    LiveVehicle {
                        key: v.vehicle_id.clone().unwrap_or_else(|| v.entity_id.clone()),
                        vehicle_id: v.vehicle_id.clone(),
                        label: v.label.clone(),
                        lat: v.lat,
                        lon: v.lon,
                        bearing: v.bearing,
                        route_name: route_name(&route),
                        route,
                        next_stop: v.stop_id.clone(),
                        trip: key.as_ref().map(|(t, _)| t.clone()),
                        start_secs: key.as_ref().map(|(_, s)| *s),
                        delay_secs: delay,
                        status: classify(delay, &window),
                    }
                })
                .collect()
        })
        .unwrap_or_default();

    let (alerts, unmatched_alerts) = live_alerts(network, [alerts, delays, positions]);
    LiveDay { window, trips, vehicles, unmatched_trips, alerts, unmatched_alerts }
}

/// The alerts in force at each snapshot's moment, resolved; the count
/// of those none of whose informed entities name anything here.
fn live_alerts<const N: usize>(network: &Network, snapshots: [Option<&RtOverview>; N]) -> (Vec<LiveAlert>, u32) {
    let mut seen = std::collections::HashSet::new();
    let mut alerts = Vec::new();
    let mut unmatched = 0u32;
    for snapshot in snapshots.into_iter().flatten() {
        for view in &snapshot.service_alerts {
            if !seen.insert(view.entity_id.clone()) || !in_force(&view.communication, snapshot.timestamp) {
                continue;
            }
            let scopes: Vec<AlertScope> = view.informed.iter().flat_map(|e| resolve_scope(network, e)).collect();
            if scopes.is_empty() {
                unmatched += 1;
                continue;
            }
            alerts.push(live_alert(view, scopes));
        }
    }
    (alerts, unmatched)
}

/// No period means always; an unstamped snapshot keeps every alert.
fn in_force(periods: &[AlertPeriod], now: Option<u64>) -> bool {
    let Some(now) = now else { return true };
    periods.is_empty()
        || periods.iter().any(|p| p.start.is_none_or(|s| s <= now) && p.end.is_none_or(|e| now <= e))
}

fn live_alert(view: &AlertView, scopes: Vec<AlertScope>) -> LiveAlert {
    LiveAlert {
        id: view.entity_id.clone(),
        cause: view.cause,
        effect: view.effect,
        severity: view.severity,
        header: view.header.clone().unwrap_or_default(),
        description: view.description.clone(),
        url: view.url.clone(),
        impact: view.impact.clone(),
        scopes,
    }
}

/// One informed entity → the scopes it names (a route_type names every
/// route published under it). Every named field must resolve, or the
/// entity names nothing. The direction is not matched: an alert for
/// one direction of a route reaches both (over-telling beats silence).
fn resolve_scope(network: &Network, e: &InformedView) -> Vec<AlertScope> {
    if e.route_id.is_none() && e.route_type.is_none() && e.trip_id.is_none() && e.stop_id.is_none() {
        return if e.agency_id.is_some() { vec![AlertScope::default()] } else { Vec::new() };
    }
    let stop = match &e.stop_id {
        Some(s) if network.stops.contains_key(s) => Some(s.clone()),
        Some(_) => return Vec::new(),
        None => None,
    };
    let (trip, start_secs) = match &e.trip_id {
        Some(t) => {
            if network.trip(t).is_none() {
                return Vec::new();
            }
            let instance = resolve_instance(network, t, e.trip_start_time.as_deref());
            if e.trip_start_time.is_some() && instance.is_none() {
                return Vec::new();
            }
            (Some(t.clone()), instance.map(|i| i.start_secs))
        }
        None => (None, None),
    };
    let routes: Vec<Option<String>> = match (&e.route_id, e.route_type) {
        (None, None) => vec![None],
        (Some(r), t) => match network.routes.get(r) {
            Some(route) if t.is_none_or(|t| transit_codec::published_route_type(route) as i32 == t) => vec![Some(r.clone())],
            _ => return Vec::new(),
        },
        (None, Some(t)) => network
            .routes
            .iter()
            .filter(|(_, route)| transit_codec::published_route_type(route) as i32 == t)
            .map(|(id, _)| Some(id.clone()))
            .collect(),
    };
    routes
        .into_iter()
        .map(|route| AlertScope { route, stop: stop.clone(), trip: trip.clone(), start_secs })
        .collect()
}
