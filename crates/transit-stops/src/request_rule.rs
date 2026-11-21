//! The rule of the reference from a stop request to its stop (D-233,
//! D-238): which states of the stop each state of a request needs.
//!
//! The existence check (transit-log) judges only that the stop is held.
//! This rule judges its state. The receive path refuses a write that
//! breaks the rule in the world its writer read, and warns about one that
//! the world broke later. A rule refuses only a contradiction, never a
//! plausibility.
//!
//! - An open request (drafted, submitted, ordered) needs a stop that is
//!   not retired on the write's date. Why: a retired stop's life has
//!   ended. The stops document refuses every event after `retired`, so
//!   no work can be done at the stop. A retirement dated later breaks
//!   nothing yet: a repair before a planned removal is real work.
//! - A completed or cancelled request accepts any state: it is history.
//! - The request that the stop's `retired` event names is exempt: the
//!   retirement is that request's outcome.
//!
//! A request's status is a string, so that a newer build's status still
//! reads. A status this build does not know is not judged, and a law
//! checks that every status in `transit_requests::STATUSES` is covered.

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use transit_requests::StopRequest;

use crate::{EventKind, Status, Stops};

/// What a request needs of its stop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Needs {
    /// A stop in any state but retired.
    NotRetired,
    /// A stop in any state.
    AnyState,
}

/// What a request at `status` needs of its stop. None for a status this
/// build does not know: it is not judged.
pub fn request_needs(status: &str) -> Option<Needs> {
    match status {
        "drafted" | "submitted" | "ordered" => Some(Needs::NotRetired),
        "completed" | "cancelled" => Some(Needs::AnyState),
        _ => None,
    }
}

/// Whether a stop at `status` gives what `needs` asks. Every state of the
/// stop is named, so a new state does not compile until the rule judges
/// it.
pub fn allows(needs: Needs, status: Status) -> bool {
    match (needs, status) {
        (Needs::AnyState, _) => true,
        (Needs::NotRetired, Status::Planned | Status::Permitted | Status::Built | Status::InService | Status::Suspended) => true,
        (Needs::NotRetired, Status::Retired) => false,
    }
}

/// A request that breaks the rule: the request, its stop, the stop's
/// status on the date, and the date that status began.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuleBreak {
    pub request: String,
    pub stop: String,
    pub status: Status,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub since: Option<String>,
}

impl RuleBreak {
    /// Why, as the end of the sentence a page shows: "Stop request R-1
    /// names stop B, which " + "was retired on 2026-10-01".
    pub fn reason(&self) -> String {
        let status = self.status.as_str().replace('_', " ");
        match &self.since {
            Some(d) => format!("was {status} on {d}"),
            None => format!("is {status}"),
        }
    }
}

/// Request `id` judged against `stops` on `on`: the break, or None when
/// the rule holds or cannot judge (a status this build does not know, or
/// a stop the document does not hold on that date).
pub fn request_rule(stops: &Stops, id: &str, request: &StopRequest, on: NaiveDate) -> Option<RuleBreak> {
    let needs = request_needs(&request.status)?;
    let stop = stops.stop_on(&request.stop, on)?;
    if allows(needs, stop.status) {
        return None;
    }
    // The retirement is this request's own outcome.
    let retired_by_it = stops
        .stops
        .get(&request.stop)
        .is_some_and(|events| events.iter().any(|e| e.event == EventKind::Retired && e.request.as_deref() == Some(id)));
    if retired_by_it {
        return None;
    }
    Some(RuleBreak { request: id.to_string(), stop: request.stop.clone(), status: stop.status, since: stop.since })
}
