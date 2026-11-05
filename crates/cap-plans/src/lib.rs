//! cap-plans — the plan-chain capability.
//!
//! One question: **which service plan governs a date?** — over the
//! plan set a planner holds (resolve / validate / periods) and over
//! the PUBLISHED plan set the server's `plans` pointer holds (publish
//! answers its next canonical document; in_force answers the pick
//! and network a client loads). The domain
//! code (resolution, governing periods, chain validity) lives in the
//! shared `transit-plans` library — the cross-document counterpart
//! of transit-sync's merge; this module is the seam. Pure,
//! stateless, no registries: the host owns the plan set (a JS-owned
//! document) and pushes its distilled metadata per call, and the
//! chain is derived on every answer, never stored.

#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use ts_rs::TS;

pub use transit_plans::{PickStatus, PlanInfo, PlanIssue, PlanPeriod, PublishedPlan, PublishedPlans};

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
    /// Publish a pick: the published plan set (`null` for the first
    /// publish) with `plan` (re)placed by id, naming its network by
    /// hash; republishing a retired pick restores it. `today` is the
    /// host's date: a new, restored, or moved pick never lands before
    /// it, a correction under its own date always publishes (D-76).
    /// Answers the canonical document and its hash — the module is the
    /// hash authority; a host stores exactly those bytes — or every
    /// reason and no document.
    Publish { published: Option<PublishedPlans>, plan: PlanInfo, network_hash: String, today: String },
    /// Retire a pick that has not taken effect (D-75): it stays in the
    /// set as the tombstone and never governs. Answers like publish.
    Retire { published: PublishedPlans, id: String, today: String },
    /// The published pick in force on `date` and its network hash —
    /// how a client chooses which network to load. Null before the
    /// first pick takes effect.
    InForce { published: PublishedPlans, date: String },
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "cap_plans/")]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Response {
    Resolve { plan: Option<String> },
    Validate { issues: Vec<PlanIssue> },
    Periods { periods: Vec<PlanPeriod> },
    Publish {
        issues: Vec<PlanIssue>,
        published: Option<PublishedPlans>,
        document: Option<String>,
        document_hash: Option<String>,
    },
    InForce { plan: Option<PublishedPlan> },
    Retire {
        issues: Vec<PlanIssue>,
        published: Option<PublishedPlans>,
        document: Option<String>,
        document_hash: Option<String>,
    },
}

/// The next set's canonical bytes and hash, or the reasons and none.
fn answered(next: Result<PublishedPlans, Vec<PlanIssue>>) -> Result<(Vec<PlanIssue>, Option<PublishedPlans>, Option<String>, Option<String>), String> {
    match next {
        Ok(set) => {
            let document = serde_json::to_string(&set).map_err(|e| e.to_string())?;
            let document_hash = cap_abi::content_hash(document.as_bytes());
            Ok((vec![], Some(set), Some(document), Some(document_hash)))
        }
        Err(issues) => Ok((issues, None, None, None)),
    }
}

/// A published set this build does not speak is refused, never guessed.
fn spoken(published: &PublishedPlans) -> Result<(), String> {
    if published.schema_version != transit_plans::PUBLISHED_SCHEMA_VERSION {
        return Err(format!(
            "published plans schema_version {} is not {} (this build speaks only that)",
            published.schema_version,
            transit_plans::PUBLISHED_SCHEMA_VERSION
        ));
    }
    Ok(())
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
        Request::Publish { published, plan, network_hash, today } => {
            let base = published.unwrap_or_else(PublishedPlans::empty);
            spoken(&base)?;
            let today = parse_date(&today)?;
            let (issues, published, document, document_hash) =
                answered(transit_plans::publish(&base, &plan, &network_hash, today))?;
            Ok(Response::Publish { issues, published, document, document_hash })
        }
        Request::Retire { published, id, today } => {
            spoken(&published)?;
            let today = parse_date(&today)?;
            let (issues, published, document, document_hash) = answered(transit_plans::retire(&published, &id, today))?;
            Ok(Response::Retire { issues, published, document, document_hash })
        }
        Request::InForce { published, date } => {
            spoken(&published)?;
            let parsed = parse_date(&date)?;
            Ok(Response::InForce { plan: transit_plans::in_force(&published, parsed)?.cloned() })
        }
    }
}

/// The JSON seam: `{"op": ...}` in, `{"ok": ...}` / `{"error": ...}`
/// out — byte-compatible with every other capability module.
pub fn dispatch_json(text: &str) -> String {
    cap_abi::answer(text, dispatch)
}

/// The plan chain takes its metadata inline — no bulk-bytes op
/// exists; the seam answers honestly instead of silently ignoring
/// one.
pub fn dispatch_bytes(op: u32, _bytes: &[u8]) -> String {
    serde_json::json!({ "error": format!("unknown bulk op {op}: the plan chain takes its metadata inline (resolve / validate / periods)") })
        .to_string()
}

cap_abi::capability! {
    id: "plans",
    json: dispatch_json,
    bytes: dispatch_bytes,
    documents: [],
}
