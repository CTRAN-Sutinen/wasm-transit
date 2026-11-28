//! `pick` is the server's whole write of a pick. It runs the
//! network's publish check, reads the stops that the pick uses on its first
//! date, freezes the pick, and adopts it. So this module also links
//! transit-network and transit-stops, and their documents register on the
//! kernel's loads. The plans log holds the published plan set and the frozen
//! versions (D-191).

#![forbid(unsafe_code)]

use cap_abi::kernel::kind;
use cap_abi::{Base, Current};
use serde::{Deserialize, Serialize};
#[cfg(feature = "bindings")]
use ts_rs::TS;

pub use transit_plans::{
    DevicePlan, FrozenVersion, FrozenVersions, PickStatus, PlanInfo, PlanIssue, PlanPeriod,
    PublishedPlan, PublishedPlans, TimelineRow,
};

pub const PICK_RECORD: &str = "pick";
pub const PLANS_DOCUMENT: &str = "plans";
pub const FROZEN_RECORD: &str = "frozen_version";
pub const FROZEN_DOCUMENT: &str = "frozen_versions";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS))]
#[cfg_attr(feature = "bindings", ts(export, export_to = "cap_plans/"))]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Request {
    Resolve {
        plans: Vec<PlanInfo>,
        date: String,
    },
    Validate {
        plans: Vec<PlanInfo>,
    },
    Timeline {
        plans: Vec<DevicePlan>,
        #[serde(default)]
        #[cfg_attr(feature = "bindings", ts(optional, as = "Option<PublishedPlans>"))]
        published: Option<Current<PublishedPlans>>,
        today: String,
    },
    Periods {
        plans: Vec<PlanInfo>,
    },
    Publish {
        #[cfg_attr(feature = "bindings", ts(as = "Option<PublishedPlans>"))]
        published: Option<Base<PublishedPlans>>,
        plan: PlanInfo,
        network_hash: String,
        today: String,
    },
    Retire {
        #[cfg_attr(feature = "bindings", ts(as = "PublishedPlans"))]
        published: Base<PublishedPlans>,
        id: String,
        today: String,
    },
    InForce {
        #[cfg_attr(feature = "bindings", ts(as = "PublishedPlans"))]
        published: Current<PublishedPlans>,
        date: String,
    },
    InUse {
        #[cfg_attr(feature = "bindings", ts(as = "PublishedPlans"))]
        published: Current<PublishedPlans>,
        today: String,
    },
    Freeze {
        #[cfg_attr(feature = "bindings", ts(as = "Option<FrozenVersions>"))]
        frozen: Option<Base<FrozenVersions>>,
        plan: PlanInfo,
        network_hash: String,
        purpose: String,
    },
    Adopt {
        #[cfg_attr(feature = "bindings", ts(as = "Option<PublishedPlans>"))]
        published: Option<Base<PublishedPlans>>,
        #[cfg_attr(feature = "bindings", ts(as = "FrozenVersions"))]
        frozen: Base<FrozenVersions>,
        network_hash: String,
        today: String,
        #[serde(default)]
        #[cfg_attr(feature = "bindings", ts(optional))]
        stops_seq: Option<u64>,
    },
    LogRecords {
        #[serde(default)]
        #[cfg_attr(feature = "bindings", ts(optional, as = "Option<PublishedPlans>"))]
        published: Option<Base<PublishedPlans>>,
        #[serde(default)]
        #[cfg_attr(feature = "bindings", ts(optional, as = "Option<FrozenVersions>"))]
        frozen: Option<Base<FrozenVersions>>,
    },
    Receive {
        #[cfg_attr(
            feature = "bindings",
            ts(
                type = "{ checkpoint?: unknown, header: unknown, unread?: Array<{ seq: number, text: string }> }"
            )
        )]
        write: transit_log::Write,
        #[serde(default)]
        #[cfg_attr(feature = "bindings", ts(type = "Record<string, unknown>"))]
        documents: std::collections::BTreeMap<String, serde_json::Value>,
        #[serde(default)]
        #[cfg_attr(feature = "bindings", ts(type = "Record<string, string>"))]
        loaded: std::collections::BTreeMap<String, String>,
        #[cfg_attr(
            feature = "bindings",
            ts(type = "Array<{ cap: string, op: string, args?: unknown }>")
        )]
        ops: Vec<transit_log::OpCall>,
    },
    LogDocument {
        kind: String,
        #[cfg_attr(
            feature = "bindings",
            ts(type = "Record<string, { version: number, value: unknown }>")
        )]
        records: std::collections::BTreeMap<String, transit_log::Held>,
        #[serde(default)]
        #[cfg_attr(
            feature = "bindings",
            ts(optional, type = "{ version: number, value: unknown }")
        )]
        document: Option<transit_log::Held>,
    },
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "bindings", derive(TS))]
#[cfg_attr(feature = "bindings", ts(export, export_to = "cap_plans/"))]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Response {
    Resolve {
        plan: Option<String>,
    },
    Validate {
        issues: Vec<PlanIssue>,
    },
    Periods {
        periods: Vec<PlanPeriod>,
    },
    Timeline {
        rows: Vec<TimelineRow>,
        in_force: Option<String>,
        periods: Vec<PlanPeriod>,
        issues: Vec<PlanIssue>,
    },
    Publish {
        issues: Vec<PlanIssue>,
        published: Option<PublishedPlans>,
        document: Option<String>,
        document_hash: Option<String>,
    },
    InForce {
        plan: Option<PublishedPlan>,
    },
    InUse {
        plans: Vec<PublishedPlan>,
    },
    Retire {
        issues: Vec<PlanIssue>,
        published: Option<PublishedPlans>,
        document: Option<String>,
        document_hash: Option<String>,
    },
    Freeze {
        issues: Vec<PlanIssue>,
        frozen: Option<FrozenVersions>,
        document: Option<String>,
        document_hash: Option<String>,
        #[cfg_attr(
            feature = "bindings",
            ts(
                type = "Array<{ kind: string, version: number, records: Record<string, unknown> }>"
            )
        )]
        next: Vec<transit_log::KindRecords>,
    },
    Adopt {
        issues: Vec<PlanIssue>,
        published: Option<PublishedPlans>,
        document: Option<String>,
        document_hash: Option<String>,
        #[cfg_attr(
            feature = "bindings",
            ts(
                type = "Array<{ kind: string, version: number, records: Record<string, unknown> }>"
            )
        )]
        next: Vec<transit_log::KindRecords>,
    },
    LogRecords {
        #[cfg_attr(
            feature = "bindings",
            ts(
                type = "Array<{ kind: string, version: number, records: Record<string, unknown> }>"
            )
        )]
        next: Vec<transit_log::KindRecords>,
    },
    Receive(transit_log::Received),
    LogDocument {
        document: String,
        document_hash: String,
    },
}

