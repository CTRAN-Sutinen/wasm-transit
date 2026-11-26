//! The [`StopOffset`]s of a trip are seconds after its start.
//! `at_timepoints` are cumulative seconds from the first timepoint. The
//! [`TimingProfile`]s of a pattern only produce times for new trips, because
//! planners adjust individual trips to observed running times.

#[cfg(feature = "bindings")]
use ts_rs::TS;
use std::collections::BTreeMap;

use transit_date::{Date, Weekday};
use serde::{Deserialize, Serialize};

use crate::demand::DemandResponsive;
use crate::error::NetworkError;
use crate::fares::{FareNetworkId, Fares};
use crate::operators::{Attribution, Operator, OperatorId, Publisher, Translation};
use crate::places::{Level, LevelId, Pathway, PathwayId, StationPart, StopInfo, StopKind, TransferRule};

pub type StopId = String;
pub type RouteId = String;
pub type PatternId = String;
pub type ServiceTypeId = String;
pub type PatternTripsId = String;
pub type BlockId = String;
pub type DirectionId = String;
pub type TripId = String;
pub type AlignmentId = String;

/// A WGS84 position. The model never stores GeoJSON order, [lon, lat].
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS, schemars::JsonSchema))]
#[cfg_attr(feature = "bindings", ts(export))]
pub struct Coord {
    pub lat: f64,
    pub lon: f64,
}

/// It carries no timing and no stops, so it only draws. Any number of
/// patterns may run one ([`Pattern::alignment`]). An alignment that no
/// pattern runs still stands.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS, schemars::JsonSchema))]
#[cfg_attr(feature = "bindings", ts(export))]
pub struct Alignment {
    pub points: Vec<AlignmentPoint>,
    #[serde(flatten)]
    #[cfg_attr(feature = "bindings", ts(skip))]
    pub extra: transit_version::Extra,
}

/// The distance is present where the publisher gave one, in the units of the
/// pattern stops. The fields are in canonical key order (RFC 8785), so
/// the canonical bytes stream in one pass.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS, schemars::JsonSchema))]
#[cfg_attr(feature = "bindings", ts(export))]
pub struct AlignmentPoint {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub distance: Option<f64>,
    pub lat: f64,
    pub lon: f64,
}

/// `NoonMinusTwelve` is the GTFS convention. No DST transition occurs near
/// noon, so offsets stay unambiguous on change days.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS, schemars::JsonSchema))]
#[cfg_attr(feature = "bindings", ts(export))]
#[serde(rename_all = "snake_case")]
pub enum Anchor {
    NoonMinusTwelve,
}

impl Default for Anchor {
    fn default() -> Self {
        Anchor::NoonMinusTwelve
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS, schemars::JsonSchema))]
#[cfg_attr(feature = "bindings", ts(export))]
pub struct Stop {
    pub name: String,
    pub lat: f64,
    pub lon: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<StopId>,
    #[serde(default, skip_serializing_if = "StopKind::is_stop")]
    #[cfg_attr(feature = "bindings", ts(optional, as = "Option<StopKind>"))]
    pub kind: StopKind,
    #[serde(default, skip_serializing_if = "StopInfo::is_empty")]
    #[cfg_attr(feature = "bindings", ts(optional, as = "Option<StopInfo>"))]
    pub info: StopInfo,
    /// Validation requires at least two routes to meet at a transfer point.
    #[serde(default)]
    pub transfer: bool,
    #[serde(flatten)]
    #[cfg_attr(feature = "bindings", ts(skip))]
    pub extra: transit_version::Extra,
}

/// `name` is the short name that riders see ("12"). Display a route by
/// [`Route::label`].
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS, schemars::JsonSchema))]
#[cfg_attr(feature = "bindings", ts(export))]
pub struct Route {
    /// Empty for a route that has only a long name.
    pub name: String,
    pub mode: Mode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub long_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub text_color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub sort_order: Option<u32>,
    /// `None` means riders board and alight only at stops.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub continuous_pickup: Option<StopRule>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub continuous_drop_off: Option<StopRule>,
    /// `None` is the only operator of the network.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub operator: Option<OperatorId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub fare_network: Option<FareNetworkId>,
    /// `None` defers to the operator.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub contactless: Option<crate::operators::Contactless>,
    /// The published code for an extended type that `mode` does not say.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub vehicle_type: Option<u16>,
    /// A pattern belongs to at most one. Empty for a route with no
    /// directions.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    #[cfg_attr(feature = "bindings", ts(optional, as = "Option<BTreeMap<DirectionId, Direction>>"))]
    pub directions: BTreeMap<DirectionId, Direction>,
    #[serde(flatten)]
    #[cfg_attr(feature = "bindings", ts(skip))]
    pub extra: transit_version::Extra,
}

