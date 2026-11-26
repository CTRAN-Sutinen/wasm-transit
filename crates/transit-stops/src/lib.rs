//! The stops document. A stop's life is its own, and service plans only read
//! it. A closure reaches a pick only as the planner's choice that service
//! stops visiting the stop, and a pick that visits a stop not in service gets
//! a warning, never a refusal.
//!
//! The stored form is plain JSON: one record per stop, each a list of dated
//! events. Nothing in the document means "today". A stop on a date is its
//! events applied in order up to that date. A correction edits the wrong
//! event, and a real change adds one, so a survey fix differs from a move.
//! GeoJSON has one point and no time for each feature, so a record would go
//! stale after a dated change or split into a feature for each period.
//!
//! Readers ask this library (the stop on a date, [`Stops::served`]), never
//! the raw event list. So a finer grain than a service day changes
//! these functions and not their callers.
//!
//! Every write identifies before it checks ([`Stops::identified`],
//! [`Stops::checked`]). A passing document is canonical, so its hash does
//! not depend on key order. Fields this repo does not know are kept through
//! a read and a write. The surveyor is the signed-in writer, set by
//! the write path ([`Stops::attribute_surveys`]), never a name the page sent.
//! Attachments are named by sha256 hash and media type, never a
//! URL.
//!
//! The site survey is C-TRAN's Service Stop
//! Worksheet. It is part of the stop's details. A core question ([`SITE`])
//! takes only an answer of its kind, and the agency's own questions are
//! kept and not checked. GTFS has no place for the answers, so export and
//! import leave them alone.
//!
//! This crate depends on transit-network only for [`stop_record`] and on
//! transit-requests only for [`request_rule`]. The network does not depend
//! on stops. The stops document sits beside the plans, and a fork never
//! copies it.

#![forbid(unsafe_code)]

pub mod request_rule;
pub mod stop_record;
pub mod load;

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use transit_date::Date;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
#[cfg(feature = "bindings")]
use ts_rs::TS;

pub const SCHEMA_VERSION: u32 = 1;

transit_version::kind_tag!(
    StopsKind = "stops"
);

impl transit_version::Versioned for Stops {
    const KIND: &'static str = "stops";
    const READS: u32 = SCHEMA_VERSION;
    fn schema_version(&self) -> u32 {
        self.schema_version.0
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS, schemars::JsonSchema))]
#[cfg_attr(feature = "bindings", ts(export))]
pub struct Stops {
    #[serde(default)]
    #[cfg_attr(feature = "bindings", ts(type = "\"stops\""))]
    pub kind: StopsKind,
    #[serde(default)]
    #[cfg_attr(feature = "bindings", ts(type = "number"))]
    pub schema_version: transit_version::Version<1>,
    /// The key stays the same through a move.
    pub stops: BTreeMap<String, Vec<Event>>,
    /// Fields this build does not know, kept as written.
    #[serde(flatten)]
    #[cfg_attr(feature = "bindings", ts(skip))]
    pub extra: transit_version::Extra,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS, schemars::JsonSchema))]
