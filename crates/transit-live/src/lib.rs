//! transit-live — THE live document.
//!
//! GTFS-rt speaks the published feed's vocabulary (its trip ids,
//! its stop_sequence numbers, a frequency run by start_time); the
//! model speaks its own (a trip instance is a `(trip, start)` pair —
//! the trip ids are the same). The live document is the rt snapshot
//! RESOLVED into model vocabulary once, at the boundary (cap-rt
//! mints it from the snapshot and the network, with the import's
//! boundary for the stop numbering), so every model-side consumer —
//! the board, the runcut, the map — reads delays by trip instance and
//! never re-joins rt by hand. No host computes a delay: parity between the browser, the
//! runner, and the Go server holds by construction.
//!
//! The document carries the ON-TIME WINDOW it was classified under,
//! so every consumer classifies the same way (one window, never a
//! threshold per view). It also carries the SERVICE ALERTS in force at
//! that moment, their informed entities resolved to routes, stops, and
//! trip instances of the network, so a reader asks "does this alert
//! touch this visit?" in model vocabulary. Pure data plus the
//! classification and alert-matching rules; nothing here reads GTFS.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// The agency's on-time policy: how early and how late still count
/// as on time, and where late becomes very late. A policy INPUT, never
/// a constant in a view.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct OnTimeWindow {
    /// Seconds early that still count as on time. Early is not on
    /// time: the default (and an absent bound) is 0 — a bus that
    /// leaves before its time is a missed bus.
    #[serde(default)]
    #[ts(optional, as = "Option<u32>")]
    pub early_secs: u32,
    /// Up to this many seconds late is on time.
    pub late_secs: u32,
    /// Past this many seconds late is very late.
    pub very_late_secs: u32,
}

impl Default for OnTimeWindow {
    /// Any earliness is early, one minute late is on time, five
    /// minutes is very late.
    fn default() -> Self {
        OnTimeWindow { early_secs: 0, late_secs: 60, very_late_secs: 300 }
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
    if (d as i64) < -(window.early_secs as i64) {
        return LiveStatus::Early;
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

/// One live vehicle, on its trip instance when the trip resolves.
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

/// Why an alert was raised (GTFS-rt's cause, by name).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum AlertCause {
    #[default]
    Unknown,
    Other,
    TechnicalProblem,
    Strike,
    Demonstration,
    Accident,
    Holiday,
    Weather,
    Maintenance,
    Construction,
    PoliceActivity,
    MedicalEmergency,
    SpecialEvent,
}

/// What an alert does to service (GTFS-rt's effect, by name).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum AlertEffect {
    NoService,
    ReducedService,
    SignificantDelays,
    Detour,
    AdditionalService,
    ModifiedService,
    Other,
    #[default]
    Unknown,
    StopMoved,
    NoEffect,
    AccessibilityIssue,
}

/// How serious the agency says an alert is.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum AlertSeverity {
    #[default]
    Unknown,
    Info,
    Warning,
    Severe,
}

/// A span of time in Unix seconds; an open end is unbounded.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct AlertPeriod {
    #[ts(type = "number | null")]
    pub start: Option<u64>,
    #[ts(type = "number | null")]
    pub end: Option<u64>,
}

/// One informed entity, resolved: every present field must match
/// (route 12 AT stop 6277); all absent means the whole agency. A trip
/// without a start names every instance of the trip.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct AlertScope {
    pub route: Option<String>,
    pub stop: Option<String>,
    pub trip: Option<String>,
    pub start_secs: Option<u32>,
}

/// One stop visit of a trip instance, as an alert sees it.
#[derive(Debug, Clone, Copy)]
pub struct Visit<'a> {
    pub route: &'a str,
    pub stop: &'a str,
    pub trip: &'a str,
    pub start_secs: u32,
}

impl AlertScope {
    fn admits(&self, v: &Visit) -> bool {
        self.route.as_deref().is_none_or(|r| r == v.route)
            && self.stop.as_deref().is_none_or(|s| s == v.stop)
            && self.trip.as_deref().is_none_or(|t| t == v.trip)
            && self.start_secs.is_none_or(|s| s == v.start_secs)
    }
}

/// A service alert in force at the document's moment, in model
/// vocabulary. The text is the feed's untagged translation, else its
/// first.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct LiveAlert {
    /// The rt entity id.
    pub id: String,
    pub cause: AlertCause,
    pub effect: AlertEffect,
    pub severity: AlertSeverity,
    pub header: String,
    pub description: Option<String>,
    pub url: Option<String>,
    /// When service is affected (the impact period, else the active
    /// period); empty means for as long as the alert is shown.
    pub impact: Vec<AlertPeriod>,
    /// What it informs about — ORed; never empty (an alert whose
    /// entities all fail to resolve is counted, not kept).
    pub scopes: Vec<AlertScope>,
}

impl LiveAlert {
    /// Does the alert speak about this visit?
    pub fn touches(&self, visit: &Visit) -> bool {
        self.scopes.iter().any(|s| s.admits(visit))
    }

    /// Does the alert speak about the stop as a whole (whatever leaves
    /// from it) — a stop scope, or the whole agency?
    pub fn touches_stop(&self, stop: &str) -> bool {
        self.scopes
            .iter()
            .any(|s| s.route.is_none() && s.trip.is_none() && s.stop.as_deref().is_none_or(|x| x == stop))
    }
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
    /// The service alerts in force at the snapshot's moment.
    #[serde(default)]
    #[ts(optional, as = "Option<Vec<LiveAlert>>")]
    pub alerts: Vec<LiveAlert>,
    /// Alerts none of whose informed entities name anything in the
    /// network — reported, never silently dropped.
    #[serde(default)]
    #[ts(optional, as = "Option<u32>")]
    pub unmatched_alerts: u32,
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
