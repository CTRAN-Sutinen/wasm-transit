//! cap-plans — the plan-chain capability.
//!
//! One question: **which service plan governs a date?** The domain
//! code (resolution, governing periods, chain validity) lives in the
//! shared `transit-plans` library — the cross-document counterpart
//! of transit-sync's merge; this module is the seam. Pure,
//! stateless, no registries: the host owns the plan set (a JS-owned
//! document) and pushes its distilled metadata per call, and the
//! chain is derived on every answer, never stored.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

pub use transit_plans::{PlanInfo, PlanIssue, PlanPeriod};

/// One operation call. Tagged like every capability's seam.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "cap_plans/")]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Request {
    /// Which plan is in force on `date`: the latest pick at or before
    /// it. Unversioned plans never resolve.
    Resolve { plans: Vec<PlanInfo>, date: String },
    /// Chain validity: unique plan ids, versioned plans
    /// effective-dated, effective dates unique.
    Validate { plans: Vec<PlanInfo> },
    /// Every chain member's governing period — [effective_from,
    /// until the next pick) — derived, sorted by pick date.
    Periods { plans: Vec<PlanInfo> },
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "cap_plans/")]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Response {
    Resolve { plan: Option<String> },
    Validate { issues: Vec<PlanIssue> },
    Periods { periods: Vec<PlanPeriod> },
}

fn parse_date(s: &str) -> Result<chrono::NaiveDate, String> {
    chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d")
        .map_err(|e| format!("invalid date {s:?}, expected YYYY-MM-DD: {e}"))
}

/// The typed dispatch (native hosts call this directly). Failures
/// are `{"error": ...}`, never panics — a total seam.
pub fn dispatch(request: Request) -> Result<Response, String> {
    match request {
        Request::Resolve { plans, date } => {
            let parsed = parse_date(&date)?;
            Ok(Response::Resolve { plan: transit_plans::resolve(&plans, parsed)? })
        }
        Request::Validate { plans } => {
            Ok(Response::Validate { issues: transit_plans::validate(&plans) })
        }
        Request::Periods { plans } => {
            Ok(Response::Periods { periods: transit_plans::periods(&plans) })
        }
    }
}

/// The JSON seam: `{"op": ...}` in, `{"ok": ...}` / `{"error": ...}`
/// out — byte-compatible with every other capability module.
pub fn dispatch_json(text: &str) -> String {
    let outcome = match serde_json::from_str::<Request>(text) {
        Ok(request) => match dispatch(request) {
            Ok(response) => serde_json::json!({ "ok": response }),
            Err(error) => serde_json::json!({ "error": error }),
        },
        Err(error) => serde_json::json!({ "error": format!("invalid request: {error}") }),
    };
    serde_json::to_string(&outcome).expect("dispatch outcomes serialize")
}

/// The plan chain takes its metadata inline — no bulk-bytes op
/// exists; the seam answers honestly instead of silently ignoring
/// one.
pub fn dispatch_bytes(op: u32, _bytes: &[u8]) -> String {
    serde_json::json!({ "error": format!("unknown bulk op {op}: the plan chain takes its metadata inline (resolve / validate / periods)") })
        .to_string()
}

cap_abi::capability! {
    json: dispatch_json,
    bytes: dispatch_bytes,
}