#[cfg_attr(feature = "bindings", ts(export))]
pub struct Event {
    /// Unique within its stop (D-166). A write gives an event without one an
    /// id ([`Stops::identified`]).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    #[cfg_attr(feature = "bindings", ts(optional, as = "Option<String>"))]
    pub id: String,
    /// "YYYY-MM-DD" in the agency's zone.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub on: Option<String>,
    #[cfg_attr(feature = "bindings", ts(type = "\"planned\" | \"permitted\" | \"built\" | \"in_service\" | \"suspended\" | \"moved\" | \"changed\" | \"retired\""))]
    pub event: EventKind,
    /// The first service date after a temporary `suspended` or `moved`,
    /// when the earlier state returns.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub until: Option<String>,
    /// WGS 84 degrees.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub lat: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub lon: Option<f64>,
    #[serde(flatten)]
    #[cfg_attr(feature = "bindings", ts(flatten))]
    pub details: Details,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[cfg_attr(feature = "bindings", ts(optional, as = "Option<Vec<String>>"))]
    #[cfg_attr(feature = "bindings", schemars(extend("x-references" = "stop")))]
    pub replaces: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub note: Option<String>,
    /// Marks the event as a site survey, dated the day of the survey.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub surveyed_by: Option<String>,
    /// The id of the stop request that this event carries out. Not
    /// checked here, because the requests are their own document.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    #[cfg_attr(feature = "bindings", schemars(extend("x-references" = "stop_request")))]
    pub request: Option<String>,
    /// The id (hash) of the feed that an import recorded this event from.
    /// A person's own event has none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    #[cfg_attr(feature = "bindings", schemars(extend("x-references" = "feed")))]
    pub from_feed: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[cfg_attr(feature = "bindings", ts(optional, as = "Option<Vec<Attachment>>"))]
    pub attachments: Vec<Attachment>,
    #[serde(flatten)]
    #[cfg_attr(feature = "bindings", ts(skip))]
    pub extra: transit_version::Extra,
}

/// The deployment names the bucket, the server hands out the link, and only
/// a signed-in person may read the file.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS, schemars::JsonSchema))]
#[cfg_attr(feature = "bindings", ts(export))]
pub struct Attachment {
    /// "sha256:" and the 64 hex digits of the file.
    #[cfg_attr(feature = "bindings", schemars(extend("x-references" = "attachment")))]
    pub hash: String,
    #[serde(rename = "type")]
    pub media_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub caption: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub taken_on: Option<String>,
    #[serde(flatten)]
    #[cfg_attr(feature = "bindings", ts(skip))]
    pub extra: transit_version::Extra,
}

impl Attachment {
    pub fn well_named(&self) -> bool {
        let hex = self.hash.strip_prefix("sha256:").unwrap_or("");
        hex.len() == 64
            && hex.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            && self.media_type.split_once('/').is_some_and(|(a, b)| !a.is_empty() && !b.is_empty())
    }
}

/// A name this build does not know reads as [`EventKind::Unknown`], and
/// [`Stops::checked`] refuses it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum EventKind {
    Planned,
    Permitted,
    Built,
    #[default]
    InService,
    Suspended,
    Moved,
    Changed,
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

#[cfg(feature = "bindings")]
impl schemars::JsonSchema for EventKind {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "EventKind".into()
    }
    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({ "type": "string", "enum": ["planned", "permitted", "built", "in_service", "suspended", "moved", "changed", "retired"] })
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS, schemars::JsonSchema))]
#[cfg_attr(feature = "bindings", ts(export))]
#[serde(rename_all = "snake_case")]
pub enum StopKind {
    #[default]
    Stop,
    Station,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS, schemars::JsonSchema))]
#[cfg_attr(feature = "bindings", ts(export))]
pub struct Details {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub kind: Option<StopKind>,
    #[cfg_attr(feature = "bindings", schemars(extend("x-references" = "stop")))]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub parent: Option<String>,
    /// The number on the sign. It is not unique.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub code: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub spoken_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub fare_zone: Option<String>,
    /// Absent means not recorded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub wheelchair: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub transfer: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub platform: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub level: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub timezone: Option<String>,
    /// Whether the stop is reached from the street, not only through its
    /// station.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub street_access: Option<bool>,
    /// `true` is there, `false` is checked and not there, and absent is not
    /// recorded. The agency's own names beside [`AMENITIES`] are
    /// kept and not checked.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    #[cfg_attr(feature = "bindings", ts(optional, as = "Option<BTreeMap<String, bool>>"))]
    pub amenities: BTreeMap<String, bool>,
    /// A survey may contradict the agency's other data. It is kept as given
    /// (D-161).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    #[cfg_attr(feature = "bindings", ts(optional, as = "Option<BTreeMap<String, String>>"))]
    pub site: BTreeMap<String, String>,
    /// Ids in other systems, by system or part name (`sign`, `tms`, `gis`).
    /// Kept and not checked.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    #[cfg_attr(feature = "bindings", ts(optional, as = "Option<BTreeMap<String, String>>"))]
    pub refs: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "bindings", derive(TS, schemars::JsonSchema))]
