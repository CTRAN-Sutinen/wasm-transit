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
//! know (another tool's field, the agency's own amenity names) is kept
//! through a read and a write.
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
}

/// The core amenity names (D-154). Seven are OpenStreetMap's keys on a
/// bus stop (`highway=bus_stop`): `shelter`, `bench`, `bin` (a garbage
/// can), `lit` (lighting), `departures_board` (a printed schedule),
/// `passenger_information_display` (a real-time sign), and
/// `tactile_paving`. OpenStreetMap has no key on a stop for the other
/// two, so they are this document's: `bicycle_parking` (a bike rack,
/// OpenStreetMap's name for one standing on its own) and
/// `boarding_pad`.
pub const AMENITIES: [&str; 9] = [
    "shelter",
    "bench",
    "bin",
    "lit",
    "departures_board",
    "passenger_information_display",
    "bicycle_parking",
    "boarding_pad",
    "tactile_paving",
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
    }
}

/// Where a stop stands on a date.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS, schemars::JsonSchema)]
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
    pub lat: f64,
    pub lon: f64,
    #[serde(flatten)]
    #[ts(flatten)]
    pub details: Details,
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
            state = Some(StopOn { stop_id: id.to_string(), status: Status::InService, lat: f64::NAN, lon: f64::NAN, details: Details::default() });
        }
        let Some(s) = state.as_mut() else { continue };
        if let Some(status) = status {
            s.status = status;
        }
        if let (Some(lat), Some(lon)) = (e.lat, e.lon) {
            s.lat = lat;
            s.lon = lon;
        }
        s.details.apply(&e.details);
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
