//! The stop requests document (D-159).
//!
//! The planning team's submissions for work at a stop — C-TRAN's Bus
//! Stop Sign & Shelter Installation Worksheet: a new stop, a reinstall,
//! a relocation or removal, a repair or improvement, a temporary stop.
//! A request is not a work order: it is the agency's own record, and it
//! tracks the work order the agency's work order system opens for it
//! (its system, id, link, and status as last read). Each request is a
//! record by id, changed by replacing it whole ([`StopRequests::put`]);
//! nothing is deleted — a request that will not go ahead is
//! `cancelled`, the tombstone.
//!
//! A request concerns one stop, by the stops document's lasting id
//! (a new stop's id once it is planned); the stop's own life stays the
//! stops document's (D-157), whose `built`, `moved`, or `retired` event
//! names the request it came from. A new stop's intended routes and
//! direction are the request's: no schedule serves it yet.
//!
//! [`StopRequests::checked`] is what a write must pass, refusing only
//! what makes a request wrong (D-152), each refusal naming its request.
//! A document that passes is canonical as it stands — requests by id —
//! and what this repo does not know is kept through a read and a write.
//! This crate depends on no other model crate.

#![forbid(unsafe_code)]

use std::collections::BTreeMap;
use std::fmt;

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

/// The version this build reads and writes.
pub const SCHEMA_VERSION: u32 = 1;

transit_version::kind_tag!(
    /// The stop requests document's kind tag: always `"stop_requests"`.
    StopRequestsKind = "stop_requests"
);

impl transit_version::Versioned for StopRequests {
    const KIND: &'static str = "stop_requests";
    const READS: u32 = SCHEMA_VERSION;
    fn schema_version(&self) -> u32 {
        self.schema_version.0
    }
}

/// The worksheet's jobs: a new stop, a reinstall, a relocation or
/// removal, a repair or improvement, a temporary stop.
pub const JOBS: [&str; 5] = ["new", "reinstall", "relocate_remove", "repair_improve", "temporary"];

/// A request's life: drafted, submitted, the work order opened
/// (`ordered`), completed — or cancelled, the tombstone.
pub const STATUSES: [&str; 5] = ["drafted", "submitted", "ordered", "completed", "cancelled"];

/// The side of the street (the worksheet's N, S, E, W).
pub const SIDES: [&str; 4] = ["north", "south", "east", "west"];

/// The direction of travel the stop serves.
pub const DIRECTIONS: [&str; 6] = ["northbound", "southbound", "eastbound", "westbound", "clockwise", "counterclockwise"];

/// How the spot is marked on the ground.
pub const MARKINGS: [&str; 4] = ["hub", "curb_mark", "stake", "other"];

/// Every request, by its id.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
pub struct StopRequests {
    /// The kind tag and version (D-115); a copy without them is this
    /// kind, version 1.
    #[serde(default)]
    #[ts(type = "\"stop_requests\"")]
    pub kind: StopRequestsKind,
    #[serde(default)]
    #[ts(type = "number")]
    pub schema_version: transit_version::Version<1>,
    pub requests: BTreeMap<String, StopRequest>,
    /// Fields this repo does not know, kept as written.
    #[serde(flatten)]
    #[ts(skip)]
    pub extra: BTreeMap<String, Value>,
}

/// One request for work at a stop: the worksheet, and the work order it
/// tracks. Every field but the stop, the job, and the status is
/// optional.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
pub struct StopRequest {
    /// The stop, by the stops document's lasting id.
    pub stop: String,
    /// One of [`JOBS`].
    pub job: String,
    /// One of [`STATUSES`].
    pub status: String,
    /// Who asked for the work (a person, a team, a city), as written.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub requested_by: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub requested_on: Option<String>,
    /// The routes meant to serve the stop, and the direction of travel
    /// (one of [`DIRECTIONS`]) — a new stop's intent: no schedule
    /// serves it yet.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[ts(optional, as = "Option<Vec<String>>")]
    pub routes: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub direction: Option<String>,
    /// Where the work is: the street, the cross street (or address) the
    /// offset is measured from the center of, the offset in feet, and
    /// the side of the street (one of [`SIDES`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub street: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub cross_street: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub offset_ft: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub side: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub city: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub county: Option<String>,
    /// The public land survey: township, range, section; and the parcel
    /// number.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub township: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub range: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub section: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub parcel: Option<String>,
    /// How the spot is marked (one of [`MARKINGS`]), and a word on it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub marked_by: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub marked_note: Option<String>,
    /// The utility locate called in before digging.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub locate: Option<Locate>,
    /// The work order the work order system opened for it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub work_order: Option<WorkOrder>,
    /// When the work was done, and by whom.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub completed_on: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub completed_by: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub note: Option<String>,
    /// Its photos and the site drawing (D-162).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[ts(optional, as = "Option<Vec<Attachment>>")]
    pub attachments: Vec<Attachment>,
    /// Fields this repo does not know, kept as written.
    #[serde(flatten)]
    #[ts(skip)]
    pub extra: BTreeMap<String, Value>,
}

