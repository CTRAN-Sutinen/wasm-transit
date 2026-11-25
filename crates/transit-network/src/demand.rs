use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::model::{AlignmentId, DirectionId, RouteId, ServiceTypeId, StopId, StopRule, TripId};

pub type LocationGroupId = String;
pub type ZoneId = String;
pub type BookingRuleId = String;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
pub struct DemandResponsive {
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    #[ts(optional, as = "Option<BTreeMap<LocationGroupId, LocationGroup>>")]
    pub location_groups: BTreeMap<LocationGroupId, LocationGroup>,
    /// A GeoJSON FeatureCollection whose feature ids are the zone ids.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "unknown")]
    pub zones: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    #[ts(optional, as = "Option<BTreeMap<BookingRuleId, BookingRule>>")]
    pub booking_rules: BTreeMap<BookingRuleId, BookingRule>,
    /// Trip ids are unique across these and the scheduled trips.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    #[ts(optional, as = "Option<BTreeMap<TripId, DemandTrip>>")]
    pub trips: BTreeMap<TripId, DemandTrip>,
    #[serde(flatten)]
    #[ts(skip)]
    pub extra: transit_version::Extra,
}

impl DemandResponsive {
    pub fn is_empty(&self) -> bool {
        *self == DemandResponsive::default()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
pub struct LocationGroup {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[ts(optional, as = "Option<Vec<StopId>>")]
    pub stops: Vec<StopId>,
    #[serde(flatten)]
    #[ts(skip)]
    pub extra: transit_version::Extra,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum BookingKind {
    #[default]
    RealTime,
    SameDay,
    PriorDays,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
pub struct BookingRule {
    pub kind: BookingKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub notice_min_minutes: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub notice_max_minutes: Option<u32>,
    /// For `PriorDays`. A day is a count of days before the ride.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub last_day: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub last_time_secs: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub start_day: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub start_time_secs: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub booking_days: Option<ServiceTypeId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub message: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub pickup_message: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub drop_off_message: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub phone: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub info_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub booking_url: Option<String>,
    #[serde(flatten)]
    #[ts(skip)]
    pub extra: transit_version::Extra,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
#[serde(tag = "kind", content = "id", rename_all = "snake_case")]
pub enum VisitPlace {
    Stop(StopId),
    LocationGroup(LocationGroupId),
    Zone(ZoneId),
}

impl Default for VisitPlace {
    fn default() -> Self {
        VisitPlace::Stop(String::new())
    }
}

/// Times and windows are seconds after the service-day anchor.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
pub struct Visit {
    pub place: VisitPlace,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub arrival_secs: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub departure_secs: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub window_start_secs: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub window_end_secs: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub headsign: Option<String>,
    #[serde(default, skip_serializing_if = "StopRule::is_regular")]
    #[ts(optional, as = "Option<StopRule>")]
    pub pickup: StopRule,
    #[serde(default, skip_serializing_if = "StopRule::is_regular")]
    #[ts(optional, as = "Option<StopRule>")]
    pub drop_off: StopRule,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub continuous_pickup: Option<StopRule>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub continuous_drop_off: Option<StopRule>,
    /// `Some(true)` is an exact time. `Some(false)` is an approximate one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub timepoint: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub distance: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub pickup_booking: Option<BookingRuleId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub drop_off_booking: Option<BookingRuleId>,
    #[serde(flatten)]
    #[ts(skip)]
    pub extra: transit_version::Extra,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
pub struct DemandTrip {
    pub route: RouteId,
    pub service_type: ServiceTypeId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub direction: Option<DirectionId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub alignment: Option<AlignmentId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub block: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub headsign: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub short_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub wheelchair: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub bikes: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub cars: Option<bool>,
    pub visits: Vec<Visit>,
    #[serde(flatten)]
    #[ts(skip)]
    pub extra: transit_version::Extra,
}
