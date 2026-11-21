//! cap-plans — the plan-chain capability.
//!
//! One question: **which service plan governs a date?** — over the
//! plan set a planner holds (resolve / validate / periods) and over
//! the PUBLISHED plan set the server's plans log holds (publish and
//! adopt answer its next canonical document; in_force answers the pick
//! and network a client loads; in_use answers the picks whose networks
//! the server brings forward at startup). The plans log also holds the
//! frozen versions (freeze, D-191); log_records and log_document turn
//! both documents into the log's records and back. A pick written
//! to the plans log is checked here too (`pick`, D-237): the network's
//! publish check (transit-network), and the stops it reads on its
//! first date (transit-stops), in the same op as the entry. The domain
//! code (resolution, governing periods, chain validity) lives in the
//! shared `transit-plans` library — the cross-document counterpart
//! of transit-sync's merge; this module is the seam. The plan set
//! comes inline: the host owns it (a JS-owned document) and pushes
//! its distilled metadata per call, and the chain is derived on every
//! answer, never stored. A pick's network and the stops document
//! register on the kernel's loads.

#![forbid(unsafe_code)]

use cap_abi::kernel::{NETWORK, STOPS};
use cap_abi::{Base, Current};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

pub use transit_plans::{
    DevicePlan, FrozenVersion, FrozenVersions, PickStatus, PlanInfo, PlanIssue, PlanPeriod, PublishedPlan, PublishedPlans, TimelineRow,
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
    /// What a planning page lists (D-228): every plan on the device,
    /// then each published pick it does not hold. A device plan is
    /// published when its id is a published pick's, or when it is the
    /// deployment's feed (`served`). The answer also gives the pick in
    /// force on `today` and the chain's periods and issues over every
    /// row with an id.
    Timeline {
        plans: Vec<DevicePlan>,
        #[serde(default)]
        #[ts(optional, as = "Option<PublishedPlans>")]
        published: Option<Current<PublishedPlans>>,
        today: String,
    },
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
    /// A write to the plans log, whole (D-228): each op over the
    /// documents at the head, given inline by store kind (`plans`,
    /// `frozen_versions`; absent: none), then the entry that records
    /// what changed. The ops (cap `plans`): `freeze` (`{plan,
    /// network_hash, purpose}`), `adopt` (`{network_hash, today,
    /// stops_seq}`), `retire` (`{id, today}`), `pick`, and `migrate`:
    /// each pick of the published set frozen as "published before the
    /// plans log", and both documents recorded (an empty log's first
    /// entry, from the old pointer). `today` is the agency's date, which
    /// the host's clock gives. A refused op answers its issues, and an
    /// op that does not run answers why (`failed`): no entry either way.
    ///
    /// `pick` (`{purpose?, adopt, today, stops_seq?}`, D-237) writes the
    /// network the host registered (`loaded.network_hash`) as a pick:
    /// refused with `{issues, refusals}` when it does not publish; its
    /// stop records read from the stops document on its first date
    /// (`loaded.stops_hash`, when adopted); frozen under `purpose` and
    /// put in force when `adopt`. The answer's `made` is the network
    /// for the host to store as the head of `plan.<id>` before the
    /// entry, and its `answer` is `{plan_id, network_hash, timezone,
    /// warnings, stop_warnings}`. A plan issue is refused as
    /// `{issues}`.
    Receive {
        #[ts(type = "{ checkpoint?: unknown, header: unknown, unread?: Array<{ seq: number, text: string }> }")]
        write: transit_log::Write,
        #[serde(default)]
        #[ts(type = "Record<string, unknown>")]
        documents: std::collections::BTreeMap<String, serde_json::Value>,
        /// The hashes of the documents the host registered on the
        /// kernel's loads before the write, by the load's answer field
        /// (`network_hash`, `stops_hash`).
        #[serde(default)]
        #[ts(type = "Record<string, string>")]
        loaded: std::collections::BTreeMap<String, String>,
        #[ts(type = "Array<{ cap: string, op: string, args?: unknown }>")]
        ops: Vec<transit_log::OpCall>,
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
    Timeline { rows: Vec<TimelineRow>, in_force: Option<String>, periods: Vec<PlanPeriod>, issues: Vec<PlanIssue> },
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
    /// What the write adds to the plans log (`heads`: the documents it
    /// wrote, by store kind).
    Receive(transit_log::Received),
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
        Request::Timeline { plans, published, today } => {
            if let Some(p) = &published {
                spoken(&p.0)?;
            }
            let today = parse_date(&today)?;
            let t = transit_plans::timeline(&plans, published.as_ref().map(|p| &p.0), today)?;
            Ok(Response::Timeline { rows: t.rows, in_force: t.in_force, periods: t.periods, issues: t.issues })
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
        Request::Receive { write, documents, loaded, ops } => receive(write, documents, loaded, ops),
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

/// An op of a write to the plans log, as the host sends it.
#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
enum Intent {
    Freeze {
        plan: PlanInfo,
        network_hash: String,
        purpose: String,
    },
    Adopt {
        network_hash: String,
        today: String,
        #[serde(default)]
        stops_seq: Option<u64>,
    },
    Retire {
        id: String,
        today: String,
    },
    Pick {
        #[serde(default)]
        purpose: Option<String>,
        #[serde(default)]
        adopt: bool,
        today: String,
        #[serde(default)]
        stops_seq: Option<u64>,
    },
    Migrate {},
}

/// What a frozen pick of a set published before the plans log is.
const MIGRATED_PURPOSE: &str = "published before the plans log";

/// The documents a write runs over, and whether an op wrote each.
struct Heads {
    published: Option<PublishedPlans>,
    frozen: Option<FrozenVersions>,
    wrote_published: bool,
    wrote_frozen: bool,
}

/// What a write's ops make beside the documents: the documents the host
/// stores before the entry, and the answer for the route.
#[derive(Default)]
struct Made {
    documents: Vec<transit_log::MadeDocument>,
    answer: Option<serde_json::Value>,
}

/// One op over the heads: none when it ran, the refusals of a refused
/// one (a plan op's issues), or the error of an op that did not run.
fn step(heads: &mut Heads, intent: Intent, loaded: &std::collections::BTreeMap<String, String>, made: &mut Made) -> Result<Option<serde_json::Value>, String> {
    let issues = |issues: Vec<PlanIssue>| serde_json::to_value(issues).map(Some).map_err(|e| e.to_string());
    match intent {
        Intent::Pick { purpose, adopt, today, stops_seq } => return pick(heads, loaded, made, purpose, adopt, &today, stops_seq),
        Intent::Freeze { plan, network_hash, purpose } => {
            let base = heads.frozen.clone().unwrap_or_else(FrozenVersions::empty);
            match transit_plans::freeze(&base, &plan, &network_hash, &purpose) {
                Ok(next) => (heads.frozen, heads.wrote_frozen) = (Some(next), true),
                Err(i) => return issues(i),
            }
        }
        Intent::Adopt { network_hash, today, stops_seq } => {
            let base = heads.published.clone().unwrap_or_else(PublishedPlans::empty);
            let frozen = heads.frozen.clone().unwrap_or_else(FrozenVersions::empty);
            match transit_plans::adopt(&base, &frozen, &network_hash, parse_date(&today)?, stops_seq) {
                Ok(next) => (heads.published, heads.wrote_published) = (Some(next), true),
                Err(i) => return issues(i),
            }
        }
        Intent::Retire { id, today } => {
            let base = heads.published.clone().ok_or("nothing is published")?;
            match transit_plans::retire(&base, &id, parse_date(&today)?) {
                Ok(next) => (heads.published, heads.wrote_published) = (Some(next), true),
                Err(i) => return issues(i),
            }
        }
        Intent::Migrate {} => {
            let mut frozen = FrozenVersions::empty();
            for p in heads.published.iter().flat_map(|set| set.plans.iter()) {
                let plan = PlanInfo { id: p.id.clone(), name: p.name.clone(), effective_from: Some(p.effective_from.clone()) };
                match transit_plans::freeze(&frozen, &plan, &p.network_hash, MIGRATED_PURPOSE) {
                    Ok(next) => frozen = next,
                    Err(i) => return issues(i),
                }
            }
            heads.wrote_published = heads.published.is_some();
            if heads.wrote_published {
                (heads.frozen, heads.wrote_frozen) = (Some(frozen), true);
            }
        }
    }
    Ok(None)
}

/// The `pick` op (D-237): the registered network checked as a pick, its
/// stop records read when adopted, then frozen and adopted over the
/// heads. A network that does not publish is refused with `{issues,
/// refusals}`, as cap-network's `publishable` answers it; a plan issue
/// with `{issues}`.
fn pick(
    heads: &mut Heads,
    loaded: &std::collections::BTreeMap<String, String>,
    made: &mut Made,
    purpose: Option<String>,
    adopt: bool,
    today: &str,
    stops_seq: Option<u64>,
) -> Result<Option<serde_json::Value>, String> {
    let network_hash = loaded.get("network_hash").ok_or("a pick writes the network the host registered (load_network), and none was")?;
    // The pick is stored: a network read one version ahead is written in
    // this build's version, and a read-only one is refused (D-186).
    let network = NETWORK.require_writable(network_hash)?;
    let check = transit_network::publish_check(&network);
    if !check.publishes() {
        return Ok(Some(serde_json::json!({ "issues": check.issues, "refusals": check.refusals })));
    }
    // A pick reads the stops document (D-157): each stop's record on its
    // first date, and the warnings of the stops it visits that are not
    // in service over its period.
    let mut stop_warnings = Vec::new();
    let mut read = None;
    if let (true, Some(stops_hash)) = (adopt, loaded.get("stops_hash")) {
        let stops = STOPS.require(stops_hash)?;
        if let Some(r) = transit_stops::stop_record::read_pick(&network, &stops, heads.published.as_ref()) {
            stop_warnings = r.warnings;
            read = Some(r.network);
        }
    }
    let network = read.as_ref().unwrap_or(&*network);
    let document = cap_abi::canonical_bytes(network)?;
    let document_hash = cap_abi::content_hash(document.as_bytes());
    if read.is_none() && &document_hash != network_hash && NETWORK.read_ahead(network_hash).is_none() {
        return Err(format!("registered under {network_hash} but serializes to {document_hash}"));
    }
    let plan: PlanInfo = serde_json::to_value(&network.plan).and_then(serde_json::from_value).map_err(|e| format!("the pick's plan frame: {e}"))?;
    let issues = |issues: Vec<PlanIssue>| Ok(Some(serde_json::json!({ "issues": issues })));
    if let Some(purpose) = purpose {
        let base = heads.frozen.clone().unwrap_or_else(FrozenVersions::empty);
        match transit_plans::freeze(&base, &plan, &document_hash, &purpose) {
            Ok(next) => (heads.frozen, heads.wrote_frozen) = (Some(next), true),
            Err(i) => return issues(i),
        }
        made.documents.push(transit_log::MadeDocument {
            // The network's store kind (the kind list's `stored`).
            kind: "network_document".to_string(),
            version: transit_network::SCHEMA_VERSION,
            document,
            document_hash: document_hash.clone(),
            pointer: Some(format!("plan.{}", plan.id)),
        });
    }
    if adopt {
        let base = heads.published.clone().unwrap_or_else(PublishedPlans::empty);
        let frozen = heads.frozen.clone().unwrap_or_else(FrozenVersions::empty);
        match transit_plans::adopt(&base, &frozen, &document_hash, parse_date(today)?, stops_seq) {
            Ok(next) => (heads.published, heads.wrote_published) = (Some(next), true),
            Err(i) => return issues(i),
        }
    }
    made.answer = Some(serde_json::json!({
        "plan_id": plan.id,
        "network_hash": document_hash,
        "timezone": network.timezone(),
        "warnings": check.warnings,
        "stop_warnings": stop_warnings,
    }));
    Ok(None)
}

/// A document given inline, read as the base of a write.
fn given<T: serde::de::DeserializeOwned + transit_version::Versioned>(documents: &std::collections::BTreeMap<String, serde_json::Value>, kind: &str) -> Result<Option<T>, String> {
    documents.get(kind).filter(|v| !v.is_null()).map(|v| cap_abi::read_base::<T>(v.clone()).map(|(doc, _)| doc)).transpose()
}

/// A write's ops over the heads, then its entry (`Request::Receive`).
fn receive(
    write: transit_log::Write,
    documents: std::collections::BTreeMap<String, serde_json::Value>,
    loaded: std::collections::BTreeMap<String, String>,
    ops: Vec<transit_log::OpCall>,
) -> Result<Response, String> {
    let answer = |received| Ok(Response::Receive(received));
    if let Some(seq) = write.landed()? {
        return answer(transit_log::Received::landed(seq));
    }
    let mut heads = Heads { published: given(&documents, PLANS_DOCUMENT)?, frozen: given(&documents, FROZEN_DOCUMENT)?, wrote_published: false, wrote_frozen: false };
    if let Some(set) = &heads.published {
        spoken(set)?;
    }
    if let Some(frozen) = &heads.frozen {
        frozen_spoken(frozen)?;
    }
    let mut made = Made::default();
    for (i, call) in ops.iter().enumerate() {
        if call.cap != "plans" {
            return answer(transit_log::Received::failed(i, format!("{} {} is not a write to the plans log (plans: freeze, adopt, retire, pick, migrate)", call.cap, call.op)));
        }
        match call.request::<Intent>().and_then(|intent| step(&mut heads, intent, &loaded, &mut made)) {
            Ok(None) => {}
            Ok(Some(refusals)) => return answer(transit_log::Received::refused(i, refusals)),
            Err(e) => return answer(transit_log::Received::failed(i, e)),
        }
    }
    let mut next = Vec::new();
    let mut written = std::collections::BTreeMap::new();
    if let (true, Some(set)) = (heads.wrote_published, &heads.published) {
        next.extend(set_records(set)?);
        written.insert(PLANS_DOCUMENT.to_string(), cap_abi::content_hash(cap_abi::canonical_bytes(set)?.as_bytes()));
    }
    if let (true, Some(frozen)) = (heads.wrote_frozen, &heads.frozen) {
        next.extend(frozen_records(frozen)?);
        written.insert(FROZEN_DOCUMENT.to_string(), cap_abi::content_hash(cap_abi::canonical_bytes(frozen)?.as_bytes()));
    }
    let mut received = transit_log::receive(&write, &next, &cap_abi::record_hash, &|e| cap_abi::canonical_bytes(e))?;
    received.heads = written;
    received.made = made.documents;
    received.answer = made.answer;
    answer(received)
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
    documents: [NETWORK, STOPS],
}
