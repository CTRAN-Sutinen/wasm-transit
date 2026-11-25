//! cap-stops wraps transit-stops. A pick only reads the stops document
//! (D-157), and a plan copies a stop's record and never writes it (D-173).
//!
//! `receive` writes the stops log whole (D-197, D-228). The stops and the
//! stop requests are one log, because one entry can change a stop and the
//! request it carries out. So this crate also links transit-requests.

#![forbid(unsafe_code)]

use cap_abi::kernel::kind;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

pub use transit_stops::{Answer, Event, Refusal, Status, StatusCount, StopOn, Stops};
use transit_requests::{RequestRefusal, StopRequest, StopRequests, WorkOrder};

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "cap_stops/")]
pub struct SiteQuestion {
    pub key: String,
    pub answer: Answer,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "cap_stops/")]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Request {
    Validate {
        #[ts(as = "Stops")]
        stops: cap_abi::Base<Stops>,
    },
    StopsOn {
        stops_hash: String,
        date: String,
        #[serde(default)]
        #[ts(optional)]
        stop_ids: Option<Vec<String>>,
        #[serde(default)]
        #[ts(optional)]
        statuses: Option<Vec<Status>>,
    },
    StopsNear {
        stops_hash: String,
        date: String,
        lat: f64,
        lon: f64,
        #[serde(default)]
        #[ts(optional)]
        limit: Option<u32>,
        #[serde(default)]
        #[ts(optional)]
        radius_m: Option<f64>,
        #[serde(default)]
        #[ts(optional)]
        statuses: Option<Vec<Status>>,
    },
    Stop { stops_hash: String, stop_id: String, date: String },
    RequestWarnings {
        stops_hash: String,
        requests_hash: String,
        date: String,
        #[serde(default)]
        #[ts(optional)]
        stop: Option<String>,
    },
    Seed { network_hash: String },
    AddEvent {
        stops_hash: String,
        stop_id: String,
        event: Event,
        #[serde(default)]
        #[ts(optional)]
        new: Option<bool>,
    },
    Vocabulary {},
    CorrectEvent {
        stops_hash: String,
        stop_id: String,
        event_id: String,
        #[serde(default)]
        #[ts(optional)]
        event: Option<Event>,
        #[serde(default)]
        #[ts(optional)]
        saw: Option<String>,
    },
    LogRecords { stops_hash: String },
    Receive {
        #[ts(type = "{ checkpoint?: unknown, header: unknown, unread?: Array<{ seq: number, text: string }> }")]
        write: transit_log::Write,
        #[serde(default)]
        #[ts(type = "Record<string, string>")]
        heads: std::collections::BTreeMap<String, String>,
        #[ts(type = "Array<{ cap: string, op: string, args?: unknown }>")]
        ops: Vec<transit_log::OpCall>,
        #[serde(default)]
        #[ts(optional)]
        by: Option<String>,
    },
    LogDocument {
        #[ts(type = "Record<string, { version: number, value: unknown }>")]
        records: std::collections::BTreeMap<String, transit_log::Held>,
        #[serde(default)]
        #[ts(optional, type = "{ version: number, value: unknown }")]
        document: Option<transit_log::Held>,
    },
    ReadRecords { network_hash: String, stops_hash: String, on: String },
    Records { stops_hash: String, stop_ids: Vec<String>, on: String },
    TakeStops { network_hash: String, stops_hash: String, stop_ids: Vec<String>, on: String },
    ReadPick {
        network_hash: String,
        stops_hash: String,
        #[serde(default)]
        #[ts(optional)]
        published: Option<transit_plans::PublishedPlans>,
    },
    ExportPick {
        network_hash: String,
        stops_hash: String,
        #[serde(default)]
        #[ts(optional)]
        published: Option<transit_plans::PublishedPlans>,
    },
    VisitWarnings {
        network_hash: String,
        stops_hash: String,
        from: String,
        #[serde(default)]
        #[ts(optional)]
        until: Option<String>,
    },
    ExportVersions {
        network_hash: String,
        stops_hash: String,
        from: String,
        #[serde(default)]
        #[ts(optional)]
        until: Option<String>,
    },
    ImportEvents {
        stops_hash: String,
        network_hash: String,
        #[serde(default)]
        #[ts(optional)]
        on: Option<String>,
        #[serde(default)]
        #[ts(optional)]
        feed: Option<String>,
    },
    FeedEvents { stops_hash: String, feed: String },
    Questions {},
    AttributeSurveys {
        #[serde(default)]
        #[ts(optional)]
        stops_hash: Option<String>,
        stops: Stops,
        by: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "cap_stops/")]
