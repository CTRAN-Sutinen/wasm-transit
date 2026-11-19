//! cap-plans — the plan-chain capability.
//!
//! One question: **which service plan governs a date?** — over the
//! plan set a planner holds (resolve / validate / periods) and over
//! the PUBLISHED plan set the server's plans log holds (publish and
//! adopt answer its next canonical document; in_force answers the pick
//! and network a client loads; in_use answers the picks whose networks
//! the server brings forward at startup). The plans log also holds the
//! frozen versions (freeze, D-191); log_records and log_document turn
//! both documents into the log's records and back. The domain
//! code (resolution, governing periods, chain validity) lives in the
//! shared `transit-plans` library — the cross-document counterpart
//! of transit-sync's merge; this module is the seam. Pure,
//! stateless, no registries: the host owns the plan set (a JS-owned
//! document) and pushes its distilled metadata per call, and the
//! chain is derived on every answer, never stored.

#![forbid(unsafe_code)]

use cap_abi::{Base, Current};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

pub use transit_plans::{
    FrozenVersion, FrozenVersions, PickStatus, PlanInfo, PlanIssue, PlanPeriod, PublishedPlan, PublishedPlans,
};

/// The plans log's record kinds (D-191): a published pick, and the
/// published plan set's own record; a frozen version, and the frozen
/// versions' own record.
pub const PICK_RECORD: &str = "pick";
pub const PLANS_DOCUMENT: &str = "plans";
pub const FROZEN_RECORD: &str = "frozen_version";
pub const FROZEN_DOCUMENT: &str = "frozen_versions";

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
    Publish {
        #[ts(as = "Option<PublishedPlans>")]
        published: Option<Base<PublishedPlans>>,
        plan: PlanInfo,
        network_hash: String,
        today: String,
    },
    /// Retire a pick that has not taken effect (D-75): it stays in the
    /// set as the tombstone and never governs. Answers like publish.
    Retire {
        #[ts(as = "PublishedPlans")]
        published: Base<PublishedPlans>,
        id: String,
        today: String,
    },
    /// The published pick in force on `date` and its network hash —
    /// how a client chooses which network to load. Null before the
    /// first pick takes effect.
    InForce {
        #[ts(as = "PublishedPlans")]
        published: Current<PublishedPlans>,
        date: String,
    },
    /// The picks in use on `today`, the host's date: the pick in force,
    /// then every published pick still to come. The server brings their
    /// networks forward at startup. A pick that stopped governing, or a
    /// retired one, stays in its version.
    InUse {
        #[ts(as = "PublishedPlans")]
        published: Current<PublishedPlans>,
        today: String,
    },
    /// Freeze a version (D-191): the frozen versions (`null` for the
    /// first freeze) with the network of `plan` by `network_hash`, and
    /// what it is (`purpose`: "proposal for public comment"). A freeze
    /// puts nothing in force. Answers the canonical document, its hash,
    /// and its records for the plans log, or every reason and none.
    Freeze {
        #[ts(as = "Option<FrozenVersions>")]
        frozen: Option<Base<FrozenVersions>>,
        plan: PlanInfo,
        network_hash: String,
        purpose: String,
    },
    /// Adopt a frozen version (D-191): the published set (`null` for the
    /// first) with the version's pick placed under its own frame, as
    /// publish places it, naming `stops_seq`, the stops log entry the
    /// adoption read. Refused when no frozen version has `network_hash`.
    /// Answers like publish.
    Adopt {
        #[ts(as = "Option<PublishedPlans>")]
        published: Option<Base<PublishedPlans>>,
        #[ts(as = "FrozenVersions")]
        frozen: Base<FrozenVersions>,
        network_hash: String,
        today: String,
        #[serde(default)]
        #[ts(optional)]
        stops_seq: Option<u64>,
    },
    /// The plans log's records of a published set and of the frozen
    /// versions, as the log capability's `compose` takes them: each
    /// collection's records, and each document's own record.
    LogRecords {
        #[serde(default)]
        #[ts(optional, as = "Option<PublishedPlans>")]
        published: Option<Base<PublishedPlans>>,
        #[serde(default)]
        #[ts(optional, as = "Option<FrozenVersions>")]
        frozen: Option<Base<FrozenVersions>>,
    },
    /// A document assembled from the plans log's records of its kind
    /// (`plans` from `pick` records, `frozen_versions` from
    /// `frozen_version` records), answered as its canonical bytes and
    /// hash. A record in another version is refused, naming both.
    LogDocument {
        kind: String,
        #[ts(type = "Record<string, { version: number, value: unknown }>")]
        records: std::collections::BTreeMap<String, transit_log::Held>,
        /// The document's own record (absent: the empty document's).
        #[serde(default)]
        #[ts(optional, type = "{ version: number, value: unknown }")]
        document: Option<transit_log::Held>,
    },
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
    InUse { plans: Vec<PublishedPlan> },
    Retire {
        issues: Vec<PlanIssue>,
        published: Option<PublishedPlans>,
        document: Option<String>,
        document_hash: Option<String>,
    },
    /// `next`: the frozen versions' records for the plans log (empty
    /// when refused).
    Freeze {
        issues: Vec<PlanIssue>,
        frozen: Option<FrozenVersions>,
        document: Option<String>,
        document_hash: Option<String>,
        #[ts(type = "Array<{ kind: string, version: number, records: Record<string, unknown> }>")]
        next: Vec<transit_log::KindRecords>,
    },
    /// `next`: the published set's records for the plans log (empty
    /// when refused).
    Adopt {
        issues: Vec<PlanIssue>,
        published: Option<PublishedPlans>,
        document: Option<String>,
        document_hash: Option<String>,
        #[ts(type = "Array<{ kind: string, version: number, records: Record<string, unknown> }>")]
        next: Vec<transit_log::KindRecords>,
    },
    LogRecords {
        #[ts(type = "Array<{ kind: string, version: number, records: Record<string, unknown> }>")]
        next: Vec<transit_log::KindRecords>,
    },
    LogDocument { document: String, document_hash: String },
}