impl Route {
    pub fn label(&self) -> &str {
        if self.name.is_empty() {
            self.long_name.as_deref().unwrap_or("")
        } else {
            &self.name
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS, schemars::JsonSchema))]
#[cfg_attr(feature = "bindings", ts(export))]
#[serde(rename_all = "snake_case")]
pub enum StopRule {
    #[default]
    Regular,
    NotAvailable,
    PhoneAgency,
    AskDriver,
}

impl StopRule {
    pub fn is_regular(&self) -> bool {
        *self == StopRule::Regular
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS, schemars::JsonSchema))]
#[cfg_attr(feature = "bindings", ts(export))]
pub struct Direction {
    pub name: String,
    #[serde(flatten)]
    #[cfg_attr(feature = "bindings", ts(skip))]
    pub extra: transit_version::Extra,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS, schemars::JsonSchema))]
#[cfg_attr(feature = "bindings", ts(export))]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    Tram,
    Subway,
    Rail,
    #[default]
    Bus,
    Ferry,
    CableCar,
    Gondola,
    Funicular,
    Trolleybus,
    Monorail,
    Other,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS, schemars::JsonSchema))]
#[cfg_attr(feature = "bindings", ts(export))]
pub struct PatternStop {
    pub stop: StopId,
    /// Every trip carries both times here. Other stops may leave times
    /// unscheduled.
    pub timepoint: bool,
    /// Meters along the pattern from its first stop, non-decreasing.
    pub distance_m: f64,
    /// `false` when `distance_m` is an estimate.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    #[cfg_attr(feature = "bindings", ts(optional, as = "Option<bool>"))]
    pub distance_published: bool,
    /// Overrides the headsign of the trip at this visit only
    /// (`visit_destination`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub headsign: Option<String>,
    #[serde(default, skip_serializing_if = "StopRule::is_regular")]
    #[cfg_attr(feature = "bindings", ts(optional, as = "Option<StopRule>"))]
    pub pickup: StopRule,
    #[serde(default, skip_serializing_if = "StopRule::is_regular")]
    #[cfg_attr(feature = "bindings", ts(optional, as = "Option<StopRule>"))]
    pub drop_off: StopRule,
    /// Applies from here to the next stop. `None` defers to the route.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub continuous_pickup: Option<StopRule>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub continuous_drop_off: Option<StopRule>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub pickup_booking: Option<crate::demand::BookingRuleId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub drop_off_booking: Option<crate::demand::BookingRuleId>,
    #[serde(flatten)]
    #[cfg_attr(feature = "bindings", ts(skip))]
    pub extra: transit_version::Extra,
}

/// The run times for new trips that depart in `[start_secs, end_secs)`.
/// `at_timepoints` has one entry for each timepoint, in pattern order. The
/// first entry is 0.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS, schemars::JsonSchema))]
#[cfg_attr(feature = "bindings", ts(export))]
pub struct TimingProfile {
    pub start_secs: u32,
    pub end_secs: u32,
    pub at_timepoints: Vec<u32>,
    #[serde(flatten)]
    #[cfg_attr(feature = "bindings", ts(skip))]
    pub extra: transit_version::Extra,
}

impl TimingProfile {
    pub fn covers(&self, t: u32) -> bool {
        self.start_secs <= t && t < self.end_secs
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS, schemars::JsonSchema))]
#[cfg_attr(feature = "bindings", ts(export))]
pub struct Pattern {
    pub route: RouteId,
    /// One of the [`Route::directions`]. Bay and short-turn variants share a
    /// direction. `None` when the route has none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub direction: Option<DirectionId>,
    pub name: String,
    /// An [`Alignment`] id. `None` means draw stop-to-stop lines.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alignment: Option<AlignmentId>,
    pub stops: Vec<PatternStop>,
    #[serde(default)]
    pub timing: Vec<TimingProfile>,
    #[serde(flatten)]
    #[cfg_attr(feature = "bindings", ts(skip))]
    pub extra: transit_version::Extra,
}