pub struct RequestWarning {
    pub request: String,
    pub stop: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "cap_stops/")]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Response {
    Validate {
        refusals: Vec<Refusal>,
        document: Option<String>,
        document_hash: Option<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        #[ts(optional, as = "Option<Vec<String>>")]
        unknown_fields: Vec<String>,
    },
    StopsOn { stops: Vec<StopOn>, counts: Vec<StatusCount> },
    StopsNear { stops: Vec<transit_stops::NearStop> },
    Stop {
        stop: Option<StopOn>,
        events: Vec<Event>,
        #[ts(type = "Array<\"planned\" | \"permitted\" | \"built\" | \"in_service\" | \"suspended\" | \"retired\">")]
        next: Vec<String>,
    },
    RequestWarnings { warnings: Vec<RequestWarning> },
    Seed { stops_hash: String, stops: Stops },
    AddEvent { refusals: Vec<Refusal>, stops_hash: Option<String>, stops: Option<Stops> },
    Vocabulary { statuses: Vec<String> },
    CorrectEvent {
        refusals: Vec<Refusal>,
        stops_hash: Option<String>,
        stops: Option<Stops>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        conflict: Option<String>,
    },
    LogRecords {
        #[ts(type = "Array<{ kind: string, version: number, records: Record<string, unknown> }>")]
        next: Vec<transit_log::KindRecords>,
    },
    Receive(transit_log::Received),
    LogDocument {
        document_hash: String,
        document: String,
    },
    /// RFC 8785 canonical bytes, so a host stores what the hash covers (D-169).
    ReadRecords {
        network_hash: String,
        #[ts(as = "transit_network::Network")]
        network: cap_abi::Canonical<transit_network::Network>,
    },
    ReadPick {
        #[serde(skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        period: Option<transit_stops::stop_record::PickPeriod>,
        network_hash: String,
        #[ts(as = "transit_network::Network")]
        network: cap_abi::Canonical<transit_network::Network>,
        warnings: Vec<transit_network::NetworkWarning>,
    },
    ExportPick {
        network_hash: String,
        #[ts(as = "transit_network::Network")]
        network: cap_abi::Canonical<transit_network::Network>,
    },
    Records { records: std::collections::BTreeMap<String, transit_network::Stop> },
    TakeStops { edits: Vec<transit_network::NetworkEdit> },
    VisitWarnings { warnings: Vec<transit_network::NetworkWarning> },
    ExportVersions {
        network_hash: String,
        #[ts(as = "transit_network::Network")]
        network: cap_abi::Canonical<transit_network::Network>,
    },
    ImportEvents {
        stops_hash: String,
        stops: Stops,
        on: String,
        added: Vec<transit_stops::stop_record::AddedEvent>,
        unlisted: Vec<String>,
        unapplied: Vec<String>,
    },
    Questions { site: Vec<SiteQuestion>, amenities: Vec<String> },
    FeedEvents { events: Vec<transit_stops::stop_record::AddedEvent> },
    AttributeSurveys { stops: Stops },
}

fn parse_date(s: &str) -> Result<chrono::NaiveDate, String> {
    chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").map_err(|e| format!("invalid date {s:?}, expected YYYY-MM-DD: {e}"))
}