/// The next set's canonical bytes and hash, or the reasons and none.
fn answered(next: Result<PublishedPlans, Vec<PlanIssue>>) -> Result<(Vec<PlanIssue>, Option<PublishedPlans>, Option<String>, Option<String>), String> {
    match next {
        Ok(set) => {
            let document = cap_abi::canonical_bytes(&set)?;
            let document_hash = cap_abi::content_hash(document.as_bytes());
            Ok((vec![], Some(set), Some(document), Some(document_hash)))
        }
        Err(issues) => Ok((issues, None, None, None)),
    }
}

/// A published set in another version is refused, never guessed —
/// naming both (docs/FORMATS.md, "Versions"). A set from the JSON lane
/// reads through `Current` or `Base` (one version ahead, D-186); this
/// check holds a typed request from a native host to the same rule.
fn spoken(published: &PublishedPlans) -> Result<(), String> {
    use transit_version::Versioned;
    cap_abi::speaks(PublishedPlans::KIND, published.schema_version(), PublishedPlans::READS)
}

/// A published set as the plans log's records.
fn set_records(set: &PublishedPlans) -> Result<Vec<transit_log::KindRecords>, String> {
    let doc = serde_json::to_value(set).map_err(|e| e.to_string())?;
    transit_log::document_records(&doc, "plans", PICK_RECORD, PLANS_DOCUMENT, transit_plans::PUBLISHED_SCHEMA_VERSION)
}

/// The frozen versions as the plans log's records.
fn frozen_records(frozen: &FrozenVersions) -> Result<Vec<transit_log::KindRecords>, String> {
    let doc = serde_json::to_value(frozen).map_err(|e| e.to_string())?;
    transit_log::document_records(&doc, "versions", FROZEN_RECORD, FROZEN_DOCUMENT, transit_plans::FROZEN_SCHEMA_VERSION)
}

