//! cap-alerts: the service alerts capability.
//!
//! One question: **what is the agency telling riders?** The service
//! alerts document (transit-alerts; D-206) holds the agency's alerts in
//! the network's ids. Dispatch and the communications staff write them,
//! and every team reads the one list (D-207).
//!
//! - `validate` answers what a write must pass: every refusal, naming
//!   its alert and problem, or the canonical document and its hash. The
//!   module is the hash authority; a host stores exactly those bytes.
//! - `put` and `end` answer the next document, checked as a write is.
//! - `list` answers the agency's list at a moment.
//! - `flags` answers what a person is told about alerts that pass: an
//!   id the network does not have, or a missing translation.
//! - What no alert explains is cap-disruptions' `gaps` (D-218).
//! - `validate_templates`, `put_template`, and `fill` are the agency's
//!   standard language with blanks (D-210).
//!
//! The domain code lives in the shared `transit-alerts` library. The
//! module writes nothing. cap-rt's `encode_alerts` writes the document
//! as a GTFS-rt Alerts feed.

#![forbid(unsafe_code)]

use cap_abi::kernel::kind;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

pub use transit_alerts::{
    AlertFlag, AlertList, AlertRefusal, AlertTemplate, AlertTemplates, FillInput, Filled, ServiceAlert, ServiceAlerts,
    TemplateRefusal,
};

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "cap_alerts/")]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Request {
    /// What a write must pass: every refusal, or, when there is none,
    /// the canonical document and its hash.
    Validate {
        #[ts(as = "ServiceAlerts")]
        alerts: cap_abi::Base<ServiceAlerts>,
    },
    /// The document with `alert` placed under `id`, new or replacing the
    /// one held, then checked as a write is. A document absent
    /// (`alerts_hash` omitted) is an empty one.
    Put {
        #[serde(default)]
        #[ts(optional)]
        alerts_hash: Option<String>,
        id: String,
        alert: ServiceAlert,
    },
    /// The document with alert `id` ended at `at` (Unix seconds, the
    /// host's clock). An id the document does not hold is an error.
    End {
        alerts_hash: String,
        id: String,
        #[ts(type = "number")]
        at: u64,
    },
    /// The agency's list at `now` (Unix seconds). A document absent is
    /// an empty one.
    List {
        #[serde(default)]
        #[ts(optional)]
        alerts_hash: Option<String>,
        #[ts(type = "number")]
        now: u64,
    },
    /// What a person is told about the alerts that are not withdrawn:
    /// each id the network does not have, and each text with no
    /// translation for one of `languages` (the deployment's).
    Flags {
        alerts_hash: String,
        network_hash: String,
        #[serde(default)]
        #[ts(optional)]
        languages: Option<Vec<String>>,
    },
    /// What a template write must pass: every refusal, or the canonical
    /// document and its hash.
    ValidateTemplates {
        #[ts(as = "AlertTemplates")]
        templates: cap_abi::Base<AlertTemplates>,
    },
    /// The templates document with `template` placed under `id`, then
    /// checked as a write is. A document absent is an empty one.
    PutTemplate {
        #[serde(default)]
        #[ts(optional)]
        templates_hash: Option<String>,
        id: String,
        template: AlertTemplate,
    },
    /// The templates offered, by name: each not retired. A document
    /// absent is an empty one.
    Templates {
        #[serde(default)]
        #[ts(optional)]
        templates_hash: Option<String>,
    },
    /// Template `template` written out: the stops' and routes' names
    /// from the network, and the dates from `input`.
    Fill { templates_hash: String, template: String, network_hash: String, input: FillInput },
    /// The documents as the alerts log's records (D-197): a
    /// `service_alert` record for each alert, an `alert_template` record
    /// for each template, and each document's own record. Each document
    /// left out gives no records.
    LogRecords {
        #[serde(default)]
        #[ts(optional)]
        alerts_hash: Option<String>,
        #[serde(default)]
        #[ts(optional)]
        templates_hash: Option<String>,
    },
    /// A write to the alerts log, whole (D-228): each op of the write's
    /// intent over the documents at the head (`heads.service_alerts`,
    /// `heads.alert_templates`, registered by the host; absent: empty),
    /// then the entry that records what changed. The ops: `put` (`{id,
    /// alert}`), `end` (`{id, at}`; the header's clock when `at` is
    /// absent), `put_template`
    /// (`{id, template}`), and the whole-document writes `validate`
    /// (`{alerts, from}`) and `validate_templates` (`{templates, from}`),
    /// where `from` is the head the writer read (empty: none). An older
    /// service alerts document comes forward first (D-216). A refused op
    /// answers its refusals, an op that does not run answers why
    /// (`failed`), and a whole document over a head it did not read
    /// answers `moved`: no entry in any of these. `heads` names each
    /// document the ops wrote.
    Receive {
        #[ts(type = "{ checkpoint?: unknown, header: unknown, unread?: Array<{ seq: number, text: string }> }")]
        write: transit_log::Write,
        #[serde(default)]
        #[ts(type = "Record<string, string>")]
        heads: std::collections::BTreeMap<String, String>,
        #[ts(type = "Array<{ cap: string, op: string, args?: unknown }>")]
        ops: Vec<transit_log::OpCall>,
    },
    /// One document (`kind`: `service_alerts` or `alert_templates`)
    /// assembled from the alerts log's records of its kind, registered
    /// and answered as its canonical bytes and hash. A record in another
    /// version is refused, naming both.
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
#[ts(export, export_to = "cap_alerts/")]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Response {
    Validate {
        refusals: Vec<AlertRefusal>,
        document: Option<String>,
        document_hash: Option<String>,
        /// The fields the document carries that this build does not
        /// know (D-167): kept in it, and flagged for a person, never
        /// refused. Absent when every field is known.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        #[ts(optional, as = "Option<Vec<String>>")]
        unknown_fields: Vec<String>,
    },
    Put { refusals: Vec<AlertRefusal>, alerts_hash: Option<String>, alerts: Option<ServiceAlerts> },
    End { refusals: Vec<AlertRefusal>, alerts_hash: Option<String>, alerts: Option<ServiceAlerts> },
    List {
        #[serde(flatten)]
        #[ts(flatten)]
        list: AlertList,
    },
    Flags { flags: Vec<AlertFlag> },
    ValidateTemplates {
        refusals: Vec<TemplateRefusal>,
        document: Option<String>,
        document_hash: Option<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        #[ts(optional, as = "Option<Vec<String>>")]
        unknown_fields: Vec<String>,
    },
    PutTemplate { refusals: Vec<TemplateRefusal>, templates_hash: Option<String>, templates: Option<AlertTemplates> },
    Templates { templates: Vec<transit_alerts::TemplateRow> },
    Fill { filled: Filled },
    LogRecords {
        /// The records of each kind, as the log capability's `compose`
        /// takes them.
        #[ts(type = "Array<{ kind: string, version: number, records: Record<string, unknown> }>")]
        next: Vec<transit_log::KindRecords>,
    },
    /// What the write adds to the alerts log.
    Receive(transit_log::Received),
    LogDocument {
        /// The document's hash (`alerts_hash` or `templates_hash` by its
        /// kind).
        document_hash: String,
        /// The document's canonical bytes, which a host serves under
        /// `document_hash`.
        document: String,
    },
}

