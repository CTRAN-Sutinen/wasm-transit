//! The authored network model — planner intent as data.
//!
//! Reading order: [`Network`] holds everything a planner authors.
//! [`Pattern`] is the stop sequence with timepoints and per-band run
//! times; [`Service`] binds a pattern to a schedule and a calendar;
//! [`Block`] is the runcut. Trips are *not* stored anywhere — they
//! are derived by [`crate::materialize`].
//!
//! Time conventions (see the crate docs): `u32` seconds after the
//! service-day [`Anchor`] of the date in question. `at_timepoints`
//! are cumulative seconds from trip start; a profile applies to trips
//! departing in `[start_secs, end_secs)`.
//!
//! Dwells are not modeled as separate fields: running time between
//! timepoints includes dwell at the intermediate stops, which is how
//! planners author timing anyway. Non-timepoint stops interpolate by
//! distance between their bracketing timepoints.

use ts_rs::TS;
use std::collections::BTreeMap;

use chrono::{Datelike, NaiveDate, Weekday};
use serde::{Deserialize, Serialize};

use crate::error::NetworkError;

/// IDs are opaque strings owned by this model. Type aliases document
/// intent without newtype ceremony (a deliberate non-goal).
pub type StopId = String;
pub type RouteId = String;
pub type PatternId = String;
pub type ServiceTypeId = String;
pub type ScheduleId = String;
pub type BlockId = String;

/// A WGS84 position. Authored geometry (a pattern's alignment) is
/// model data — `lat` first, matching `Stop`'s field order. (GeoJSON
/// views at the boundary reorder to [lon, lat]; the model never
/// stores GeoJSON order.)
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Coord {
    pub lat: f64,
    pub lon: f64,
}

/// Serde helper: dates cross every boundary as `"YYYY-MM-DD"`.
/// (chrono's serde feature is deliberately not enabled workspace-wide.)
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

/// Where a service day's second zero sits. `NoonMinusTwelve` is the
/// GTFS convention, adopted on its merits rather than for
/// compatibility: no DST transition occurs near noon, so offsets
/// stay unambiguous on change days. The authored model makes the
/// convention *explicit* where GTFS leaves it to inference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
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

/// A stop as a planner authors it: one name, one location, optional
/// parent (platform-in-station groupings).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Stop {
    pub name: String,
    pub lat: f64,
    pub lon: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<StopId>,
    /// A designated transfer point: where the network intends riders
    /// to change vehicles. Authored intent, not a badge — the flag
    /// only means something composed with the routes that meet there
    /// (validation requires at least two; the analysis ops measure
    /// the timed connections it enables).
    #[serde(default)]
    pub transfer: bool,
}

/// A route as a planner authors it: one display name (GTFS's
/// short/long split is a serialization concern), a closed mode set.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Route {
    pub name: String,
    pub mode: Mode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
}

/// Vehicle modes as a domain concept, not GTFS's numeric codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    Tram,
    Subway,
    Rail,
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PatternStop {
    pub stop: StopId,
    /// Timepoints carry authored times; non-timepoints interpolate
    /// by distance between their bracketing timepoints.
    pub timepoint: bool,
    /// Meters along the pattern from its first stop. Non-decreasing.
    pub distance_m: f64,
}

/// Run-time profile for trips departing in a time band. Applies to
/// trips departing in `[start_secs, end_secs)`.
///
/// `at_timepoints` is cumulative seconds from trip start at each of
/// the pattern's timepoints, in pattern order. The first entry is 0
/// (the trip *departs* its first stop at `start_secs`); the last is
/// the arrival at the final stop.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct TimingProfile {
    pub start_secs: u32,
    pub end_secs: u32,
    pub at_timepoints: Vec<u32>,
}

impl TimingProfile {
    /// Does this profile govern a trip departing at `t`?
    pub fn covers(&self, t: u32) -> bool {
        self.start_secs <= t && t < self.end_secs
    }
}

/// A route pattern: the stop sequence a planner draws, with its
/// timing. Trips are instances of a pattern, not free-standing rows.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Pattern {
    pub route: RouteId,
    /// Human-facing description, e.g. "5-K Ave to Downtown via MLK".
    pub name: String,
    /// The alignment the pattern follows on the ground, if authored.
    /// Purely geographic: it carries no timing and no stops, and
    /// materialization only uses it for map rendering (shapes), never
    /// for scheduling. `None` (omitted) means "draw stop-to-stop lines".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<Vec<Coord>>,
    pub stops: Vec<PatternStop>,
    pub timing: Vec<TimingProfile>,
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

    /// The timing profile governing a trip departing at `t`.
    pub fn profile_for(&self, t: u32) -> Option<&TimingProfile> {
        self.timing.iter().find(|p| p.covers(t))
    }
}