/// A photo or drawing a record carries (D-162): named by its content
/// hash, never a URL — the deployment names the bucket, the server hands
/// out the link, and only a signed-in person may read it.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export, rename = "RequestAttachment")]
pub struct Attachment {
    /// "sha256:" and the 64 hex digits of the file's bytes.
    pub hash: String,
    /// Its media type ("image/jpeg", "application/pdf").
    #[serde(rename = "type")]
    pub media_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub name: Option<String>,
    /// What it shows, in a few words ("approaching", "the cross street").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub caption: Option<String>,
    /// The day it was taken.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub taken_on: Option<String>,
    #[serde(flatten)]
    #[ts(skip)]
    pub extra: BTreeMap<String, Value>,
}

impl Attachment {
    /// Whether it is named as a record names a file: a sha256 content
    /// hash and a media type.
    pub fn well_named(&self) -> bool {
        let hex = self.hash.strip_prefix("sha256:").unwrap_or("");
        hex.len() == 64
            && hex.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            && self.media_type.split_once('/').is_some_and(|(a, b)| !a.is_empty() && !b.is_empty())
    }
}

/// A utility locate: its ticket, the day it was called in and by whom,
/// and the utilities the locator calls.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
pub struct Locate {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub ticket: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub called_on: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub called_by: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[ts(optional, as = "Option<Vec<String>>")]
    pub utilities: Vec<String>,
    #[serde(flatten)]
    #[ts(skip)]
    pub extra: BTreeMap<String, Value>,
}

/// The work order a request tracks, as the work order system says:
/// which system, its id there (the worksheet's Control #), a link, and
/// its status as last read, and when — shown as given.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
pub struct WorkOrder {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub system: Option<String>,
    #[serde(default)]
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub status: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub read_on: Option<String>,
    #[serde(flatten)]
    #[ts(skip)]
    pub extra: BTreeMap<String, Value>,
}

/// How many requests stand at one status.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
pub struct RequestCount {
    pub status: String,
    pub requests: u32,
}

/// Why a write is refused: the request, and the problem.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
pub struct RequestRefusal {
    pub request: String,
    pub problem: RequestProblem,
}

/// What is wrong.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RequestProblem {
    /// A request with an empty id.
    NoId,
    /// A request naming no stop.
    NoStop,
    /// A job, status, or listed answer (side, direction, marked_by) this
    /// build does not know.
    Unknown { field: String, value: String },
    /// A date that is not "YYYY-MM-DD".
    BadDate { date: String },
    /// An offset below zero.
    NegativeOffset,
    /// At `ordered` with no work order id.
    NoWorkOrder,
    /// At `completed` with no completion date.
    NoCompletion,
    /// Completed before it was requested.
    CompletedBeforeRequested,
    /// An attachment not named by a sha256 content hash and a media type.
    BadAttachment { hash: String },
}

impl fmt::Display for RequestRefusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let what = match &self.problem {
            RequestProblem::NoId => "has no id".to_string(),
            RequestProblem::NoStop => "names no stop".into(),
            RequestProblem::Unknown { field, value } => format!("has {field} {value:?}, which this build does not know"),
            RequestProblem::BadDate { date } => format!("has {date:?}, not a YYYY-MM-DD date"),
            RequestProblem::NegativeOffset => "has an offset below zero".into(),
            RequestProblem::NoWorkOrder => "says its work order is open but names none".into(),
            RequestProblem::NoCompletion => "is completed with no completion date".into(),
            RequestProblem::CompletedBeforeRequested => "is completed before it was requested".into(),
            RequestProblem::BadAttachment { hash } => format!("names an attachment {hash:?} that is not a sha256 hash with a media type"),
        };
        write!(f, "request {:?} {what}", self.request)
    }
}

fn parse_date(s: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").ok()
}

impl StopRequests {
    /// One request placed by id — new, or replacing the one held; every
    /// other request as it was. Unchecked: a write passes
    /// [`StopRequests::checked`].
    pub fn put(&self, id: &str, request: StopRequest) -> StopRequests {
        let mut next = self.clone();
        next.requests.insert(id.to_string(), request);
        next
    }

