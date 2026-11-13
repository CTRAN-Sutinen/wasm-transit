//! cap-stops — the stops capability.
//!
//! One question: **what is a stop, on a date?** The stops document
//! (transit-stops; D-153) holds every stop as a list of dated events —
//! planned, permitted, built, in_service, suspended, moved, changed,
//! retired — and
//! nothing in it means "today". `validate` answers what a write must
//! pass: every refusal, naming its stop, event, and problem, or the
//! canonical document and its hash (the module is the hash authority;
//! a host stores exactly those bytes). `stops_on` and `stop` answer
//! stops as their events make them on a date, over a document the
//! kernel's `load_stops` registered (refused there unless `validate`
//! would pass it). `seed` starts a stops document from a network's
//! stops (transit-network's `stop_record`), and `add_event` places one
//! edit and checks it. A pick only reads the document (D-157):
//! `read_records` gives it each stop's record on its first date, and
//! `visit_warnings` tells its planner of the stops it visits that are
//! not in service on its dates. GTFS out and in carry the stops'
//! history (D-150, D-151): `export_versions` gives cap-network's export
//! one stop per version, and `import_events` records a feed's
//! differences as events. `questions` answers what a site survey asks
//! (D-158): the core site questions and amenity names; and
//! `attribute_surveys` names the signed-in writer as the surveyor of
//! every survey a write adds (the receive path's, before `validate`).
//! The domain code lives in the shared `transit-stops` library; the
//! module writes nothing.

#![forbid(unsafe_code)]

use cap_abi::kernel::{NETWORK, STOPS};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

pub use transit_stops::{Answer, Event, Refusal, Status, StatusCount, StopOn, Stops};

/// One core site question (D-158) and the kind of answer it takes.
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
    /// What a write must pass: every refusal (its stop, its event's
    /// index, its problem), or — when there is none — the canonical
    /// document and its hash.
    Validate { stops: Stops },
    /// Every stop as its events make it on `date`, in id order —
    /// or only `stop_ids`, where given, and only those at `statuses`,
    /// where given — with the stops on the date counted by status. A
    /// stop not yet there on the date is not listed.
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
    /// One stop on `date` (null before it exists), every event it has,
    /// oldest first — the card's record and its history — and what it
    /// may become that day: the lifecycle events a write would accept.
    Stop { stops_hash: String, stop_id: String, date: String },
    /// A stops document from a network's stops: each one `in_service`
    /// event with no date, carrying its point and details — registered,
    /// answered with its hash.
    Seed { network_hash: String },
    /// The document with `event` added to stop `stop_id` in its place
    /// by date — an id the document does not hold begins a stop — then
    /// checked as a write is: every refusal, or the document registered
    /// and answered with its hash.
    AddEvent { stops_hash: String, stop_id: String, event: Event },
    /// The document with event `index` of stop `stop_id` corrected —
    /// taken out and `event` placed by date, or, with no `event`, taken
    /// out — then checked as a write is. A correction edits the event
    /// that was wrong (D-153).
    CorrectEvent {
        stops_hash: String,
        stop_id: String,
        index: u32,
        #[serde(default)]
        #[ts(optional)]
        event: Option<Event>,
    },
    /// The pick with each stop's record as the stops document has it on
    /// `on`, its first date (D-157: a pick only reads the stops) —
    /// registered, answered with its hash.
    ReadRecords { network_hash: String, stops_hash: String, on: String },
    /// What the planner is told of the stops the pick visits over
    /// `[from, until)`: each not in service, and each suspended for the
    /// whole period or with no end — warnings, never refusals.
    VisitWarnings {
        network_hash: String,
        stops_hash: String,
        from: String,
        #[serde(default)]
        #[ts(optional)]
        until: Option<String>,
    },
    /// The network over `[from, until)` with the stops' history in GTFS
    /// terms (D-151): one stop per version of a stop whose record
    /// changes inside the dates, each visiting pattern split at them —
    /// registered, answered with its hash, for cap-network's export.
    ExportVersions {
        network_hash: String,
        stops_hash: String,
        from: String,
        #[serde(default)]
        #[ts(optional)]
        until: Option<String>,
    },
    /// A feed's stops (an imported network) recorded on `on`, its first
    /// service date (D-150): each difference an event dated that day,
    /// checked as a write is — registered, answered with the events
    /// added, the stops the feed does not list, and those whose
    /// difference does not fit their history.
    ImportEvents {
        stops_hash: String,
        network_hash: String,
        /// The feed's first service date; omitted, the feed's own (the
        /// earliest date any of its service runs).
        #[serde(default)]
        #[ts(optional)]
        on: Option<String>,
        /// The feed's id (its hash): every event recorded names it
        /// (`from_feed`), so the planner is shown what the feed did.
        #[serde(default)]
        #[ts(optional)]
        feed: Option<String>,
    },
    /// The events the document holds that the feed `feed` (its id)
    /// recorded — what the planner is shown of an import (D-150).
    FeedEvents { stops_hash: String, feed: String },
    /// What a survey asks (D-158): the core site questions in the
    /// worksheet's order, each with the kind of answer it takes, and
    /// the core amenity names (D-154). Reads no document.
    Questions {},
    /// A write's surveys attributed to the signed-in writer `by`, as the
    /// receive path takes them (D-158): every survey event the held
    /// document (`stops_hash`; none for a first write) does not hold
    /// names `by`, whatever name the client sent. Unchecked: validate
    /// follows.
    AttributeSurveys {
        #[serde(default)]
        #[ts(optional)]
        stops_hash: Option<String>,
        stops: Stops,
        by: String,
    },
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "cap_stops/")]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Response {
    Validate { refusals: Vec<Refusal>, document: Option<String>, document_hash: Option<String> },
    StopsOn { stops: Vec<StopOn>, counts: Vec<StatusCount> },
    Stop {
        stop: Option<StopOn>,
        events: Vec<Event>,
        #[ts(type = "Array<\"planned\" | \"permitted\" | \"built\" | \"in_service\" | \"suspended\" | \"retired\">")]
        next: Vec<String>,
    },
    Seed { stops_hash: String, stops: Stops },
    AddEvent { refusals: Vec<Refusal>, stops_hash: Option<String>, stops: Option<Stops> },
    CorrectEvent { refusals: Vec<Refusal>, stops_hash: Option<String>, stops: Option<Stops> },
    ReadRecords { network_hash: String, network: transit_network::Network },
    VisitWarnings { warnings: Vec<transit_network::NetworkWarning> },
    ExportVersions { network_hash: String, network: transit_network::Network },
    ImportEvents {
        stops_hash: String,
        stops: Stops,
        /// The date the differences were recorded on.
        on: String,
        added: Vec<transit_network::stop_record::AddedEvent>,
        unlisted: Vec<String>,
        unapplied: Vec<String>,
    },
    Questions { site: Vec<SiteQuestion>, amenities: Vec<String> },
    FeedEvents { events: Vec<transit_network::stop_record::AddedEvent> },
    AttributeSurveys { stops: Stops },
}

