//! The stop requests document, at version 2 (the chain from
//! v1 is in `version`).
//!
//! A request is its own record and not an event on the stop, because the
//! work has its own life and is not a fact about the stop. It tracks the work
//! order that the agency's system opens, shows it as given, and never decides
//! it here. [`StopRequests::checked`] refuses only what makes a request wrong.
//!
//! A request names its stop, and the stop's event names the request it
//! carries out. Both live in the stops log, so completing a request and
//! adding its stop's event is one entry.

#![forbid(unsafe_code)]

use std::collections::BTreeMap;
use std::fmt;

use transit_date::Date;
use serde::{Deserialize, Serialize};
#[cfg(feature = "bindings")]
use ts_rs::TS;

pub mod version;
pub mod load;
pub use version::SCHEMA_VERSION;

transit_version::kind_tag!(
    StopRequestsKind = "stop_requests"
);

impl transit_version::Versioned for StopRequests {
    const KIND: &'static str = "stop_requests";
    const READS: u32 = SCHEMA_VERSION;
    fn schema_version(&self) -> u32 {
        self.schema_version.0
    }
}

pub const JOBS: [&str; 5] = ["new", "reinstall", "relocate_remove", "repair_improve", "temporary"];

/// The work queue opens on the statuses of work still in flight.
pub const OPEN_STATUSES: [&str; 3] = ["drafted", "submitted", "ordered"];
pub const STATUSES: [&str; 5] = ["drafted", "submitted", "ordered", "completed", "cancelled"];

pub const SIDES: [&str; 4] = ["north", "south", "east", "west"];

pub const DIRECTIONS: [&str; 6] = ["northbound", "southbound", "eastbound", "westbound", "clockwise", "counterclockwise"];

pub const MARKINGS: [&str; 4] = ["hub", "curb_mark", "stake", "other"];

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS, schemars::JsonSchema))]
#[cfg_attr(feature = "bindings", ts(export))]
pub struct StopRequests {
    /// A copy without a version reads as v2, since every v1 document is a v2
    /// document. A tagged v1 is brought forward by cap-migrate.
    #[serde(default)]
    #[cfg_attr(feature = "bindings", ts(type = "\"stop_requests\""))]
    pub kind: StopRequestsKind,
    #[serde(default)]
    #[cfg_attr(feature = "bindings", ts(type = "number"))]
    pub schema_version: transit_version::Version<2>,
    pub requests: BTreeMap<String, StopRequest>,
    /// Fields this build does not know, kept as written.
    #[serde(flatten)]
    #[cfg_attr(feature = "bindings", ts(skip))]
    pub extra: transit_version::Extra,
}

/// C-TRAN's Bus Stop Sign & Shelter Installation Worksheet.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS, schemars::JsonSchema))]
#[cfg_attr(feature = "bindings", ts(export))]
pub struct StopRequest {
    #[cfg_attr(feature = "bindings", schemars(extend("x-references" = "stop")))]
    pub stop: String,
    /// One of [`JOBS`].
    pub job: String,
    /// One of [`STATUSES`].
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub requested_by: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub requested_on: Option<String>,
    /// A new stop's intent, because no schedule serves it yet. The routes are
    /// names the planner writes, not references into the network, because a
    /// route can be one that no pick runs yet. `direction` is one of
    /// [`DIRECTIONS`].
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[cfg_attr(feature = "bindings", ts(optional, as = "Option<Vec<String>>"))]
    pub routes: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub direction: Option<String>,
    /// The offset is in feet, measured from the center of the cross street
    /// or address. The side is one of [`SIDES`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub street: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub cross_street: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub offset_ft: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub side: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub city: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub county: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub township: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub range: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub section: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub parcel: Option<String>,
    /// One of [`MARKINGS`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub marked_by: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub marked_note: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub locate: Option<Locate>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub work_order: Option<WorkOrder>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub completed_on: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub completed_by: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub note: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[cfg_attr(feature = "bindings", ts(optional, as = "Option<Vec<Attachment>>"))]
    pub attachments: Vec<Attachment>,
    /// Fields this build does not know, kept as written.
    #[serde(flatten)]
    #[cfg_attr(feature = "bindings", ts(skip))]
    pub extra: transit_version::Extra,
}

/// The deployment names the bucket, the server hands out the link, and only
/// a signed-in person may read the file.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS, schemars::JsonSchema))]
#[cfg_attr(feature = "bindings", ts(export, rename = "RequestAttachment"))]
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

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS, schemars::JsonSchema))]
#[cfg_attr(feature = "bindings", ts(export))]
pub struct Locate {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub ticket: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub called_on: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub called_by: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[cfg_attr(feature = "bindings", ts(optional, as = "Option<Vec<String>>"))]
    pub utilities: Vec<String>,
    #[serde(flatten)]
    #[cfg_attr(feature = "bindings", ts(skip))]
    pub extra: transit_version::Extra,
}

