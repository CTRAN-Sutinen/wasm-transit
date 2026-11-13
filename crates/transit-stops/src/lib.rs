//! The stops document (D-153, D-154, D-157).
//!
//! A stop has a life of its own: planned, permitted, built, in service,
//! suspended for a while, retired — on a timeline set by permits and
//! construction, not by service plans, which only read it (D-157).
//! Every stop is a list of dated events, oldest first — `planned`,
//! `permitted`, `built`, `in_service`, `suspended`, `moved`, `changed`,
//! `retired` — and nothing in the document means "today". A stop on a date is its
//! events applied in order up to that date ([`Stops::stop_on`]); a
//! temporary event (`until`) applies only while it lasts, so the state
//! before it returns on its `until`. Every reader asks this library —
//! the stop on a date, and whether a trip serves it ([`Stops::served`])
//! — never the raw list, so a finer grain than a service day (D-148)
//! changes these functions and not their callers.
//!
//! A correction edits the event that was wrong (the store keeps the
//! earlier version of the document); a real change adds an event.
//!
//! [`Stops::checked`] is what a write must pass: each refusal names
//! its stop, its event, and its problem. A document that passes is
//! canonical as it stands — stops by id, each stop's events in the
//! date order its writer gave — so its bytes, and so its hash, do not
//! depend on how a writer ordered its keys. What this repo does not
//! know (another tool's field, the agency's own amenity names and site
//! questions) is kept through a read and a write.
//!
//! A stop's site survey (D-158) is details like any other: the answers
//! to the site questions ([`SITE`]), its references to other systems
//! (D-160), and on the event that records it, who surveyed.
//!
//! The stops document sits beside the plans: every plan refers to a
//! stop by its id, and a fork never copies it (D-147). This crate
//! depends on no other model crate.

#![forbid(unsafe_code)]

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use chrono::NaiveDate;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;
use ts_rs::TS;

/// The version this build reads and writes.
pub const SCHEMA_VERSION: u32 = 1;

transit_version::kind_tag!(
    /// The stops document's kind tag: always `"stops"`.
    StopsKind = "stops"
);

impl transit_version::Versioned for Stops {
    const KIND: &'static str = "stops";
    const READS: u32 = SCHEMA_VERSION;
    fn schema_version(&self) -> u32 {
        self.schema_version.0
    }
}

/// Every stop, by its lasting id, each a list of dated events.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
pub struct Stops {
    /// The kind tag and version (D-115); a copy without them is this
    /// kind, version 1.
    #[serde(default)]
    #[ts(type = "\"stops\"")]
    pub kind: StopsKind,
    #[serde(default)]
    #[ts(type = "number")]
    pub schema_version: transit_version::Version<1>,
    /// Each stop's events, oldest first. The key is the stop's lasting
    /// id: it stays the same through a move (D-145).
    pub stops: BTreeMap<String, Vec<Event>>,
    /// Fields this repo does not know, kept as written.
    #[serde(flatten)]
    #[ts(skip)]
    pub extra: BTreeMap<String, Value>,
}

/// One thing that happened, or will happen, to a stop.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
pub struct Event {
    /// The service date it takes effect ("YYYY-MM-DD", the agency's
    /// zone). Only a stop's first event, an `in_service`, may have
    /// none: an imported stop whose start no feed records.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub on: Option<String>,
    #[ts(type = "\"planned\" | \"permitted\" | \"built\" | \"in_service\" | \"suspended\" | \"moved\" | \"changed\" | \"retired\"")]
    pub event: EventKind,
    /// A `suspended` or `moved` that is temporary: the first service
    /// date after it, when the earlier state returns.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub until: Option<String>,
    /// The stop's point (WGS 84 degrees): set by a stage event and by
    /// `moved`; required of a stop's first event and of a move.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub lat: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub lon: Option<f64>,
    /// The details it sets (a stage event, `changed`): only those
    /// listed change.
    #[serde(flatten)]
    #[ts(flatten)]
    pub details: Details,
    /// The stops a consolidation or split succeeds (a stage event).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[ts(optional, as = "Option<Vec<String>>")]
    pub replaces: Vec<String>,
    /// A person's note on the event ("water main work").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub note: Option<String>,
    /// Who surveyed the stop, as they gave their name: the event records
    /// a site survey (D-158), dated the day of the survey. Only on an
    /// event that sets details (a stage, `changed`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub surveyed_by: Option<String>,
    /// Fields this repo does not know, kept as written.
    #[serde(flatten)]
    #[ts(skip)]
    pub extra: BTreeMap<String, Value>,
}

