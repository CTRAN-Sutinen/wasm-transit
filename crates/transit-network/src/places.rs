//! Where riders are: what a stop is beyond its name and point, the
//! parts of a station riders walk through (entrances, nodes, boarding
//! areas), its levels and the pathways between them, and the
//! transfer rules between stops, routes, and trips.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::model::{Coord, RouteId, StopId, TripId};

pub type LevelId = String;
pub type PathwayId = String;

/// What kind of place a [`crate::Stop`] is: a stop or platform riders
/// board at, or a station grouping them.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
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

/// What a rider can know about a place beyond its name and point.
/// Shared by stops and station parts.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct StopInfo {
    /// The short code riders see on the sign or type into a phone.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub code: Option<String>,
    /// How the name is read aloud, where it differs from the text.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub spoken_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub url: Option<String>,
    /// The fare zone the place is in (zone fares).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub fare_zone: Option<String>,
    /// Whether a wheelchair user can board here (or, for an entrance,
    /// get in); `None` = unknown (a child place inherits its parent's).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub wheelchair: Option<bool>,
    /// The platform's own identifier ("G", "3").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub platform: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub level: Option<LevelId>,
    /// The place's own timezone, where it differs from its operator's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub timezone: Option<String>,
    /// A station's stop: reached straight from the street (`true`) or
    /// only through the station (`false`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub street_access: Option<bool>,
}

impl StopInfo {
    pub fn is_empty(&self) -> bool {
        *self == StopInfo::default()
    }
}

/// A part of a station riders walk through but never board at.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum StationPartKind {
    #[default]
    Entrance,
    /// A point where pathways meet.
    Node,
    /// Where on a platform a rider boards.
    BoardingArea,
}

/// An entrance, node, or boarding area: part of a station (a boarding
/// area is part of its platform), linked to the rest by pathways.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct StationPart {
    pub kind: StationPartKind,
    /// The station (or, for a boarding area, the platform) it is part of.
    pub parent: StopId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub name: Option<String>,
    /// Where it is, when published (required of an entrance).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub at: Option<Coord>,
    #[serde(default, skip_serializing_if = "StopInfo::is_empty")]
    #[ts(optional, as = "Option<StopInfo>")]
    pub info: StopInfo,
}

/// One floor of a station.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Level {
    /// Order of the levels: 0 is the ground, negative is below it.
    pub index: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub name: Option<String>,
}

/// How a pathway is walked.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum PathwayMode {
    #[default]
    Walkway,
    Stairs,
    MovingSidewalk,
    Escalator,
    Elevator,
    /// A fare gate: paying happens crossing it.
    FareGate,
    /// An exit gate: a fare gate's way out.
    ExitGate,
}

/// A way between two places of a station (stops, station parts).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Pathway {
    pub from: StopId,
    pub to: StopId,
    pub mode: PathwayMode,
    /// Walkable both ways (`false`: only from → to).
    pub both_ways: bool,
    /// Meters.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub length: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub traversal_secs: Option<u32>,
    /// Steps, negative going down.
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
    /// What the signs say walking it from → to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub signposted_as: Option<String>,
    /// What the signs say walking it to → from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub reversed_signposted_as: Option<String>,
}

/// What a transfer rule says about changing vehicles.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum TransferKind {
    /// A recommended place to change.
    #[default]
    Recommended,
    /// The departing vehicle waits for the arriving one.
    Timed,
    /// Changing takes at least `min_transfer_secs`.
    MinimumTime,
    /// Changing is not possible here.
    NotPossible,
    /// The rider stays in the vehicle from one trip to the next.
    InSeat,
    /// The rider must leave the vehicle between the two trips.
    InSeatNotAllowed,
}

/// One side of a transfer rule: where (and from or onto what) the
/// rider changes. Unset parts match anything.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
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
}

/// How riders change vehicles between two places, routes, or trips.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct TransferRule {
    pub from: TransferEnd,
    pub to: TransferEnd,
    pub kind: TransferKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub min_transfer_secs: Option<u32>,
}