/// The next alerts document, checked and registered, or its refusals.
fn registered(next: ServiceAlerts) -> Result<(Vec<AlertRefusal>, Option<String>, Option<ServiceAlerts>), String> {
    Ok(match next.checked() {
        Ok(next) => {
            let (hash, held) = kind::<transit_alerts::ServiceAlerts>().register(next)?;
            (vec![], Some(hash), Some((*held).clone()))
        }
        Err(refusals) => (refusals, None, None),
    })
}

/// An op of a write to the alerts log, as a page sends it.
#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
enum Intent {
    Put { id: String, alert: ServiceAlert },
    /// `at` is when the person ended it (Unix seconds): the header's
    /// clock when the page names none.
    End {
        id: String,
        #[serde(default)]
        at: Option<u64>,
    },
    PutTemplate { id: String, template: AlertTemplate },
    Validate {
        alerts: serde_json::Value,
        #[serde(default)]
        from: String,
    },
    ValidateTemplates {
        templates: serde_json::Value,
        #[serde(default)]
        from: String,
    },
}

fn to_value<T: Serialize>(v: &T) -> Result<serde_json::Value, String> {
    serde_json::to_value(v).map_err(|e| e.to_string())
}

/// The documents a write runs over: each head, whether an op wrote it,
/// and the unknown fields of the whole documents written.
#[derive(Default)]
struct Heads {
    alerts: Option<String>,
    templates: Option<String>,
    wrote_alerts: bool,
    wrote_templates: bool,
    unknown_fields: Vec<String>,
}