fn parse_date(s: &str) -> Result<chrono::NaiveDate, String> {
    chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").map_err(|e| format!("invalid date {s:?}, expected YYYY-MM-DD: {e}"))
}

/// The typed dispatch (native hosts call this directly). Failures
/// are `{"error": ...}`, never panics — a total seam.
pub fn dispatch(request: Request) -> Result<Response, String> {
    match request {
        Request::AttributeSurveys { stops_hash, stops, by } => {
            let stops = match stops_hash {
                Some(h) => STOPS.require(&h)?.attribute_surveys(stops, &by),
                None => Stops::default().attribute_surveys(stops, &by),
            };
            Ok(Response::AttributeSurveys { stops })
        }
        Request::Questions {} => Ok(Response::Questions {
            site: transit_stops::SITE.iter().map(|(key, answer)| SiteQuestion { key: key.to_string(), answer: answer.clone() }).collect(),
            amenities: transit_stops::AMENITIES.iter().map(|a| a.to_string()).collect(),
        }),
        Request::Validate { stops } => {
            use transit_version::Versioned;
            cap_abi::speaks(Stops::KIND, stops.schema_version(), Stops::READS)?;
            Ok(match stops.checked() {
                Ok(stops) => {
                    let document = serde_json::to_string(&stops).map_err(|e| e.to_string())?;
                    let document_hash = cap_abi::content_hash(document.as_bytes());
                    Response::Validate { refusals: vec![], document: Some(document), document_hash: Some(document_hash) }
                }
                Err(refusals) => Response::Validate { refusals, document: None, document_hash: None },
            })
        }
        Request::StopsOn { stops_hash, date, stop_ids, statuses } => {
            let date = parse_date(&date)?;
            let stops = STOPS.require(&stops_hash)?;
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
        Request::Stop { stops_hash, stop_id, date } => {
            let date = parse_date(&date)?;
            let stops = STOPS.require(&stops_hash)?;
            let events = stops.stops.get(&stop_id).ok_or_else(|| format!("no stop {stop_id:?} in the stops document"))?;
            let next = stops.next_events(&stop_id, date).iter().map(|k| k.as_str().to_string()).collect();
            Ok(Response::Stop { stop: stops.stop_on(&stop_id, date), events: events.clone(), next })
        }
        Request::Seed { network_hash } => {
            let network = NETWORK.require(&network_hash)?;
            let seeded = transit_network::stop_record::seed(&network).checked().map_err(|refusals| {
                let reasons: Vec<String> = refusals.iter().map(|r| r.to_string()).collect();
                format!("the network's stops do not make a stops document: {}", reasons.join("; "))
            })?;
            let (stops_hash, held) = STOPS.register(seeded)?;
            Ok(Response::Seed { stops_hash, stops: (*held).clone() })
        }
        Request::AddEvent { stops_hash, stop_id, event } => {
            let next = STOPS.require(&stops_hash)?.add_event(&stop_id, event);
            Ok(match next.checked() {
                Ok(next) => {
                    let (stops_hash, next) = STOPS.register(next)?;
                    Response::AddEvent { refusals: vec![], stops_hash: Some(stops_hash), stops: Some((*next).clone()) }
                }
                Err(refusals) => Response::AddEvent { refusals, stops_hash: None, stops: None },
            })
        }
        Request::CorrectEvent { stops_hash, stop_id, index, event } => {
            let next = STOPS
                .require(&stops_hash)?
                .correct_event(&stop_id, index as usize, event)
                .ok_or_else(|| format!("no event {index} of stop {stop_id:?} in the stops document"))?;
            Ok(match next.checked() {
                Ok(next) => {
                    let (stops_hash, next) = STOPS.register(next)?;
                    Response::CorrectEvent { refusals: vec![], stops_hash: Some(stops_hash), stops: Some((*next).clone()) }
                }
                Err(refusals) => Response::CorrectEvent { refusals, stops_hash: None, stops: None },
            })
        }
        Request::ReadRecords { network_hash, stops_hash, on } => {
            let on = parse_date(&on)?;
            let network = NETWORK.require(&network_hash)?;
            let stops = STOPS.require(&stops_hash)?;
            let read = transit_network::stop_record::read_records(&network, &stops, on);
            let (network_hash, read) = NETWORK.register(read)?;
            Ok(Response::ReadRecords { network_hash, network: (*read).clone() })
        }
        Request::VisitWarnings { network_hash, stops_hash, from, until } => {
            let from = parse_date(&from)?;
            let until = until.as_deref().map(parse_date).transpose()?;
            if until.is_some_and(|u| u <= from) {
                return Err("until must come after from".into());
            }
            let network = NETWORK.require(&network_hash)?;
            let stops = STOPS.require(&stops_hash)?;
            Ok(Response::VisitWarnings { warnings: transit_network::stop_record::visit_warnings(&network, &stops, from, until) })
        }
        Request::ExportVersions { network_hash, stops_hash, from, until } => {
            let from = parse_date(&from)?;
            let until = until.as_deref().map(parse_date).transpose()?;
            if until.is_some_and(|u| u <= from) {
                return Err("until must come after from".into());
            }
            let network = NETWORK.require(&network_hash)?;
            let stops = STOPS.require(&stops_hash)?;
            let exported = transit_network::stop_record::export_versions(&network, &stops, from, until);
            let (network_hash, exported) = NETWORK.register(exported)?;
            Ok(Response::ExportVersions { network_hash, network: (*exported).clone() })
        }
        Request::FeedEvents { stops_hash, feed } => {
            let stops = STOPS.require(&stops_hash)?;
            Ok(Response::FeedEvents { events: transit_network::stop_record::feed_events(&stops, &feed) })
        }
        Request::ImportEvents { stops_hash, network_hash, on, feed } => {
            let network = NETWORK.require(&network_hash)?;
            let on = match on {
                Some(on) => parse_date(&on)?,
                None => transit_network::first_service_date(&network).ok_or("the feed runs no service, so it has no first service date: name `on`")?,
            };
            let stops = STOPS.require(&stops_hash)?;
            let imported = match feed {
                Some(feed) => transit_network::stop_record::import_feed(&stops, &network, on, &feed),
                None => transit_network::stop_record::import_events(&stops, &network, on),
            };
            let (stops_hash, next) = STOPS.register(imported.stops)?;
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

/// The JSON seam: `{"op": ...}` in, `{"ok": ...}` / `{"error": ...}`
/// out — byte-compatible with every other capability module.
pub fn dispatch_json(text: &str) -> String {
    cap_abi::answer(text, dispatch)
}

/// The stops take their questions inline; a document registers on the
/// kernel's `load_stops` (bulk op 270).
pub fn dispatch_bytes(op: u32, _bytes: &[u8]) -> String {
    serde_json::json!({ "error": format!("unknown bulk op {op}: a stops document registers on the kernel's load_stops") }).to_string()
}

cap_abi::capability! {
    id: "stops",
    json: dispatch_json,
    bytes: dispatch_bytes,
    documents: [STOPS, NETWORK],
}