pub fn dispatch(request: Request) -> Result<Response, String> {
    match request {
        Request::AttributeSurveys { stops_hash, stops, by } => {
            let stops = match stops_hash {
                Some(h) => kind::<transit_stops::Stops>().require(&h)?.attribute_surveys(stops, &by),
                None => Stops::default().attribute_surveys(stops, &by),
            };
            Ok(Response::AttributeSurveys { stops })
        }
        Request::Questions {} => Ok(Response::Questions {
            site: transit_stops::SITE.iter().map(|(key, answer)| SiteQuestion { key: key.to_string(), answer: answer.clone() }).collect(),
            amenities: transit_stops::AMENITIES.iter().map(|a| a.to_string()).collect(),
        }),
        Request::Validate { stops } => {
            Ok(match stops.0.identified().checked() {
                Ok(stops) => {
                    let document = cap_abi::canonical_bytes(&stops)?;
                    let document_hash = cap_abi::content_hash(document.as_bytes());
                    Response::Validate { refusals: vec![], document: Some(document), document_hash: Some(document_hash), unknown_fields: cap_abi::request_unknown_fields() }
                }
                Err(refusals) => Response::Validate { refusals, document: None, document_hash: None, unknown_fields: cap_abi::request_unknown_fields() },
            })
        }
        Request::StopsOn { stops_hash, date, stop_ids, statuses } => {
            let date = parse_date(&date)?;
            let stops = kind::<transit_stops::Stops>().require(&stops_hash)?;
            let listed = match stop_ids {
                None => stops.on_date(date),
                Some(ids) => {
                    let mut ids = ids;
                    ids.sort();
                    ids.dedup();
                    ids.iter().filter_map(|id| stops.stop_on(id, date)).collect()
                }
            };
            Ok(Response::StopsOn {
                stops: listed.into_iter().filter(|s| statuses.as_ref().is_none_or(|w| w.contains(&s.status))).collect(),
                counts: stops.counts_on(date),
            })
        }
        Request::StopsNear { stops_hash, date, lat, lon, limit, radius_m, statuses } => {
            let date = parse_date(&date)?;
            if !(-90.0..=90.0).contains(&lat) || !(-180.0..=180.0).contains(&lon) {
                return Err(format!("the point {lat}, {lon} is not on the earth"));
            }
            if radius_m.is_some_and(|r| !(r >= 0.0)) {
                return Err("radius_m must not be negative".into());
            }
            let stops = kind::<transit_stops::Stops>().require(&stops_hash)?;
            Ok(Response::StopsNear { stops: stops.near(date, lat, lon, limit.unwrap_or(10) as usize, radius_m, statuses.as_deref()) })
        }
        Request::RequestWarnings { stops_hash, requests_hash, date, stop } => {
            let date = parse_date(&date)?;
            let stops = kind::<transit_stops::Stops>().require(&stops_hash)?;
            let requests = kind::<transit_requests::StopRequests>().require(&requests_hash)?;
            let warnings = requests
                .requests
                .iter()
                .filter(|(_, r)| stop.as_ref().is_none_or(|s| *s == r.stop))
                .filter_map(|(id, r)| transit_stops::request_rule::request_rule(&stops, id, r, date))
                .map(|b| RequestWarning { reason: b.reason(), request: b.request, stop: b.stop })
                .collect();
            Ok(Response::RequestWarnings { warnings })
        }
        Request::Stop { stops_hash, stop_id, date } => {
            let date = parse_date(&date)?;
            let stops = kind::<transit_stops::Stops>().require(&stops_hash)?;
            let events = stops.stops.get(&stop_id).ok_or_else(|| format!("no stop {stop_id:?} in the stops document"))?;
            let next = stops.next_events(&stop_id, date).iter().map(|k| k.as_str().to_string()).collect();
            Ok(Response::Stop { stop: stops.stop_on(&stop_id, date), events: events.clone(), next })
        }
        Request::Seed { network_hash } => {
            let network = kind::<transit_network::Network>().require(&network_hash)?;
            let seeded = transit_stops::stop_record::seed(&network).identified().checked().map_err(|refusals| {
                let reasons: Vec<String> = refusals.iter().map(|r| r.to_string()).collect();
                format!("the network's stops do not make a stops document: {}", reasons.join("; "))
            })?;
            let (stops_hash, held) = kind::<transit_stops::Stops>().register(seeded)?;
            Ok(Response::Seed { stops_hash, stops: (*held).clone() })
        }
        Request::Vocabulary {} => Ok(Response::Vocabulary { statuses: Status::ALL.iter().map(|s| s.as_str().to_string()).collect() }),
        Request::AddEvent { stops_hash, stop_id, event, new } => {
            let held = kind::<transit_stops::Stops>().require_writable(&stops_hash)?;
            if let Some(taken) = new.unwrap_or(false).then(|| held.refuse_taken(&stop_id)).flatten() {
                return Ok(Response::AddEvent { refusals: vec![taken], stops_hash: None, stops: None });
            }
            let next = held.add_event(&stop_id, event);
            Ok(match next.identified().checked() {
                Ok(next) => {
                    let (stops_hash, next) = kind::<transit_stops::Stops>().register(next)?;
                    Response::AddEvent { refusals: vec![], stops_hash: Some(stops_hash), stops: Some((*next).clone()) }
                }
                Err(refusals) => Response::AddEvent { refusals, stops_hash: None, stops: None },
            })
        }
        Request::CorrectEvent { stops_hash, stop_id, event_id, event, saw } => {
            let held = kind::<transit_stops::Stops>().require_writable(&stops_hash)?;
            if let Some(saw) = saw {
                let now = held
                    .stops
                    .get(&stop_id)
                    .and_then(|events| events.iter().find(|e| e.id == event_id))
                    .map(cap_abi::canonical_hash)
                    .transpose()?;
                if now.as_deref() != Some(saw.as_str()) {
                    return Ok(Response::CorrectEvent { refusals: vec![], stops_hash: None, stops: None, conflict: Some(event_id) });
                }
            }
            let next = held
                .correct_event_by_id(&stop_id, &event_id, event)
                .ok_or_else(|| format!("no event {event_id:?} of stop {stop_id:?} in the stops document"))?;
            Ok(match next.identified().checked() {
                Ok(next) => {
                    let (stops_hash, next) = kind::<transit_stops::Stops>().register(next)?;
                    Response::CorrectEvent { refusals: vec![], stops_hash: Some(stops_hash), stops: Some((*next).clone()), conflict: None }
                }
                Err(refusals) => Response::CorrectEvent { refusals, stops_hash: None, stops: None, conflict: None },
            })
        }
        Request::LogRecords { stops_hash } => {
            let held = kind::<transit_stops::Stops>().require(&stops_hash)?;
            let doc = serde_json::to_value(&*held).map_err(|e| e.to_string())?;
            Ok(Response::LogRecords { next: transit_log::document_records(&doc, "stops", "stop", "stops", transit_stops::SCHEMA_VERSION)? })
        }
        Request::LogDocument { records, document } => {
            let template = serde_json::json!({"kind": "stops", "schema_version": transit_stops::SCHEMA_VERSION});
            let stops: Stops = transit_log::assemble(template, document.as_ref(), "stops", &records, false)?;
            let document = cap_abi::canonical_bytes(&stops)?;
            let (document_hash, _) = kind::<transit_stops::Stops>().register(stops)?;
            Ok(Response::LogDocument { document_hash, document })
        }
        Request::Receive { write, heads, ops, by } => receive(write, heads, ops, by),
        Request::ReadRecords { network_hash, stops_hash, on } => {
            let on = parse_date(&on)?;
            let network = kind::<transit_network::Network>().require(&network_hash)?;
            let stops = kind::<transit_stops::Stops>().require(&stops_hash)?;
            let read = transit_stops::stop_record::read_records(&network, &stops, on);
            let (network_hash, read) = kind::<transit_network::Network>().register(read)?;
            Ok(Response::ReadRecords { network_hash, network: cap_abi::Canonical((*read).clone()) })
        }
        Request::ReadPick { network_hash, stops_hash, published } => {
            let network = kind::<transit_network::Network>().require(&network_hash)?;
            let stops = kind::<transit_stops::Stops>().require(&stops_hash)?;
            match transit_stops::stop_record::read_pick(&network, &stops, published.as_ref()) {
                Some(read) => {
                    let (network_hash, network) = kind::<transit_network::Network>().register(read.network)?;
                    Ok(Response::ReadPick { period: Some(read.period), network_hash, network: cap_abi::Canonical((*network).clone()), warnings: read.warnings })
                }
                None => Ok(Response::ReadPick { period: None, network_hash, network: cap_abi::Canonical((*network).clone()), warnings: Vec::new() }),
            }
        }
        Request::ExportPick { network_hash, stops_hash, published } => {
            let network = kind::<transit_network::Network>().require(&network_hash)?;
            let stops = kind::<transit_stops::Stops>().require(&stops_hash)?;
            let exported = transit_stops::stop_record::export_pick(&network, &stops, published.as_ref()).unwrap_or_else(|| (*network).clone());
            let (network_hash, exported) = kind::<transit_network::Network>().register(exported)?;
            Ok(Response::ExportPick { network_hash, network: cap_abi::Canonical((*exported).clone()) })
        }
        Request::Records { stops_hash, stop_ids, on } => {
            let on = parse_date(&on)?;
            let stops = kind::<transit_stops::Stops>().require(&stops_hash)?;
            Ok(Response::Records { records: transit_stops::stop_record::records(&stops, &stop_ids, on) })
        }
        Request::TakeStops { network_hash, stops_hash, stop_ids, on } => {
            let on = parse_date(&on)?;
            let network = kind::<transit_network::Network>().require(&network_hash)?;
            let stops = kind::<transit_stops::Stops>().require(&stops_hash)?;
            match transit_stops::stop_record::take_stops(&network, &stops, &stop_ids, on) {
                Ok(edits) => Ok(Response::TakeStops { edits }),
                Err(missing) => Err(format!("not in the stops record on {on}: {}", missing.join(", ")).into()),
            }
        }
        Request::VisitWarnings { network_hash, stops_hash, from, until } => {
            let from = parse_date(&from)?;
            let until = until.as_deref().map(parse_date).transpose()?;
            if until.is_some_and(|u| u <= from) {
                return Err("until must come after from".into());
            }
            let network = kind::<transit_network::Network>().require(&network_hash)?;
            let stops = kind::<transit_stops::Stops>().require(&stops_hash)?;
            Ok(Response::VisitWarnings { warnings: transit_stops::stop_record::visit_warnings(&network, &stops, from, until) })
        }
        Request::ExportVersions { network_hash, stops_hash, from, until } => {
            let from = parse_date(&from)?;
            let until = until.as_deref().map(parse_date).transpose()?;
            if until.is_some_and(|u| u <= from) {
                return Err("until must come after from".into());
            }
            let network = kind::<transit_network::Network>().require(&network_hash)?;
            let stops = kind::<transit_stops::Stops>().require(&stops_hash)?;
            let exported = transit_stops::stop_record::export_versions(&network, &stops, from, until);
            let (network_hash, exported) = kind::<transit_network::Network>().register(exported)?;
            Ok(Response::ExportVersions { network_hash, network: cap_abi::Canonical((*exported).clone()) })
        }
        Request::FeedEvents { stops_hash, feed } => {
            let stops = kind::<transit_stops::Stops>().require(&stops_hash)?;
            Ok(Response::FeedEvents { events: transit_stops::stop_record::feed_events(&stops, &feed) })
        }
        Request::ImportEvents { stops_hash, network_hash, on, feed } => {
            let network = kind::<transit_network::Network>().require(&network_hash)?;
            let on = match on {
                Some(on) => parse_date(&on)?,
                None => transit_network::first_service_date(&network).ok_or("the feed runs no service, so it has no first service date: name `on`")?,
            };
            let stops = kind::<transit_stops::Stops>().require(&stops_hash)?;
            let imported = match feed {
                Some(feed) => transit_stops::stop_record::import_feed(&stops, &network, on, &feed),
                None => transit_stops::stop_record::import_events(&stops, &network, on),
            };
            let (stops_hash, next) = kind::<transit_stops::Stops>().register(imported.stops)?;
            Ok(Response::ImportEvents {
                stops_hash,
                stops: (*next).clone(),
                on: on.to_string(),
                added: imported.added,
                unlisted: imported.unlisted,
                unapplied: imported.unapplied,
            })
        }
    }
}