/// One op of a write over the heads: none when it ran, the answer that
/// ends the write when it was refused or moved, or the error of an op
/// that did not run.
fn step(heads: &mut Heads, intent: Intent, op: usize, now: u64) -> Result<Option<transit_log::Received>, String> {
    let refused = |refusals: serde_json::Value| Ok(Some(transit_log::Received::refused(op, refusals)));
    match intent {
        Intent::Put { id, alert } => match dispatch(Request::Put { alerts_hash: heads.alerts.clone(), id, alert })? {
            Response::Put { alerts_hash: Some(h), .. } => (heads.alerts, heads.wrote_alerts) = (Some(h), true),
            Response::Put { refusals, .. } => return refused(to_value(&refusals)?),
            _ => unreachable!("put answers put"),
        },
        Intent::End { id, at } => {
            let held = heads.alerts.clone().ok_or_else(|| format!("no alert {id:?} in the document"))?;
            match dispatch(Request::End { alerts_hash: held, id, at: at.unwrap_or(now) })? {
                Response::End { alerts_hash: Some(h), .. } => (heads.alerts, heads.wrote_alerts) = (Some(h), true),
                Response::End { refusals, .. } => return refused(to_value(&refusals)?),
                _ => unreachable!("end answers end"),
            }
        }
        Intent::PutTemplate { id, template } => match dispatch(Request::PutTemplate { templates_hash: heads.templates.clone(), id, template })? {
            Response::PutTemplate { templates_hash: Some(h), .. } => (heads.templates, heads.wrote_templates) = (Some(h), true),
            Response::PutTemplate { refusals, .. } => return refused(to_value(&refusals)?),
            _ => unreachable!("put_template answers put_template"),
        },
        Intent::Validate { alerts: doc, from } => {
            if from != heads.alerts.clone().unwrap_or_default() {
                return Ok(Some(transit_log::Received::moved(heads.alerts.clone())));
            }
            // An absent version reads as v1, as the library reads it.
            let older = transit_alerts::version::schema_version_of(&doc)? < transit_alerts::ALERTS_VERSION;
            let doc = if older { transit_alerts::version::migrate_to(doc, transit_alerts::ALERTS_VERSION)?.0 } else { doc };
            let (doc, unknown) = cap_abi::read_base::<ServiceAlerts>(doc)?;
            match doc.checked() {
                Ok(doc) => (heads.alerts, heads.wrote_alerts) = (Some(kind::<transit_alerts::ServiceAlerts>().register(doc)?.0), true),
                Err(refusals) => return refused(to_value(&refusals)?),
            }
            heads.unknown_fields.extend(unknown);
        }
        Intent::ValidateTemplates { templates: doc, from } => {
            if from != heads.templates.clone().unwrap_or_default() {
                return Ok(Some(transit_log::Received::moved(heads.templates.clone())));
            }
            let (doc, unknown) = cap_abi::read_base::<AlertTemplates>(doc)?;
            match doc.checked() {
                Ok(doc) => (heads.templates, heads.wrote_templates) = (Some(kind::<transit_alerts::AlertTemplates>().register(doc)?.0), true),
                Err(refusals) => return refused(to_value(&refusals)?),
            }
            heads.unknown_fields.extend(unknown);
        }
    }
    Ok(None)
}

/// A write's ops over the heads, then its entry (`Request::Receive`).
fn receive(write: transit_log::Write, given: std::collections::BTreeMap<String, String>, ops: Vec<transit_log::OpCall>) -> Result<Response, String> {
    let answer = |received| Ok(Response::Receive(received));
    if let Some(seq) = write.landed()? {
        return answer(transit_log::Received::landed(seq));
    }
    let (_, now) = write.stamp();
    let mut heads = Heads { alerts: given.get("service_alerts").cloned(), templates: given.get("alert_templates").cloned(), ..Default::default() };
    for (i, call) in ops.iter().enumerate() {
        if call.cap != "alerts" {
            let error = format!("{} {} is not a write to the alerts log (alerts: put, end, put_template, validate, validate_templates)", call.cap, call.op);
            return answer(transit_log::Received::failed(i, error));
        }
        match call.request::<Intent>().and_then(|intent| step(&mut heads, intent, i, now)) {
            Ok(None) => {}
            Ok(Some(ended)) => return answer(ended),
            Err(e) => return answer(transit_log::Received::failed(i, e)),
        }
    }
    let (alerts, templates) = (heads.alerts.filter(|_| heads.wrote_alerts), heads.templates.filter(|_| heads.wrote_templates));
    let records = Request::LogRecords { alerts_hash: alerts.clone(), templates_hash: templates.clone() };
    let Response::LogRecords { next } = dispatch(records)? else { unreachable!("log_records answers log_records") };
    let mut received = transit_log::receive(&write, &next, &cap_abi::record_hash, &|e| cap_abi::canonical_bytes(e))?;
    received.heads.extend(alerts.map(|h| ("service_alerts".to_string(), h)));
    received.heads.extend(templates.map(|h| ("alert_templates".to_string(), h)));
    heads.unknown_fields.sort();
    heads.unknown_fields.dedup();
    received.unknown_fields = heads.unknown_fields;
    answer(received)
}

