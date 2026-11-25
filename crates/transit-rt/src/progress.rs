use serde::Serialize;
use transit_codec::Boundary;
use transit_network::{trip_instance, Departs, MaterializedTrip, Network};
use ts_rs::TS;

use crate::overview::{RtOverview, TripUpdateView, VehicleView};

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct ProgressStop {
    pub stop_id: String,
    pub stop_sequence: u32,
    pub scheduled_arrival: Option<u32>,
    pub scheduled_departure: Option<u32>,
    /// Seconds. Positive is late, negative is early.
    pub arrival_delay: Option<i32>,
    pub departure_delay: Option<i32>,
    pub predicted_arrival: Option<u32>,
    pub predicted_departure: Option<u32>,
    pub skipped: bool,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct TripProgress {
    pub trip_id: String,
    pub start_secs: u32,
    pub route_id: Option<String>,
    pub block_id: Option<String>,
    pub worst_delay_secs: i32,
    pub stops: Vec<ProgressStop>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct LateTrip {
    pub trip_id: Option<String>,
    pub route_id: Option<String>,
    pub block_id: Option<String>,
    pub worst_delay_secs: i32,
    /// VehicleDescriptor.id. Some agencies leave `label` empty (C-TRAN
    /// does), so this still names the bus.
    pub vehicle_id: Option<String>,
    pub vehicle_label: Option<String>,
    /// A trip is delayed but dark when this is false, because no vehicle
    /// position names it or any trip of its block.
    pub has_position: bool,
    /// This trip, or another trip of the same block. A delay that
    /// propagates to the block's next trip while the bus finishes the trip
    /// before is a late bus, not a dark one.
    pub position_trip_id: Option<String>,
    pub lat: Option<f64>,
    pub lon: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub route_name: Option<String>,
    /// Absent for a frequency trip, whose instances share the trip.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub start_secs: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub first_stop: Option<StopName>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub headsign: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub near_stop: Option<StopName>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct StopName {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub name: Option<String>,
}

fn predicted(scheduled: Option<u32>, delay: Option<i32>) -> Option<u32> {
    match (scheduled, delay) {
        (Some(s), Some(d)) => Some((s as i64 + d as i64).max(0) as u32),
        (Some(s), None) => Some(s),
        (None, _) => None,
    }
}

pub(crate) fn parse_time(text: &str) -> Option<u32> {
    let mut parts = text.trim().split(':');
    let (h, m, s) = (parts.next()?, parts.next()?, parts.next()?);
    Some(h.parse::<u32>().ok()? * 3600 + m.parse::<u32>().ok()? * 60 + s.parse::<u32>().ok()?)
}

pub fn resolve_instance(network: &Network, trip_id: &str, start_time: Option<&str>) -> Option<MaterializedTrip> {
    let (_, _, trip) = network.trip(trip_id)?;
    let start = match trip.departs {
        Departs::At { start_secs } => start_secs,
        Departs::Every { .. } => parse_time(start_time?)?,
    };
    trip_instance(network, trip_id, start)
}

fn sequences(boundary: &Boundary, trip: &str, n: usize) -> Vec<u32> {
    boundary
        .stop_sequences
        .get(trip)
        .filter(|s| s.len() == n)
        .cloned()
        .unwrap_or_else(|| (1..=n as u32).collect())
}

pub(crate) fn progress_of(network: &Network, boundary: &Boundary, update: &TripUpdateView, instance: &MaterializedTrip) -> TripProgress {
    let seqs = sequences(boundary, &instance.trip, instance.stop_times.len());
    let mut stops = Vec::with_capacity(instance.stop_times.len());
    let mut worst = 0i32;
    let mut carried: Option<i32> = None;
    for (k, (row, &seq)) in instance.stop_times.iter().zip(&seqs).enumerate() {
        // An update that names no sequence pairs with the stop's first visit.
        let first_visit = instance.stop_times.iter().position(|r| r.stop == row.stop) == Some(k);
        let upd = update.stop_updates.iter().find(|s| match s.stop_sequence {
            Some(q) => q == seq,
            None => first_visit && s.stop_id.as_deref() == Some(row.stop.as_str()),
        });
        let skipped = upd.is_some_and(|s| s.skipped);
        let (arrival_delay, departure_delay) = match upd {
            Some(s) if !s.skipped => (s.arrival_delay, s.departure_delay),
            Some(_) => (None, None),
            None => (carried, carried),
        };
        if let Some(d) = departure_delay.or(arrival_delay) {
            carried = Some(d);
        }
        worst = worst.max(arrival_delay.max(departure_delay).unwrap_or(0));
        stops.push(ProgressStop {
            stop_id: row.stop.clone(),
            stop_sequence: seq,
            scheduled_arrival: Some(row.arrival_secs),
            scheduled_departure: Some(row.departure_secs),
            arrival_delay,
            departure_delay,
            predicted_arrival: predicted(Some(row.arrival_secs), arrival_delay),
            predicted_departure: predicted(Some(row.departure_secs), departure_delay),
            skipped,
        });
    }
    TripProgress {
        trip_id: instance.trip.clone(),
        start_secs: instance.start_secs,
        route_id: Some(instance.route.clone()),
        block_id: block_of(network, &instance.trip),
        worst_delay_secs: worst,
        stops,
    }
}

fn block_of(network: &Network, trip: &str) -> Option<String> {
    network.blocks.iter().find(|(_, b)| b.trips.iter().any(|r| r.trip == trip)).map(|(id, _)| id.clone())
}

/// A trip with no update answers its schedule alone, with no delay.
pub fn trip_progress(network: &Network, boundary: &Boundary, overview: &RtOverview, trip_id: &str) -> Option<TripProgress> {
    let quiet = TripUpdateView { trip_id: Some(trip_id.to_string()), route_id: None, start_time: None, start_date: None, canceled: false, stop_updates: vec![] };
    let update = overview.trips.iter().find(|t| t.trip_id.as_deref() == Some(trip_id)).unwrap_or(&quiet);
    let instance = resolve_instance(network, trip_id, update.start_time.as_deref())?;
    Some(progress_of(network, boundary, update, &instance))
}

pub fn late_trips(
    network: &Network,
    delays: &RtOverview,
    positions: Option<&RtOverview>,
    min_delay_secs: i32,
) -> Vec<LateTrip> {
    let vehicles = positions.unwrap_or(delays).vehicle_positions.as_slice();
    let position_of = |trip_id: &str, block_id: Option<&str>| -> Option<&VehicleView> {
        vehicles.iter().find(|v| v.trip_id.as_deref() == Some(trip_id)).or_else(|| {
            let block_id = block_id?;
            vehicles
                .iter()
                .find(|v| v.trip_id.as_deref().and_then(|t| block_of(network, t)).as_deref() == Some(block_id))
        })
    };
    let route_of = |trip_id: &str| {
        let (_, pt, _) = network.trip(trip_id)?;
        network.patterns.get(&pt.pattern).map(|p| p.route.clone())
    };
    let stop_name = |id: &str| StopName { id: id.to_string(), name: network.stops.get(id).map(|s| s.name.clone()) };
    let stops_of = |trip_id: &str| -> Vec<String> {
        network
            .trip(trip_id)
            .and_then(|(_, pt, _)| network.patterns.get(&pt.pattern))
            .map(|p| p.stops.iter().map(|s| s.stop.to_string()).collect())
            .unwrap_or_default()
    };
    // An equirectangular distance orders stops within a city well enough.
    let nearest = |trip_id: &str, lat: f64, lon: f64| -> Option<StopName> {
        let scale = lat.to_radians().cos();
        stops_of(trip_id)
            .into_iter()
            .filter_map(|id| network.stops.get(id.as_str()).map(|s| (((s.lat - lat).powi(2) + ((s.lon - lon) * scale).powi(2)), id)))
            .min_by(|a, b| a.0.total_cmp(&b.0).then_with(|| a.1.cmp(&b.1)))
            .map(|(_, id)| stop_name(&id))
    };

    let mut late: Vec<LateTrip> = Vec::new();
    for update in &delays.trips {
        let Some(trip_id) = update.trip_id.as_deref().filter(|_| !update.canceled) else { continue };
        let worst = update
            .stop_updates
            .iter()
            .filter(|s| !s.skipped)
            .map(|s| (s.arrival_delay).max(s.departure_delay).unwrap_or(0))
            .max()
            .unwrap_or(0);
        if worst < min_delay_secs {
            continue;
        }
        let block_id = block_of(network, trip_id);
        let vehicle = position_of(trip_id, block_id.as_deref());
        let trip = network.trip(trip_id).map(|(_, _, t)| t);
        let route_id = update.route_id.clone().or_else(|| route_of(trip_id));
        late.push(LateTrip {
            route_name: route_id.as_deref().and_then(|r| network.routes.get(r)).map(|r| r.name.clone()),
            start_secs: trip.and_then(|t| match t.departs {
                transit_network::Departs::At { start_secs } => Some(start_secs),
                _ => None,
            }),
            first_stop: stops_of(trip_id).first().map(|s| stop_name(s)),
            headsign: trip.and_then(|t| t.headsign.clone()),
            near_stop: vehicle.and_then(|v| nearest(v.trip_id.as_deref().unwrap_or(trip_id), v.lat, v.lon)),
            trip_id: update.trip_id.clone(),
            route_id,
            block_id,
            worst_delay_secs: worst,
            vehicle_id: vehicle.and_then(|v| v.vehicle_id.clone()),
            vehicle_label: vehicle.and_then(|v| v.label.clone()),
            has_position: vehicle.is_some(),
            position_trip_id: vehicle.and_then(|v| v.trip_id.clone()),
            lat: vehicle.map(|v| v.lat),
            lon: vehicle.map(|v| v.lon),
        });
    }
    late.sort_by(|a, b| b.worst_delay_secs.cmp(&a.worst_delay_secs).then_with(|| a.trip_id.cmp(&b.trip_id)));
    late
}
