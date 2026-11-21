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
//! would pass it), and `stops_near` the stops nearest a point on a date
//! (the field's "which stop am I at?"). `seed` starts a stops document from a network's
//! stops (transit-stops's `stop_record`), and `add_event` places one
//! edit and checks it. A pick only reads the document (D-157):
//! `read_records` gives it each stop's record on its first date, and
//! `visit_warnings` tells its planner of the stops it visits that are
//! not in service on its dates. `read_pick` and `export_pick` ask the
//! same over the pick's own period, which the published plan set gives
//! (retired picks bound nothing), so no host derives it. GTFS out and in carry the stops'
//! history (D-150, D-151): `export_versions` gives cap-network's export
//! one stop per version, and `import_events` records a feed's
//! differences as events. `questions` answers what a site survey asks
//! (D-158): the core site questions and amenity names; and
//! `attribute_surveys` names the signed-in writer as the surveyor of
//! every survey a write adds (the receive path's, before `validate`).
//! `receive` writes the stops log whole (D-228): the stops document and
//! the stop requests document are one log, because one entry can change
//! a stop and the request it carries out. So the module also links
//! `transit-requests`, and registers stop requests documents.
//! The domain code lives in the shared `transit-stops` library; the
//! module writes nothing.

#![forbid(unsafe_code)]

use cap_abi::kernel::{NETWORK, STOPS, STOP_REQUESTS};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