impl Pattern {
    pub fn timepoint_positions(&self) -> Vec<usize> {
        self.stops
            .iter()
            .enumerate()
            .filter(|(_, s)| s.timepoint)
            .map(|(i, _)| i)
            .collect()
    }

    pub fn profile_for(&self, t: u32) -> Option<&TimingProfile> {
        self.timing.iter().find(|p| p.covers(t))
    }
}

/// GTFS calls it service_id.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS, schemars::JsonSchema))]
#[cfg_attr(feature = "bindings", ts(export))]
pub struct ServiceType {
    pub name: String,
    pub rule: CalendarRule,
    #[serde(flatten)]
    #[cfg_attr(feature = "bindings", ts(skip))]
    pub extra: transit_version::Extra,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS, schemars::JsonSchema))]
#[cfg_attr(feature = "bindings", ts(export))]
pub struct CalendarRule {
    #[serde(default)]
    pub days: WeekdaySet,
    /// Inclusive.
    #[serde(default)]
    pub ranges: Vec<DateRange>,
    #[serde(default)]
    pub exceptions: Vec<CalendarException>,
    #[serde(flatten)]
    #[cfg_attr(feature = "bindings", ts(skip))]
    pub extra: transit_version::Extra,
}

impl CalendarRule {
    /// An exception for the date always wins.
    pub fn runs_on(&self, date: Date) -> bool {
        if let Some(exc) = self.exceptions.iter().find(|e| e.date == date) {
            return exc.state == DayState::Added;
        }
        self.days.runs_on(date.weekday())
            && self.ranges.iter().any(|r| r.start <= date && date <= r.end)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS, schemars::JsonSchema))]
#[cfg_attr(feature = "bindings", ts(export))]
pub struct DateRange {
    #[cfg_attr(feature = "bindings", ts(type = "string"))]
    pub start: Date,
    #[cfg_attr(feature = "bindings", ts(type = "string"))]
    pub end: Date,
    #[serde(flatten)]
    #[cfg_attr(feature = "bindings", ts(skip))]
    pub extra: transit_version::Extra,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS, schemars::JsonSchema))]
#[cfg_attr(feature = "bindings", ts(export))]
pub struct CalendarException {
    #[cfg_attr(feature = "bindings", ts(type = "string"))]
    pub date: Date,
    pub state: DayState,
    #[serde(flatten)]
    #[cfg_attr(feature = "bindings", ts(skip))]
    pub extra: transit_version::Extra,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS, schemars::JsonSchema))]
#[cfg_attr(feature = "bindings", ts(export))]
#[serde(rename_all = "snake_case")]
pub enum DayState {
    Added,
    Removed,
}

/// A rule with no flags still runs on dates that are `Added`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS, schemars::JsonSchema))]
#[cfg_attr(feature = "bindings", ts(export))]
pub struct WeekdaySet {
    #[serde(default)]
    pub mon: bool,
    #[serde(default)]
    pub tue: bool,
    #[serde(default)]
    pub wed: bool,
    #[serde(default)]
    pub thu: bool,
    #[serde(default)]
    pub fri: bool,
    #[serde(default)]
    pub sat: bool,
    #[serde(default)]
    pub sun: bool,
}

impl WeekdaySet {
    pub fn none() -> Self {
        Self::default()
    }

    pub fn weekdays() -> Self {
        Self {
            mon: true,
            tue: true,
            wed: true,
            thu: true,
            fri: true,
            sun: false,
            sat: false,
        }
    }

    pub fn runs_on(&self, day: Weekday) -> bool {
        match day {
            Weekday::Mon => self.mon,
            Weekday::Tue => self.tue,
            Weekday::Wed => self.wed,
            Weekday::Thu => self.thu,
            Weekday::Fri => self.fri,
            Weekday::Sat => self.sat,
            Weekday::Sun => self.sun,
        }
    }

    pub fn is_empty(&self) -> bool {
        !self.mon
            && !self.tue
            && !self.wed
            && !self.thu
            && !self.fri
            && !self.sat
            && !self.sun
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS, schemars::JsonSchema))]
#[cfg_attr(feature = "bindings", ts(export))]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Departs {
    At { start_secs: u32 },
    /// One instance at `start_secs + k * headway_secs` while the start is
    /// `< end_secs`, for each band. All instances share the id and times of
    /// the trip. Without `exact`, riders get the headway, not the minutes.
    Every { bands: Vec<HeadwayBand>, exact: bool },
}