#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
enum StopsIntent {
    AddEvent {
        stop_id: String,
        event: Event,
        #[serde(default)]
        new: Option<bool>,
    },
    CorrectEvent {
        stop_id: String,
        event_id: String,
        #[serde(default)]
        event: Option<Event>,
        #[serde(default)]
        saw: Option<String>,
    },
    Validate {
        stops: serde_json::Value,
        #[serde(default)]
        from: String,
    },
    ImportEvents {
        network_hash: String,
        #[serde(default)]
        on: Option<String>,
        #[serde(default)]
        feed: Option<String>,
    },
    Migrate {},
}

#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
enum RequestsIntent {
    Put {
        id: String,
        request: StopRequest,
        #[serde(default)]
        new: Option<bool>,
    },
    RecordWorkOrder {
        id: String,
        work_order: WorkOrder,
    },
    Validate {
        requests: serde_json::Value,
        #[serde(default)]
        from: String,
    },
    Migrate {},
}

#[derive(Default)]
struct Heads {
    held_stops: Option<String>,
    stops: Option<String>,
    /// False when only `migrate` wrote the stops document. Then its surveys
    /// are not attributed and the document is not checked.
    check_stops: bool,
    requests: Option<String>,
    wrote_stops: Option<usize>,
    wrote_requests: Option<usize>,
    unknown_fields: Vec<String>,
}