fn answered(
    next: Result<PublishedPlans, Vec<PlanIssue>>,
) -> Result<
    (
        Vec<PlanIssue>,
        Option<PublishedPlans>,
        Option<String>,
        Option<String>,
    ),
    String,
> {
    match next {
        Ok(set) => {
            let document = cap_abi::canonical_bytes(&set)?;
            let document_hash = cap_abi::content_hash(document.as_bytes());
            Ok((vec![], Some(set), Some(document), Some(document_hash)))
        }
        Err(issues) => Ok((issues, None, None, None)),
    }
}

/// A published set in another version is refused, naming both versions. This
/// holds a typed request from a native host to the rule that the JSON entry
/// point gets through `Current` or `Base`.
fn spoken(published: &PublishedPlans) -> Result<(), cap_abi::Refusal> {
    use transit_version::Versioned;
    cap_abi::speaks(
        PublishedPlans::KIND,
        published.schema_version(),
        PublishedPlans::READS,
    )
}

fn set_records(set: &PublishedPlans) -> Result<Vec<transit_log::KindRecords>, String> {
    let doc = serde_json::to_value(set).map_err(|e| e.to_string())?;
    transit_log::document_records(
        &doc,
        "plans",
        PICK_RECORD,
        PLANS_DOCUMENT,
        transit_plans::PUBLISHED_SCHEMA_VERSION,
    )
}

