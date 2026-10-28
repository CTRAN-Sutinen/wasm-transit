//! Host-friendly view of a decoded GTFS-rt feed: header, entity
//! counts by type, trip updates (delays), live vehicles, and service
//! alerts — the
//! serialization target of cap-rt's `decode` op, shared with every
//! host that wants meaning out of the protobuf bytes.

use ts_rs::TS;
use serde::Serialize;

/// Flat view of one trip update in a feed.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct TripUpdateView {
    pub trip_id: Option<String>,
    pub route_id: Option<String>,
    /// The instance's start ("HH:MM:SS") — how a frequency trip's
    /// update names which run it is.
    pub start_time: Option<String>,
    pub stop_updates: Vec<StopUpdateView>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct StopUpdateView {
    pub stop_id: Option<String>,
    pub stop_sequence: Option<u32>,
    /// Seconds of delay; positive = late.
    pub arrival_delay: Option<i32>,
    pub departure_delay: Option<i32>,
}

/// One live vehicle from a VehiclePositions feed.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct VehicleView {
    pub entity_id: String,
    pub trip_id: Option<String>,
    /// The instance's start ("HH:MM:SS"), as on a trip update.
    pub start_time: Option<String>,
    pub route_id: Option<String>,
    pub lat: f64,
    pub lon: f64,
    pub bearing: Option<f32>,
    pub stop_id: Option<String>,
    /// VehicleDescriptor: the internal vehicle id and the user-
    /// visible label — the bus number a rider reads off the vehicle.
    pub vehicle_id: Option<String>,
    pub label: Option<String>,
}

/// One informed entity of an alert, in the feed's vocabulary (the
/// live document resolves it into the network's).
#[derive(Debug, Clone, Default, Serialize, TS)]
#[ts(export)]
pub struct InformedView {
    pub agency_id: Option<String>,
    pub route_id: Option<String>,
    pub route_type: Option<i32>,
    pub trip_id: Option<String>,
    /// The trip instance's start ("HH:MM:SS"), for a frequency trip.
    pub trip_start_time: Option<String>,
    pub stop_id: Option<String>,
    pub direction_id: Option<u32>,
}

/// One service alert from an Alerts feed. Text is the untagged
/// translation, else the first.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct AlertView {
    pub entity_id: String,
    pub cause: transit_live::AlertCause,
    pub effect: transit_live::AlertEffect,
    pub severity: transit_live::AlertSeverity,
    pub header: Option<String>,
    pub description: Option<String>,
    pub url: Option<String>,
    /// When to show it (the communication period, else the deprecated
    /// active period); empty = always.
    pub communication: Vec<transit_live::AlertPeriod>,
    /// When service is affected; empty = while shown.
    pub impact: Vec<transit_live::AlertPeriod>,
    pub informed: Vec<InformedView>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct RtOverview {
    pub gtfs_realtime_version: Option<String>,
    /// Unix seconds, as served in the feed header.
    pub timestamp: Option<u64>,
    pub entities: usize,
    pub trip_updates: usize,
    pub vehicles: usize,
    pub alerts: usize,
    /// Individual trip updates, capped for UI sanity.
    pub trips: Vec<TripUpdateView>,
    /// Live vehicles with positions, for the map.
    pub vehicle_positions: Vec<VehicleView>,
    /// The feed's service alerts.
    pub service_alerts: Vec<AlertView>,
}

/// Rendered trip updates, capped for UI sanity. Kept generous: the UI
/// joins these onto departure boards and vehicle dots by trip_id, so
/// a low cap silently drops delay information.
const MAX_TRIPS_SHOWN: usize = 1000;

