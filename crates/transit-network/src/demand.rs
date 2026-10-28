//! Demand-responsive service: trips that serve groups of stops or
//! zones within windows rather than stops at times, and the booking
//! rules riders follow to ride them.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::model::{AlignmentId, DirectionId, RouteId, ServiceTypeId, StopId, StopRule, TripId};

pub type LocationGroupId = String;
pub type ZoneId = String;
pub type BookingRuleId = String;

/// Everything demand-responsive the network describes.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct DemandResponsive {
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    #[ts(optional, as = "Option<BTreeMap<LocationGroupId, LocationGroup>>")]
    pub location_groups: BTreeMap<LocationGroupId, LocationGroup>,
    /// The zones riders are picked up or dropped off in: a GeoJSON
    /// FeatureCollection whose features' ids are the zone ids.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "unknown")]
    pub zones: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    #[ts(optional, as = "Option<BTreeMap<BookingRuleId, BookingRule>>")]
    pub booking_rules: BTreeMap<BookingRuleId, BookingRule>,
    /// The demand-responsive trips, by id (trip ids are unique across
    /// these and the network's scheduled trips).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    #[ts(optional, as = "Option<BTreeMap<TripId, DemandTrip>>")]
    pub trips: BTreeMap<TripId, DemandTrip>,
}

impl DemandResponsive {
    pub fn is_empty(&self) -> bool {
        *self == DemandResponsive::default()
    }
}

/// Stops served as one: a rider may board or alight at any of them.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct LocationGroup {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[ts(optional, as = "Option<Vec<StopId>>")]
    pub stops: Vec<StopId>,
}

/// How far ahead a ride must be booked.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum BookingKind {
    /// Booked any time up to the ride.
    #[default]
    RealTime,
    /// Booked the same day, some minutes ahead.
    SameDay,
    /// Booked one or more days ahead.
    PriorDays,
}

/// How riders book a demand-responsive ride.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct BookingRule {
    pub kind: BookingKind,
    /// Same day: the minimum and maximum minutes of notice.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub notice_min_minutes: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub notice_max_minutes: Option<u32>,
    /// Prior days: the last day (days before the ride) and time a
    /// booking is taken, and the first.
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
    /// The service type whose days count as booking days.
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
}

/// Where a demand-responsive visit happens.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
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

/// One visit of a demand-responsive trip: a place, served at times or
/// within a window (seconds after the service day's anchor).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Visit {
    pub place: VisitPlace,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub arrival_secs: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub departure_secs: Option<u32>,
    /// The window riders are picked up or dropped off in.
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
    /// An exact time (`Some(true)`), an approximate one
    /// (`Some(false)`), or unstated.
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
}

/// A demand-responsive trip: a route's service under a service type,
/// visiting places in order.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
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
    /// The vehicle's block, as published.
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
}
