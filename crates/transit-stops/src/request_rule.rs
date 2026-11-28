//! Which states of a stop each state of a request needs (D-233).
//!
//! The existence check (transit-log) judges only that the stop is held. This
//! rule judges its state. The write path refuses a write that breaks the
//! rule in the world its writer read, and warns about a write that the world
//! broke later. The rule refuses only a contradiction, not a plausibility.
//!
//! An open request needs a stop that is not retired on the write's date,
//! because the stops document refuses every event after `retired`. A
//! retirement dated later breaks nothing yet: a repair before a planned
//! removal is real work. A completed or cancelled request is history, so it
//! accepts any state. The request that the stop's `retired` event names is
//! exempt, because the retirement is its outcome.
//!
//! A request's status is a string, so a newer build's status still reads. A
//! property test checks that the rule covers every status in
//! `transit_requests::STATUSES`.

use serde::{Deserialize, Serialize};
use transit_date::Date;
use transit_requests::StopRequest;

use crate::{EventKind, Status, Stops};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Needs {
    NotRetired,
    AnyState,
}

pub fn request_needs(status: &str) -> Option<Needs> {
    match status {
        "drafted" | "submitted" | "ordered" => Some(Needs::NotRetired),
        "completed" | "cancelled" => Some(Needs::AnyState),
        _ => None,
    }
}

/// The match names every state of the stop, so a new state does not compile
/// until the rule judges it.
pub fn allows(needs: Needs, status: Status) -> bool {
    match (needs, status) {
        (Needs::AnyState, _) => true,
        (
            Needs::NotRetired,
            Status::Planned
            | Status::Permitted
            | Status::Built
            | Status::InService
            | Status::Suspended,
        ) => true,
        (Needs::NotRetired, Status::Retired) => false,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuleBreak {
    pub request: String,
    pub stop: String,
    pub status: Status,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub since: Option<String>,
}

impl RuleBreak {
    /// The end of the sentence that a page shows: "Stop request R-1 names
    /// stop B, which " + "was retired on 2026-10-01".
    pub fn reason(&self) -> String {
        let status = self.status.as_str().replace('_', " ");
        match &self.since {
            Some(d) => format!("was {status} on {d}"),
            None => format!("is {status}"),
        }
    }
}

pub fn request_rule(stops: &Stops, id: &str, request: &StopRequest, on: Date) -> Option<RuleBreak> {
    let needs = request_needs(&request.status)?;
    let stop = stops.stop_on(&request.stop, on)?;
    if allows(needs, stop.status) {
        return None;
    }
    let retired_by_it = stops.stops.get(&request.stop).is_some_and(|events| {
        events
            .iter()
            .any(|e| e.event == EventKind::Retired && e.request.as_deref() == Some(id))
    });
    if retired_by_it {
        return None;
    }
    Some(RuleBreak {
        request: id.to_string(),
        stop: request.stop.clone(),
        status: stop.status,
        since: stop.since,
    })
}