#[cfg_attr(feature = "bindings", ts(export))]
#[serde(tag = "kind", content = "answers", rename_all = "snake_case")]
pub enum Answer {
    OneOf(#[cfg_attr(feature = "bindings", ts(type = "Array<string>"))] &'static [&'static str]),
    Number,
    Text,
}

const YES_NO: &[&str] = &["yes", "no"];

/// The core site questions, in the order of C-TRAN's Service Stop Worksheet.
/// The worksheet's stop number, point, lift access, and amenities
/// are fields of [`Details`], not questions. A part's type is an answer
/// here, and its id is in `refs`. What a schedule says (the routes, a time
/// point, the neighboring stops) is never asked.
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

/// The core amenity names. Seven are OpenStreetMap's keys on a
/// `highway=bus_stop`. OpenStreetMap has no stop key for `bicycle_parking`
/// and `boarding_pad`, so they are this document's. The site survey
/// adds `rear_boarding_pad`, `seat` (a seat that is not a bench), and
/// `post`.
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS, schemars::JsonSchema))]
#[cfg_attr(feature = "bindings", ts(export))]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Planned,
    Permitted,
    Built,
    InService,
    Suspended,
    Retired,
}

impl Status {
    pub const ALL: [Status; 6] = [Status::Planned, Status::Permitted, Status::Built, Status::InService, Status::Suspended, Status::Retired];

    pub fn as_str(self) -> &'static str {
        match self {
            Status::Planned => "planned",
            Status::Permitted => "permitted",
            Status::Built => "built",
            Status::InService => "in_service",
            Status::Suspended => "suspended",
            Status::Retired => "retired",
        }
    }

    /// A suspension counts as in service.
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS, schemars::JsonSchema))]
#[cfg_attr(feature = "bindings", ts(export))]
pub struct StopOn {
    pub stop_id: String,
    pub status: Status,
    /// None for a stop in service from before any record.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub since: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub until: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub note: Option<String>,
    /// None if the stop was never surveyed, or the survey was undated.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub surveyed_on: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub surveyed_by: Option<String>,
    pub lat: f64,
    pub lon: f64,
    #[serde(flatten)]
    #[cfg_attr(feature = "bindings", ts(flatten))]
    pub details: Details,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS, schemars::JsonSchema))]
#[cfg_attr(feature = "bindings", ts(export))]
pub struct NearStop {
    pub distance_m: f64,
    #[serde(flatten)]
    #[cfg_attr(feature = "bindings", ts(flatten))]
    pub stop: StopOn,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS, schemars::JsonSchema))]
#[cfg_attr(feature = "bindings", ts(export))]
pub struct StatusCount {
    pub status: Status,
    pub stops: u32,
}

/// `event` is the event's index in the stop's list, none for the stop as a
/// whole.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS, schemars::JsonSchema))]
#[cfg_attr(feature = "bindings", ts(export))]
pub struct Refusal {
    pub stop: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub event: Option<u32>,
    pub problem: Problem,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS, schemars::JsonSchema))]
#[cfg_attr(feature = "bindings", ts(export))]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Problem {
    NoEvents,
    UnknownEvent { event: String },
    BadDate { date: String },
    NoDate,
    OutOfOrder,
    FirstEvent,
    AfterRetired,
    StageBack,
    NotInService,
    AlreadyInService,
    UntilNotAfter,
    Misplaced { field: String },
    NoPoint,
    BadPoint,
    BadAnswer { key: String, value: String },
    BadAttachment { hash: String },
    UnknownStop { stop: String },
    ReplacesItself,
    DuplicateEventId { id: String },
    StopExists,
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
            Problem::BadAttachment { hash } => format!("names an attachment {hash:?} that is not a sha256 hash with a media type"),
            Problem::BadAnswer { key, value } => format!("answers {key} with {value:?}, which it does not take"),
            Problem::UnknownStop { stop } => format!("replaces {stop:?}, which is not in the document"),
            Problem::ReplacesItself => "replaces itself".into(),
            Problem::DuplicateEventId { id } => format!("has the id {id:?}, which an event above it has"),
            Problem::StopExists => "is new, but the document already holds a stop with its id".into(),
        };
        match self.event {
            Some(i) => write!(f, "stop {:?} event {i} {what}", self.stop),
            None => write!(f, "stop {:?} {what}", self.stop),
        }
    }
}

