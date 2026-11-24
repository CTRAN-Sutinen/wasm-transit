//! The authored network model: planner intent as data.
//!
//! Reading order: [`Network`] holds everything a planner authors.
//! [`Route`]s run in directions. A [`Pattern`] is one stop sequence of a
//! route with its timepoints. A [`ServiceType`] is the day type that
//! planners design (Weekday, Saturday…). [`PatternTrips`] are one pattern's
//! [`Trip`]s under one service type. [`Block`] is the runcut. A planner's
//! timetable (a route, one direction, one service type) is every trip of
//! that direction's patterns (`listing::timetable`).
//!
//! A trip is a stored record. It has its own id, its own departure
//! ([`Departs`]: one start, or a frequency), and its own time at every stop.
//! A pattern's [`TimingProfile`]s produce times for new trips. They are never
//! the schedule. Planners adjust individual trips to observed running times.
//!
//! Time conventions (see the crate docs): `u32` seconds after the service-day
//! [`Anchor`] of the date in question. A trip's [`StopOffset`]s are seconds
//! after its start. `at_timepoints` are cumulative seconds from the first
//! timepoint.

use ts_rs::TS;
use std::collections::BTreeMap;

use chrono::{Datelike, NaiveDate, Weekday};
use serde::{Deserialize, Serialize};

use crate::demand::DemandResponsive;
use crate::error::NetworkError;
use crate::fares::{FareNetworkId, Fares};
use crate::operators::{Attribution, Operator, OperatorId, Publisher, Translation};
use crate::places::{Level, LevelId, Pathway, PathwayId, StationPart, StopInfo, StopKind, TransferRule};

/// IDs are opaque strings owned by this model. Type aliases document intent
/// without newtype ceremony (a non-goal).
pub type StopId = String;
pub type RouteId = String;
pub type PatternId = String;
pub type ServiceTypeId = String;
pub type PatternTripsId = String;
pub type BlockId = String;
pub type DirectionId = String;
pub type TripId = String;
pub type AlignmentId = String;

/// A WGS84 position. Authored geometry (a pattern's alignment) is model
/// data. `lat` comes first, matching `Stop`'s field order. GeoJSON views at
/// the boundary reorder to [lon, lat]. The model never stores GeoJSON order.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
pub struct Coord {
    pub lat: f64,
    pub lon: f64,
}

/// A line on the ground: the path vehicles take, in travel order. It is
/// purely geographic. It carries no timing and no stops. Materialization uses
/// it for drawing only, never for scheduling. Patterns run one by reference
/// ([`Pattern::alignment`]). Any number may share one. One that no pattern
/// runs still stands.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
pub struct Alignment {
    pub points: Vec<AlignmentPoint>,
    /// Fields this build does not know, kept as written (D-167).
    #[serde(flatten)]
    #[ts(skip)]
    pub extra: transit_version::Extra,
}

/// One point of an [`Alignment`], with the distance along it where
/// one was published (in the units of the pattern stops' distances).
/// Its fields are declared in canonical key order (RFC 8785, D-169), so
/// its canonical bytes stream in one pass.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
pub struct AlignmentPoint {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub distance: Option<f64>,
    pub lat: f64,
    pub lon: f64,
}