fn to_value<T: Serialize>(v: &T) -> Result<serde_json::Value, String> {
    serde_json::to_value(v).map_err(|e| e.to_string())
}

const STOPS_LOG_OPS: &str = "stops: add_event, correct_event, validate, import_events, migrate; requests: put, record_work_order, validate, migrate";

/// None when the op ran. Otherwise the answer that ends the write.
fn step_stops(heads: &mut Heads, intent: StopsIntent, op: usize) -> Result<Option<transit_log::Received>, String> {
    let refused = |refusals: &[Refusal]| Ok(Some(transit_log::Received::refused(op, to_value(&refusals)?)));
    let held = || heads.stops.clone().ok_or_else(|| "no stops document yet".to_string());
    match intent {
        StopsIntent::AddEvent { stop_id, event, new } => match dispatch(Request::AddEvent { stops_hash: held()?, stop_id, event, new })? {
            Response::AddEvent { stops_hash: Some(h), .. } => (heads.stops, heads.wrote_stops) = (Some(h), Some(op)),
            Response::AddEvent { refusals, .. } => return refused(&refusals),
            _ => unreachable!("add_event answers add_event"),
        },
        StopsIntent::CorrectEvent { stop_id, event_id, event, saw } => match dispatch(Request::CorrectEvent { stops_hash: held()?, stop_id, event_id, event, saw })? {
            Response::CorrectEvent { conflict: Some(id), .. } => return Ok(Some(transit_log::Received::conflict(op, id))),
            Response::CorrectEvent { stops_hash: Some(h), .. } => (heads.stops, heads.wrote_stops) = (Some(h), Some(op)),
            Response::CorrectEvent { refusals, .. } => return refused(&refusals),
            _ => unreachable!("correct_event answers correct_event"),
        },
        StopsIntent::Validate { stops, from } => {
            if from != heads.stops.clone().unwrap_or_default() {
                return Ok(Some(transit_log::Received::moved(heads.stops.clone())));
            }
            let (doc, unknown) = cap_abi::read_base::<Stops>(stops)?;
            (heads.stops, heads.wrote_stops) = (Some(kind::<transit_stops::Stops>().register(doc)?.0), Some(op));
            heads.unknown_fields.extend(unknown);
        }
        StopsIntent::ImportEvents { network_hash, on, feed } => {
            let Some(stops_hash) = heads.stops.clone() else { return Ok(None) };
            match dispatch(Request::ImportEvents { stops_hash, network_hash, on, feed })? {
                Response::ImportEvents { added, .. } if added.is_empty() => {}
                Response::ImportEvents { stops_hash, .. } => (heads.stops, heads.wrote_stops) = (Some(stops_hash), Some(op)),
                _ => unreachable!("import_events answers import_events"),
            }
        }
        StopsIntent::Migrate {} => return Ok(migrate(heads, op)),
    }
    heads.check_stops = true;
    Ok(None)
}

