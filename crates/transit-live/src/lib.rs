//! Every status is classified under the one on-time window that the document
//! records. What does not run (a cancelled or deleted trip instance, a skipped
//! stop) is asked through [`LiveDay::runs`] alone. A board shows no row for
//! it and the router never boards there, because a live cancel shown at its
//! time misled riders (D-100..D-102). Cover, hold, and swap are
//! CAD/AVL's day, not edits. A stop id matches as written, so a
//! station's id selects no departure at its bays (D-221).

pub mod load;

use serde::{Deserialize, Serialize};
use transit_alerts::{AlertCause, AlertEffect, AlertPeriod, AlertSeverity};
#[cfg(feature = "bindings")]
use ts_rs::TS;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS, schemars::JsonSchema))]
#[cfg_attr(feature = "bindings", ts(export))]
pub struct OnTimeWindow {
    /// The default is 0, because a bus that leaves before its time is a
    /// missed bus.
    #[serde(default)]
    #[cfg_attr(feature = "bindings", ts(optional, as = "Option<u32>"))]
    pub early_secs: u32,
    pub late_secs: u32,
    pub very_late_secs: u32,
}

impl Default for OnTimeWindow {
    fn default() -> Self {
        OnTimeWindow { early_secs: 0, late_secs: 60, very_late_secs: 300 }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS, schemars::JsonSchema))]
#[cfg_attr(feature = "bindings", ts(export))]
#[serde(rename_all = "snake_case")]
pub enum LiveStatus {
    Early,
    OnTime,
    Late,
    VeryLate,
    NoPrediction,
}

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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS, schemars::JsonSchema))]
#[cfg_attr(feature = "bindings", ts(export))]
pub struct LiveStop {
    pub stop: String,
    /// Seconds after the service-day anchor. The delay is relative to it.
    pub scheduled_departure: Option<u32>,
    pub arrival_delay: Option<i32>,
    pub departure_delay: Option<i32>,
    pub predicted_departure: Option<u32>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    #[cfg_attr(feature = "bindings", ts(optional, as = "Option<bool>"))]
    pub skipped: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS, schemars::JsonSchema))]
#[cfg_attr(feature = "bindings", ts(export))]
pub struct LiveTrip {
    pub trip: String,
    /// "YYYY-MM-DD". Absent is the current service day ([`LiveDay::on`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub date: Option<String>,
    pub start_secs: u32,
    pub route: Option<String>,
    pub route_name: Option<String>,
    pub worst_delay_secs: Option<i32>,
    pub status: LiveStatus,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    #[cfg_attr(feature = "bindings", ts(optional, as = "Option<bool>"))]
    pub canceled: bool,
    pub stops: Vec<LiveStop>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS, schemars::JsonSchema))]
#[cfg_attr(feature = "bindings", ts(export))]
pub struct LiveVehicle {
    /// The vehicle id, else the rt entity id, so it is stable per snapshot.
    pub key: String,
    pub vehicle_id: Option<String>,
    pub label: Option<String>,
    pub lat: f64,
    pub lon: f64,
    pub bearing: Option<f32>,
    pub route: Option<String>,
    pub route_name: Option<String>,
    pub next_stop: Option<String>,
    pub trip: Option<String>,
    /// "YYYY-MM-DD". Absent is the current service day ([`LiveDay::on`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub date: Option<String>,
    pub start_secs: Option<u32>,
    pub delay_secs: Option<i32>,
    pub status: LiveStatus,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS, schemars::JsonSchema))]
#[cfg_attr(feature = "bindings", ts(export))]
pub struct AlertScope {
    pub route: Option<String>,
    /// A model direction of `route`. Present only with `route`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub direction: Option<String>,
    pub stop: Option<String>,
    pub trip: Option<String>,
    /// "YYYY-MM-DD". Absent is the current service day ([`LiveDay::on`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub date: Option<String>,
    pub start_secs: Option<u32>,
}

#[derive(Debug, Clone, Copy)]
pub struct Visit<'a> {
    pub route: &'a str,
    pub direction: Option<&'a str>,
    pub stop: &'a str,
    pub trip: &'a str,
    pub start_secs: u32,
}

impl AlertScope {
    /// A scope with no trip speaks on every date. A trip scope with no date
    /// speaks on `current`, the current service day.
    pub fn on(&self, date: &str, current: &str) -> bool {
        self.trip.is_none() || self.date.as_deref().unwrap_or(current) == date
    }

    fn admits(&self, v: &Visit) -> bool {
        self.route.as_deref().is_none_or(|r| r == v.route)
            && self.direction.as_deref().is_none_or(|d| Some(d) == v.direction)
            && self.stop.as_deref().is_none_or(|s| s == v.stop)
            && self.trip.as_deref().is_none_or(|t| t == v.trip)
            && self.start_secs.is_none_or(|s| s == v.start_secs)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS, schemars::JsonSchema))]