/// A service type: the service identifier planners design —
/// Weekday, Saturday, Sunday, a holiday — linked to the days it
/// runs by its calendar rule. This, not the day-of-month, is the
/// axis planners design on (GTFS calls it service_id).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ServiceType {
    /// Display name ("Weekday", "Saturday service", …).
    pub name: String,
    /// The days it runs: weekday flags plus date ranges, with
    /// explicit per-date exceptions that always win.
    pub rule: CalendarRule,
}

/// Which days a service type runs on, as authored rules rather than
/// GTFS exception rows: weekday flags plus date ranges, with explicit
/// per-date exceptions that always win.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct DateRange {
    #[serde(with = "iso_date")]
    #[ts(type = "string")]
    pub start: NaiveDate,
    #[serde(with = "iso_date")]
    #[ts(type = "string")]
    pub end: NaiveDate,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct CalendarException {
    #[serde(with = "iso_date")]
    #[ts(type = "string")]
    pub date: NaiveDate,
    pub state: DayState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
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
/// explicitly `Added` (special-event service) — see `runs_on`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
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

/// When trips of a service run. A planner authors either headway
/// bands ("15-minute peak service") or exact departure offsets.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Schedule {
    /// Trips depart at `start_secs + k * headway_secs` while the
    /// departure is `< end_secs` (end-exclusive: the band boundary
    /// belongs to the next band, if any).
    HeadwayBands { bands: Vec<HeadwayBand> },
    /// Explicit trip start offsets from the service-day anchor,
    /// sorted, unique.
    ExactTimes { starts: Vec<u32> },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct HeadwayBand {
    pub start_secs: u32,
    pub end_secs: u32,
    pub headway_secs: u32,
}

/// One pattern's trips under one service type: this pattern, on this
/// service, at these times. Everything RAPTOR and dispatch consume
/// derives from these. (The name matters: a *service* is the
/// service type — the day-level design; this is its per-pattern
/// schedule.)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PatternSchedule {
    pub pattern: PatternId,
    pub service_type: ServiceTypeId,
    pub schedule: Schedule,
}

/// One vehicle's work: trips in order with the layovers between
/// them. Authored (runcutting), not derived from trip rows.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Block {
    pub trips: Vec<TripRef>,
    pub layovers: Vec<Layover>,
}

/// A block's reference to a trip — trips being derived values, the
/// (schedule, start) pair *is* the trip's identity.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct TripRef {
    pub schedule: ScheduleId,
    pub start_secs: u32,
}

/// Recovery time between two trips of a block.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Layover {
    /// Index into the block's trips: the layover follows that trip.
    pub after_trip: usize,
    pub secs: u32,
    /// Where the vehicle waits, if it matters.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at: Option<StopId>,
}

/// The service-plan frame of a network document. Agencies implement
/// a few service changes ("picks") a year at defined dates — the
/// only times permanent stop/route changes are allowed — and each
/// pick is one of these documents, effective from its service
/// change date until superseded by the next. A document without
/// plan metadata is unversioned (e.g. a fresh GTFS import).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PlanMeta {
    /// The plan's identity — the sync key (LWW per plan). Empty on
    /// unversioned documents.
    #[serde(default)]
    pub id: String,
    /// Display name ("Fall 2026 pick").
    #[serde(default)]
    pub name: String,
    /// The service change date this plan takes effect, "YYYY-MM-DD".
    /// None on unversioned documents; a plan in a chain must have one.
    #[serde(default, with = "iso_opt_date", skip_serializing_if = "Option::is_none")]
    #[ts(type = "string | null")]
    pub effective_from: Option<NaiveDate>,
}

/// The authored network document. BTreeMap-keyed so serialization is
/// canonical and per-entity sync falls out later.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Network {
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
    /// The service types: what planners design and link to days.
    pub service_types: BTreeMap<ServiceTypeId, ServiceType>,
    /// The schedules: one pattern's trips under one service type.
    pub schedules: BTreeMap<ScheduleId, PatternSchedule>,
    pub blocks: BTreeMap<BlockId, Block>,
}

impl Network {
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
