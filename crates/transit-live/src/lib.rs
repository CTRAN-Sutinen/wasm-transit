//! transit-live — THE live document.
//!
//! GTFS-rt speaks the agency's vocabulary (agency trip ids, rt
//! stop sequences); the model speaks its own (a derived trip is a
//! `(schedule, start)` pair). The live document is the rt snapshot
//! RESOLVED into model vocabulary once, at the boundary (cap-rt
//! mints it from the snapshot, the static feed, and the import's
//! trip map), so every model-side consumer — the board, the runcut,
//! the map — reads delays by derived trip and never re-joins rt by
//! hand. No host computes a delay: parity between the browser, the
//! runner, and the Go server holds by construction.
//!
//! The document carries the ON-TIME WINDOW it was classified under,
//! so every consumer classifies the same way (one window, never a
//! threshold per view). Pure data plus the classification rule;
//! nothing here reads GTFS.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// The agency's on-time policy: how early and how late still count
/// as on time, and where late becomes very late. A policy INPUT, never
/// a constant in a view.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct OnTimeWindow {
    /// Seconds early that still count as on time; `None` means any
    /// earliness is on time (no Early status).
    #[serde(default)]
    #[ts(optional)]
    pub early_secs: Option<u32>,
    /// Up to this many seconds late is on time.
    pub late_secs: u32,
    /// Past this many seconds late is very late.
    pub very_late_secs: u32,
}

impl Default for OnTimeWindow {
    /// One minute late is on time, five minutes is very late.
    fn default() -> Self {
        OnTimeWindow { early_secs: None, late_secs: 60, very_late_secs: 300 }
    }
}

/// A delay, classified under a window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum LiveStatus {
    Early,
    OnTime,
    Late,
    VeryLate,
    /// No delay is known — schedule only.
    NoPrediction,
}

/// The classification rule — the one place a delay becomes a status.
pub fn classify(delay: Option<i32>, window: &OnTimeWindow) -> LiveStatus {
    let Some(d) = delay else { return LiveStatus::NoPrediction };
    if let Some(early) = window.early_secs {
        if d < -(early as i64) as i32 {
            return LiveStatus::Early;
        }
    }
    if d <= window.late_secs as i32 {
        LiveStatus::OnTime
    } else if d <= window.very_late_secs as i32 {
        LiveStatus::Late
    } else {
        LiveStatus::VeryLate
    }
}

/// One stop of a live trip: the delay the rt feed states (or carries
/// downstream — GTFS-rt's propagation rule) and the predicted
/// departure. Stops before the first stated delay are absent.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct LiveStop {
    pub stop: String,
    /// The base schedule's departure here (seconds after the service-
    /// day anchor) — what the delay is relative to.
    pub scheduled_departure: Option<u32>,
    pub arrival_delay: Option<i32>,
    pub departure_delay: Option<i32>,
    /// When the vehicle should actually leave: schedule + delay.
    pub predicted_departure: Option<u32>,
}

/// One trip instance the snapshot speaks about: the network's trip
/// (the rt trip id is the model's) and the instance's start.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct LiveTrip {
    pub trip: String,
    pub start_secs: u32,
    pub route: Option<String>,
    /// The route's name as the network names it.
    pub route_name: Option<String>,
    /// The worst delay the snapshot states anywhere on the trip —
    /// the trip's headline lateness.
    pub worst_delay_secs: Option<i32>,
    pub status: LiveStatus,
    pub stops: Vec<LiveStop>,
}

/// One live vehicle, on its derived trip when the trip resolves.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct LiveVehicle {
    /// Stable per snapshot: the vehicle id, else the rt entity id.
    pub key: String,
    pub vehicle_id: Option<String>,
    /// The user-visible bus number.
    pub label: Option<String>,
    pub lat: f64,
    pub lon: f64,
    pub bearing: Option<f32>,
    pub route: Option<String>,
    /// The route's name as the network names it.
    pub route_name: Option<String>,
    /// The stop the vehicle is at or heading to.
    pub next_stop: Option<String>,
    /// The trip instance it runs — absent when the rt trip does not
    /// resolve (or the vehicle names no trip).
    pub trip: Option<String>,
    pub start_secs: Option<u32>,
    /// Its trip's worst stated delay (the same headline as the trip).
    pub delay_secs: Option<i32>,
    pub status: LiveStatus,
}