fn step_requests(heads: &mut Heads, intent: RequestsIntent, op: usize) -> Result<Option<transit_log::Received>, String> {
    let refused = |refusals: &[RequestRefusal]| Ok(Some(transit_log::Received::refused(op, to_value(&refusals)?)));
    let held = |hash: &Option<String>| -> Result<StopRequests, String> {
        Ok(match hash {
            Some(h) => (*kind::<transit_requests::StopRequests>().require_writable(h)?).clone(),
            None => StopRequests::default(),
        })
    };
    let next = match intent {
        RequestsIntent::Put { id, request, new } => {
            let held = held(&heads.requests)?;
            if let Some(taken) = new.unwrap_or(false).then(|| held.refuse_taken(&id)).flatten() {
                return refused(&[taken]);
            }
            held.put(&id, request)
        }
        RequestsIntent::RecordWorkOrder { id, work_order } => held(&heads.requests)?.record_work_order(&id, work_order).ok_or_else(|| format!("no request {id:?} in the document"))?,
        RequestsIntent::Validate { requests, from } => {
            if from != heads.requests.clone().unwrap_or_default() {
                return Ok(Some(transit_log::Received::moved(heads.requests.clone())));
            }
            let (doc, unknown) = cap_abi::read_base::<StopRequests>(transit_requests::version::forward(requests)?)?;
            heads.unknown_fields.extend(unknown);
            doc
        }
        RequestsIntent::Migrate {} => return Ok(migrate(heads, op)),
    };
    match next.checked() {
        Ok(next) => (heads.requests, heads.wrote_requests) = (Some(kind::<transit_requests::StopRequests>().register(next)?.0), Some(op)),
        Err(refusals) => return refused(&refusals),
    }
    Ok(None)
}