fn held_alerts(hash: Option<&str>) -> Result<std::sync::Arc<ServiceAlerts>, String> {
    match hash {
        Some(h) => kind::<transit_alerts::ServiceAlerts>().require(h),
        None => Ok(std::sync::Arc::new(ServiceAlerts::default())),
    }
}

/// The typed dispatch (native hosts call this directly). Failures are
/// `{"error": ...}`, never panics: a total seam.
pub fn dispatch(request: Request) -> Result<Response, String> {
    match request {
        Request::Validate { alerts } => Ok(match alerts.0.checked() {
            Ok(alerts) => {
                let document = cap_abi::canonical_bytes(&alerts)?;
                let document_hash = cap_abi::content_hash(document.as_bytes());
                Response::Validate { refusals: vec![], document: Some(document), document_hash: Some(document_hash), unknown_fields: cap_abi::request_unknown_fields() }
            }
            Err(refusals) => Response::Validate { refusals, document: None, document_hash: None, unknown_fields: cap_abi::request_unknown_fields() },
        }),
        Request::Put { alerts_hash, id, alert } => {
            let next = match alerts_hash {
                Some(h) => kind::<transit_alerts::ServiceAlerts>().require_writable(&h)?.put(&id, alert),
                None => ServiceAlerts::default().put(&id, alert),
            };
            let (refusals, alerts_hash, alerts) = registered(next)?;
            Ok(Response::Put { refusals, alerts_hash, alerts })
        }
        Request::End { alerts_hash, id, at } => {
            let next = kind::<transit_alerts::ServiceAlerts>().require_writable(&alerts_hash)?.end(&id, at).ok_or_else(|| format!("no alert {id:?} in the document"))?;
            let (refusals, alerts_hash, alerts) = registered(next)?;
            Ok(Response::End { refusals, alerts_hash, alerts })
        }
        Request::List { alerts_hash, now } => Ok(Response::List { list: held_alerts(alerts_hash.as_deref())?.list(now) }),
        Request::Flags { alerts_hash, network_hash, languages } => {
            let alerts = kind::<transit_alerts::ServiceAlerts>().require(&alerts_hash)?;
            let network = kind::<transit_network::Network>().require(&network_hash)?;
            Ok(Response::Flags { flags: alerts.flags(&network, languages.as_deref().unwrap_or_default()) })
        }
        Request::ValidateTemplates { templates } => Ok(match templates.0.checked() {
            Ok(templates) => {
                let document = cap_abi::canonical_bytes(&templates)?;
                let document_hash = cap_abi::content_hash(document.as_bytes());
                Response::ValidateTemplates { refusals: vec![], document: Some(document), document_hash: Some(document_hash), unknown_fields: cap_abi::request_unknown_fields() }
            }
            Err(refusals) => Response::ValidateTemplates { refusals, document: None, document_hash: None, unknown_fields: cap_abi::request_unknown_fields() },
        }),
        Request::PutTemplate { templates_hash, id, template } => {
            let next = match templates_hash {
                Some(h) => kind::<transit_alerts::AlertTemplates>().require_writable(&h)?.put(&id, template),
                None => AlertTemplates::default().put(&id, template),
            };
            Ok(match next.checked() {
                Ok(next) => {
                    let (hash, held) = kind::<transit_alerts::AlertTemplates>().register(next)?;
                    Response::PutTemplate { refusals: vec![], templates_hash: Some(hash), templates: Some((*held).clone()) }
                }
                Err(refusals) => Response::PutTemplate { refusals, templates_hash: None, templates: None },
            })
        }
        Request::Templates { templates_hash } => Ok(Response::Templates {
            templates: match templates_hash {
                Some(h) => kind::<transit_alerts::AlertTemplates>().require(&h)?.offered(),
                None => Vec::new(),
            },
        }),
        Request::Fill { templates_hash, template, network_hash, input } => {
            let templates = kind::<transit_alerts::AlertTemplates>().require(&templates_hash)?;
            let t = templates.templates.get(&template).ok_or_else(|| format!("no template {template:?} in the document"))?;
            let network = kind::<transit_network::Network>().require(&network_hash)?;
            Ok(Response::Fill { filled: transit_alerts::fill(t, &network, &input) })
        }
        Request::LogRecords { alerts_hash, templates_hash } => {
            let mut next = Vec::new();
            if let Some(h) = alerts_hash {
                let doc = serde_json::to_value(&*kind::<transit_alerts::ServiceAlerts>().require(&h)?).map_err(|e| e.to_string())?;
                next.extend(transit_log::document_records(&doc, "alerts", "service_alert", "service_alerts", transit_alerts::ALERTS_VERSION)?);
            }
            if let Some(h) = templates_hash {
                let doc = serde_json::to_value(&*kind::<transit_alerts::AlertTemplates>().require(&h)?).map_err(|e| e.to_string())?;
                next.extend(transit_log::document_records(&doc, "templates", "alert_template", "alert_templates", transit_alerts::TEMPLATES_VERSION)?);
            }
            Ok(Response::LogRecords { next })
        }
        Request::Receive { write, heads, ops } => receive(write, heads, ops),
        Request::LogDocument { kind, records, document } => {
            let (document_hash, document) = match kind.as_str() {
                "service_alerts" => {
                    // The log keeps each record in the version it was
                    // written in: bring each one forward (D-216).
                    let version = transit_alerts::ALERTS_VERSION;
                    let forward = |kind: &str, held: &transit_log::Held| -> Result<transit_log::Held, String> {
                        let value = transit_alerts::version::record_forward(kind, held.value.clone(), held.version)?;
                        Ok(transit_log::Held { version, value })
                    };
                    let records = records.iter().map(|(id, held)| Ok((id.clone(), forward("service_alert", held)?))).collect::<Result<_, String>>()?;
                    let document = document.as_ref().map(|held| forward("service_alerts", held)).transpose()?;
                    let template = serde_json::json!({"kind": kind, "schema_version": version});
                    let alerts: ServiceAlerts = transit_log::assemble(template, document.as_ref(), "alerts", &records, false)?;
                    let bytes = cap_abi::canonical_bytes(&alerts)?;
                    (cap_abi::kernel::kind::<transit_alerts::ServiceAlerts>().register(alerts)?.0, bytes)
                }
                "alert_templates" => {
                    let template = serde_json::json!({"kind": kind, "schema_version": transit_alerts::TEMPLATES_VERSION});
                    let templates: AlertTemplates = transit_log::assemble(template, document.as_ref(), "templates", &records, false)?;
                    let bytes = cap_abi::canonical_bytes(&templates)?;
                    (cap_abi::kernel::kind::<transit_alerts::AlertTemplates>().register(templates)?.0, bytes)
                }
                other => return Err(format!("log_document: {other:?} is not service_alerts or alert_templates")),
            };
            Ok(Response::LogDocument { document_hash, document })
        }
    }
}