/// What an event is. A name this build does not know reads as
/// [`EventKind::Unknown`] and is refused by [`Stops::checked`], naming
/// it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum EventKind {
    /// The stop is planned: where it will go.
    Planned,
    /// Its permits are granted.
    Permitted,
    /// It is built, not yet served.
    Built,
    /// In service: served. A stop's first event when it is already
    /// there (an import, a seed); later, a return from a suspension.
    #[default]
    InService,
    /// Out of service for a while — a closure, works, an event; with
    /// `until`, it returns by itself on that date. It is expected to
    /// return, and may not (a long closure is a long suspension).
    Suspended,
    /// A new point; with `until`, temporary.
    Moved,
    /// Some details change; only those listed.
    Changed,
    /// The stop ends for good; nothing follows (the tombstone).
    Retired,
    Unknown(String),
}

impl EventKind {
    const NAMES: [(&'static str, EventKind); 8] = [
        ("planned", EventKind::Planned),
        ("permitted", EventKind::Permitted),
        ("built", EventKind::Built),
        ("in_service", EventKind::InService),
        ("suspended", EventKind::Suspended),
        ("moved", EventKind::Moved),
        ("changed", EventKind::Changed),
        ("retired", EventKind::Retired),
    ];

    pub fn as_str(&self) -> &str {
        match self {
            EventKind::Unknown(name) => name,
            known => Self::NAMES.iter().find(|(_, k)| k == known).map(|(n, _)| *n).unwrap_or(""),
        }
    }

    /// The stages, in order: how far along the stop is (`None` for an
    /// event that is not one).
    fn stage(&self) -> Option<u8> {
        match self {
            EventKind::Planned => Some(0),
            EventKind::Permitted => Some(1),
            EventKind::Built => Some(2),
            EventKind::InService => Some(3),
            _ => None,
        }
    }

    fn sets_point(&self) -> bool {
        self.stage().is_some() || *self == EventKind::Moved
    }

    fn sets_details(&self) -> bool {
        self.stage().is_some() || *self == EventKind::Changed
    }
}

impl Serialize for EventKind {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for EventKind {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let name = String::deserialize(d)?;
        Ok(EventKind::NAMES.iter().find(|(n, _)| *n == name).map(|(_, k)| k.clone()).unwrap_or(EventKind::Unknown(name)))
    }
}

impl schemars::JsonSchema for EventKind {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "EventKind".into()
    }
    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({ "type": "string", "enum": ["planned", "permitted", "built", "in_service", "suspended", "moved", "changed", "retired"] })
    }
}

/// A stop or a station grouping stops.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum StopKind {
    #[default]
    Stop,
    Station,
}

/// What a rider can know about a stop beyond its point. Every detail
/// is optional; an event sets the ones it lists.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
pub struct Details {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub name: Option<String>,
    /// A stop riders board at, or a station grouping stops.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub kind: Option<StopKind>,
    /// The station a stop is part of.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub parent: Option<String>,
    /// The number riders see on the sign — the rider's identifier, not
    /// unique (D-151).
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub fare_zone: Option<String>,
    /// Whether a wheelchair user can board here; absent = not
    /// recorded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub wheelchair: Option<bool>,
    /// A designated transfer point.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub transfer: Option<bool>,
    /// The platform's own identifier ("G", "3").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub platform: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub level: Option<String>,
    /// The stop's own timezone, where it differs from its operator's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub timezone: Option<String>,
    /// A station's stop: reached straight from the street or only
    /// through the station.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub street_access: Option<bool>,
    /// What is at the stop (D-154): `true` there, `false` checked and
    /// not there, absent not recorded. The core names are
    /// OpenStreetMap's bus-stop tags ([`AMENITIES`]); the agency's own
    /// names are kept and not checked.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    #[ts(optional, as = "Option<BTreeMap<String, bool>>")]
    pub amenities: BTreeMap<String, bool>,
    /// The site as surveyed (D-158): each question's answer by name,
    /// absent not recorded. The core questions ([`SITE`]) are C-TRAN's
    /// Service Stop Worksheet's, each answer checked against its kind;
    /// the agency's own questions are kept and not checked. What a
    /// survey says the agency's other data may contradict (D-161) — both
    /// are kept as given.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    #[ts(optional, as = "Option<BTreeMap<String, String>>")]
    pub site: BTreeMap<String, String>,
    /// The stop and its parts in other systems (D-160): each system or
    /// part by name, its id there (`sign` — the Sign ID, `tms`, `hp`,
    /// `gis`, `shelter`, …). Kept and not checked.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    #[ts(optional, as = "Option<BTreeMap<String, String>>")]
    pub refs: BTreeMap<String, String>,
}