/// Serde helper: dates cross every boundary as `"YYYY-MM-DD"`.
/// (chrono's serde feature is not enabled workspace-wide.)
pub mod iso_date {
    use chrono::NaiveDate;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(
        date: &NaiveDate,
        s: S,
    ) -> Result<S::Ok, S::Error> {
        s.serialize_str(&date.format("%Y-%m-%d").to_string())
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(
        d: D,
    ) -> Result<NaiveDate, D::Error> {
        let raw = String::deserialize(d)?;
        NaiveDate::parse_from_str(raw.trim(), "%Y-%m-%d")
            .map_err(|e| serde::de::Error::custom(format!("invalid date {raw:?}: {e}")))
    }
}

/// Same, optional: None serializes as null.
pub mod iso_opt_date {
    use chrono::NaiveDate;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(date: &Option<NaiveDate>, s: S) -> Result<S::Ok, S::Error> {
        match date {
            Some(d) => s.serialize_str(&d.format("%Y-%m-%d").to_string()),
            None => s.serialize_none(),
        }
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Option<NaiveDate>, D::Error> {
        let raw: Option<String> = Option::deserialize(d)?;
        match raw {
            None => Ok(None),
            Some(raw) => NaiveDate::parse_from_str(raw.trim(), "%Y-%m-%d")
                .map(Some)
                .map_err(|e| serde::de::Error::custom(format!("invalid date {raw:?}: {e}"))),
        }
    }
}

/// Where a service day's second zero sits. `NoonMinusTwelve` is the GTFS
/// convention. It is chosen on its merits, not for compatibility. No DST
/// transition occurs near noon, so offsets stay unambiguous on change days.
/// The authored model states the convention where GTFS leaves it to
/// inference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum Anchor {
    /// Seconds after "noon minus 12h" of the service date.
    NoonMinusTwelve,
}

impl Default for Anchor {
    fn default() -> Self {
        Anchor::NoonMinusTwelve
    }
}

/// A stop as a planner authors it: one name, one location, and an optional
/// parent (platform-in-station groupings). A stop is either a stop riders
/// board at, or a station that groups stops.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
pub struct Stop {
    pub name: String,
    pub lat: f64,
    pub lon: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<StopId>,
    #[serde(default, skip_serializing_if = "StopKind::is_stop")]
    #[ts(optional, as = "Option<StopKind>")]
    pub kind: StopKind,
    #[serde(default, skip_serializing_if = "StopInfo::is_empty")]
    #[ts(optional, as = "Option<StopInfo>")]
    pub info: StopInfo,
        /// A designated transfer point: where the network intends riders to
        /// change vehicles. This is authored intent, not a badge. The flag
        /// means something only when composed with the routes that meet there.
        /// Validation requires at least two. The analysis ops measure the timed
        /// connections it enables.
    #[serde(default)]
    pub transfer: bool,
    /// Fields this build does not know, kept as written (D-167).
    #[serde(flatten)]
    #[ts(skip)]
    pub extra: transit_version::Extra,
}

/// A route as a planner authors it: its public name (the short one
/// riders see, "12"), its long name ("Lincoln"), a closed mode set.
/// Display it by [`Route::label`].
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
pub struct Route {
    /// The short public name; empty for a route known only by its
    /// long name.
    pub name: String,
    pub mode: Mode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub long_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub url: Option<String>,
    /// The text color drawn over `color`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub text_color: Option<String>,
    /// Where the route sorts in lists (lower first).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub sort_order: Option<u32>,
    /// Riders may board (alight) anywhere along the route's line, by
    /// this rule; `None` = only at stops.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub continuous_pickup: Option<StopRule>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub continuous_drop_off: Option<StopRule>,
    /// Who runs it; `None` = the network's only operator.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub operator: Option<OperatorId>,
    /// The fare network it belongs to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub fare_network: Option<FareNetworkId>,
    /// Whether contactless bank cards are accepted on board; `None` =
    /// as the operator says.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub contactless: Option<crate::operators::Contactless>,
    /// The published vehicle type code, where the mode alone does not
    /// say it (an extended type such as a regional rail or night bus).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub vehicle_type: Option<u16>,
    /// The ways the route runs, by id, each named the way the planner
    /// says it ("To Downtown", "Northbound"). A pattern belongs to at
    /// most one; a timetable is one direction on one service type.
    /// Empty (omitted) for a route nobody has split into directions.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    #[ts(optional, as = "Option<BTreeMap<DirectionId, Direction>>")]
    pub directions: BTreeMap<DirectionId, Direction>,
    /// Fields this build does not know, kept as written (D-167).
    #[serde(flatten)]
    #[ts(skip)]
    pub extra: transit_version::Extra,
}

impl Route {
    /// What to call the route: its public name, else its long name.
    pub fn label(&self) -> &str {
        if self.name.is_empty() {
            self.long_name.as_deref().unwrap_or("")
        } else {
            &self.name
        }
    }
}

/// Whether, and how, riders may board or alight: at a stop (pickup,
/// drop-off) or anywhere along the line (continuous stopping).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum StopRule {
    #[default]
    Regular,
    NotAvailable,
    /// The rider phones the agency to arrange it.
    PhoneAgency,
    /// The rider arranges it with the driver.
    AskDriver,
}

impl StopRule {
    pub fn is_regular(&self) -> bool {
        *self == StopRule::Regular
    }
}

