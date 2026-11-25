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
    Validate {
        #[ts(as = "ServiceAlerts")]
        alerts: cap_abi::Base<ServiceAlerts>,
    },
    Put {
        #[serde(default)]
        #[ts(optional)]
        alerts_hash: Option<String>,
        id: String,
        alert: ServiceAlert,
    },
    End {
        alerts_hash: String,
        id: String,
        #[ts(type = "number")]
        at: u64,
    },
    List {
        #[serde(default)]
        #[ts(optional)]
        alerts_hash: Option<String>,
        #[ts(type = "number")]
        now: u64,
    },
    Flags {
        alerts_hash: String,
        network_hash: String,
        #[serde(default)]
        #[ts(optional)]
        languages: Option<Vec<String>>,
    },
    ValidateTemplates {
        #[ts(as = "AlertTemplates")]
        templates: cap_abi::Base<AlertTemplates>,
    },
    PutTemplate {
        #[serde(default)]
        #[ts(optional)]
        templates_hash: Option<String>,
        id: String,
        template: AlertTemplate,
    },
    Templates {
        #[serde(default)]
        #[ts(optional)]
        templates_hash: Option<String>,
    },
    Fill { templates_hash: String, template: String, network_hash: String, input: FillInput },
    LogRecords {
        #[serde(default)]
        #[ts(optional)]
        alerts_hash: Option<String>,
        #[serde(default)]
        #[ts(optional)]
        templates_hash: Option<String>,
    },
    Receive {
        #[ts(type = "{ checkpoint?: unknown, header: unknown, unread?: Array<{ seq: number, text: string }> }")]
        write: transit_log::Write,
        #[serde(default)]
        #[ts(type = "Record<string, string>")]
        heads: std::collections::BTreeMap<String, String>,
        #[ts(type = "Array<{ cap: string, op: string, args?: unknown }>")]
        ops: Vec<transit_log::OpCall>,
    },
    LogDocument {
        kind: String,
        #[ts(type = "Record<string, { version: number, value: unknown }>")]
        records: std::collections::BTreeMap<String, transit_log::Held>,
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
        #[ts(type = "Array<{ kind: string, version: number, records: Record<string, unknown> }>")]
        next: Vec<transit_log::KindRecords>,
    },
    Receive(transit_log::Received),
    LogDocument {
        document_hash: String,
        document: String,
    },
}

fn registered(next: ServiceAlerts) -> Result<(Vec<AlertRefusal>, Option<String>, Option<ServiceAlerts>), String> {
    Ok(match next.checked() {
        Ok(next) => {
            let (hash, held) = kind::<transit_alerts::ServiceAlerts>().register(next)?;
            (vec![], Some(hash), Some((*held).clone()))
        }
        Err(refusals) => (refusals, None, None),
    })
}

#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
enum Intent {
    Put { id: String, alert: ServiceAlert },
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

#[derive(Default)]
struct Heads {
    alerts: Option<String>,
    templates: Option<String>,
    wrote_alerts: bool,
    wrote_templates: bool,
    unknown_fields: Vec<String>,
}

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
            // An absent version reads as v1.
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
                    // The log keeps each record in the version it was written
                    // in (D-216).
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

pub fn dispatch_json(text: &str) -> String {
    cap_abi::answer(text, dispatch)
}

pub fn dispatch_bytes(op: u32, _bytes: &[u8]) -> String {
    serde_json::json!({ "error": format!("unknown bulk op {op}: an alerts or templates document registers on the kernel's load_service_alerts or load_alert_templates") }).to_string()
}

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