fn frozen_records(frozen: &FrozenVersions) -> Result<Vec<transit_log::KindRecords>, String> {
    let doc = serde_json::to_value(frozen).map_err(|e| e.to_string())?;
    transit_log::document_records(
        &doc,
        "versions",
        FROZEN_RECORD,
        FROZEN_DOCUMENT,
        transit_plans::FROZEN_SCHEMA_VERSION,
    )
}

fn frozen_spoken(frozen: &FrozenVersions) -> Result<(), cap_abi::Refusal> {
    use transit_version::Versioned;
    cap_abi::speaks(
        FrozenVersions::KIND,
        frozen.schema_version(),
        FrozenVersions::READS,
    )
}

pub fn dispatch(request: Request) -> Result<Response, cap_abi::Refusal> {
    match request {
        Request::Resolve { plans, date } => {
            let parsed = transit_date::Date::read(&date)?;
            Ok(Response::Resolve {
                plan: transit_plans::resolve(&plans, parsed)?,
            })
        }
        Request::Validate { plans } => Ok(Response::Validate {
            issues: transit_plans::validate(&plans),
        }),
        Request::Timeline {
            plans,
            published,
            today,
        } => {
            if let Some(p) = &published {
                spoken(&p.0)?;
            }
            let today = transit_date::Date::read(&today)?;
            let t = transit_plans::timeline(&plans, published.as_ref().map(|p| &p.0), today)?;
            Ok(Response::Timeline {
                rows: t.rows,
                in_force: t.in_force,
                periods: t.periods,
                issues: t.issues,
            })
        }
        Request::Periods { plans } => Ok(Response::Periods {
            periods: transit_plans::periods(&plans),
        }),
        Request::Publish {
            published,
            plan,
            network_hash,
            today,
        } => {
            let base = published.map(|b| b.0).unwrap_or_else(PublishedPlans::empty);
            spoken(&base)?;
            let today = transit_date::Date::read(&today)?;
            let (issues, published, document, document_hash) =
                answered(transit_plans::publish(&base, &plan, &network_hash, today))?;
            Ok(Response::Publish {
                issues,
                published,
                document,
                document_hash,
            })
        }
        Request::Retire {
            published,
            id,
            today,
        } => {
            spoken(&published.0)?;
            let today = transit_date::Date::read(&today)?;
            let (issues, published, document, document_hash) =
                answered(transit_plans::retire(&published.0, &id, today))?;
            Ok(Response::Retire {
                issues,
                published,
                document,
                document_hash,
            })
        }
        Request::InForce { published, date } => {
            spoken(&published.0)?;
            let parsed = transit_date::Date::read(&date)?;
            Ok(Response::InForce {
                plan: transit_plans::in_force(&published.0, parsed)?.cloned(),
            })
        }
        Request::InUse { published, today } => {
            spoken(&published.0)?;
            let today = transit_date::Date::read(&today)?;
            Ok(Response::InUse {
                plans: transit_plans::in_use(&published.0, today)?
                    .into_iter()
                    .cloned()
                    .collect(),
            })
        }
        Request::Freeze {
            frozen,
            plan,
            network_hash,
            purpose,
        } => {
            let base = frozen.map(|b| b.0).unwrap_or_else(FrozenVersions::empty);
            frozen_spoken(&base)?;
            Ok(
                match transit_plans::freeze(&base, &plan, &network_hash, &purpose) {
                    Ok(next) => {
                        let document = cap_abi::canonical_bytes(&next)?;
                        let document_hash = cap_abi::content_hash(document.as_bytes());
                        let records = frozen_records(&next)?;
                        Response::Freeze {
                            issues: vec![],
                            frozen: Some(next),
                            document: Some(document),
                            document_hash: Some(document_hash),
                            next: records,
                        }
                    }
                    Err(issues) => Response::Freeze {
                        issues,
                        frozen: None,
                        document: None,
                        document_hash: None,
                        next: vec![],
                    },
                },
            )
        }
        Request::Adopt {
            published,
            frozen,
            network_hash,
            today,
            stops_seq,
        } => {
            let base = published.map(|b| b.0).unwrap_or_else(PublishedPlans::empty);
            spoken(&base)?;
            frozen_spoken(&frozen.0)?;
            let today = transit_date::Date::read(&today)?;
            let (issues, published, document, document_hash) = answered(transit_plans::adopt(
                &base,
                &frozen.0,
                &network_hash,
                today,
                stops_seq,
            ))?;
            let next = match &published {
                Some(set) => set_records(set)?,
                None => vec![],
            };
            Ok(Response::Adopt {
                issues,
                published,
                document,
                document_hash,
                next,
            })
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
        Request::Receive {
            write,
            documents,
            loaded,
            ops,
        } => receive(write, documents, loaded, ops),
        Request::LogDocument {
            kind,
            records,
            document,
        } => {
            let document = match kind.as_str() {
                PLANS_DOCUMENT => {
                    let template =
                        serde_json::to_value(PublishedPlans::empty()).map_err(|e| e.to_string())?;
                    let mut set: PublishedPlans = transit_log::assemble(
                        template,
                        document.as_ref(),
                        "plans",
                        &records,
                        true,
                    )?;
                    set.plans.sort_by(|a, b| {
                        (a.effective_from.as_str(), a.id.as_str())
                            .cmp(&(b.effective_from.as_str(), b.id.as_str()))
                    });
                    cap_abi::canonical_bytes(&set)?
                }
                FROZEN_DOCUMENT => {
                    let template =
                        serde_json::to_value(FrozenVersions::empty()).map_err(|e| e.to_string())?;
                    let frozen: FrozenVersions = transit_log::assemble(
                        template,
                        document.as_ref(),
                        "versions",
                        &records,
                        false,
                    )?;
                    cap_abi::canonical_bytes(&frozen)?
                }
                other => {
                    return Err(format!(
                        "the plans log holds no {other:?} document (plans, frozen_versions)"
                    )
                    .into())
                }
            };
            let document_hash = cap_abi::content_hash(document.as_bytes());
            Ok(Response::LogDocument {
                document,
                document_hash,
            })
        }
    }
}

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
        from_network_hash: Option<String>,
        #[serde(default)]
        adopt: bool,
        today: String,
        #[serde(default)]
        stops_seq: Option<u64>,
    },
    Migrate {},
}