/// The kind of answer a site question takes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS, schemars::JsonSchema)]
#[ts(export)]
#[serde(tag = "kind", content = "answers", rename_all = "snake_case")]
pub enum Answer {
    /// One answer from its list.
    OneOf(#[ts(type = "Array<string>")] &'static [&'static str]),
    /// A whole number ("0", "4", "35").
    Number,
    /// Any text.
    Text,
}

const YES_NO: &[&str] = &["yes", "no"];

/// The core site questions (D-158): C-TRAN's Service Stop Worksheet, in
/// its order. What the worksheet asks that the document already holds
/// elsewhere is there: the stop number (`code`), the point, lift access
/// (`wheelchair`), and what is at the stop (the amenities — a front and
/// rear door pad, a bench and a seat, a shelter, a post, a schedule
/// kiosk, a garbage can, lighting); a part's type is a written answer
/// here and its id a reference (`refs`). What a schedule says — the
/// routes, a time point, the previous and next stop — is the schedule's,
/// never asked.
pub const SITE: [(&str, Answer); 32] = [
    ("on_street", Answer::Text),
    ("cross_street", Answer::Text),
    ("city", Answer::Text),
    ("county", Answer::Text),
    ("lanes", Answer::Number),
    ("speed_limit_mph", Answer::Number),
    ("direction", Answer::OneOf(&["northbound", "southbound", "eastbound", "westbound", "clockwise", "counterclockwise"])),
    ("side", Answer::OneOf(&["north", "south", "east", "west"])),
    ("position", Answer::OneOf(&["near_side", "far_side", "opposite", "mid_block"])),
    ("land_use", Answer::OneOf(&["residential", "commercial", "public", "school", "hospital", "mixed"])),
    ("shoulder", Answer::OneOf(&["improved", "unimproved", "none"])),
    ("surface", Answer::OneOf(&["asphalt", "concrete", "brick", "gravel", "dirt"])),
    ("sidewalk", Answer::OneOf(YES_NO)),
    ("sidewalk:surface", Answer::Text),
    ("sidewalk:connection", Answer::OneOf(&["continuous", "one_direction", "isolated", "none"])),
    ("ada", Answer::OneOf(YES_NO)),
    ("curb", Answer::OneOf(YES_NO)),
    ("curb_cut", Answer::OneOf(&["at_all_corners", "at_stop_corners", "driveway_close", "none"])),
    ("street_light", Answer::OneOf(&["at_stop", "within_50ft", "other_source", "none"])),
    ("turnout", Answer::OneOf(&["yes", "no", "right_turn_only"])),
    ("crossing", Answer::OneOf(&["within_200ft", "none"])),
    ("advertising", Answer::OneOf(YES_NO)),
    ("light:source", Answer::OneOf(&["commercial", "solar"])),
    ("light:type", Answer::Text),
    ("post:type", Answer::Text),
    ("sign:type", Answer::Text),
    ("shelter:type", Answer::Text),
    ("kiosk:type", Answer::Text),
    ("bin:type", Answer::Text),
    ("seat:type", Answer::Text),
    ("landmark", Answer::Text),
    ("remarks", Answer::Text),
];

impl Answer {
    fn takes(&self, value: &str) -> bool {
        match self {
            Answer::OneOf(list) => list.contains(&value),
            Answer::Number => !value.is_empty() && value.bytes().all(|b| b.is_ascii_digit()),
            Answer::Text => true,
        }
    }
}

/// The core amenity names (D-154). Seven are OpenStreetMap's keys on a
/// bus stop (`highway=bus_stop`): `shelter`, `bench`, `bin` (a garbage
/// can), `lit` (lighting), `departures_board` (a printed schedule),
/// `passenger_information_display` (a real-time sign), and
/// `tactile_paving`. OpenStreetMap has no key on a stop for the other
/// two, so they are this document's: `bicycle_parking` (a bike rack,
/// OpenStreetMap's name for one standing on its own) and
/// `boarding_pad` (the front door's). The site survey (D-158) adds
/// three of the agency's: `rear_boarding_pad`, `seat` (a seat that is
/// not a bench, such as a Simme Seat), and `post` (the sign's own post).
pub const AMENITIES: [&str; 12] = [
    "shelter",
    "bench",
    "bin",
    "lit",
    "departures_board",
    "passenger_information_display",
    "bicycle_parking",
    "boarding_pad",
    "tactile_paving",
    "rear_boarding_pad",
    "seat",
    "post",
];

impl Details {
    pub fn is_empty(&self) -> bool {
        *self == Details::default()
    }

