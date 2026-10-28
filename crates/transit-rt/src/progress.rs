//! The join the dispatcher actually asks for: one trip's remaining
//! stops with the SCHEDULE, the live DELAY, and the PREDICTED time
//! in one answer — plus the ranked "what's late right now" view.
//!
//! This is the layer where realtime meets the network: rt gives
//! delays, the network's trips give times, and no host should have
//! to join them by hand. An rt trip id IS a model trip id; the
//! instance is the trip's start, or a frequency trip's rt start_time.
//! The join is over the SEQUENCE: GTFS-rt's StopTimeUpdate carries
//! the stop_sequence the published feed carries — the import's
//! `Boundary` numbering, 1..n when it has none — so the rows pair
//! without guessing; an update naming no sequence pairs by stop id.

use serde::Serialize;
use transit_codec::Boundary;
use transit_network::{trip_instance, Departs, MaterializedTrip, Network};
use ts_rs::TS;

use crate::overview::{RtOverview, TripUpdateView, VehicleView};

/// One stop on a trip's progress: the schedule, the live delay, and
/// the prediction (schedule + delay) — the three numbers a
/// dispatcher reads off one row.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct ProgressStop {
    pub stop_id: String,
    pub stop_sequence: u32,
    pub scheduled_arrival: Option<u32>,
    pub scheduled_departure: Option<u32>,
    /// Seconds of delay; positive = late, negative = early. A stop
    /// with no update of its own carries the last stated delay
    /// upstream of it (the GTFS-rt propagation rule); null before
    /// the first update.
    pub arrival_delay: Option<i32>,
    pub departure_delay: Option<i32>,
    /// Schedule + delay, clamped at 0 — when this vehicle should
    /// actually be here.
    pub predicted_arrival: Option<u32>,
    pub predicted_departure: Option<u32>,
}

/// One trip's progress: the joined board for a single vehicle.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct TripProgress {
    pub trip_id: String,
    /// The instance's start.
    pub start_secs: u32,
    pub route_id: Option<String>,
    pub block_id: Option<String>,
    /// The worst delay across the trip's stops — the headline number.
    pub worst_delay_secs: i32,
    pub stops: Vec<ProgressStop>,
}

/// One late trip in the ranked view.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct LateTrip {
    pub trip_id: Option<String>,
    pub route_id: Option<String>,
    /// The trip's block in the network — the chain this delay lands
    /// in.
    pub block_id: Option<String>,
    /// The worst delay at any stop of the trip.
    pub worst_delay_secs: i32,
    /// The located vehicle's internal id (VehicleDescriptor.id) —
    /// agencies that leave `label` empty (C-TRAN does) still name
    /// the bus here.
    pub vehicle_id: Option<String>,
    /// The user-visible bus number, when the located vehicle has one.
    pub vehicle_label: Option<String>,
    /// DELAYED BUT DARK: a delay exists but no vehicle position
    /// names this trip OR any trip of its block — the dispatcher
    /// cannot see where the bus is.
    pub has_position: bool,
    /// The trip the located vehicle is actually running: this trip,
    /// or another trip of the same block — a delay propagated to the
    /// block's NEXT trip while the bus finishes the one before is a
    /// late bus, not a dark one.
    pub position_trip_id: Option<String>,
    pub lat: Option<f64>,
    pub lon: Option<f64>,
}

fn predicted(scheduled: Option<u32>, delay: Option<i32>) -> Option<u32> {
    match (scheduled, delay) {
        (Some(s), Some(d)) => Some((s as i64 + d as i64).max(0) as u32),
        (Some(s), None) => Some(s),
        (None, _) => None,
    }
}

/// "HH:MM:SS" → seconds.
fn parse_time(text: &str) -> Option<u32> {
    let mut parts = text.trim().split(':');
    let (h, m, s) = (parts.next()?, parts.next()?, parts.next()?);
    Some(h.parse::<u32>().ok()? * 3600 + m.parse::<u32>().ok()? * 60 + s.parse::<u32>().ok()?)
}

/// The trip instance an rt trip descriptor names: the network's trip
/// by id, at its start — a frequency trip's at the descriptor's
/// start_time. `None` when the trip is unknown or the start is not
/// one of its instances.
pub fn resolve_instance(network: &Network, trip_id: &str, start_time: Option<&str>) -> Option<MaterializedTrip> {
    let (_, _, trip) = network.trip(trip_id)?;
    let start = match trip.departs {
        Departs::At { start_secs } => start_secs,
        Departs::Every { .. } => parse_time(start_time?)?,
    };
    trip_instance(network, trip_id, start)
}