const MIGRATED_PURPOSE: &str = "published before the plans log";

struct Heads {
    published: Option<PublishedPlans>,
    frozen: Option<FrozenVersions>,
    wrote_published: bool,
    wrote_frozen: bool,
}

#[derive(Default)]
struct Made {
    documents: Vec<transit_log::MadeDocument>,
    answer: Option<serde_json::Value>,
}

fn step(
    heads: &mut Heads,
    intent: Intent,
    loaded: &std::collections::BTreeMap<String, String>,
    made: &mut Made,
) -> Result<Option<serde_json::Value>, cap_abi::Refusal> {
    let issues = |issues: Vec<PlanIssue>| {
        serde_json::to_value(issues)
            .map(Some)
            .map_err(|e| cap_abi::Refusal::from(e.to_string()))
    };
    match intent {
        Intent::Pick {
            purpose,
            from_network_hash,
            adopt,
            today,
            stops_seq,
        } => {
            return pick(
                heads,
                loaded,
                made,
                purpose,
                from_network_hash,
                adopt,
                &today,
                stops_seq,
            )
        }
        Intent::Freeze {
            plan,
            network_hash,
            purpose,
        } => {
            let base = heads.frozen.clone().unwrap_or_else(FrozenVersions::empty);
            match transit_plans::freeze(&base, &plan, &network_hash, &purpose) {
                Ok(next) => (heads.frozen, heads.wrote_frozen) = (Some(next), true),
                Err(i) => return issues(i),
            }
        }
        Intent::Adopt {
            network_hash,
            today,
            stops_seq,
        } => {
            let base = heads
                .published
                .clone()
                .unwrap_or_else(PublishedPlans::empty);
            let frozen = heads.frozen.clone().unwrap_or_else(FrozenVersions::empty);
            match transit_plans::adopt(
                &base,
                &frozen,
                &network_hash,
                transit_date::Date::read(&today)?,
                stops_seq,
            ) {
                Ok(next) => (heads.published, heads.wrote_published) = (Some(next), true),
                Err(i) => return issues(i),
            }
        }
        Intent::Retire { id, today } => {
            let base = heads.published.clone().ok_or("nothing is published")?;
            match transit_plans::retire(&base, &id, transit_date::Date::read(&today)?) {
                Ok(next) => (heads.published, heads.wrote_published) = (Some(next), true),
                Err(i) => return issues(i),
            }
        }
        Intent::Migrate {} => {
            let mut frozen = FrozenVersions::empty();
            for p in heads.published.iter().flat_map(|set| set.plans.iter()) {
                let plan = PlanInfo {
                    id: p.id.clone(),
                    name: p.name.clone(),
                    effective_from: Some(p.effective_from.clone()),
                };
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

/// A plan issue is refused with `{issues}`. A network that does not publish
/// is refused with `{issues, refusals}`, as cap-network's `publishable`
/// answers it.
#[allow(clippy::too_many_arguments)]
fn pick(
    heads: &mut Heads,
    loaded: &std::collections::BTreeMap<String, String>,
    made: &mut Made,
    purpose: Option<String>,
    from_network_hash: Option<String>,
    adopt: bool,
    today: &str,
    stops_seq: Option<u64>,
) -> Result<Option<serde_json::Value>, cap_abi::Refusal> {
    let network_hash = loaded
        .get("network_hash")
        .ok_or("a pick writes the network the host registered (load_network), and none was")?;
    // A network read one version ahead is stored in this build's version. A
    // read-only one is refused.
    let network = kind::<transit_network::Network>().require_writable(network_hash)?;
    let check = transit_network::publish_check(&network);
    if !check.publishes() {
        return Ok(Some(
            serde_json::json!({ "issues": check.issues, "refusals": check.refusals }),
        ));
    }
    // A pick takes each stop's record on its first date, and the
    // warnings of the stops it visits that are out of service in its period.
    let mut stop_warnings = Vec::new();
    let mut read = None;
    if let (true, Some(stops_hash)) = (adopt, loaded.get("stops_hash")) {
        let stops = kind::<transit_stops::Stops>().require(stops_hash)?;
        if let Some(r) =
            transit_stops::stop_record::read_pick(&network, &stops, heads.published.as_ref())
        {
            stop_warnings = r.warnings;
            read = Some(r.network);
        }
    }
    let network = read.as_ref().unwrap_or(&*network);
    let document = cap_abi::canonical_bytes(network)?;
    let document_hash = cap_abi::content_hash(document.as_bytes());
    if read.is_none()
        && &document_hash != network_hash
        && kind::<transit_network::Network>()
            .read_ahead(network_hash)
            .is_none()
    {
        return Err(
            format!("registered under {network_hash} but serializes to {document_hash}").into(),
        );
    }
    let plan: PlanInfo = serde_json::to_value(&network.plan)
        .and_then(serde_json::from_value)
        .map_err(|e| format!("the pick's plan frame: {e}"))?;
    let issues = |issues: Vec<PlanIssue>| Ok(Some(serde_json::json!({ "issues": issues })));
    if let Some(purpose) = purpose {
        let base = heads.frozen.clone().unwrap_or_else(FrozenVersions::empty);
        match transit_plans::freeze_from(
            &base,
            &plan,
            &document_hash,
            &purpose,
            from_network_hash.as_deref(),
        ) {
            Ok(next) => (heads.frozen, heads.wrote_frozen) = (Some(next), true),
            Err(i) => return issues(i),
        }
        made.documents.push(transit_log::MadeDocument {
            kind: "network_document".to_string(),
            version: transit_network::SCHEMA_VERSION,
            document,
            document_hash: document_hash.clone(),
            pointer: Some(format!("plan.{}", plan.id)),
        });
    }
    if adopt {
        let base = heads
            .published
            .clone()
            .unwrap_or_else(PublishedPlans::empty);
        let frozen = heads.frozen.clone().unwrap_or_else(FrozenVersions::empty);
        match transit_plans::adopt(
            &base,
            &frozen,
            &document_hash,
            transit_date::Date::read(today)?,
            stops_seq,
        ) {
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

fn given<T: serde::de::DeserializeOwned + transit_version::Versioned>(
    documents: &std::collections::BTreeMap<String, serde_json::Value>,
    kind: &str,
) -> Result<Option<T>, cap_abi::Refusal> {
    documents
        .get(kind)
        .filter(|v| !v.is_null())
        .map(|v| cap_abi::read_base::<T>(v.clone()).map(|(doc, _)| doc))
        .transpose()
}

fn receive(
    write: transit_log::Write,
    documents: std::collections::BTreeMap<String, serde_json::Value>,
    loaded: std::collections::BTreeMap<String, String>,
    ops: Vec<transit_log::OpCall>,
) -> Result<Response, cap_abi::Refusal> {
    let answer = |received| Ok(Response::Receive(received));
    if let Some(seq) = write.landed()? {
        return answer(transit_log::Received::landed(seq));
    }
    let mut heads = Heads {
        published: given(&documents, PLANS_DOCUMENT)?,
        frozen: given(&documents, FROZEN_DOCUMENT)?,
        wrote_published: false,
        wrote_frozen: false,
    };
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
        match call
            .request::<Intent>()
            .map_err(Into::into)
            .and_then(|intent| step(&mut heads, intent, &loaded, &mut made))
        {
            Ok(None) => {}
            Ok(Some(refusals)) => return answer(transit_log::Received::refused(i, refusals)),
            Err(e) => return answer(transit_log::Received::failed(i, e)),
        }
    }
    let mut next = Vec::new();
    let mut written = std::collections::BTreeMap::new();
    if let (true, Some(set)) = (heads.wrote_published, &heads.published) {
        next.extend(set_records(set)?);
        written.insert(
            PLANS_DOCUMENT.to_string(),
            cap_abi::content_hash(cap_abi::canonical_bytes(set)?.as_bytes()),
        );
    }
    if let (true, Some(frozen)) = (heads.wrote_frozen, &heads.frozen) {
        next.extend(frozen_records(frozen)?);
        written.insert(
            FROZEN_DOCUMENT.to_string(),
            cap_abi::content_hash(cap_abi::canonical_bytes(frozen)?.as_bytes()),
        );
    }
    let mut received = transit_log::receive(&write, &next, &cap_abi::record_hash, &|e| {
        cap_abi::canonical_bytes(e)
    })?;
    received.heads = written;
    received.made = made.documents;
    received.answer = made.answer;
    answer(received)
}

pub fn dispatch_json(text: &str) -> String {
    cap_abi::answer(text, dispatch)
}

pub fn dispatch_bytes(op: u32, _bytes: &[u8]) -> String {
    serde_json::json!({ "error": { "message": format!("unknown bulk op {op}: the plan chain takes its metadata inline (resolve / validate / periods)") } })
        .to_string()
}

#[cfg(feature = "examples")]
pub mod examples;

cap_abi::capability! {
    id: "plans",
    json: dispatch_json,
    bytes: dispatch_bytes,
    documents: [transit_network::Network, transit_stops::Stops],
    request: Request,
    consumes: &[("network", "network")],
    speaks: &[
        ("plans", &[1]),
        ("frozen_versions", &[1]),
        ("network", &[transit_network::SCHEMA_VERSION]),
        ("stops", &[transit_stops::SCHEMA_VERSION]),
    ],
}