/// THE live document: one rt moment, in model vocabulary.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct LiveDay {
    /// The window every status in this document was classified under.
    pub window: OnTimeWindow,
    pub trips: Vec<LiveTrip>,
    pub vehicles: Vec<LiveVehicle>,
    /// Trip updates naming no trip instance of the network (an
    /// unknown trip, or a start that is not one of its instances) —
    /// reported, never silently dropped.
    pub unmatched_trips: u32,
}

impl LiveDay {
    /// The live answer for one visit of a trip instance: the stop
    /// entry whose base departure matches `scheduled_departure`
    /// (a loop visits a stop twice), else the trip's first entry at
    /// that stop.
    pub fn visit(&self, trip: &str, start_secs: u32, stop: &str, scheduled_departure: u32) -> Option<&LiveStop> {
        let trip = self.trips.iter().find(|t| t.trip == trip && t.start_secs == start_secs)?;
        let mut at_stop = trip.stops.iter().filter(|s| s.stop == stop);
        let first = at_stop.clone().next();
        at_stop.find(|s| s.scheduled_departure == Some(scheduled_departure)).or(first)
    }
}

/// Vehicles per status.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct StatusCounts {
    pub early: u32,
    pub on_time: u32,
    pub late: u32,
    pub very_late: u32,
    pub no_prediction: u32,
}

/// One row of the "what's late" list.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct LateEntry {
    pub key: String,
    pub route: Option<String>,
    pub route_name: Option<String>,
    /// The bus number, when a vehicle is out on it.
    pub label: Option<String>,
    pub trip: Option<String>,
    pub start_secs: Option<u32>,
    pub delay_secs: i32,
    pub status: LiveStatus,
}

/// The live overview a status panel reads: the fleet by status and
/// the late list, worst first.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct LiveSummary {
    pub vehicles: u32,
    pub counts: StatusCounts,
    pub late: Vec<LateEntry>,
    /// No vehicle positions in the snapshot: the late list is late
    /// TRIPS (predictions), not buses on the street.
    pub late_are_predictions: bool,
}

/// Summarize a live document: vehicles counted by status; the late
/// list is late BUSES when the snapshot has positions, late trips
/// (predictions) when it has none. At most `limit` late rows.
pub fn summarize(day: &LiveDay, limit: usize) -> LiveSummary {
    let late_status = |s: LiveStatus| matches!(s, LiveStatus::Late | LiveStatus::VeryLate);
    let mut counts = StatusCounts::default();
    for v in &day.vehicles {
        *match v.status {
            LiveStatus::Early => &mut counts.early,
            LiveStatus::OnTime => &mut counts.on_time,
            LiveStatus::Late => &mut counts.late,
            LiveStatus::VeryLate => &mut counts.very_late,
            LiveStatus::NoPrediction => &mut counts.no_prediction,
        } += 1;
    }
    let late_are_predictions = day.vehicles.is_empty();
    let mut late: Vec<LateEntry> = if late_are_predictions {
        day.trips
            .iter()
            .filter(|t| late_status(t.status))
            .map(|t| LateEntry {
                key: format!("{}@{}", t.trip, t.start_secs),
                route: t.route.clone(),
                route_name: t.route_name.clone(),
                label: None,
                trip: Some(t.trip.clone()),
                start_secs: Some(t.start_secs),
                delay_secs: t.worst_delay_secs.unwrap_or(0),
                status: t.status,
            })
            .collect()
    } else {
        day.vehicles
            .iter()
            .filter(|v| late_status(v.status))
            .map(|v| LateEntry {
                key: v.key.clone(),
                route: v.route.clone(),
                route_name: v.route_name.clone(),
                label: v.label.clone().or_else(|| v.vehicle_id.clone()),
                trip: v.trip.clone(),
                start_secs: v.start_secs,
                delay_secs: v.delay_secs.unwrap_or(0),
                status: v.status,
            })
            .collect()
    };
    late.sort_by(|a, b| b.delay_secs.cmp(&a.delay_secs).then_with(|| a.key.cmp(&b.key)));
    late.truncate(limit);
    LiveSummary { vehicles: day.vehicles.len() as u32, counts, late, late_are_predictions }
}