fn active(e: &Event, date: Date) -> bool {
    let from = match &e.on {
        Some(on) => Date::parse(on).ok().is_some_and(|on| on <= date),
        None => true,
    };
    let until = match &e.until {
        Some(u) => Date::parse(u).ok().is_some_and(|u| date < u),
        None => true,
    };
    from && until
}

fn fold(id: &str, events: &[Event], date: Date) -> Option<StopOn> {
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
    if let Some(s) = state.as_mut() {
        if let Some(until) = s.until.as_deref().and_then(|d| Date::parse(d).ok()) {
            let sooner = events
                .iter()
                .filter(|e| e.event.stage().is_some() || matches!(e.event, EventKind::Suspended | EventKind::Retired))
                .filter_map(|e| e.on.as_deref().and_then(|d| Date::parse(d).ok()))
                .filter(|&on| on > date && on < until)
                .min();
            if let Some(on) = sooner {
                s.until = Some(on.to_string());
            }
        }
    }
    state
}

impl Stops {
    pub fn empty() -> Self {
        Stops::default()
    }

    pub fn stop_on(&self, id: &str, date: Date) -> Option<StopOn> {
        fold(id, self.stops.get(id)?, date)
    }

    pub fn on_date(&self, date: Date) -> Vec<StopOn> {
        self.stops.iter().filter_map(|(id, events)| fold(id, events, date)).collect()
    }

    pub fn near(&self, date: Date, lat: f64, lon: f64, limit: usize, radius_m: Option<f64>, statuses: Option<&[Status]>) -> Vec<NearStop> {
        let mut out: Vec<NearStop> = self
            .on_date(date)
            .into_iter()
            .filter(|s| statuses.is_none_or(|w| w.contains(&s.status)))
            .map(|s| NearStop { distance_m: transit_geo::distance_m(lat, lon, s.lat, s.lon), stop: s })
            .filter(|n| radius_m.is_none_or(|r| n.distance_m <= r))
            .collect();
        out.sort_by(|a, b| a.distance_m.total_cmp(&b.distance_m).then_with(|| a.stop.stop_id.cmp(&b.stop.stop_id)));
        out.truncate(limit);
        out
    }

    /// `_secs` is not read at a service day's grain. It stays so
    /// that a finer grain changes this function and not its callers.
    pub fn served(&self, id: &str, date: Date, _secs: u32) -> bool {
        self.stop_on(id, date).is_some_and(|s| s.status == Status::InService)
    }

    /// An event added to a held stop is an ordinary write, so the writer
    /// states the intent "new".
    pub fn refuse_taken(&self, id: &str) -> Option<Refusal> {
        self.stops.contains_key(id).then(|| Refusal { stop: id.to_string(), event: None, problem: Problem::StopExists })
    }

    /// A write passes `checked` after it.
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

    pub fn next_events(&self, id: &str, date: Date) -> Vec<EventKind> {
        let on = date.to_string();
        [EventKind::Planned, EventKind::Permitted, EventKind::Built, EventKind::InService, EventKind::Suspended, EventKind::Retired]
            .into_iter()
            .filter(|kind| {
                let event = Event { on: Some(on.clone()), event: kind.clone(), ..Event::default() };
                self.add_event(id, event).checked().is_ok()
            })
            .collect()
    }