impl Default for Departs {
    fn default() -> Self {
        Departs::At { start_secs: 0 }
    }
}

impl Departs {
    /// A zero headway is invalid. It yields no instances, so it cannot loop
    /// forever.
    pub fn instances(&self) -> Vec<u32> {
        match self {
            Departs::At { start_secs } => vec![*start_secs],
            Departs::Every { bands, .. } => {
                let mut out = Vec::new();
                for band in bands.iter().filter(|b| b.headway_secs > 0) {
                    let mut t = band.start_secs;
                    while t < band.end_secs {
                        out.push(t);
                        t += band.headway_secs;
                    }
                }
                out
            }
        }
    }

    pub fn first_start(&self) -> Option<u32> {
        match self {
            Departs::At { start_secs } => Some(*start_secs),
            Departs::Every { bands, .. } => bands.iter().map(|b| b.start_secs).min(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS, schemars::JsonSchema))]
#[cfg_attr(feature = "bindings", ts(export))]
pub struct HeadwayBand {
    pub start_secs: u32,
    pub end_secs: u32,
    pub headway_secs: u32,
    #[serde(flatten)]
    #[cfg_attr(feature = "bindings", ts(skip))]
    pub extra: transit_version::Extra,
}

/// Seconds after the start of the instance. `None` means not scheduled there.
/// The first arrival is 0. The first and last stops, and every timepoint,
/// carry both times.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS, schemars::JsonSchema))]
#[cfg_attr(feature = "bindings", ts(export))]
pub struct StopOffset {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub arrival_secs: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub departure_secs: Option<u32>,
}

/// `times[i]` is the time at `pattern.stops[i]`. The fields are in canonical
/// key order (RFC 8785), so the canonical bytes stream in one pass.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS, schemars::JsonSchema))]
#[cfg_attr(feature = "bindings", ts(export))]
pub struct Trip {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub bikes: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub cars: Option<bool>,
    pub departs: Departs,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub headsign: Option<String>,
    pub id: TripId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub short_name: Option<String>,
    pub times: Vec<StopOffset>,
    /// `None` is unknown.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub wheelchair: Option<bool>,
    #[serde(flatten)]
    #[cfg_attr(feature = "bindings", ts(skip))]
    pub extra: transit_version::Extra,
}

impl Trip {
    pub fn run_secs(&self) -> Option<u32> {
        self.times.last().and_then(|o| o.arrival_secs.or(o.departure_secs))
    }
}

/// The trips of one pattern under one service type. A timetable spans a
/// direction of a route across its patterns (`listing::timetable`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS, schemars::JsonSchema))]
#[cfg_attr(feature = "bindings", ts(export))]
pub struct PatternTrips {
    pub pattern: PatternId,
    pub service_type: ServiceTypeId,
    pub trips: Vec<Trip>,
    #[serde(flatten)]
    #[cfg_attr(feature = "bindings", ts(skip))]
    pub extra: transit_version::Extra,
}

/// A block may hold trips of service types that never share a day. On each
/// day it is the trips that run that day, in this order.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS, schemars::JsonSchema))]
#[cfg_attr(feature = "bindings", ts(export))]
pub struct Block {
    pub trips: Vec<TripRef>,
    pub layovers: Vec<Layover>,
    #[serde(flatten)]
    #[cfg_attr(feature = "bindings", ts(skip))]
    pub extra: transit_version::Extra,
}

/// A frequency trip in a block is every one of its instances, back to back.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS, schemars::JsonSchema))]
#[cfg_attr(feature = "bindings", ts(export))]
pub struct TripRef {
    pub trip: TripId,
    #[serde(flatten)]
    #[cfg_attr(feature = "bindings", ts(skip))]
    pub extra: transit_version::Extra,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS, schemars::JsonSchema))]
#[cfg_attr(feature = "bindings", ts(export))]
pub struct Layover {
    /// An index into the trips of the block.
    pub after_trip: usize,
    pub secs: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at: Option<StopId>,
    #[serde(flatten)]
    #[cfg_attr(feature = "bindings", ts(skip))]
    pub extra: transit_version::Extra,
}