    /// Each detail `next` lists replaces this one's; amenities merge by
    /// name.
    fn apply(&mut self, next: &Details) {
        fn set<T: Clone>(field: &mut Option<T>, next: &Option<T>) {
            if next.is_some() {
                field.clone_from(next);
            }
        }
        set(&mut self.name, &next.name);
        set(&mut self.kind, &next.kind);
        set(&mut self.parent, &next.parent);
        set(&mut self.code, &next.code);
        set(&mut self.spoken_name, &next.spoken_name);
        set(&mut self.description, &next.description);
        set(&mut self.url, &next.url);
        set(&mut self.fare_zone, &next.fare_zone);
        set(&mut self.wheelchair, &next.wheelchair);
        set(&mut self.transfer, &next.transfer);
        set(&mut self.platform, &next.platform);
        set(&mut self.level, &next.level);
        set(&mut self.timezone, &next.timezone);
        set(&mut self.street_access, &next.street_access);
        self.amenities.extend(next.amenities.iter().map(|(k, v)| (k.clone(), *v)));
        self.site.extend(next.site.iter().map(|(k, v)| (k.clone(), v.clone())));
        self.refs.extend(next.refs.iter().map(|(k, v)| (k.clone(), v.clone())));
    }
}

/// Where a stop stands on a date.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Planned,
    Permitted,
    /// Built, not yet served.
    Built,
    /// Served.
    InService,
    /// Out of service for a while.
    Suspended,
    Retired,
}

impl Status {
    /// How far along the stop is: the stages in order, a suspension
    /// counting as in service.
    fn rank(self) -> u8 {
        match self {
            Status::Planned => 0,
            Status::Permitted => 1,
            Status::Built => 2,
            Status::InService | Status::Suspended => 3,
            Status::Retired => 4,
        }
    }
}

/// A stop as its events make it on one date.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
pub struct StopOn {
    pub stop_id: String,
    pub status: Status,
    /// The date the status began (none for a stop in service from
    /// before any record).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub since: Option<String>,
    /// The date a temporary status ends (a suspension with an end).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub until: Option<String>,
    /// The note on the event that set the status.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub note: Option<String>,
    /// The date and the surveyor of the last survey (D-158); none if the
    /// stop was never surveyed (an undated survey has no date).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub surveyed_on: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub surveyed_by: Option<String>,
    pub lat: f64,
    pub lon: f64,
    #[serde(flatten)]
    #[ts(flatten)]
    pub details: Details,
}

/// How many stops stand at a status on a date.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
pub struct StatusCount {
    pub status: Status,
    pub stops: u32,
}

/// Why a write is refused: the stop, the event (its index in the
/// stop's list; none for the stop as a whole), and the problem.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
pub struct Refusal {
    pub stop: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub event: Option<u32>,
    pub problem: Problem,
}