/// The frozen versions in another version are refused, never guessed.
fn frozen_spoken(frozen: &FrozenVersions) -> Result<(), String> {
    use transit_version::Versioned;
    cap_abi::speaks(FrozenVersions::KIND, frozen.schema_version(), FrozenVersions::READS)
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
            let base = published.map(|b| b.0).unwrap_or_else(PublishedPlans::empty);
            spoken(&base)?;
            let today = parse_date(&today)?;
            let (issues, published, document, document_hash) =
                answered(transit_plans::publish(&base, &plan, &network_hash, today))?;
            Ok(Response::Publish { issues, published, document, document_hash })
        }
        Request::Retire { published, id, today } => {
            spoken(&published.0)?;
            let today = parse_date(&today)?;
            let (issues, published, document, document_hash) = answered(transit_plans::retire(&published.0, &id, today))?;
            Ok(Response::Retire { issues, published, document, document_hash })
        }
        Request::InForce { published, date } => {
            spoken(&published.0)?;
            let parsed = parse_date(&date)?;
            Ok(Response::InForce { plan: transit_plans::in_force(&published.0, parsed)?.cloned() })
        }
        Request::InUse { published, today } => {
            spoken(&published.0)?;
            let today = parse_date(&today)?;
            Ok(Response::InUse { plans: transit_plans::in_use(&published.0, today)?.into_iter().cloned().collect() })
        }
        Request::Freeze { frozen, plan, network_hash, purpose } => {
            let base = frozen.map(|b| b.0).unwrap_or_else(FrozenVersions::empty);
            frozen_spoken(&base)?;
            Ok(match transit_plans::freeze(&base, &plan, &network_hash, &purpose) {
                Ok(next) => {
                    let document = cap_abi::canonical_bytes(&next)?;
                    let document_hash = cap_abi::content_hash(document.as_bytes());
                    let records = frozen_records(&next)?;
                    Response::Freeze { issues: vec![], frozen: Some(next), document: Some(document), document_hash: Some(document_hash), next: records }
                }
                Err(issues) => Response::Freeze { issues, frozen: None, document: None, document_hash: None, next: vec![] },
            })
        }
        Request::Adopt { published, frozen, network_hash, today, stops_seq } => {
            let base = published.map(|b| b.0).unwrap_or_else(PublishedPlans::empty);
            spoken(&base)?;
            frozen_spoken(&frozen.0)?;
            let today = parse_date(&today)?;
            let (issues, published, document, document_hash) =
                answered(transit_plans::adopt(&base, &frozen.0, &network_hash, today, stops_seq))?;
            let next = match &published {
                Some(set) => set_records(set)?,
                None => vec![],
            };
            Ok(Response::Adopt { issues, published, document, document_hash, next })
        }
        Request::LogRecords { published, frozen } => {
            let mut next = Vec::new();
            if let Some(set) = published {
                spoken(&set.0)?;
                next.extend(set_records(&set.0)?);
            }
            if let Some(frozen) = frozen {
                frozen_spoken(&frozen.0)?;
                next.extend(frozen_records(&frozen.0)?);
            }
            Ok(Response::LogRecords { next })
        }
        Request::LogDocument { kind, records, document } => {
            let document = match kind.as_str() {
                PLANS_DOCUMENT => {
                    let template = serde_json::to_value(PublishedPlans::empty()).map_err(|e| e.to_string())?;
                    let mut set: PublishedPlans = transit_log::assemble(template, document.as_ref(), "plans", &records, true)?;
                    // The set's one order: by date, then id.
                    set.plans.sort_by(|a, b| (a.effective_from.as_str(), a.id.as_str()).cmp(&(b.effective_from.as_str(), b.id.as_str())));
                    cap_abi::canonical_bytes(&set)?
                }
                FROZEN_DOCUMENT => {
                    let template = serde_json::to_value(FrozenVersions::empty()).map_err(|e| e.to_string())?;
                    let frozen: FrozenVersions = transit_log::assemble(template, document.as_ref(), "versions", &records, false)?;
                    cap_abi::canonical_bytes(&frozen)?
                }
                other => return Err(format!("the plans log holds no {other:?} document (plans, frozen_versions)")),
            };
            let document_hash = cap_abi::content_hash(document.as_bytes());
            Ok(Response::LogDocument { document, document_hash })
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