fn migrate(heads: &mut Heads, op: usize) -> Option<transit_log::Received> {
    heads.wrote_stops = heads.wrote_stops.or(heads.stops.as_ref().map(|_| op));
    heads.wrote_requests = heads.wrote_requests.or(heads.requests.as_ref().map(|_| op));
    None
}

fn receive(write: transit_log::Write, given: std::collections::BTreeMap<String, String>, ops: Vec<transit_log::OpCall>, by: Option<String>) -> Result<Response, String> {
    let answer = |received| Ok(Response::Receive(received));
    if let Some(seq) = write.landed()? {
        return answer(transit_log::Received::landed(seq));
    }
    let stops = given.get("stops").cloned();
    let mut heads = Heads { held_stops: stops.clone(), stops, requests: given.get("stop_requests").cloned(), ..Default::default() };
    for (i, call) in ops.iter().enumerate() {
        let stepped = match call.cap.as_str() {
            "stops" => call.request::<StopsIntent>().and_then(|intent| step_stops(&mut heads, intent, i)),
            "requests" => call.request::<RequestsIntent>().and_then(|intent| step_requests(&mut heads, intent, i)),
            _ => Err(format!("{} {} is not a write to the stops log ({STOPS_LOG_OPS})", call.cap, call.op)),
        };
        match stepped {
            Ok(None) => {}
            Ok(Some(ended)) => return answer(ended),
            Err(e) => return answer(transit_log::Received::failed(i, e)),
        }
    }
    let mut next = Vec::new();
    let mut written = std::collections::BTreeMap::new();
    if let (Some(_), Some(now), false) = (heads.wrote_stops, heads.stops.clone(), heads.check_stops) {
        let Response::LogRecords { next: records } = dispatch(Request::LogRecords { stops_hash: now.clone() })? else { unreachable!("log_records answers log_records") };
        next.extend(records);
        written.insert("stops".to_string(), now);
    } else if let (Some(op), Some(now)) = (heads.wrote_stops, heads.stops.clone()) {
        // The signed-in writer is the surveyor, whatever name the client sent
        // (D-158).
        let by = by.as_deref().unwrap_or(&write.header.writer);
        let now = (*kind::<transit_stops::Stops>().require(&now)?).clone();
        let attributed = match &heads.held_stops {
            Some(h) => kind::<transit_stops::Stops>().require(h)?.attribute_surveys(now, by),
            None => Stops::default().attribute_surveys(now, by),
        };
        let checked = match attributed.identified().checked() {
            Ok(checked) => kind::<transit_stops::Stops>().register(checked)?.0,
            Err(refusals) => return answer(transit_log::Received::refused(op, to_value(&refusals)?)),
        };
        let Response::LogRecords { next: records } = dispatch(Request::LogRecords { stops_hash: checked.clone() })? else { unreachable!("log_records answers log_records") };
        next.extend(records);
        written.insert("stops".to_string(), checked);
    }
    if let (Some(_), Some(now)) = (heads.wrote_requests, heads.requests.clone()) {
        let doc = to_value(&*kind::<transit_requests::StopRequests>().require(&now)?)?;
        next.extend(transit_log::document_records(&doc, "requests", "stop_request", "stop_requests", transit_requests::SCHEMA_VERSION)?);
        written.insert("stop_requests".to_string(), now);
    }
    let after_stops = written.get("stops").cloned().or_else(|| heads.stops.clone());
    let rules = judge_requests(&write, given.get("stops"), given.get("stop_requests"), after_stops.as_ref(), heads.requests.as_ref())?;
    if !rules.refused.is_empty() {
        return answer(transit_log::Received { broken: rules.refused, ..Default::default() });
    }
    let mut received = transit_log::receive(&write, &next, &cap_abi::record_hash, &|e| cap_abi::canonical_bytes(e))?;
    if received.entry.is_some() {
        received.references.extend(rules.warned);
        received.references.sort();
    }
    received.heads = written;
    heads.unknown_fields.sort();
    heads.unknown_fields.dedup();
    received.unknown_fields = heads.unknown_fields;
    answer(received)
}

#[derive(Default)]
struct Judged {
    refused: Vec<transit_log::Broken>,
    warned: Vec<transit_log::Broken>,
}