/// What is wrong.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Problem {
    /// The stop has no events.
    NoEvents,
    /// An event type this build does not know.
    UnknownEvent { event: String },
    /// A date that is not "YYYY-MM-DD".
    BadDate { date: String },
    /// No `on`: only a stop's first event, an `in_service`, may go
    /// undated.
    NoDate,
    /// Dated before the event above it.
    OutOfOrder,
    /// A stop begins at a stage: `planned`, `permitted`, `built`, or
    /// `in_service`.
    FirstEvent,
    /// An event after `retired`.
    AfterRetired,
    /// A `planned`, `permitted`, or `built` when the stop is already at
    /// or past that stage on its date: the stages only go forward.
    StageBack,
    /// A `suspended` when the stop is not in service on its date.
    NotInService,
    /// An `in_service` when the stop is already in service on its date.
    AlreadyInService,
    /// An `until` on or before the event's own date.
    UntilNotAfter,
    /// A field its event type does not set (`until`, `point`,
    /// `details`, `replaces`).
    Misplaced { field: String },
    /// A point missing where one is required, or half a point.
    NoPoint,
    /// A point off the earth (latitude beyond ±90°, longitude beyond
    /// ±180°, or not a number).
    BadPoint,
    /// A core site question (D-158) answered outside its kind: an
    /// answer not on its list, or a count that is not a whole number.
    BadAnswer { key: String, value: String },
    /// A consolidation naming a stop the document does not hold.
    UnknownStop { stop: String },
    /// A consolidation naming the stop itself.
    ReplacesItself,
}

impl fmt::Display for Refusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let what = match &self.problem {
            Problem::NoEvents => "has no events".to_string(),
            Problem::UnknownEvent { event } => format!("is an unknown event {event:?}"),
            Problem::BadDate { date } => format!("has {date:?}, not a YYYY-MM-DD date"),
            Problem::NoDate => "has no date (only a first in_service may go undated)".into(),
            Problem::OutOfOrder => "is dated before the event above it".into(),
            Problem::FirstEvent => "must be a stage first: planned, permitted, built, or in_service".into(),
            Problem::AfterRetired => "follows the stop's retirement".into(),
            Problem::StageBack => "is a stage the stop is already at or past".into(),
            Problem::NotInService => "suspends a stop that is not in service".into(),
            Problem::AlreadyInService => "puts in service a stop already in service".into(),
            Problem::UntilNotAfter => "ends (until) on or before it starts".into(),
            Problem::Misplaced { field } => format!("carries {field}, which its type does not set"),
            Problem::NoPoint => "lacks a whole point (lat and lon)".into(),
            Problem::BadPoint => "has a point off the earth".into(),
            Problem::BadAnswer { key, value } => format!("answers {key} with {value:?}, which it does not take"),
            Problem::UnknownStop { stop } => format!("replaces {stop:?}, which is not in the document"),
            Problem::ReplacesItself => "replaces itself".into(),
        };
        match self.event {
            Some(i) => write!(f, "stop {:?} event {i} {what}", self.stop),
            None => write!(f, "stop {:?} {what}", self.stop),
        }
    }
}

fn parse_date(s: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").ok()
}

/// Whether an event applies on `date`: from its `on` (an undated one
/// always), until its `until`.
fn active(e: &Event, date: NaiveDate) -> bool {
    let from = match &e.on {
        Some(on) => parse_date(on).is_some_and(|on| on <= date),
        None => true,
    };
    let until = match &e.until {
        Some(u) => parse_date(u).is_some_and(|u| date < u),
        None => true,
    };
    from && until
}

/// A stop's events applied in order up to `date`: it is there from its
/// first stage event.
fn fold(id: &str, events: &[Event], date: NaiveDate) -> Option<StopOn> {
    let mut state: Option<StopOn> = None;
    for e in events.iter().filter(|e| active(e, date)) {
        let status = match e.event {
            EventKind::Planned => Some(Status::Planned),
            EventKind::Permitted => Some(Status::Permitted),
            EventKind::Built => Some(Status::Built),
            EventKind::InService => Some(Status::InService),
            EventKind::Suspended => Some(Status::Suspended),
            EventKind::Retired => Some(Status::Retired),
            EventKind::Moved | EventKind::Changed | EventKind::Unknown(_) => None,
        };
        if e.event.stage().is_some() && state.is_none() {
            state = Some(StopOn {
                stop_id: id.to_string(),
                status: Status::InService,
                since: None,
                until: None,
                note: None,
                surveyed_on: None,
                surveyed_by: None,
                lat: f64::NAN,
                lon: f64::NAN,
                details: Details::default(),
            });
        }
        let Some(s) = state.as_mut() else { continue };
        if let Some(status) = status {
            s.status = status;
            s.since.clone_from(&e.on);
            s.until.clone_from(&e.until);
            s.note.clone_from(&e.note);
        }
        if let (Some(lat), Some(lon)) = (e.lat, e.lon) {
            s.lat = lat;
            s.lon = lon;
        }
        s.details.apply(&e.details);
        if e.surveyed_by.is_some() {
            s.surveyed_on.clone_from(&e.on);
            s.surveyed_by.clone_from(&e.surveyed_by);
        }
    }
    state
}