/// The JSON seam: `{"op": ...}` in, `{"ok": ...}` / `{"error": ...}`
/// out, byte-compatible with every other capability module.
pub fn dispatch_json(text: &str) -> String {
    cap_abi::answer(text, dispatch)
}

/// The alerts take their questions inline; a document registers on the
/// kernel's `load_service_alerts` (bulk op 272) or
/// `load_alert_templates` (bulk op 273).
pub fn dispatch_bytes(op: u32, _bytes: &[u8]) -> String {
    serde_json::json!({ "error": format!("unknown bulk op {op}: an alerts or templates document registers on the kernel's load_service_alerts or load_alert_templates") }).to_string()
}

/// The worked examples, which the generator runs (no module build turns
/// the feature on).
#[cfg(feature = "examples")]
pub mod examples;

cap_abi::capability! {
    id: "alerts",
    json: dispatch_json,
    bytes: dispatch_bytes,
    documents: [transit_alerts::ServiceAlerts, transit_alerts::AlertTemplates, transit_network::Network],
    request: Request,
    speaks: &[
        ("service_alerts", &[transit_alerts::ALERTS_VERSION]),
        ("alert_templates", &[transit_alerts::TEMPLATES_VERSION]),
        ("network", &[transit_network::SCHEMA_VERSION]),
    ],
}