/// A request that is already broken before the write is not refused, so a
/// person can still change or cancel it (D-233, D-238).
fn judge_requests(
    write: &transit_log::Write,
    stops_before: Option<&String>,
    requests_before: Option<&String>,
    stops_after: Option<&String>,
    requests_after: Option<&String>,
) -> Result<Judged, String> {
    use transit_stops::request_rule::{request_rule, RuleBreak};
    let mut out = Judged::default();
    let (Some(on), Some(requests_after)) = (write.today.as_deref().and_then(|d| parse_date(d).ok()), requests_after) else { return Ok(out) };
    let requests_after = kind::<transit_requests::StopRequests>().require(requests_after)?;
    let requests_before = requests_before.map(|h| kind::<transit_requests::StopRequests>().require(h)).transpose()?;
    let stops_before = stops_before.map(|h| kind::<transit_stops::Stops>().require(h)).transpose()?;
    let stops_after = match stops_after {
        Some(h) => Some(kind::<transit_stops::Stops>().require(h)?),
        None => stops_before.clone(),
    };
    let empty = Stops::default();
    let before = stops_before.as_deref().unwrap_or(&empty);
    let after = stops_after.as_deref().unwrap_or(&empty);
    let changed_stops: std::collections::BTreeSet<&String> = after.stops.iter().filter(|(id, events)| before.stops.get(*id) != Some(events)).map(|(id, _)| id).collect();
    let held = |id: &str| requests_before.as_ref().and_then(|b| b.requests.get(id));
    let changed: Vec<(&String, &StopRequest)> = requests_after.requests.iter().filter(|(id, r)| held(id) != Some(*r)).collect();
    let broken = |b: RuleBreak, entry: Option<transit_log::EntryRef>| transit_log::Broken {
        log: "stops".into(),
        kind: "stop_request".into(),
        id: b.request.clone(),
        field: "/stop".into(),
        target: "stop".into(),
        names: b.stop.clone(),
        entry,
        rule: Some(b.reason()),
    };
    if let Some(mut seen) = write.read.as_ref().and_then(|r| r.get("stops")).and_then(stops_of) {
        for id in &changed_stops {
            seen.stops.insert((*id).clone(), after.stops[*id].clone());
        }
        for (id, r) in &changed {
            if let Some(b) = request_rule(&seen, id, r, on) {
                if !held(id).is_some_and(|old| request_rule(&seen, id, old, on).is_some()) {
                    out.refused.push(broken(b, None));
                }
            }
        }
    }
    let entry = transit_log::EntryRef { log: write.header.log.clone(), seq: write.header.seq };
    for (id, r) in &requests_after.requests {
        if changed.iter().any(|(c, _)| *c == id) || changed_stops.contains(&r.stop) {
            if let Some(b) = request_rule(after, id, r, on) {
                out.warned.push(broken(b, Some(entry.clone())));
            }
        }
    }
    Ok(out)
}

/// None when the records do not make a document in this build's version. A
/// check that cannot judge refuses nothing.
fn stops_of(records: &transit_log::Records) -> Option<Stops> {
    let stops = records.get("stop").cloned().unwrap_or_default();
    let own = records.get("stops").and_then(|m| m.get("document")).cloned();
    let template = serde_json::json!({"kind": "stops", "schema_version": transit_stops::SCHEMA_VERSION});
    transit_log::assemble(template, own.as_ref(), "stops", &stops, false).ok()
}

pub fn dispatch_json(text: &str) -> String {
    cap_abi::answer(text, dispatch)
}

/// A document registers on the kernel's `load_stops` (270) or
/// `load_stop_requests` (271), so this crate has no bytes ops.
pub fn dispatch_bytes(op: u32, _bytes: &[u8]) -> String {
    serde_json::json!({ "error": format!("unknown bulk op {op}: a stops document registers on the kernel's load_stops") }).to_string()
}

#[cfg(feature = "examples")]
pub mod examples;

cap_abi::capability! {
    id: "stops",
    json: dispatch_json,
    bytes: dispatch_bytes,
    documents: [transit_stops::Stops, transit_requests::StopRequests, transit_network::Network],
    request: Request,
    consumes: &[("network", "network")],
    speaks: &[
        ("stops", &[transit_stops::SCHEMA_VERSION]),
        ("stop_requests", &[transit_requests::SCHEMA_VERSION]),
        ("network", &[transit_network::SCHEMA_VERSION]),
    ],
}