impl Stops {
    /// An empty document.
    pub fn empty() -> Self {
        Stops::default()
    }

    /// The stop as its events make it on `date`, or none before it
    /// exists (or for an id the document does not hold).
    pub fn stop_on(&self, id: &str, date: NaiveDate) -> Option<StopOn> {
        fold(id, self.stops.get(id)?, date)
    }

    /// Every stop that exists on `date`, in id order.
    pub fn on_date(&self, date: NaiveDate) -> Vec<StopOn> {
        self.stops.iter().filter_map(|(id, events)| fold(id, events, date)).collect()
    }

    /// Whether a trip at the stop on `date`, at `secs` since the service
    /// day's midnight, is served there: the stop is in service. At a service
    /// day's grain (D-148) the time is not read; it is the question's
    /// so a finer grain changes this function and not its callers.
    pub fn served(&self, id: &str, date: NaiveDate, _secs: u32) -> bool {
        self.stop_on(id, date).is_some_and(|s| s.status == Status::InService)
    }

    /// The document with `event` added to stop `id` in its place by
    /// date: after every event dated on or before it (an undated first
    /// event first), before every later one; every other event keeps its
    /// order. An id the document does not hold begins a stop. Unchecked:
    /// a write passes `checked`.
    pub fn add_event(&self, id: &str, event: Event) -> Stops {
        let mut next = self.clone();
        let events = next.stops.entry(id.to_string()).or_default();
        let at = match event.on.as_deref() {
            None => events.iter().take_while(|e| e.on.is_none()).count(),
            Some(on) => events.iter().take_while(|e| e.on.as_deref().is_none_or(|d| d <= on)).count(),
        };
        events.insert(at, event);
        next
    }

    /// What stop `id` may become on `date`: the lifecycle events
    /// (`planned`, `permitted`, `built`, `in_service`, `suspended`,
    /// `retired`) a write would accept dated that day, in that order —
    /// each tried as [`Stops::add_event`] and kept where the document
    /// still passes [`Stops::checked`]. A move or a change of details is
    /// always one more choice while the stop is not retired.
    pub fn next_events(&self, id: &str, date: NaiveDate) -> Vec<EventKind> {
        let on = date.format("%Y-%m-%d").to_string();
        [EventKind::Planned, EventKind::Permitted, EventKind::Built, EventKind::InService, EventKind::Suspended, EventKind::Retired]
            .into_iter()
            .filter(|kind| {
                let event = Event { on: Some(on.clone()), event: kind.clone(), ..Event::default() };
                self.add_event(id, event).checked().is_ok()
            })
            .collect()
    }

    /// The stops on `date` counted by status, in status order; only the
    /// statuses some stop has.
    pub fn counts_on(&self, date: NaiveDate) -> Vec<StatusCount> {
        let mut counts: BTreeMap<Status, u32> = BTreeMap::new();
        for s in self.on_date(date) {
            *counts.entry(s.status).or_default() += 1;
        }
        counts.into_iter().map(|(status, stops)| StatusCount { status, stops }).collect()
    }

    /// The document with event `index` of stop `id` corrected — the
    /// wrong event taken out and `event` placed by date
    /// ([`Stops::add_event`]), or with none, taken out. A correction
    /// edits the event that was wrong (D-153); the store keeps the
    /// earlier version. None when the stop or the index is not there.
    /// Unchecked: a write passes `checked`.
    pub fn correct_event(&self, id: &str, index: usize, event: Option<Event>) -> Option<Stops> {
        let mut next = self.clone();
        let events = next.stops.get_mut(id)?;
        if index >= events.len() {
            return None;
        }
        let was = events.remove(index);
        Some(match event {
            Some(e) if e == was => self.clone(),
            Some(e) => next.add_event(id, e),
            None => next,
        })
    }

    /// The document, if a write may store it, or every reason it may
    /// not.
    pub fn checked(self) -> Result<Self, Vec<Refusal>> {
        let ids: BTreeSet<&str> = self.stops.keys().map(String::as_str).collect();
        let refusals: Vec<Refusal> = self.stops.iter().flat_map(|(id, events)| check_stop(id, events, &ids)).collect();
        if refusals.is_empty() {
            Ok(self)
        } else {
            Err(refusals)
        }
    }
}