/// Decode a GTFS-rt protobuf payload into the overview view.
pub fn decode_overview(bytes: &[u8]) -> Result<RtOverview, String> {
    let msg = crate::decode(bytes).map_err(|e| e.to_string())?;

    // gtfs-realtime.proto is proto2: `required` fields (header, and the
    // TripDescriptor inside a TripUpdate) are plain values in prost.
    let mut overview = RtOverview {
        gtfs_realtime_version: Some(msg.header.gtfs_realtime_version.clone()),
        timestamp: msg.header.timestamp,
        entities: msg.entity.len(),
        trip_updates: 0,
        vehicles: 0,
        alerts: 0,
        trips: Vec::new(),
        vehicle_positions: Vec::new(),
        service_alerts: Vec::new(),
    };

    for entity in &msg.entity {
        if let Some(tu) = entity.trip_update.as_ref() {
            overview.trip_updates += 1;
            if overview.trips.len() < MAX_TRIPS_SHOWN {
                overview.trips.push(TripUpdateView {
                    trip_id: tu.trip.trip_id.clone(),
                    route_id: tu.trip.route_id.clone(),
                    start_time: tu.trip.start_time.clone(),
                    stop_updates: tu
                        .stop_time_update
                        .iter()
                        .map(|s| StopUpdateView {
                            stop_id: s.stop_id.clone(),
                            stop_sequence: s.stop_sequence,
                            arrival_delay: s.arrival.as_ref().and_then(|e| e.delay),
                            departure_delay: s.departure.as_ref().and_then(|e| e.delay),
                        })
                        .collect(),
                });
            }
        }
        if let Some(vehicle) = entity.vehicle.as_ref() {
            overview.vehicles += 1;
            if let Some(position) = vehicle.position.as_ref() {
                overview.vehicle_positions.push(VehicleView {
                    entity_id: entity.id.clone(),
                    trip_id: vehicle.trip.as_ref().and_then(|t| t.trip_id.clone()),
                    start_time: vehicle.trip.as_ref().and_then(|t| t.start_time.clone()),
                    route_id: vehicle.trip.as_ref().and_then(|t| t.route_id.clone()),
                    lat: position.latitude as f64,
                    lon: position.longitude as f64,
                    bearing: position.bearing,
                    stop_id: vehicle.stop_id.clone(),
                    vehicle_id: vehicle.vehicle.as_ref().and_then(|d| d.id.clone()),
                    label: vehicle.vehicle.as_ref().and_then(|d| d.label.clone()),
                });
            }
        }
        if let Some(alert) = entity.alert.as_ref() {
            overview.alerts += 1;
            overview.service_alerts.push(alert_view(&entity.id, alert));
        }
    }
    Ok(overview)
}

fn alert_view(entity_id: &str, alert: &crate::gtfs_rt::Alert) -> AlertView {
    use crate::gtfs_rt::alert::{Cause, Effect, SeverityLevel};
    use transit_live::{AlertCause as C, AlertEffect as E, AlertPeriod, AlertSeverity as S};
    let periods = |ranges: &[crate::gtfs_rt::TimeRange]| {
        ranges.iter().map(|r| AlertPeriod { start: r.start, end: r.end }).collect::<Vec<_>>()
    };
    let cause = match alert.cause() {
        Cause::UnknownCause => C::Unknown,
        Cause::OtherCause => C::Other,
        Cause::TechnicalProblem => C::TechnicalProblem,
        Cause::Strike => C::Strike,
        Cause::Demonstration => C::Demonstration,
        Cause::Accident => C::Accident,
        Cause::Holiday => C::Holiday,
        Cause::Weather => C::Weather,
        Cause::Maintenance => C::Maintenance,
        Cause::Construction => C::Construction,
        Cause::PoliceActivity => C::PoliceActivity,
        Cause::MedicalEmergency => C::MedicalEmergency,
        Cause::SpecialEvent => C::SpecialEvent,
    };
    let effect = match alert.effect() {
        Effect::NoService => E::NoService,
        Effect::ReducedService => E::ReducedService,
        Effect::SignificantDelays => E::SignificantDelays,
        Effect::Detour => E::Detour,
        Effect::AdditionalService => E::AdditionalService,
        Effect::ModifiedService => E::ModifiedService,
        Effect::OtherEffect => E::Other,
        Effect::UnknownEffect => E::Unknown,
        Effect::StopMoved => E::StopMoved,
        Effect::NoEffect => E::NoEffect,
        Effect::AccessibilityIssue => E::AccessibilityIssue,
    };
    let severity = match alert.severity_level() {
        SeverityLevel::UnknownSeverity => S::Unknown,
        SeverityLevel::Info => S::Info,
        SeverityLevel::Warning => S::Warning,
        SeverityLevel::Severe => S::Severe,
    };
    #[allow(deprecated)]
    let communication = if alert.communication_period.is_empty() {
        periods(&alert.active_period)
    } else {
        periods(&alert.communication_period)
    };
    AlertView {
        entity_id: entity_id.to_string(),
        cause,
        effect,
        severity,
        header: translated(alert.header_text.as_ref()),
        description: translated(alert.description_text.as_ref()),
        url: translated(alert.url.as_ref()),
        communication,
        impact: periods(&alert.impact_period),
        informed: alert
            .informed_entity
            .iter()
            .map(|e| InformedView {
                agency_id: e.agency_id.clone(),
                route_id: e.route_id.clone(),
                route_type: e.route_type,
                trip_id: e.trip.as_ref().and_then(|t| t.trip_id.clone()),
                trip_start_time: e.trip.as_ref().and_then(|t| t.start_time.clone()),
                stop_id: e.stop_id.clone(),
                direction_id: e.direction_id,
            })
            .collect(),
    }
}

/// The untagged translation, else the first.
fn translated(text: Option<&crate::gtfs_rt::TranslatedString>) -> Option<String> {
    let translations = &text?.translation;
    translations
        .iter()
        .find(|t| t.language.as_deref().is_none_or(str::is_empty))
        .or(translations.first())
        .map(|t| t.text.clone())
}
