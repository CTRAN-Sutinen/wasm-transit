//! The boundary resolution: one rt moment (trip updates and/or
//! vehicle positions) plus the static feed plus the import's trip-id
//! map → THE live document in model vocabulary. The agency's trip
//! ids resolve HERE, once; downstream readers key by derived trip.

use std::collections::HashMap;

use transit_codec::TripIdMap;
use transit_core::Gtfs;
use transit_live::{classify, LiveDay, LiveStop, LiveTrip, LiveVehicle, OnTimeWindow};

use crate::overview::{RtOverview, TripUpdateView};
use crate::progress::trip_progress;

/// The worst delay an update states anywhere (arrival or departure)
/// — `None` when it states none. The same headline `late_trips` and
/// `trip_progress` rank by.
fn stated_worst(update: &TripUpdateView) -> Option<i32> {
    update.stop_updates.iter().filter_map(|s| s.arrival_delay.max(s.departure_delay)).max()
}

/// Resolve a snapshot. `delays` carries trip updates, `positions`
/// vehicles; agencies that serve one combined feed pass it as
/// `delays` and `None` (its own vehicles are used). Every status is
/// classified under `window`, which the document records.
pub fn live_day(
    gtfs: &Gtfs,
    delays: Option<&RtOverview>,
    positions: Option<&RtOverview>,
    trip_map: &TripIdMap,
    window: OnTimeWindow,
) -> LiveDay {
    let derived: HashMap<&str, (&str, u32)> = trip_map
        .entries
        .iter()
        .flat_map(|e| e.agency_trip_ids.iter().map(move |id| (id.as_str(), (e.pattern_trips.as_str(), e.start_secs))))
        .collect();

    let mut trips = Vec::new();
    let mut unmatched_trips = 0u32;
    let mut worst_by_agency: HashMap<&str, Option<i32>> = HashMap::new();
    let mut route_by_agency: HashMap<&str, Option<String>> = HashMap::new();
    // The import names a route by its GTFS display name — the same
    // name the model carries, answered from the feed in hand.
    let route_name = |route: &Option<String>| route.as_deref().and_then(|r| gtfs.routes.get(r)).map(|r| r.display_name());
    if let Some(delays) = delays {
        for update in &delays.trips {
            let Some(trip_id) = update.trip_id.as_deref() else { continue };
            let (Some(&(pattern_trips, start_secs)), Some(progress)) =
                (derived.get(trip_id), trip_progress(gtfs, delays, trip_id))
            else {
                unmatched_trips += 1;
                continue;
            };
            let worst = stated_worst(update);
            let route = update.route_id.clone().or_else(|| progress.route_id.clone());
            worst_by_agency.insert(trip_id, worst);
            route_by_agency.insert(trip_id, route.clone());
            trips.push(LiveTrip {
                pattern_trips: pattern_trips.to_string(),
                start_secs,
                agency_trip_id: trip_id.to_string(),
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
                    let trip_id = v.trip_id.as_deref();
                    let resolved = trip_id.and_then(|t| derived.get(t));
                    let delay = trip_id.and_then(|t| worst_by_agency.get(t).copied().flatten());
                    let route = v
                        .route_id
                        .clone()
                        .or_else(|| trip_id.and_then(|t| route_by_agency.get(t).cloned().flatten()))
                        .or_else(|| trip_id.and_then(|t| gtfs.trips.get(t)).map(|t| t.route_id.clone()));
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
                        pattern_trips: resolved.map(|(s, _)| s.to_string()),
                        start_secs: resolved.map(|(_, start)| *start),
                        agency_trip_id: v.trip_id.clone(),
                        delay_secs: delay,
                        status: classify(delay, &window),
                    }
                })
                .collect()
        })
        .unwrap_or_default();

    LiveDay { window, trips, vehicles, unmatched_trips }
}
