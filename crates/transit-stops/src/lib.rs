//! The stops document (`docs/units/stop-lifecycle.md`; D-153, D-154).
//!
//! Every stop is a list of dated events, oldest first — `opened`,
//! `closed`, `reopened`, `moved`, `changed`, `proposed`, `retired` —
//! and nothing in the document means "today". A stop on a date is its
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
    /// zone). Only a stop's first event, an `opened`, may have none:
    /// an imported stop whose opening no feed records.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub on: Option<String>,
    #[ts(type = "\"opened\" | \"closed\" | \"reopened\" | \"moved\" | \"changed\" | \"proposed\" | \"retired\"")]
    pub event: EventKind,
    /// A `closed` or `moved` that is temporary: the first service date
    /// after it, when the earlier state returns.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub until: Option<String>,
    /// The draft plan a `proposed` stop belongs to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub plan: Option<String>,
    /// The stop's point (WGS 84 degrees): set by `opened`, `proposed`,
    /// and `moved`; required of a stop's first event and of a move.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub lat: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub lon: Option<f64>,
    /// The details it sets (`opened`, `proposed`, `changed`): only
    /// those listed change.
    #[serde(flatten)]
    #[ts(flatten)]
    pub details: Details,
    /// The stops a consolidation or split succeeds (`opened`,
    /// `proposed`).
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
    /// The stop starts: a stop's first event, with its point.
    #[default]
    Opened,
    /// Not served; with `until`, it reopens by itself on that date.
    Closed,
    /// Served again after a closure with no `until`.
    Reopened,
    /// A new point; with `until`, temporary.
    Moved,
    /// Some details change; only those listed.
    Changed,
    /// A draft plan adds the stop (a stop's first event, naming the
    /// plan).
    Proposed,
    /// The stop ends for good; nothing follows (the tombstone).
    Retired,
    Unknown(String),
}

impl EventKind {
    const NAMES: [(&'static str, EventKind); 7] = [
        ("opened", EventKind::Opened),
        ("closed", EventKind::Closed),
        ("reopened", EventKind::Reopened),
        ("moved", EventKind::Moved),
        ("changed", EventKind::Changed),
        ("proposed", EventKind::Proposed),
        ("retired", EventKind::Retired),
    ];

    pub fn as_str(&self) -> &str {
        match self {
            EventKind::Unknown(name) => name,
            known => Self::NAMES.iter().find(|(_, k)| k == known).map(|(n, _)| *n).unwrap_or(""),
        }
    }

    fn sets_point(&self) -> bool {
        matches!(self, EventKind::Opened | EventKind::Proposed | EventKind::Moved)
    }

    fn sets_details(&self) -> bool {
        matches!(self, EventKind::Opened | EventKind::Proposed | EventKind::Changed)
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
        schemars::json_schema!({ "type": "string", "enum": ["opened", "closed", "reopened", "moved", "changed", "proposed", "retired"] })
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
    /// A draft plan's stop: not built, not served.
    Proposed,
    Open,
    Closed,
    Retired,
}

/// A stop as its events make it on one date.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
pub struct StopOn {
    pub stop_id: String,
    pub status: Status,
    pub lat: f64,
    pub lon: f64,
    /// The plan that proposed the stop, when one did.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub plan: Option<String>,
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
    /// No `on`: only a stop's first event, an `opened`, may go
    /// undated.
    NoDate,
    /// Dated before the event above it.
    OutOfOrder,
    /// A stop begins with `opened` or `proposed`, and a proposal is
    /// only ever a stop's first event.
    FirstEvent,
    /// An event after `retired`.
    AfterRetired,
    /// A `reopened` when the stop is not closed on its date.
    NothingClosed,
    /// A `proposed` naming no plan.
    NoPlan,
    /// An `until` on or before the event's own date.
    UntilNotAfter,
    /// A field its event type does not set (`until`, `plan`, `point`,
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
            Problem::NoDate => "has no date (only a first opened may go undated)".into(),
            Problem::OutOfOrder => "is dated before the event above it".into(),
            Problem::FirstEvent => "must be opened or proposed first, and proposed only first".into(),
            Problem::AfterRetired => "follows the stop's retirement".into(),
            Problem::NothingClosed => "reopens a stop that is not closed".into(),
            Problem::NoPlan => "proposes the stop for no plan".into(),
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

/// A stop's events applied in order up to `date`.
fn fold(id: &str, events: &[Event], date: NaiveDate) -> Option<StopOn> {
    let mut state: Option<StopOn> = None;
    for e in events.iter().filter(|e| active(e, date)) {
        let status = match e.event {
            EventKind::Opened | EventKind::Reopened => Some(Status::Open),
            EventKind::Proposed => Some(Status::Proposed),
            EventKind::Closed => Some(Status::Closed),
            EventKind::Retired => Some(Status::Retired),
            EventKind::Moved | EventKind::Changed | EventKind::Unknown(_) => None,
        };
        if matches!(e.event, EventKind::Opened | EventKind::Proposed) && state.is_none() {
            state = Some(StopOn {
                stop_id: id.to_string(),
                status: Status::Open,
                lat: f64::NAN,
                lon: f64::NAN,
                plan: None,
                details: Details::default(),
            });
        }
        let Some(s) = state.as_mut() else { continue };
        if let Some(status) = status {
            s.status = status;
        }
        if let (Some(lat), Some(lon)) = (e.lat, e.lon) {
            s.lat = lat;
            s.lon = lon;
        }
        if e.plan.is_some() {
            s.plan.clone_from(&e.plan);
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
    /// day's midnight, is served there: the stop is open. At a service
    /// day's grain (D-148) the time is not read; it is the question's
    /// so a finer grain changes this function and not its callers.
    pub fn served(&self, id: &str, date: NaiveDate, _secs: u32) -> bool {
        self.stop_on(id, date).is_some_and(|s| s.status == Status::Open)
    }

    /// The document with `event` added to stop `id` in its place by
    /// date: after every event dated on or before it (an undated opening
    /// first), before every later one; every other event keeps its
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
        match on {
            Some(d) => {
                if last.is_some_and(|l| d < l) {
                    refuse(Problem::OutOfOrder);
                }
                last = Some(d);
            }
            None if first && e.event == EventKind::Opened => {}
            None => refuse(Problem::NoDate),
        }
        if let Some(u) = &e.until {
            if !matches!(e.event, EventKind::Closed | EventKind::Moved) {
                refuse(Problem::Misplaced { field: "until".into() });
            } else {
                match parse_date(u) {
                    None => refuse(Problem::BadDate { date: u.clone() }),
                    Some(u) if on.is_none_or(|on| u <= on) => refuse(Problem::UntilNotAfter),
                    Some(_) => {}
                }
            }
        }

        // where it stands in the stop's life
        let begins = matches!(e.event, EventKind::Opened | EventKind::Proposed);
        if (first && !begins) || (!first && e.event == EventKind::Proposed) {
            refuse(Problem::FirstEvent);
        }
        match (&e.event, &e.plan) {
            (EventKind::Proposed, None) => refuse(Problem::NoPlan),
            (EventKind::Proposed, Some(_)) | (_, None) => {}
            (_, Some(_)) => refuse(Problem::Misplaced { field: "plan".into() }),
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

        // a reopening ends a closure in force on its date
        if e.event == EventKind::Reopened {
            if let Some(d) = on {
                if fold(id, &events[..i], d).map(|s| s.status) != Some(Status::Closed) {
                    refuse(Problem::NothingClosed);
                }
            }
        }
        if e.event == EventKind::Retired {
            retired = true;
        }
    }
    out
}