/// One direction of a route, as a planner names it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
pub struct Direction {
    pub name: String,
    /// Fields this build does not know, kept as written (D-167).
    #[serde(flatten)]
    #[ts(skip)]
    pub extra: transit_version::Extra,
}

/// Vehicle modes as a domain concept, not GTFS's numeric codes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
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

/// One stop visit within a pattern.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
pub struct PatternStop {
    pub stop: StopId,
    /// An exact stop: every trip of the pattern carries both its
    /// arrival and its departure here. Other stops may leave times
    /// unscheduled (they materialize by distance between scheduled
    /// neighbours).
    pub timepoint: bool,
    /// Meters along the pattern from its first stop. Non-decreasing.
    pub distance_m: f64,
    /// Whether `distance_m` was published (in the publisher's own
    /// units) rather than estimated; an estimate is never published.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    #[ts(optional, as = "Option<bool>")]
    pub distance_published: bool,
    /// What the vehicle's sign says at this visit, where it differs
    /// from the trip's headsign (this visit only: `visit_destination`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub headsign: Option<String>,
    #[serde(default, skip_serializing_if = "StopRule::is_regular")]
    #[ts(optional, as = "Option<StopRule>")]
    pub pickup: StopRule,
    #[serde(default, skip_serializing_if = "StopRule::is_regular")]
    #[ts(optional, as = "Option<StopRule>")]
    pub drop_off: StopRule,
    /// Continuous stopping from here to the next stop; `None` = the
    /// route's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub continuous_pickup: Option<StopRule>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub continuous_drop_off: Option<StopRule>,
    /// The booking rule a rider follows to board (alight) here.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub pickup_booking: Option<crate::demand::BookingRuleId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub drop_off_booking: Option<crate::demand::BookingRuleId>,
    /// Fields this build does not know, kept as written (D-167).
    #[serde(flatten)]
    #[ts(skip)]
    pub extra: transit_version::Extra,
}

/// Run-time profile for new trips that depart in a time band
/// `[start_secs, end_secs)`. It is the planner's production tool. Stored
/// trips carry their own times. A profile never governs them.
///
/// `at_timepoints` is cumulative seconds from the first timepoint at
/// each of the pattern's timepoints, in pattern order (the first
/// entry is 0).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
pub struct TimingProfile {
    pub start_secs: u32,
    pub end_secs: u32,
    pub at_timepoints: Vec<u32>,
    /// Fields this build does not know, kept as written (D-167).
    #[serde(flatten)]
    #[ts(skip)]
    pub extra: transit_version::Extra,
}

impl TimingProfile {
    /// Does this profile govern a trip departing at `t`?
    pub fn covers(&self, t: u32) -> bool {
        self.start_secs <= t && t < self.end_secs
    }
}

/// A route pattern: the stop sequence a planner draws along its alignment.
/// The profiles that new trips are timed from are also here. Every trip runs
/// a pattern. A document written before alignments (an inline `path`) is
/// version 1. Load refuses it. The migration (`migrate.rs`) brings it forward
/// and moves its line.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
pub struct Pattern {
    pub route: RouteId,
    /// Which of the route's [`Route::directions`] this pattern runs
    /// in; `None` (omitted) when the route has none or the pattern is
    /// not yet placed. Bay and short-turn variants share a direction.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub direction: Option<DirectionId>,
    /// Human-facing description, e.g. "5-K Ave to Downtown via MLK".
    pub name: String,
    /// The [`Alignment`] the pattern runs, by id. `None` (omitted)
    /// means "draw stop-to-stop lines".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alignment: Option<AlignmentId>,
    pub stops: Vec<PatternStop>,
    #[serde(default)]
    pub timing: Vec<TimingProfile>,
    /// Fields this build does not know, kept as written (D-167).
    #[serde(flatten)]
    #[ts(skip)]
    pub extra: transit_version::Extra,
}

impl Pattern {
    /// Indices of the timepoint stops, in order.
    pub fn timepoint_positions(&self) -> Vec<usize> {
        self.stops
            .iter()
            .enumerate()
            .filter(|(_, s)| s.timepoint)
            .map(|(i, _)| i)
            .collect()
    }

    /// The timing profile a new trip departing at `t` is produced from.
    pub fn profile_for(&self, t: u32) -> Option<&TimingProfile> {
        self.timing.iter().find(|p| p.covers(t))
    }
}