/// Shown as the work order system gave it. `id` is the worksheet's Control #.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS, schemars::JsonSchema))]
#[cfg_attr(feature = "bindings", ts(export))]
pub struct WorkOrder {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub system: Option<String>,
    #[serde(default)]
    #[cfg_attr(feature = "bindings", schemars(extend("x-references" = "external.work_order")))]
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub status: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub read_on: Option<String>,
    #[serde(flatten)]
    #[cfg_attr(feature = "bindings", ts(skip))]
    pub extra: transit_version::Extra,
}

/// The host makes the call, and `record_work_order` records the
/// answer.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "bindings", derive(TS, schemars::JsonSchema))]
#[cfg_attr(feature = "bindings", ts(export))]
pub struct WorkOrderCall {
    pub action: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub request: Option<StopRequest>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub work_order: Option<WorkOrder>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS, schemars::JsonSchema))]
#[cfg_attr(feature = "bindings", ts(export))]
pub struct RequestCount {
    pub status: String,
    pub requests: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS, schemars::JsonSchema))]
#[cfg_attr(feature = "bindings", ts(export))]
pub struct RequestRefusal {
    pub request: String,
    pub problem: RequestProblem,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS, schemars::JsonSchema))]
#[cfg_attr(feature = "bindings", ts(export))]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RequestProblem {
    NoId,
    NoStop,
    Unknown { field: String, value: String },
    BadDate { date: String },
    NegativeOffset,
    NoWorkOrder,
    NoCompletion,
    CompletedBeforeRequested,
    BadAttachment { hash: String },
    IdTaken,
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
            RequestProblem::IdTaken => "is new, but the document already holds a request with its id".into(),
        };
        write!(f, "request {:?} {what}", self.request)
    }
}

impl StopRequests {
    pub fn work_order_call(&self, id: &str, action: &str) -> Result<WorkOrderCall, String> {
        let request = self.requests.get(id).ok_or_else(|| format!("no request {id:?} in the document"))?;
        match action {
            "open" => Ok(WorkOrderCall { action: action.into(), request: Some(request.clone()), work_order: request.work_order.clone() }),
            "refresh" => Ok(WorkOrderCall { action: action.into(), request: None, work_order: request.work_order.clone() }),
            other => Err(format!("unknown work order action {other:?}: open or refresh")),
        }
    }

    /// An ordinary `put` replaces the request held, so the writer states the
    /// intent "new".
    pub fn refuse_taken(&self, id: &str) -> Option<RequestRefusal> {
        self.requests.contains_key(id).then(|| RequestRefusal { request: id.to_string(), problem: RequestProblem::IdTaken })
    }

    /// A write passes [`StopRequests::checked`] after it.
    pub fn put(&self, id: &str, request: StopRequest) -> StopRequests {
        let mut next = self.clone();
        next.requests.insert(id.to_string(), request);
        next
    }

    /// A field that the answer lacks keeps the held one. A write
    /// passes [`StopRequests::checked`] after it.
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

    pub fn for_stop(&self, stop: &str) -> Vec<(&str, &StopRequest)> {
        self.requests.iter().filter(|(_, r)| r.stop == stop).map(|(k, r)| (k.as_str(), r)).collect()
    }

    pub fn list(&self, statuses: Option<&[String]>, jobs: Option<&[String]>) -> Vec<(&str, &StopRequest)> {
        self.requests
            .iter()
            .filter(|(_, r)| statuses.is_none_or(|s| s.contains(&r.status)))
            .filter(|(_, r)| jobs.is_none_or(|j| j.contains(&r.job)))
            .map(|(k, r)| (k.as_str(), r))
            .collect()
    }

    pub fn counts(&self) -> Vec<RequestCount> {
        STATUSES
            .iter()
            .map(|st| RequestCount { status: st.to_string(), requests: self.requests.values().filter(|r| r.status == *st).count() as u32 })
            .filter(|c| c.requests > 0)
            .collect()
    }

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
        if Date::parse(d).is_err() {
            out.push(RequestProblem::BadDate { date: d.clone() });
        }
    }
    for a in &r.attachments {
        if !a.well_named() {
            out.push(RequestProblem::BadAttachment { hash: a.hash.clone() });
        } else if let Some(d) = a.taken_on.as_ref().filter(|d| Date::parse(d).is_err()) {
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
    if let (Some(a), Some(b)) = (r.requested_on.as_deref().and_then(|d| Date::parse(d).ok()), r.completed_on.as_deref().and_then(|d| Date::parse(d).ok())) {
        if b < a {
            out.push(RequestProblem::CompletedBeforeRequested);
        }
    }
    out
}
