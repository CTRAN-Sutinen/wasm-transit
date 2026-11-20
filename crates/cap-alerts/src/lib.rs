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

use cap_abi::kernel::{ALERT_TEMPLATES, NETWORK, SERVICE_ALERTS};
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
            let (hash, held) = SERVICE_ALERTS.register(next)?;
            (vec![], Some(hash), Some((*held).clone()))
        }
        Err(refusals) => (refusals, None, None),
    })
}

fn held_alerts(hash: Option<&str>) -> Result<std::sync::Arc<ServiceAlerts>, String> {
    match hash {
        Some(h) => SERVICE_ALERTS.require(h),
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
                Some(h) => SERVICE_ALERTS.require_writable(&h)?.put(&id, alert),
                None => ServiceAlerts::default().put(&id, alert),
            };
            let (refusals, alerts_hash, alerts) = registered(next)?;
            Ok(Response::Put { refusals, alerts_hash, alerts })
        }
        Request::End { alerts_hash, id, at } => {
            let next = SERVICE_ALERTS.require_writable(&alerts_hash)?.end(&id, at).ok_or_else(|| format!("no alert {id:?} in the document"))?;
            let (refusals, alerts_hash, alerts) = registered(next)?;
            Ok(Response::End { refusals, alerts_hash, alerts })
        }
        Request::List { alerts_hash, now } => Ok(Response::List { list: held_alerts(alerts_hash.as_deref())?.list(now) }),
        Request::Flags { alerts_hash, network_hash, languages } => {
            let alerts = SERVICE_ALERTS.require(&alerts_hash)?;
            let network = NETWORK.require(&network_hash)?;
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
                Some(h) => ALERT_TEMPLATES.require_writable(&h)?.put(&id, template),
                None => AlertTemplates::default().put(&id, template),
            };
            Ok(match next.checked() {
                Ok(next) => {
                    let (hash, held) = ALERT_TEMPLATES.register(next)?;
                    Response::PutTemplate { refusals: vec![], templates_hash: Some(hash), templates: Some((*held).clone()) }
                }
                Err(refusals) => Response::PutTemplate { refusals, templates_hash: None, templates: None },
            })
        }
        Request::Templates { templates_hash } => Ok(Response::Templates {
            templates: match templates_hash {
                Some(h) => ALERT_TEMPLATES.require(&h)?.offered(),
                None => Vec::new(),
            },
        }),
        Request::Fill { templates_hash, template, network_hash, input } => {
            let templates = ALERT_TEMPLATES.require(&templates_hash)?;
            let t = templates.templates.get(&template).ok_or_else(|| format!("no template {template:?} in the document"))?;
            let network = NETWORK.require(&network_hash)?;
            Ok(Response::Fill { filled: transit_alerts::fill(t, &network, &input) })
        }
        Request::LogRecords { alerts_hash, templates_hash } => {
            let mut next = Vec::new();
            if let Some(h) = alerts_hash {
                let doc = serde_json::to_value(&*SERVICE_ALERTS.require(&h)?).map_err(|e| e.to_string())?;
                next.extend(transit_log::document_records(&doc, "alerts", "service_alert", "service_alerts", transit_alerts::ALERTS_VERSION)?);
            }
            if let Some(h) = templates_hash {
                let doc = serde_json::to_value(&*ALERT_TEMPLATES.require(&h)?).map_err(|e| e.to_string())?;
                next.extend(transit_log::document_records(&doc, "templates", "alert_template", "alert_templates", transit_alerts::TEMPLATES_VERSION)?);
            }
            Ok(Response::LogRecords { next })
        }
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
                    (SERVICE_ALERTS.register(alerts)?.0, bytes)
                }
                "alert_templates" => {
                    let template = serde_json::json!({"kind": kind, "schema_version": transit_alerts::TEMPLATES_VERSION});
                    let templates: AlertTemplates = transit_log::assemble(template, document.as_ref(), "templates", &records, false)?;
                    let bytes = cap_abi::canonical_bytes(&templates)?;
                    (ALERT_TEMPLATES.register(templates)?.0, bytes)
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

cap_abi::capability! {
    id: "alerts",
    json: dispatch_json,
    bytes: dispatch_bytes,
    documents: [SERVICE_ALERTS, ALERT_TEMPLATES, NETWORK],
}
