//! The boundary resolution: one rt moment (trip updates and/or
//! vehicle positions) plus the network → THE live document in model
//! vocabulary. An rt trip id IS a model trip id; each update resolves
//! HERE, once, to the trip instance it names, and downstream readers
//! key by (trip, start).

use std::collections::HashMap;

use transit_codec::Boundary;
use transit_live::{classify, LiveDay, LiveStop, LiveTrip, LiveVehicle, OnTimeWindow};
use transit_network::Network;

use crate::overview::{RtOverview, TripUpdateView};
use crate::progress::{progress_of, resolve_instance};

/// The worst delay an update states anywhere (arrival or departure)
/// — `None` when it states none. The same headline `late_trips` and
/// `trip_progress` rank by.
fn stated_worst(update: &TripUpdateView) -> Option<i32> {
    update.stop_updates.iter().filter_map(|s| s.arrival_delay.max(s.departure_delay)).max()
}

/// Resolve a snapshot. `delays` carries trip updates, `positions`
/// vehicles; agencies that serve one combined feed pass it as
/// `delays` and `None` (its own vehicles are used). `boundary` is the
/// published numbering the updates' stop_sequences use. Every status
/// is classified under `window`, which the document records.
pub fn live_day(
    network: &Network,
    boundary: &Boundary,
    delays: Option<&RtOverview>,
    positions: Option<&RtOverview>,
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

    LiveDay { window, trips, vehicles, unmatched_trips }
}