/// Each pick is one document, effective from its service change date until
/// the next pick supersedes it. A document without plan metadata is
/// unversioned, such as a fresh GTFS import.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS, schemars::JsonSchema))]
#[cfg_attr(feature = "bindings", ts(export))]
pub struct PlanMeta {
    /// The sync key (LWW for each plan). Empty on unversioned documents.
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
    /// The service change date. None on unversioned documents. A plan in a
    /// chain must have one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(type = "string | null"))]
    pub effective_from: Option<Date>,
    #[serde(flatten)]
    #[cfg_attr(feature = "bindings", ts(skip))]
    pub extra: transit_version::Extra,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS, schemars::JsonSchema))]
#[cfg_attr(feature = "bindings", ts(export))]
pub struct Network {
    #[serde(default)]
    #[cfg_attr(feature = "bindings", ts(type = "\"network\""))]
    pub kind: crate::version::NetworkKind,
    /// A document without one is version 1.
    #[serde(default = "crate::version::NetworkVersion::untagged")]
    #[cfg_attr(feature = "bindings", ts(type = "number"))]
    pub schema_version: crate::version::NetworkVersion,
    pub name: String,
    #[serde(default)]
    pub plan: PlanMeta,
    /// Every offset in the network counts from it.
    #[serde(default)]
    pub anchor: Anchor,
    pub stops: BTreeMap<StopId, Stop>,
    pub routes: BTreeMap<RouteId, Route>,
    pub patterns: BTreeMap<PatternId, Pattern>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub alignments: BTreeMap<AlignmentId, Alignment>,
    pub service_types: BTreeMap<ServiceTypeId, ServiceType>,
    pub pattern_trips: BTreeMap<PatternTripsId, PatternTrips>,
    pub blocks: BTreeMap<BlockId, Block>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    #[cfg_attr(feature = "bindings", ts(optional, as = "Option<BTreeMap<OperatorId, Operator>>"))]
    pub operators: BTreeMap<OperatorId, Operator>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub publisher: Option<Publisher>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[cfg_attr(feature = "bindings", ts(optional, as = "Option<Vec<Attribution>>"))]
    pub attributions: Vec<Attribution>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[cfg_attr(feature = "bindings", ts(optional, as = "Option<Vec<Translation>>"))]
    pub translations: Vec<Translation>,
    /// Ids are unique across these and the stops.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    #[cfg_attr(feature = "bindings", ts(optional, as = "Option<BTreeMap<StopId, StationPart>>"))]
    pub station_parts: BTreeMap<StopId, StationPart>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    #[cfg_attr(feature = "bindings", ts(optional, as = "Option<BTreeMap<LevelId, Level>>"))]
    pub levels: BTreeMap<LevelId, Level>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    #[cfg_attr(feature = "bindings", ts(optional, as = "Option<BTreeMap<PathwayId, Pathway>>"))]
    pub pathways: BTreeMap<PathwayId, Pathway>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[cfg_attr(feature = "bindings", ts(optional, as = "Option<Vec<TransferRule>>"))]
    pub transfers: Vec<TransferRule>,
    #[serde(default, skip_serializing_if = "Fares::is_empty")]
    #[cfg_attr(feature = "bindings", ts(optional, as = "Option<Fares>"))]
    pub fares: Fares,
    #[serde(default, skip_serializing_if = "DemandResponsive::is_empty")]
    #[cfg_attr(feature = "bindings", ts(optional, as = "Option<DemandResponsive>"))]
    pub demand: DemandResponsive,
    #[serde(flatten)]
    #[cfg_attr(feature = "bindings", ts(skip))]
    pub extra: transit_version::Extra,
}

impl Network {
    pub fn trips(&self) -> impl Iterator<Item = (&str, &PatternTrips, &Trip)> {
        self.pattern_trips
            .iter()
            .flat_map(|(key, pt)| pt.trips.iter().map(move |t| (key.as_str(), pt, t)))
    }

    pub fn trip(&self, id: &str) -> Option<(&str, &PatternTrips, &Trip)> {
        self.trips().find(|(_, _, t)| t.id == id)
    }

    /// `validation_issues` returns the full list.
    pub fn validate(&self) -> Result<(), NetworkError> {
        let issues = crate::validate::validation_issues(self);
        if issues.is_empty() {
            Ok(())
        } else {
            Err(issues[0].clone())
        }
    }
}