    /// What the work order system answered, on request `id` (D-165): its
    /// reference replaces the one held, a field it lacks kept (the link,
    /// the system), and a drafted or submitted request moves to
    /// `ordered` — its work order is open; one ordered, completed, or
    /// cancelled keeps its status. None for an id the document does not
    /// hold. Unchecked: a write passes [`StopRequests::checked`].
    pub fn record_work_order(&self, id: &str, answer: WorkOrder) -> Option<StopRequests> {
        let mut next = self.clone();
        let r = next.requests.get_mut(id)?;
        let held = r.work_order.take().unwrap_or_default();
        r.work_order = Some(WorkOrder {
            system: answer.system.or(held.system),
            id: if answer.id.trim().is_empty() { held.id } else { answer.id },
            url: answer.url.or(held.url),
            status: answer.status.or(held.status),
            read_on: answer.read_on.or(held.read_on),
            extra: if answer.extra.is_empty() { held.extra } else { answer.extra },
        });
        if matches!(r.status.as_str(), "drafted" | "submitted") {
            r.status = "ordered".into();
        }
        Some(next)
    }

    /// A stop's requests, in id order.
    pub fn for_stop(&self, stop: &str) -> Vec<(&str, &StopRequest)> {
        self.requests.iter().filter(|(_, r)| r.stop == stop).map(|(k, r)| (k.as_str(), r)).collect()
    }

    /// The work across every stop: every request in id order, only those
    /// at `statuses` and of `jobs` — each filter only where given.
    pub fn list(&self, statuses: Option<&[String]>, jobs: Option<&[String]>) -> Vec<(&str, &StopRequest)> {
        self.requests
            .iter()
            .filter(|(_, r)| statuses.is_none_or(|s| s.contains(&r.status)))
            .filter(|(_, r)| jobs.is_none_or(|j| j.contains(&r.job)))
            .map(|(k, r)| (k.as_str(), r))
            .collect()
    }

    /// Every request counted by status, in [`STATUSES`]' order, only the
    /// statuses some request has.
    pub fn counts(&self) -> Vec<RequestCount> {
        STATUSES
            .iter()
            .map(|st| RequestCount { status: st.to_string(), requests: self.requests.values().filter(|r| r.status == *st).count() as u32 })
            .filter(|c| c.requests > 0)
            .collect()
    }

    /// The document, if a write may store it, or every reason it may
    /// not.
    pub fn checked(self) -> Result<Self, Vec<RequestRefusal>> {
        let refusals: Vec<RequestRefusal> = self
            .requests
            .iter()
            .flat_map(|(id, r)| check(r).into_iter().map(move |problem| RequestRefusal { request: id.clone(), problem }))
            .chain(self.requests.keys().filter(|k| k.is_empty()).map(|_| RequestRefusal { request: String::new(), problem: RequestProblem::NoId }))
            .collect();
        if refusals.is_empty() {
            Ok(self)
        } else {
            Err(refusals)
        }
    }
}

fn check(r: &StopRequest) -> Vec<RequestProblem> {
    let mut out = Vec::new();
    if r.stop.trim().is_empty() {
        out.push(RequestProblem::NoStop);
    }
    let listed = |out: &mut Vec<RequestProblem>, field: &str, value: Option<&String>, list: &[&str]| {
        if let Some(v) = value {
            if !list.contains(&v.as_str()) {
                out.push(RequestProblem::Unknown { field: field.into(), value: v.clone() });
            }
        }
    };
    listed(&mut out, "job", Some(&r.job), &JOBS);
    listed(&mut out, "status", Some(&r.status), &STATUSES);
    listed(&mut out, "side", r.side.as_ref(), &SIDES);
    listed(&mut out, "direction", r.direction.as_ref(), &DIRECTIONS);
    listed(&mut out, "marked_by", r.marked_by.as_ref(), &MARKINGS);
    let dates = [
        r.requested_on.as_ref(),
        r.completed_on.as_ref(),
        r.locate.as_ref().and_then(|l| l.called_on.as_ref()),
        r.work_order.as_ref().and_then(|w| w.read_on.as_ref()),
    ];
    for d in dates.into_iter().flatten() {
        if parse_date(d).is_none() {
            out.push(RequestProblem::BadDate { date: d.clone() });
        }
    }
    for a in &r.attachments {
        if !a.well_named() {
            out.push(RequestProblem::BadAttachment { hash: a.hash.clone() });
        } else if let Some(d) = a.taken_on.as_ref().filter(|d| parse_date(d).is_none()) {
            out.push(RequestProblem::BadDate { date: d.clone() });
        }
    }
        if r.offset_ft.is_some_and(|o| !(o >= 0.0)) {
        out.push(RequestProblem::NegativeOffset);
    }
    match r.status.as_str() {
        "ordered" if r.work_order.as_ref().is_none_or(|w| w.id.trim().is_empty()) => out.push(RequestProblem::NoWorkOrder),
        "completed" if r.completed_on.is_none() => out.push(RequestProblem::NoCompletion),
        _ => {}
    }
    if let (Some(a), Some(b)) = (r.requested_on.as_deref().and_then(parse_date), r.completed_on.as_deref().and_then(parse_date)) {
        if b < a {
            out.push(RequestProblem::CompletedBeforeRequested);
        }
    }
    out
}