/// A service type: the service identifier planners design, such as Weekday,
/// Saturday, Sunday, or a holiday. It links to the days it runs by its
/// calendar rule. This is the axis planners design on, not the day of the
/// month. GTFS calls it service_id.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
pub struct ServiceType {
    /// Display name ("Weekday", "Saturday service", …).
    pub name: String,
    /// The days it runs: weekday flags plus date ranges, with
    /// explicit per-date exceptions that always win.
    pub rule: CalendarRule,
    /// Fields this build does not know, kept as written (D-167).
    #[serde(flatten)]
    #[ts(skip)]
    pub extra: transit_version::Extra,
}

/// Which days a service type runs on, as authored rules rather than
/// GTFS exception rows: weekday flags plus date ranges, with explicit
/// per-date exceptions that always win.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
pub struct CalendarRule {
    #[serde(default)]
    pub days: WeekdaySet,
    /// Inclusive date ranges; a date in any range with its weekday
    /// flagged runs.
    #[serde(default)]
    pub ranges: Vec<DateRange>,
    /// Explicit per-date overrides; always beat the weekly rule.
    #[serde(default)]
    pub exceptions: Vec<CalendarException>,
    /// Fields this build does not know, kept as written (D-167).
    #[serde(flatten)]
    #[ts(skip)]
    pub extra: transit_version::Extra,
}