#[cfg_attr(feature = "bindings", ts(export))]
pub struct LiveAlert {
    pub id: String,
    pub cause: AlertCause,
    pub effect: AlertEffect,
    pub severity: AlertSeverity,
    pub header: String,
    pub description: Option<String>,
    pub url: Option<String>,
    /// The impact period, else the active period. Empty means for as long as
    /// the alert is shown.
    pub impact: Vec<AlertPeriod>,
    /// An alert whose entities all fail to resolve is counted in
    /// `unmatched_alerts`, not kept, so this is never empty.
    pub scopes: Vec<AlertScope>,
}

impl LiveAlert {
    pub fn touches(&self, visit: &Visit) -> bool {
        self.scopes.iter().any(|s| s.admits(visit))
    }

    pub fn touches_stop(&self, stop: &str) -> bool {
        self.scopes
            .iter()
            .any(|s| s.route.is_none() && s.trip.is_none() && s.stop.as_deref().is_none_or(|x| x == stop))
    }
}

transit_version::kind_tag!(
    LiveKind = "live"
);

impl transit_version::Versioned for LiveDay {
    const KIND: &'static str = "live";
    const READS: u32 = 2;
    fn schema_version(&self) -> u32 {
        self.schema_version.0
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS, schemars::JsonSchema))]
#[cfg_attr(feature = "bindings", ts(export))]
pub struct LiveDay {
    #[serde(default)]
    #[cfg_attr(feature = "bindings", ts(type = "\"live\""))]
    pub kind: LiveKind,
    #[serde(default = "version_one")]
    #[cfg_attr(feature = "bindings", ts(type = "number"))]
    pub schema_version: transit_version::Version<2>,
    pub window: OnTimeWindow,
    pub trips: Vec<LiveTrip>,
    pub vehicles: Vec<LiveVehicle>,
    pub unmatched_trips: u32,
    #[serde(default)]
    #[cfg_attr(feature = "bindings", ts(optional, as = "Option<Vec<LiveAlert>>"))]
    pub alerts: Vec<LiveAlert>,
    #[serde(default)]
    #[cfg_attr(feature = "bindings", ts(optional, as = "Option<u32>"))]
    pub unmatched_alerts: u32,
}

fn version_one() -> transit_version::Version<2> {
    transit_version::Version(1)
}

impl LiveDay {
    /// An instance with no date is on `current`, the current service day. An
    /// alert keeps the scopes that speak on `date`, and leaves when none
    /// does.
    pub fn on(&self, date: &str, current: &str) -> LiveDay {
        let here = |d: &Option<String>| d.as_deref().unwrap_or(current) == date;
        let alerts = self
            .alerts
            .iter()
            .filter_map(|a| {
                let scopes: Vec<AlertScope> = a.scopes.iter().filter(|s| s.on(date, current)).cloned().collect();
                (!scopes.is_empty()).then(|| LiveAlert { scopes, ..a.clone() })
            })
            .collect();
        LiveDay {
            kind: self.kind,
            schema_version: self.schema_version,
            window: self.window,
            trips: self.trips.iter().filter(|t| here(&t.date)).cloned().collect(),
            vehicles: self.vehicles.iter().filter(|v| here(&v.date)).cloned().collect(),
            unmatched_trips: self.unmatched_trips,
            alerts,
            unmatched_alerts: self.unmatched_alerts,
        }
    }

    /// A loop visits a stop twice, so the entry is the one whose
    /// `scheduled_departure` matches, else the first at the stop.
    pub fn visit(&self, trip: &str, start_secs: u32, stop: &str, scheduled_departure: u32) -> Option<&LiveStop> {
        let trip = self.trips.iter().find(|t| t.trip == trip && t.start_secs == start_secs)?;
        let mut at_stop = trip.stops.iter().filter(|s| s.stop == stop);
        let first = at_stop.clone().next();
        at_stop.find(|s| s.scheduled_departure == Some(scheduled_departure)).or(first)
    }

    pub fn runs(&self, trip: &str, start_secs: u32, stop: &str, scheduled_departure: u32) -> bool {
        if self.trips.iter().any(|t| t.trip == trip && t.start_secs == start_secs && t.canceled) {
            return false;
        }
        self.visit(trip, start_secs, stop, scheduled_departure).is_none_or(|s| !s.skipped)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS, schemars::JsonSchema))]
#[cfg_attr(feature = "bindings", ts(export))]
pub struct StatusCounts {
    pub early: u32,
    pub on_time: u32,
    pub late: u32,
    pub very_late: u32,
    pub no_prediction: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS, schemars::JsonSchema))]
#[cfg_attr(feature = "bindings", ts(export))]
pub struct LateEntry {
    pub key: String,
    pub route: Option<String>,
    pub route_name: Option<String>,
    pub label: Option<String>,
    pub trip: Option<String>,
    pub start_secs: Option<u32>,
    pub delay_secs: i32,
    pub status: LiveStatus,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS, schemars::JsonSchema))]
#[cfg_attr(feature = "bindings", ts(export))]
pub struct LiveSummary {
    pub vehicles: u32,
    pub counts: StatusCounts,
    pub late: Vec<LateEntry>,
    /// True when the snapshot has no vehicle positions, so `late` lists
    /// trips, not buses.
    pub late_are_predictions: bool,
}

/// The late list is late buses when the snapshot has positions, and late
/// trips (predictions) when it has none.
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