    pub fn counts_on(&self, date: Date) -> Vec<StatusCount> {
        let mut counts: BTreeMap<Status, u32> = BTreeMap::new();
        for s in self.on_date(date) {
            *counts.entry(s.status).or_default() += 1;
        }
        counts.into_iter().map(|(status, stops)| StatusCount { status, stops }).collect()
    }

    /// A correction edits the wrong event, and the store keeps the earlier
    /// version. A write passes `checked` after it.
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

    /// Another writer's event added above does not change which event is
    /// edited (D-166).
    pub fn correct_event_by_id(&self, id: &str, event_id: &str, event: Option<Event>) -> Option<Stops> {
        let index = self.stops.get(id)?.iter().position(|e| e.id == event_id)?;
        let event = event.map(|e| Event { id: event_id.to_string(), ..e });
        self.correct_event(id, index, event)
    }

    /// The id derives from the stop and the content, so an import of the
    /// same feed gives the same ids (D-166).
    pub fn identified(mut self) -> Stops {
        use sha2::{Digest, Sha256};
        for (id, events) in self.stops.iter_mut() {
            let mut taken: BTreeSet<String> = events.iter().filter(|e| !e.id.is_empty()).map(|e| e.id.clone()).collect();
            for e in events.iter_mut().filter(|e| e.id.is_empty()) {
                let content = serde_json::to_string(&*e).expect("events serialize");
                let digest = Sha256::new().chain_update(id.as_bytes()).chain_update([0]).chain_update(content.as_bytes()).finalize();
                let base: String = digest.iter().take(6).map(|b| format!("{b:02x}")).collect();
                let mut minted = format!("e{base}");
                let mut n = 2;
                while taken.contains(&minted) {
                    minted = format!("e{base}-{n}");
                    n += 1;
                }
                taken.insert(minted.clone());
                e.id = minted;
            }
        }
        self
    }

    /// A client's name is never kept. A survey it sent, or a held one it
    /// rewrote, is the writer's. Attributing twice changes nothing.
    pub fn attribute_surveys(&self, mut next: Stops, by: &str) -> Stops {
        for (id, events) in next.stops.iter_mut() {
            let mut held: Vec<&Event> = self.stops.get(id).map(|v| v.iter().collect()).unwrap_or_default();
            for e in events.iter_mut() {
                if let Some(i) = held.iter().position(|h| *h == e) {
                    held.swap_remove(i);
                } else if e.surveyed_by.is_some() {
                    e.surveyed_by = Some(by.to_string());
                }
            }
        }
        next
    }

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
    let mut last: Option<Date> = None;
    let mut retired = false;
    let mut seen: BTreeSet<&str> = BTreeSet::new();
    for (i, e) in events.iter().enumerate() {
        let mut refuse = |problem| out.push(Refusal { stop: id.into(), event: Some(i as u32), problem });
        if !e.id.is_empty() && !seen.insert(e.id.as_str()) {
            refuse(Problem::DuplicateEventId { id: e.id.clone() });
        }
        if retired {
            refuse(Problem::AfterRetired);
            continue;
        }
        if let EventKind::Unknown(name) = &e.event {
            refuse(Problem::UnknownEvent { event: name.clone() });
            continue;
        }
        let first = i == 0;

        let on = match &e.on {
            Some(s) => match Date::parse(s).ok() {
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
                match Date::parse(u).ok() {
                    None => refuse(Problem::BadDate { date: u.clone() }),
                    Some(u) if on.is_none_or(|on| u <= on) => refuse(Problem::UntilNotAfter),
                    Some(_) => {}
                }
            }
        }

        let begins = e.event.stage().is_some();
        if first && !begins {
            refuse(Problem::FirstEvent);
        }
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
        for a in &e.attachments {
            if !a.well_named() {
                refuse(Problem::BadAttachment { hash: a.hash.clone() });
            } else if let Some(d) = a.taken_on.as_ref().filter(|d| Date::parse(d).ok().is_none()) {
                refuse(Problem::BadDate { date: d.clone() });
            }
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