fn check_stop(id: &str, events: &[Event], ids: &BTreeSet<&str>) -> Vec<Refusal> {
    let mut out = Vec::new();
    if events.is_empty() {
        out.push(Refusal { stop: id.into(), event: None, problem: Problem::NoEvents });
        return out;
    }
    let mut last: Option<NaiveDate> = None;
    let mut retired = false;
    for (i, e) in events.iter().enumerate() {
        let mut refuse = |problem| out.push(Refusal { stop: id.into(), event: Some(i as u32), problem });
        if retired {
            refuse(Problem::AfterRetired);
            continue;
        }
        if let EventKind::Unknown(name) = &e.event {
            refuse(Problem::UnknownEvent { event: name.clone() });
            continue;
        }
        let first = i == 0;

        // the dates
        let on = match &e.on {
            Some(s) => match parse_date(s) {
                Some(d) => Some(d),
                None => {
                    refuse(Problem::BadDate { date: s.clone() });
                    continue;
                }
            },
            None => None,
        };
        let mut in_order = true;
        match on {
            Some(d) => {
                if last.is_some_and(|l| d < l) {
                    refuse(Problem::OutOfOrder);
                    in_order = false;
                }
                last = Some(d);
            }
            None if first && e.event == EventKind::InService => {}
            None => refuse(Problem::NoDate),
        }
        if let Some(u) = &e.until {
            if !matches!(e.event, EventKind::Suspended | EventKind::Moved) {
                refuse(Problem::Misplaced { field: "until".into() });
            } else {
                match parse_date(u) {
                    None => refuse(Problem::BadDate { date: u.clone() }),
                    Some(u) if on.is_none_or(|on| u <= on) => refuse(Problem::UntilNotAfter),
                    Some(_) => {}
                }
            }
        }

        // where it stands in the stop's life: a stage first, the stages
        // forward, a suspension of a stop in service, a return of one
        // that is not
        let begins = e.event.stage().is_some();
        if first && !begins {
            refuse(Problem::FirstEvent);
        }
        // (an event out of date order is refused for that alone)
        if let (false, true, Some(d)) = (first, in_order, on) {
            let now = fold(id, &events[..i], d).map(|s| s.status);
            match (&e.event, now) {
                (EventKind::InService, Some(Status::InService)) => refuse(Problem::AlreadyInService),
                (EventKind::InService, _) => {}
                (EventKind::Suspended, Some(Status::InService)) => {}
                (EventKind::Suspended, _) => refuse(Problem::NotInService),
                (kind, Some(now)) if kind.stage().is_some_and(|k| now.rank() >= k) => refuse(Problem::StageBack),
                _ => {}
            }
        }

        // what it sets
        match (e.lat, e.lon) {
            (Some(lat), Some(lon)) => {
                if !e.event.sets_point() {
                    refuse(Problem::Misplaced { field: "point".into() });
                } else if !(lat.is_finite() && lon.is_finite() && lat.abs() <= 90.0 && lon.abs() <= 180.0) {
                    refuse(Problem::BadPoint);
                }
            }
            (None, None) => {
                if first || e.event == EventKind::Moved {
                    refuse(Problem::NoPoint);
                }
            }
            _ => refuse(Problem::NoPoint),
        }
        if !e.details.is_empty() && !e.event.sets_details() {
            refuse(Problem::Misplaced { field: "details".into() });
        }
        if e.surveyed_by.is_some() && !e.event.sets_details() {
            refuse(Problem::Misplaced { field: "surveyed_by".into() });
        }
        for (key, value) in &e.details.site {
            if SITE.iter().any(|(k, a)| k == key && !a.takes(value)) {
                refuse(Problem::BadAnswer { key: key.clone(), value: value.clone() });
            }
        }
        if !e.replaces.is_empty() {
            if !begins {
                refuse(Problem::Misplaced { field: "replaces".into() });
            }
            for r in e.replaces.iter().filter(|_| begins) {
                if r == id {
                    refuse(Problem::ReplacesItself);
                } else if !ids.contains(r.as_str()) {
                    refuse(Problem::UnknownStop { stop: r.clone() });
                }
            }
        }

        if e.event == EventKind::Retired {
            retired = true;
        }
    }
    out
}
