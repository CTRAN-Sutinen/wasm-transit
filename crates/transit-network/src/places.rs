use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::model::{Coord, RouteId, StopId, TripId};

pub type LevelId = String;
pub type PathwayId = String;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum StopKind {
    #[default]
    Stop,
    Station,
}

impl StopKind {
    pub fn is_stop(&self) -> bool {
        *self == StopKind::Stop
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
pub struct StopInfo {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub code: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub spoken_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub fare_zone: Option<String>,
    /// `None` is unknown. A child place inherits its parent's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub wheelchair: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub platform: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub level: Option<LevelId>,
    /// Set only where it differs from the operator's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub timezone: Option<String>,
    /// `false` when riders reach the place only through the station, not
    /// from the street.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub street_access: Option<bool>,
    #[serde(flatten)]
    #[ts(skip)]
    pub extra: transit_version::Extra,
}

impl StopInfo {
    pub fn is_empty(&self) -> bool {
        *self == StopInfo::default()
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum StationPartKind {
    #[default]
    Entrance,
    Node,
    BoardingArea,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
pub struct StationPart {
    pub kind: StationPartKind,
    /// For a boarding area, the platform.
    pub parent: StopId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub name: Option<String>,
    /// Required of an entrance.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub at: Option<Coord>,
    #[serde(default, skip_serializing_if = "StopInfo::is_empty")]
    #[ts(optional, as = "Option<StopInfo>")]
    pub info: StopInfo,
    #[serde(flatten)]
    #[ts(skip)]
    pub extra: transit_version::Extra,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
pub struct Level {
    /// 0 is the ground, and a negative index is below it.
    pub index: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub name: Option<String>,
    #[serde(flatten)]
    #[ts(skip)]
    pub extra: transit_version::Extra,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum PathwayMode {
    #[default]
    Walkway,
    Stairs,
    MovingSidewalk,
    Escalator,
    Elevator,
    FareGate,
    ExitGate,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
pub struct Pathway {
    pub from: StopId,
    pub to: StopId,
    pub mode: PathwayMode,
    /// `false` when the pathway runs only from `from` to `to`.
    pub both_ways: bool,
    /// Meters.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub length: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub traversal_secs: Option<u32>,
    /// Negative going down.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub stair_count: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub max_slope: Option<f64>,
    /// Meters.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub min_width: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub signposted_as: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub reversed_signposted_as: Option<String>,
    #[serde(flatten)]
    #[ts(skip)]
    pub extra: transit_version::Extra,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum TransferKind {
    #[default]
    Recommended,
    /// The departing vehicle waits for the arriving one.
    Timed,
    MinimumTime,
    NotPossible,
    /// The rider stays in the vehicle from one trip to the next.
    InSeat,
    InSeatNotAllowed,
}

/// Unset parts match anything.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
pub struct TransferEnd {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub stop: Option<StopId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub route: Option<RouteId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub trip: Option<TripId>,
    #[serde(flatten)]
    #[ts(skip)]
    pub extra: transit_version::Extra,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
pub struct TransferRule {
    pub from: TransferEnd,
    pub to: TransferEnd,
    pub kind: TransferKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub min_transfer_secs: Option<u32>,
    #[serde(flatten)]
    #[ts(skip)]
    pub extra: transit_version::Extra,
}