impl CalendarRule {
    /// Does this rule run on `date`? An exception for the date always
    /// wins; otherwise the weekday must be flagged and the date must
    /// fall in a range.
    pub fn runs_on(&self, date: NaiveDate) -> bool {
        if let Some(exc) = self.exceptions.iter().find(|e| e.date == date) {
            return exc.state == DayState::Added;
        }
        self.days.runs_on(date.weekday())
            && self.ranges.iter().any(|r| r.start <= date && date <= r.end)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
pub struct DateRange {
    #[serde(with = "iso_date")]
    #[schemars(with = "chrono::NaiveDate")]
    #[ts(type = "string")]
    pub start: NaiveDate,
    #[serde(with = "iso_date")]
    #[schemars(with = "chrono::NaiveDate")]
    #[ts(type = "string")]
    pub end: NaiveDate,
    /// Fields this build does not know, kept as written (D-167).
    #[serde(flatten)]
    #[ts(skip)]
    pub extra: transit_version::Extra,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
pub struct CalendarException {
    #[serde(with = "iso_date")]
    #[schemars(with = "chrono::NaiveDate")]
    #[ts(type = "string")]
    pub date: NaiveDate,
    pub state: DayState,
    /// Fields this build does not know, kept as written (D-167).
    #[serde(flatten)]
    #[ts(skip)]
    pub extra: transit_version::Extra,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum DayState {
    /// Service runs this date even though the weekly rule says no.
    Added,
    /// Service does not run this date even though the weekly rule
    /// says yes.
    Removed,
}

/// Authored weekday flags. A rule with no flags still runs on dates
/// explicitly `Added` (special-event service). See `runs_on`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
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

/// When a trip departs: once, or as a frequency.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Departs {
    /// One instance, starting at `start_secs`.
    At { start_secs: u32 },
    /// A frequency trip: one instance at `start_secs + k * headway_secs`
    /// while the start is `< end_secs`, per band (end-exclusive), all
    /// sharing the trip's id and times. `exact`: the instances run on
    /// exactly that grid (otherwise riders are promised the headway,
    /// not the minutes).
    Every { bands: Vec<HeadwayBand>, exact: bool },
}

impl Default for Departs {
    fn default() -> Self {
        Departs::At { start_secs: 0 }
    }
}

impl Departs {
    /// Every instance start, in band order. A zero headway (invalid,
    /// flagged by validation) yields none rather than looping forever.
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

    /// The first instance start (a frequency trip's earliest band).
    pub fn first_start(&self) -> Option<u32> {
        match self {
            Departs::At { start_secs } => Some(*start_secs),
            Departs::Every { bands, .. } => bands.iter().map(|b| b.start_secs).min(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
pub struct HeadwayBand {
    pub start_secs: u32,
    pub end_secs: u32,
    pub headway_secs: u32,
    /// Fields this build does not know, kept as written (D-167).
    #[serde(flatten)]
    #[ts(skip)]
    pub extra: transit_version::Extra,
}

/// A trip's time at one pattern stop: seconds after the instance's
/// start. `None` = not scheduled there (the stop materializes between
/// its scheduled neighbours). The first arrival is 0; the first and
/// last stops, and every timepoint, carry both.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
pub struct StopOffset {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub arrival_secs: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub departure_secs: Option<u32>,
}

/// One trip: the agency's id, when it departs, and its own time at
/// every stop of its pattern (`times[i]` is `pattern.stops[i]`). Its
/// fields are declared in canonical key order (RFC 8785, D-169), so its
/// canonical bytes stream in one pass (`cap-abi`, `jcs_law_test.rs`).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
pub struct Trip {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub bikes: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub cars: Option<bool>,
    pub departs: Departs,
    /// What the vehicle's sign says.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub headsign: Option<String>,
    pub id: TripId,
    /// The trip's public number, where riders know it by one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub short_name: Option<String>,
    pub times: Vec<StopOffset>,
    /// Whether the vehicle takes a wheelchair (bikes, cars); `None` =
    /// unknown.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub wheelchair: Option<bool>,
    /// Fields this build does not know, kept as written (D-167).
    #[serde(flatten)]
    #[ts(skip)]
    pub extra: transit_version::Extra,
}

impl Trip {
    /// Seconds from start to the arrival at the last stop.
    pub fn run_secs(&self) -> Option<u32> {
        self.times.last().and_then(|o| o.arrival_secs.or(o.departure_secs))
    }
}

/// One pattern's trips under one service type. Everything RAPTOR and dispatch
/// consume derives from these. They are not a planner's "schedule". The
/// timetable a planner reads is a route's direction under one service type,
/// across all its patterns (`listing::timetable`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
pub struct PatternTrips {
    pub pattern: PatternId,
    pub service_type: ServiceTypeId,
    pub trips: Vec<Trip>,
    /// Fields this build does not know, kept as written (D-167).
    #[serde(flatten)]
    #[ts(skip)]
    pub extra: transit_version::Extra,
}

/// One vehicle's work: trips in order with the layovers between
/// them. Authored (runcutting), not derived from trip rows. A block
/// may hold trips of service types that never share a day; on each
/// day it is the trips running that day, in this order.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
pub struct Block {
    pub trips: Vec<TripRef>,
    pub layovers: Vec<Layover>,
    /// Fields this build does not know, kept as written (D-167).
    #[serde(flatten)]
    #[ts(skip)]
    pub extra: transit_version::Extra,
}

/// A block's reference to a trip, by id. A block runs a trip whole
/// (as GTFS has it): a frequency trip in a block is every one of its
/// instances, back to back.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
pub struct TripRef {
    pub trip: TripId,
    /// Fields this build does not know, kept as written (D-167).
    #[serde(flatten)]
    #[ts(skip)]
    pub extra: transit_version::Extra,
}

/// Recovery time between two trips of a block.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
pub struct Layover {
    /// Index into the block's trips: the layover follows that trip.
    pub after_trip: usize,
    pub secs: u32,
    /// Where the vehicle waits, if it matters.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at: Option<StopId>,
    /// Fields this build does not know, kept as written (D-167).
    #[serde(flatten)]
    #[ts(skip)]
    pub extra: transit_version::Extra,
}

/// The service-plan frame of a network document. Agencies make a few service
/// changes ("picks") a year, on defined dates. These are the only times that
/// permanent stop/route changes are allowed. Each pick is one of these
/// documents. It is effective from its service change date until the next
/// pick supersedes it. A document without plan metadata is unversioned, such
/// as a fresh GTFS import.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
pub struct PlanMeta {
        /// The plan's identity, which is the sync key (LWW per plan). Empty on
        /// unversioned documents.
    #[serde(default)]
    pub id: String,
    /// Display name ("Fall 2026 pick").
    #[serde(default)]
    pub name: String,
    /// The service change date this plan takes effect, "YYYY-MM-DD".
    /// None on unversioned documents; a plan in a chain must have one.
    #[serde(default, with = "iso_opt_date", skip_serializing_if = "Option::is_none")]
    #[schemars(with = "Option<chrono::NaiveDate>")]
    #[ts(type = "string | null")]
    pub effective_from: Option<NaiveDate>,
    /// Fields this build does not know, kept as written (D-167).
    #[serde(flatten)]
    #[ts(skip)]
    pub extra: transit_version::Extra,
}

/// The authored network document. BTreeMap-keyed so serialization is
/// canonical and per-entity sync falls out later.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
pub struct Network {
    /// `"network"`: the document names its kind (D-115).
    #[serde(default)]
    #[ts(type = "\"network\"")]
    pub kind: crate::version::NetworkKind,
    /// The document's version (D-62): written on every write; a
    /// document without one is version 1 (`version.rs`).
    #[serde(default = "crate::version::NetworkVersion::untagged")]
    #[ts(type = "number")]
    pub schema_version: crate::version::NetworkVersion,
    pub name: String,
    /// The service plan this document is: the versioning frame for
    /// service changes. Defaults to unversioned.
    #[serde(default)]
    pub plan: PlanMeta,
    /// The service-day time anchor every offset in the network is
    /// measured from. Defaults to the noon-minus-12h convention.
    #[serde(default)]
    pub anchor: Anchor,
    pub stops: BTreeMap<StopId, Stop>,
    pub routes: BTreeMap<RouteId, Route>,
    pub patterns: BTreeMap<PatternId, Pattern>,
    /// The lines on the ground patterns run, by id.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub alignments: BTreeMap<AlignmentId, Alignment>,
    /// The service types: what planners design and link to days.
    pub service_types: BTreeMap<ServiceTypeId, ServiceType>,
    /// Every pattern's trips, grouped by (pattern, service type).
    pub pattern_trips: BTreeMap<PatternTripsId, PatternTrips>,
    pub blocks: BTreeMap<BlockId, Block>,
    /// Who runs the service, by id.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    #[ts(optional, as = "Option<BTreeMap<OperatorId, Operator>>")]
    pub operators: BTreeMap<OperatorId, Operator>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub publisher: Option<Publisher>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[ts(optional, as = "Option<Vec<Attribution>>")]
    pub attributions: Vec<Attribution>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[ts(optional, as = "Option<Vec<Translation>>")]
    pub translations: Vec<Translation>,
    /// Entrances, nodes, and boarding areas, by id (ids are unique
    /// across these and the stops).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    #[ts(optional, as = "Option<BTreeMap<StopId, StationPart>>")]
    pub station_parts: BTreeMap<StopId, StationPart>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    #[ts(optional, as = "Option<BTreeMap<LevelId, Level>>")]
    pub levels: BTreeMap<LevelId, Level>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    #[ts(optional, as = "Option<BTreeMap<PathwayId, Pathway>>")]
    pub pathways: BTreeMap<PathwayId, Pathway>,
    /// Transfer rules, in published order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[ts(optional, as = "Option<Vec<TransferRule>>")]
    pub transfers: Vec<TransferRule>,
    #[serde(default, skip_serializing_if = "Fares::is_empty")]
    #[ts(optional, as = "Option<Fares>")]
    pub fares: Fares,
    #[serde(default, skip_serializing_if = "DemandResponsive::is_empty")]
    #[ts(optional, as = "Option<DemandResponsive>")]
    pub demand: DemandResponsive,
    /// Fields this build does not know, kept as written (D-167).
    #[serde(flatten)]
    #[ts(skip)]
    pub extra: transit_version::Extra,
}

impl Network {
    /// Every trip, with the id of the pattern trips holding it.
    pub fn trips(&self) -> impl Iterator<Item = (&str, &PatternTrips, &Trip)> {
        self.pattern_trips
            .iter()
            .flat_map(|(key, pt)| pt.trips.iter().map(move |t| (key.as_str(), pt, t)))
    }

    /// The trip with this id, with its pattern trips id.
    pub fn trip(&self, id: &str) -> Option<(&str, &PatternTrips, &Trip)> {
        self.trips().find(|(_, _, t)| t.id == id)
    }

    /// Validate: every invariant the model can carry. `Ok` means
    /// materialization is guaranteed to succeed for every date.
    /// `validation_issues` returns the full list when it's wanted.
    pub fn validate(&self) -> Result<(), NetworkError> {
        let issues = crate::validate::validation_issues(self);
        if issues.is_empty() {
            Ok(())
        } else {
            Err(issues[0].clone())
        }
    }
}
