//! What riding costs. Two generations of fare description live side
//! by side, as agencies publish them: zone fares (a fare, the routes
//! and zones it applies to) and fare products (what riders buy, the
//! legs and transfers each covers, by area, network, time, rider, and
//! payment medium).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::model::{RouteId, ServiceTypeId, StopId};
use crate::operators::OperatorId;

pub type FareId = String;
pub type AreaId = String;
pub type FareNetworkId = String;
pub type TimeframeId = String;
pub type RiderCategoryId = String;
pub type FareMediaId = String;
pub type FareProductId = String;
pub type LegGroupId = String;

/// Every fare the network describes.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Fares {
    /// Zone fares, by id.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    #[ts(optional, as = "Option<BTreeMap<FareId, ZoneFare>>")]
    pub zone_fares: BTreeMap<FareId, ZoneFare>,
    /// Where each zone fare applies, in published order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[ts(optional, as = "Option<Vec<ZoneFareRule>>")]
    pub zone_fare_rules: Vec<ZoneFareRule>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    #[ts(optional, as = "Option<BTreeMap<AreaId, Area>>")]
    pub areas: BTreeMap<AreaId, Area>,
    /// Groups of routes fares treat alike (a route names its own).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    #[ts(optional, as = "Option<BTreeMap<FareNetworkId, FareNetwork>>")]
    pub networks: BTreeMap<FareNetworkId, FareNetwork>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    #[ts(optional, as = "Option<BTreeMap<TimeframeId, Vec<Timeframe>>>")]
    pub timeframes: BTreeMap<TimeframeId, Vec<Timeframe>>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    #[ts(optional, as = "Option<BTreeMap<RiderCategoryId, RiderCategory>>")]
    pub rider_categories: BTreeMap<RiderCategoryId, RiderCategory>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    #[ts(optional, as = "Option<BTreeMap<FareMediaId, FareMedia>>")]
    pub media: BTreeMap<FareMediaId, FareMedia>,
    /// What riders buy, by id — one offer per rider category and
    /// medium it is sold for.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    #[ts(optional, as = "Option<BTreeMap<FareProductId, Vec<FareOffer>>>")]
    pub products: BTreeMap<FareProductId, Vec<FareOffer>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[ts(optional, as = "Option<Vec<FareLegRule>>")]
    pub leg_rules: Vec<FareLegRule>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[ts(optional, as = "Option<Vec<FareLegJoinRule>>")]
    pub leg_join_rules: Vec<FareLegJoinRule>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[ts(optional, as = "Option<Vec<FareTransferRule>>")]
    pub transfer_rules: Vec<FareTransferRule>,
}

impl Fares {
    pub fn is_empty(&self) -> bool {
        *self == Fares::default()
    }
}

/// A zone fare: its price and how it may be used.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ZoneFare {
    pub price: f64,
    pub currency: String,
    /// Paid before boarding (`false`: on board).
    pub paid_before_boarding: bool,
    /// Transfers allowed; `None` = unlimited.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub transfers: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub operator: Option<OperatorId>,
    /// How long transfers (or the fare) stay valid.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub transfer_secs: Option<u32>,
}

/// Where a zone fare applies: on a route, from a zone to a zone, or
/// through a zone. Unset parts match anything.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ZoneFareRule {
    pub fare: FareId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub route: Option<RouteId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub origin_zone: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub destination_zone: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub contains_zone: Option<String>,
}

/// A fare area: a set of stops.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Area {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[ts(optional, as = "Option<Vec<StopId>>")]
    pub stops: Vec<StopId>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct FareNetwork {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub name: Option<String>,
}

/// A span of a service type's day fares treat alike (peak, off-peak).
/// No start and end means the whole day.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Timeframe {
    pub service_type: ServiceTypeId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub start_secs: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub end_secs: Option<u32>,
}

/// Who a fare is for (adult, senior, youth).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct RiderCategory {
    pub name: String,
    /// The category a rider is in unless they say otherwise.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    #[ts(optional, as = "Option<bool>")]
    pub default: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub eligibility_url: Option<String>,
}

/// What a fare is held on.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum FareMediaKind {
    /// Nothing: cash or a fare paid with no medium.
    #[default]
    None,
    PaperTicket,
    TransitCard,
    /// Contactless bank card or device.
    Contactless,
    MobileApp,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct FareMedia {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub name: Option<String>,
    pub kind: FareMediaKind,
}

/// One way a fare product is sold: its price for a rider category on
/// a medium (unset: any).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct FareOffer {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub rider_category: Option<RiderCategoryId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub media: Option<FareMediaId>,
    pub amount: f64,
    pub currency: String,
}

/// Which product a leg costs: by network, areas, and timeframes
/// (unset: any), the most specific (highest priority) rule winning.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct FareLegRule {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub leg_group: Option<LegGroupId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub network: Option<FareNetworkId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub from_area: Option<AreaId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub to_area: Option<AreaId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub from_timeframe: Option<TimeframeId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub to_timeframe: Option<TimeframeId>,
    pub product: FareProductId,
    /// Higher wins; `None` is 0.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub priority: Option<u32>,
}

/// Two consecutive legs that fares count as one.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct FareLegJoinRule {
    pub from_network: FareNetworkId,
    pub to_network: FareNetworkId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub from_stop: Option<StopId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub to_stop: Option<StopId>,
}

/// How the time limit of a fare transfer is measured.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum DurationLimit {
    DepartureToArrival,
    DepartureToDeparture,
    ArrivalToDeparture,
    ArrivalToArrival,
}

/// What a transfer between legs costs.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum FareTransferKind {
    /// The first leg's fare plus the transfer's.
    #[default]
    FirstLegPlusTransfer,
    /// Every leg's fare plus the transfer's.
    EveryLegPlusTransfer,
    /// The transfer's fare alone.
    TransferOnly,
}

/// What changing from one leg group to another costs.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct FareTransferRule {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub from_leg_group: Option<LegGroupId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub to_leg_group: Option<LegGroupId>,
    /// Transfers allowed; -1 = unlimited.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub transfer_count: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub duration_limit_secs: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub duration_limit: Option<DurationLimit>,
    pub kind: FareTransferKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub product: Option<FareProductId>,
}
