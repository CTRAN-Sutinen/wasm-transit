//! The join the dispatcher actually asks for: one trip's remaining
//! stops with the SCHEDULE, the live DELAY, and the PREDICTED time
//! in one answer — plus the ranked "what's late right now" view over
//! a decoded feed.
//!
//! This is the layer where realtime meets the static feed: rt gives
//! delays, the schedule gives times, and no host should have to join
//! them by hand (the founding dispatcher sessions did — twice). The
//! join is over the SEQUENCE: GTFS-rt's StopTimeUpdate carries the
//! stop_sequence the schedule row carries, so the rows pair without
//! guessing; a stop_id fallback covers feeds that omit sequences.

use serde::Serialize;
use ts_rs::TS;
use transit_core::Gtfs;

use crate::overview::{RtOverview, VehicleView};

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
    /// Seconds of delay; positive = late, negative = early.
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
    /// The vehicle's block, from the static feed — the chain this
    /// delay lands in.
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

/// Join one trip's rt stop updates with its schedule rows. `None`
/// when the rt feed names a trip the static feed does not carry (or
/// carries with no stop times — schedule-only trips answer from the
/// schedule alone, delay-free, so callers can still show the plan).
pub fn trip_progress(gtfs: &Gtfs, overview: &RtOverview, trip_id: &str) -> Option<TripProgress> {
    let rows = gtfs.trip_stop_times(trip_id)?;
    let trip = gtfs.trips.get(trip_id)?;
    let update = overview.trips.iter().find(|t| t.trip_id.as_deref() == Some(trip_id));

    let mut stops = Vec::with_capacity(rows.len());
    let mut worst = 0i32;
    for row in &rows {
        // Pair by sequence first (the spec's own key); fall back to
        // stop id for feeds that omit stop_sequence in rt.
        let upd = update.and_then(|u| {
            u.stop_updates.iter().find(|s| {
                s.stop_sequence == Some(row.stop_sequence) || s.stop_id.as_deref() == Some(row.stop_id.as_str())
            })
        });
        let arrival_delay = upd.and_then(|s| s.arrival_delay);
        let departure_delay = upd.and_then(|s| s.departure_delay);
        let stop_worst = arrival_delay.max(departure_delay).unwrap_or(0);
        worst = worst.max(stop_worst);
        stops.push(ProgressStop {
            stop_id: row.stop_id.clone(),
            stop_sequence: row.stop_sequence,
            scheduled_arrival: row.arrival_time,
            scheduled_departure: row.departure_time,
            arrival_delay,
            departure_delay,
            predicted_arrival: predicted(row.arrival_time, arrival_delay),
            predicted_departure: predicted(row.departure_time, departure_delay),
        });
    }
    Some(TripProgress {
        trip_id: trip_id.to_string(),
        route_id: Some(trip.route_id.clone()),
        block_id: trip.block_id.clone(),
        worst_delay_secs: worst,
        stops,
    })
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
    gtfs: &Gtfs,
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
            vehicles.iter().find(|v| {
                v.trip_id
                    .as_deref()
                    .and_then(|t| gtfs.trips.get(t))
                    .and_then(|t| t.block_id.as_deref())
                    == Some(block_id)
            })
        })
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
        let trip = gtfs.trips.get(trip_id);
        let block_id = trip.and_then(|t| t.block_id.clone());
        let vehicle = position_of(trip_id, block_id.as_deref());
        late.push(LateTrip {
            trip_id: update.trip_id.clone(),
            route_id: update.route_id.clone().or_else(|| trip.map(|t| t.route_id.clone())),
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
    late.sort_by(|a, b| b.worst_delay_secs.cmp(&a.worst_delay_secs).then_with(|| {
        a.trip_id.cmp(&b.trip_id)
    }));
    late
}