/// The publisher's stop_sequence numbers for a trip of `n` stops.
fn sequences(boundary: &Boundary, trip: &str, n: usize) -> Vec<u32> {
    boundary
        .stop_sequences
        .get(trip)
        .filter(|s| s.len() == n)
        .cloned()
        .unwrap_or_else(|| (1..=n as u32).collect())
}

/// One update joined with its instance's schedule.
pub(crate) fn progress_of(network: &Network, boundary: &Boundary, update: &TripUpdateView, instance: &MaterializedTrip) -> TripProgress {
    let seqs = sequences(boundary, &instance.trip, instance.stop_times.len());
    let mut stops = Vec::with_capacity(instance.stop_times.len());
    let mut worst = 0i32;
    // The last delay the feed stated, carried downstream: GTFS-rt
    // propagates a stop's delay to the following stops that have no
    // update of their own. Stops before the first update stay
    // unknown (null), never zero.
    let mut carried: Option<i32> = None;
    for (k, (row, &seq)) in instance.stop_times.iter().zip(&seqs).enumerate() {
        // Pair by the publisher's sequence (the spec's own key); an
        // update naming none pairs with the stop's first visit.
        let first_visit = instance.stop_times.iter().position(|r| r.stop == row.stop) == Some(k);
        let upd = update.stop_updates.iter().find(|s| match s.stop_sequence {
            Some(q) => q == seq,
            None => first_visit && s.stop_id.as_deref() == Some(row.stop.as_str()),
        });
        let (arrival_delay, departure_delay) = match upd {
            Some(s) => (s.arrival_delay, s.departure_delay),
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

/// The block a trip runs in.
fn block_of(network: &Network, trip: &str) -> Option<String> {
    network.blocks.iter().find(|(_, b)| b.trips.iter().any(|r| r.trip == trip)).map(|(id, _)| id.clone())
}

/// One trip's progress under the snapshot's (first) update for it:
/// every scheduled stop of the instance the update names, with the
/// live delay and the prediction. A trip the snapshot does not
/// mention answers its schedule alone (delay-free) when it has one
/// start. `None` when the trip is unknown, the update names no
/// instance of it, or — a frequency trip with no update — no instance
/// is named at all.
pub fn trip_progress(network: &Network, boundary: &Boundary, overview: &RtOverview, trip_id: &str) -> Option<TripProgress> {
    let quiet = TripUpdateView { trip_id: Some(trip_id.to_string()), route_id: None, start_time: None, stop_updates: vec![] };
    let update = overview.trips.iter().find(|t| t.trip_id.as_deref() == Some(trip_id)).unwrap_or(&quiet);
    let instance = resolve_instance(network, trip_id, update.start_time.as_deref())?;
    Some(progress_of(network, boundary, update, &instance))
}

/// The ranked "what's late right now" view: every trip whose worst
/// stop delay reaches `min_delay_secs`, worst first, each carrying
/// the vehicle's position when one names the trip (or, failing that,
/// another trip of its block — the bus finishing the trip before) —
/// and the DELAYED-BUT-DARK flag when none does.
///
/// `positions` is the snapshot vehicles come from: agencies serve
/// TripUpdates and VehiclePositions as SEPARATE feeds, so the
/// delays and the positions are usually two snapshots — `None` means
/// the delays snapshot carries its own positions (a combined feed).
pub fn late_trips(
    network: &Network,
    delays: &RtOverview,
    positions: Option<&RtOverview>,
    min_delay_secs: i32,
) -> Vec<LateTrip> {
    let vehicles = positions.unwrap_or(delays).vehicle_positions.as_slice();
    // The trip's own vehicle first; failing that, the vehicle running
    // another trip of the same block (the bus is out, finishing the
    // trip before this one).
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

    let mut late: Vec<LateTrip> = Vec::new();
    for update in &delays.trips {
        let Some(trip_id) = update.trip_id.as_deref() else { continue };
        let worst = update
            .stop_updates
            .iter()
            .map(|s| (s.arrival_delay).max(s.departure_delay).unwrap_or(0))
            .max()
            .unwrap_or(0);
        if worst < min_delay_secs {
            continue;
        }
        let block_id = block_of(network, trip_id);
        let vehicle = position_of(trip_id, block_id.as_deref());
        late.push(LateTrip {
            trip_id: update.trip_id.clone(),
            route_id: update.route_id.clone().or_else(|| route_of(trip_id)),
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