pub use transit_stops::{Answer, Event, Refusal, Status, StatusCount, StopOn, Stops};
use transit_requests::{RequestRefusal, StopRequest, StopRequests, WorkOrder};

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
    Validate {
        #[ts(as = "Stops")]
        stops: cap_abi::Base<Stops>,
    },
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
    /// The stops nearest a point on `date`, nearest first (ties by id),
    /// each with its great-circle distance: at most `limit` (default 10),
    /// none farther than `radius_m` where given, only those at `statuses`
    /// where given.
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
    ///
    /// With `new`, the write says the stop is new (D-228): an id the
    /// document holds is refused (`stop_exists`), and nothing is added.
    AddEvent {
        stops_hash: String,
        stop_id: String,
        event: Event,
        #[serde(default)]
        #[ts(optional)]
        new: Option<bool>,
    },
    /// The lists a stop's answers come from: its statuses, in the order a
    /// stop goes through them. The op reads no document.
    Vocabulary {},
    /// The document with the event `event_id` of stop `stop_id`
    /// corrected — taken out and `event` placed by date, keeping its id,
    /// or, with no `event`, taken out — then checked as a write is. A
    /// correction edits the event that was wrong (D-153), named by its
    /// id (D-166).
    CorrectEvent {
        stops_hash: String,
        stop_id: String,
        event_id: String,
        #[serde(default)]
        #[ts(optional)]
        event: Option<Event>,
        /// The canonical hash of the event as the writer read it. When
        /// the document holds the event changed since, the answer is a
        /// conflict that names it, and nothing is corrected (D-166).
        #[serde(default)]
        #[ts(optional)]
        saw: Option<String>,
    },
    /// The stops document as the stops log's records (D-197): one record
    /// of kind `stop` for each stop, its events whole.
    LogRecords { stops_hash: String },
    /// A write to the stops log, whole (D-228): each op of the write's
    /// intent over the documents at the head (`heads.stops`,
    /// `heads.stop_requests`, registered by the host; absent: none), then
    /// the entry that records what changed. The ops:
    ///
    /// - `stops`: `add_event`, `correct_event` (`{stop_id, ...}`, as the
    ///   ops of the same name take them, with no `stops_hash`);
    ///   `validate` (`{stops, from}`), a whole document over the head
    ///   `from`; `import_events` (`{network_hash, feed, on}`), a feed's
    ///   differences, over a network the host registered;
    /// - `requests`: `put`, `record_work_order` (as cap-requests takes
    ///   them, with no `requests_hash`); `validate` (`{requests, from}`);
    /// - `migrate`, under either: the documents at the head recorded as
    ///   they are (an empty log's first entry, from the old pointers).
    ///
    /// Every survey a written stops document adds names `by` (D-158;
    /// absent: the header's writer), then each written document is
    /// checked. A refused op answers its refusals, an op that does not
    /// run answers why (`failed`), and an event changed since the writer
    /// read it answers `conflict`: no entry in each case.
    Receive {
        #[ts(type = "{ checkpoint?: unknown, header: unknown, unread?: Array<{ seq: number, text: string }> }")]
        write: transit_log::Write,
        #[serde(default)]
        #[ts(type = "Record<string, string>")]
        heads: std::collections::BTreeMap<String, String>,
        #[ts(type = "Array<{ cap: string, op: string, args?: unknown }>")]
        ops: Vec<transit_log::OpCall>,
        /// The writer's name, as the host verified it.
        #[serde(default)]
        #[ts(optional)]
        by: Option<String>,
    },
    /// The stops document assembled from the stops log's `stop` records,
    /// registered and answered as its canonical bytes and hash. A record
    /// in another version is refused, naming both.
    LogDocument {
        #[ts(type = "Record<string, { version: number, value: unknown }>")]
        records: std::collections::BTreeMap<String, transit_log::Held>,
        /// The document's own record (absent: the empty document's).
        #[serde(default)]
        #[ts(optional, type = "{ version: number, value: unknown }")]
        document: Option<transit_log::Held>,
    },
    /// The pick with each stop's record as the stops document has it on
    /// `on`, its first date (D-157: a pick only reads the stops) —
    /// registered, answered with its hash.
    ReadRecords { network_hash: String, stops_hash: String, on: String },
    /// The named stops' records in the network's terms on `on` — what a
    /// plan takes when its planner picks a stop the stops document holds
    /// (D-173); a stop not there yet that day, or not held, is not
    /// answered.
    Records { stops_hash: String, stop_ids: Vec<String>, on: String },
    /// The edits that take stops into a plan (D-173, D-228): `upsert_stop`
    /// for each of `stop_ids` that the plan does not hold, once each, in
    /// order, with its record on `on`. A stop not answered on `on`
    /// refuses the take.
    TakeStops { network_hash: String, stops_hash: String, stop_ids: Vec<String>, on: String },
    /// What a pick reads of the stops over the period the published set
    /// gives it (D-228): the pick with each stop's record on its first
    /// date, and the warnings of the stops it visits that are not in
    /// service over the period — registered, answered with its hash.
    ReadPick {
        network_hash: String,
        stops_hash: String,
        /// The published plan set (the plans document), none before the
        /// first publish.
        #[serde(default)]
        #[ts(optional)]
        published: Option<transit_plans::PublishedPlans>,
    },
    /// The pick with the stops' history over the period the published
    /// set gives it, in GTFS terms (D-151, D-228): what `export_versions`
    /// answers, for the pick's own dates.
    ExportPick {
        network_hash: String,
        stops_hash: String,
        #[serde(default)]
        #[ts(optional)]
        published: Option<transit_plans::PublishedPlans>,
    },
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
    Validate {
        refusals: Vec<Refusal>,
        document: Option<String>,
        document_hash: Option<String>,
        /// The fields the document carries that this build does not
        /// know (D-167): kept in it, and flagged for a person, never
        /// refused. Absent when every field is known.
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
    Seed { stops_hash: String, stops: Stops },
    AddEvent { refusals: Vec<Refusal>, stops_hash: Option<String>, stops: Option<Stops> },
    Vocabulary { statuses: Vec<String> },
    CorrectEvent {
        refusals: Vec<Refusal>,
        stops_hash: Option<String>,
        stops: Option<Stops>,
        /// The event the writer read has changed since: its id. Nothing
        /// was corrected.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        conflict: Option<String>,
    },
    LogRecords {
        /// The records of each kind, as the log capability's `compose`
        /// takes them: the collection's records, and the document's own
        /// record (`document`), which keeps the fields outside the
        /// collection.
        #[ts(type = "Array<{ kind: string, version: number, records: Record<string, unknown> }>")]
        next: Vec<transit_log::KindRecords>,
    },
    /// What the write adds to the stops log (`heads`: the documents it
    /// wrote, by store kind).
    Receive(transit_log::Received),
    LogDocument {
        /// The document's hash (`stops_hash`).
        document_hash: String,
        /// The document's canonical bytes, which a host serves under
        /// `document_hash`.
        document: String,
    },
    /// The pick as its canonical bytes exactly (RFC 8785, D-169): a
    /// host stores and serves it under `network_hash` as it stands.
    ReadRecords {
        network_hash: String,
        #[ts(as = "transit_network::Network")]
        network: cap_abi::Canonical<transit_network::Network>,
    },
    /// `period` is none when the pick has no id or no date: it is no
    /// pick, and the network comes back as it was, with no warnings.
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
        /// The date the differences were recorded on.
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
            // A received document: the base of a write (cap_abi::Base).
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
        Request::StopsNear { stops_hash, date, lat, lon, limit, radius_m, statuses } => {
            let date = parse_date(&date)?;
            if !(-90.0..=90.0).contains(&lat) || !(-180.0..=180.0).contains(&lon) {
                return Err(format!("the point {lat}, {lon} is not on the earth"));
            }
            if radius_m.is_some_and(|r| !(r >= 0.0)) {
                return Err("radius_m must not be negative".into());
            }
            let stops = STOPS.require(&stops_hash)?;
            Ok(Response::StopsNear { stops: stops.near(date, lat, lon, limit.unwrap_or(10) as usize, radius_m, statuses.as_deref()) })
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
            let seeded = transit_stops::stop_record::seed(&network).identified().checked().map_err(|refusals| {
                let reasons: Vec<String> = refusals.iter().map(|r| r.to_string()).collect();
                format!("the network's stops do not make a stops document: {}", reasons.join("; "))
            })?;
            let (stops_hash, held) = STOPS.register(seeded)?;
            Ok(Response::Seed { stops_hash, stops: (*held).clone() })
        }
        Request::Vocabulary {} => Ok(Response::Vocabulary { statuses: Status::ALL.iter().map(|s| s.as_str().to_string()).collect() }),
        Request::AddEvent { stops_hash, stop_id, event, new } => {
            let held = STOPS.require_writable(&stops_hash)?;
            if let Some(taken) = new.unwrap_or(false).then(|| held.refuse_taken(&stop_id)).flatten() {
                return Ok(Response::AddEvent { refusals: vec![taken], stops_hash: None, stops: None });
            }
            let next = held.add_event(&stop_id, event);
            Ok(match next.identified().checked() {
                Ok(next) => {
                    let (stops_hash, next) = STOPS.register(next)?;
                    Response::AddEvent { refusals: vec![], stops_hash: Some(stops_hash), stops: Some((*next).clone()) }
                }
                Err(refusals) => Response::AddEvent { refusals, stops_hash: None, stops: None },
            })
        }
        Request::CorrectEvent { stops_hash, stop_id, event_id, event, saw } => {
            let held = STOPS.require_writable(&stops_hash)?;
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
                    let (stops_hash, next) = STOPS.register(next)?;
                    Response::CorrectEvent { refusals: vec![], stops_hash: Some(stops_hash), stops: Some((*next).clone()), conflict: None }
                }
                Err(refusals) => Response::CorrectEvent { refusals, stops_hash: None, stops: None, conflict: None },
            })
        }
        Request::LogRecords { stops_hash } => {
            let held = STOPS.require(&stops_hash)?;
            let doc = serde_json::to_value(&*held).map_err(|e| e.to_string())?;
            Ok(Response::LogRecords { next: transit_log::document_records(&doc, "stops", "stop", "stops", transit_stops::SCHEMA_VERSION)? })
        }
        Request::LogDocument { records, document } => {
            let template = serde_json::json!({"kind": "stops", "schema_version": transit_stops::SCHEMA_VERSION});
            let stops: Stops = transit_log::assemble(template, document.as_ref(), "stops", &records, false)?;
            let document = cap_abi::canonical_bytes(&stops)?;
            let (document_hash, _) = STOPS.register(stops)?;
            Ok(Response::LogDocument { document_hash, document })
        }
        Request::Receive { write, heads, ops, by } => receive(write, heads, ops, by),
        Request::ReadRecords { network_hash, stops_hash, on } => {
            let on = parse_date(&on)?;
            let network = NETWORK.require(&network_hash)?;
            let stops = STOPS.require(&stops_hash)?;
            let read = transit_stops::stop_record::read_records(&network, &stops, on);
            let (network_hash, read) = NETWORK.register(read)?;
            Ok(Response::ReadRecords { network_hash, network: cap_abi::Canonical((*read).clone()) })
        }
        Request::ReadPick { network_hash, stops_hash, published } => {
            let network = NETWORK.require(&network_hash)?;
            let stops = STOPS.require(&stops_hash)?;
            match transit_stops::stop_record::read_pick(&network, &stops, published.as_ref()) {
                Some(read) => {
                    let (network_hash, network) = NETWORK.register(read.network)?;
                    Ok(Response::ReadPick { period: Some(read.period), network_hash, network: cap_abi::Canonical((*network).clone()), warnings: read.warnings })
                }
                None => Ok(Response::ReadPick { period: None, network_hash, network: cap_abi::Canonical((*network).clone()), warnings: Vec::new() }),
            }
        }
        Request::ExportPick { network_hash, stops_hash, published } => {
            let network = NETWORK.require(&network_hash)?;
            let stops = STOPS.require(&stops_hash)?;
            let exported = transit_stops::stop_record::export_pick(&network, &stops, published.as_ref()).unwrap_or_else(|| (*network).clone());
            let (network_hash, exported) = NETWORK.register(exported)?;
            Ok(Response::ExportPick { network_hash, network: cap_abi::Canonical((*exported).clone()) })
        }
        Request::Records { stops_hash, stop_ids, on } => {
            let on = parse_date(&on)?;
            let stops = STOPS.require(&stops_hash)?;
            Ok(Response::Records { records: transit_stops::stop_record::records(&stops, &stop_ids, on) })
        }
        Request::TakeStops { network_hash, stops_hash, stop_ids, on } => {
            let on = parse_date(&on)?;
            let network = NETWORK.require(&network_hash)?;
            let stops = STOPS.require(&stops_hash)?;
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
            let network = NETWORK.require(&network_hash)?;
            let stops = STOPS.require(&stops_hash)?;
            Ok(Response::VisitWarnings { warnings: transit_stops::stop_record::visit_warnings(&network, &stops, from, until) })
        }
        Request::ExportVersions { network_hash, stops_hash, from, until } => {
            let from = parse_date(&from)?;
            let until = until.as_deref().map(parse_date).transpose()?;
            if until.is_some_and(|u| u <= from) {
                return Err("until must come after from".into());
            }
            let network = NETWORK.require(&network_hash)?;
            let stops = STOPS.require(&stops_hash)?;
            let exported = transit_stops::stop_record::export_versions(&network, &stops, from, until);
            let (network_hash, exported) = NETWORK.register(exported)?;
            Ok(Response::ExportVersions { network_hash, network: cap_abi::Canonical((*exported).clone()) })
        }
        Request::FeedEvents { stops_hash, feed } => {
            let stops = STOPS.require(&stops_hash)?;
            Ok(Response::FeedEvents { events: transit_stops::stop_record::feed_events(&stops, &feed) })
        }
        Request::ImportEvents { stops_hash, network_hash, on, feed } => {
            let network = NETWORK.require(&network_hash)?;
            let on = match on {
                Some(on) => parse_date(&on)?,
                None => transit_network::first_service_date(&network).ok_or("the feed runs no service, so it has no first service date: name `on`")?,
            };
            let stops = STOPS.require(&stops_hash)?;
            let imported = match feed {
                Some(feed) => transit_stops::stop_record::import_feed(&stops, &network, on, &feed),
                None => transit_stops::stop_record::import_events(&stops, &network, on),
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

/// An op of a write to the stops log over the stops document, as a page
/// sends it.
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

/// An op of a write to the stops log over the stop requests document.
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

/// The documents a write runs over: each one at the head and now, the
/// last op that wrote it, and the unknown fields of the whole documents
/// written.
#[derive(Default)]
struct Heads {
    held_stops: Option<String>,
    stops: Option<String>,
    /// Whether an op other than `migrate` wrote the stops document: only
    /// then are its surveys attributed and the document checked.
    check_stops: bool,
    requests: Option<String>,
    wrote_stops: Option<usize>,
    wrote_requests: Option<usize>,
    unknown_fields: Vec<String>,
}

fn to_value<T: Serialize>(v: &T) -> Result<serde_json::Value, String> {
    serde_json::to_value(v).map_err(|e| e.to_string())
}

/// The ops a write to the stops log takes, for an op it does not take.
const STOPS_LOG_OPS: &str = "stops: add_event, correct_event, validate, import_events, migrate; requests: put, record_work_order, validate, migrate";

/// One op over the stops document: none when it ran, the answer that
/// ends the write when it was refused, moved, or in conflict, or the
/// error of an op that did not run.
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
            (heads.stops, heads.wrote_stops) = (Some(STOPS.register(doc)?.0), Some(op));
            heads.unknown_fields.extend(unknown);
        }
        StopsIntent::ImportEvents { network_hash, on, feed } => {
            // A feed is recorded over a stops document: with none, it
            // records nothing.
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

/// One op over the stop requests document, as [`step_stops`] answers.
fn step_requests(heads: &mut Heads, intent: RequestsIntent, op: usize) -> Result<Option<transit_log::Received>, String> {
    let refused = |refusals: &[RequestRefusal]| Ok(Some(transit_log::Received::refused(op, to_value(&refusals)?)));
    let held = |hash: &Option<String>| -> Result<StopRequests, String> {
        Ok(match hash {
            Some(h) => (*STOP_REQUESTS.require_writable(h)?).clone(),
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
            // An older document comes forward first (transit-requests'
            // chain), as the library reads it.
            let (doc, unknown) = cap_abi::read_base::<StopRequests>(transit_requests::version::forward(requests)?)?;
            heads.unknown_fields.extend(unknown);
            doc
        }
        RequestsIntent::Migrate {} => return Ok(migrate(heads, op)),
    };
    match next.checked() {
        Ok(next) => (heads.requests, heads.wrote_requests) = (Some(STOP_REQUESTS.register(next)?.0), Some(op)),
        Err(refusals) => return refused(&refusals),
    }
    Ok(None)
}

/// `migrate`: each document at the head recorded as it is, unchecked.
fn migrate(heads: &mut Heads, op: usize) -> Option<transit_log::Received> {
    heads.wrote_stops = heads.wrote_stops.or(heads.stops.as_ref().map(|_| op));
    heads.wrote_requests = heads.wrote_requests.or(heads.requests.as_ref().map(|_| op));
    None
}

/// A write's ops over the heads, then its entry (`Request::Receive`).
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
        // The surveyor is the signed-in writer (D-158): every survey the
        // held document does not hold names them, whatever name the
        // client sent. Then the document is checked as a write is.
        let by = by.as_deref().unwrap_or(&write.header.writer);
        let now = (*STOPS.require(&now)?).clone();
        let attributed = match &heads.held_stops {
            Some(h) => STOPS.require(h)?.attribute_surveys(now, by),
            None => Stops::default().attribute_surveys(now, by),
        };
        let checked = match attributed.identified().checked() {
            Ok(checked) => STOPS.register(checked)?.0,
            Err(refusals) => return answer(transit_log::Received::refused(op, to_value(&refusals)?)),
        };
        let Response::LogRecords { next: records } = dispatch(Request::LogRecords { stops_hash: checked.clone() })? else { unreachable!("log_records answers log_records") };
        next.extend(records);
        written.insert("stops".to_string(), checked);
    }
    if let (Some(_), Some(now)) = (heads.wrote_requests, heads.requests.clone()) {
        let doc = to_value(&*STOP_REQUESTS.require(&now)?)?;
        next.extend(transit_log::document_records(&doc, "requests", "stop_request", "stop_requests", transit_requests::SCHEMA_VERSION)?);
        written.insert("stop_requests".to_string(), now);
    }
    // The rule of each request's reference to its stop (D-233, D-238):
    // refused as of what the writer read, warned as of now.
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

/// The requests a write breaks the stop rule for
/// (`transit_stops::request_rule`): refused, and warned.
#[derive(Default)]
struct Judged {
    refused: Vec<transit_log::Broken>,
    warned: Vec<transit_log::Broken>,
}

/// The stop rule over a write (D-233, D-238), on the write's date
/// (`write.today`; absent: not judged). Judged as of the read: each
/// request the write changes, against the stops the writer read
/// (`write.read`) with the write's own stop changes, refused unless it
/// was already broken before the write, so a person can still change or
/// cancel it. Judged as of now: each request the write changes and each
/// request on a stop the write changes, against the heads after the
/// write, warned.
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
    let requests_after = STOP_REQUESTS.require(requests_after)?;
    let requests_before = requests_before.map(|h| STOP_REQUESTS.require(h)).transpose()?;
    let stops_before = stops_before.map(|h| STOPS.require(h)).transpose()?;
    let stops_after = match stops_after {
        Some(h) => Some(STOPS.require(h)?),
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

/// The stops document from the stops log's records as a fold holds
/// them. None when they do not make one in this build's version: a
/// check that cannot judge refuses nothing.
fn stops_of(records: &transit_log::Records) -> Option<Stops> {
    let stops = records.get("stop").cloned().unwrap_or_default();
    let own = records.get("stops").and_then(|m| m.get("document")).cloned();
    let template = serde_json::json!({"kind": "stops", "schema_version": transit_stops::SCHEMA_VERSION});
    transit_log::assemble(template, own.as_ref(), "stops", &stops, false).ok()
}

/// The JSON seam: `{"op": ...}` in, `{"ok": ...}` / `{"error": ...}`
/// out — byte-compatible with every other capability module.
pub fn dispatch_json(text: &str) -> String {
    cap_abi::answer(text, dispatch)
}

/// The stops take their questions inline; a document registers on the
/// kernel's `load_stops` (bulk op 270), a stop requests document on
/// `load_stop_requests` (271).
pub fn dispatch_bytes(op: u32, _bytes: &[u8]) -> String {
    serde_json::json!({ "error": format!("unknown bulk op {op}: a stops document registers on the kernel's load_stops") }).to_string()
}

cap_abi::capability! {
    id: "stops",
    json: dispatch_json,
    bytes: dispatch_bytes,
    documents: [STOPS, STOP_REQUESTS, NETWORK],
}
