//! cap-stops — the stops capability.
//!
//! One question: **what is a stop, on a date?** The stops document
//! (transit-stops; D-153) holds every stop as a list of dated events —
//! opened, closed, reopened, moved, changed, proposed, retired — and
//! nothing in it means "today". `validate` answers what a write must
//! pass: every refusal, naming its stop, event, and problem, or the
//! canonical document and its hash (the module is the hash authority;
//! a host stores exactly those bytes). `stops_on` and `stop` answer
//! stops as their events make them on a date, over a document the
//! kernel's `load_stops` registered (refused there unless `validate`
//! would pass it). A pick's stops are a copy (D-156): `seed` starts a
//! stops document from a network's stops, and `refresh` rewrites a
//! pick's stops from the stops document for its window, answering the
//! refreshed network under its hash (transit-network's `stop_record`).
//! The domain code lives in the shared `transit-stops` library; the
//! module writes nothing.

#![forbid(unsafe_code)]

use cap_abi::kernel::{NETWORK, STOPS};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

pub use transit_stops::{Event, Refusal, StopOn, Stops};

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "cap_stops/")]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Request {
    /// What a write must pass: every refusal (its stop, its event's
    /// index, its problem), or — when there is none — the canonical
    /// document and its hash.
    Validate { stops: Stops },
    /// Every stop as its events make it on `date`, in id order —
    /// or only `stop_ids`, where given. A stop not yet opened or
    /// proposed on the date is not listed.
    StopsOn {
        stops_hash: String,
        date: String,
        #[serde(default)]
        #[ts(optional)]
        stop_ids: Option<Vec<String>>,
    },
    /// One stop on `date` (null before it exists) and every event it
    /// has, oldest first — the card's record and its history.
    Stop { stops_hash: String, stop_id: String, date: String },
    /// A stops document from a network's stops: each one `opened`
    /// event with no date, carrying its point and details — registered,
    /// answered with its hash. Refreshing the network from it gives the
    /// network back.
    Seed { network_hash: String },
    /// The network with its stops refreshed from the stops document for
    /// the window `[from, until)` (open-ended without `until`), for the
    /// pick's `plan`: each stop the document holds as the window opens,
    /// carrying the events inside the window unless it is open as
    /// recorded every day of it; another plan's proposals left out; a
    /// stop the document does not hold left as it was. Registered,
    /// answered with its hash.
    Refresh {
        network_hash: String,
        stops_hash: String,
        from: String,
        #[serde(default)]
        #[ts(optional)]
        until: Option<String>,
        #[serde(default)]
        #[ts(optional)]
        plan: Option<String>,
    },
    /// The document with `plan`'s proposed stops opened from `from`,
    /// the plan's first date (or the proposal's own, when later) — what
    /// publishing the plan writes. Registered, answered with its hash
    /// and the stops it opened.
    OpenPlan { stops_hash: String, plan: String, from: String },
    /// The document with `event` added to stop `stop_id` in its place
    /// by date — an id the document does not hold begins a stop — then
    /// checked as a write is: every refusal, or the document registered
    /// and answered with its hash.
    AddEvent { stops_hash: String, stop_id: String, event: Event },
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "cap_stops/")]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Response {
    Validate { refusals: Vec<Refusal>, document: Option<String>, document_hash: Option<String> },
    StopsOn { stops: Vec<StopOn> },
    Stop { stop: Option<StopOn>, events: Vec<Event> },
    Seed { stops_hash: String, stops: Stops },
    Refresh { network_hash: String, network: transit_network::Network },
    OpenPlan { stops_hash: String, stops: Stops, opened: Vec<String> },
    AddEvent { refusals: Vec<Refusal>, stops_hash: Option<String>, stops: Option<Stops> },
}

fn parse_date(s: &str) -> Result<chrono::NaiveDate, String> {
    chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").map_err(|e| format!("invalid date {s:?}, expected YYYY-MM-DD: {e}"))
}

/// The typed dispatch (native hosts call this directly). Failures
/// are `{"error": ...}`, never panics — a total seam.
pub fn dispatch(request: Request) -> Result<Response, String> {
    match request {
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
        Request::StopsOn { stops_hash, date, stop_ids } => {
            let date = parse_date(&date)?;
            let stops = STOPS.require(&stops_hash)?;
            Ok(Response::StopsOn {
                stops: match stop_ids {
                    None => stops.on_date(date),
                    Some(ids) => {
                        let mut ids = ids;
                        ids.sort();
                        ids.dedup();
                        ids.iter().filter_map(|id| stops.stop_on(id, date)).collect()
                    }
                },
            })
        }
        Request::Stop { stops_hash, stop_id, date } => {
            let date = parse_date(&date)?;
            let stops = STOPS.require(&stops_hash)?;
            let events = stops.stops.get(&stop_id).ok_or_else(|| format!("no stop {stop_id:?} in the stops document"))?;
            Ok(Response::Stop { stop: stops.stop_on(&stop_id, date), events: events.clone() })
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
        Request::Refresh { network_hash, stops_hash, from, until, plan } => {
            let from = parse_date(&from)?;
            let until = until.as_deref().map(parse_date).transpose()?;
            if until.is_some_and(|u| u <= from) {
                return Err("until must come after from".into());
            }
            let network = NETWORK.require(&network_hash)?;
            let stops = STOPS.require(&stops_hash)?;
            let next = transit_network::stop_record::refresh(&network, &stops, from, until, plan.as_deref());
            let (network_hash, next) = NETWORK.register(next)?;
            Ok(Response::Refresh { network_hash, network: (*next).clone() })
        }
        Request::OpenPlan { stops_hash, plan, from } => {
            let from = parse_date(&from)?;
            let (next, opened) = STOPS.require(&stops_hash)?.open_plan(&plan, from);
            let (stops_hash, next) = STOPS.register(next)?;
            Ok(Response::OpenPlan { stops_hash, stops: (*next).clone(), opened })
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
