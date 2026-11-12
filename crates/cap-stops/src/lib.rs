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
//! would pass it). The domain code lives in the shared `transit-stops`
//! library; the module writes nothing.

#![forbid(unsafe_code)]

use cap_abi::kernel::STOPS;
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
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "cap_stops/")]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Response {
    Validate { refusals: Vec<Refusal>, document: Option<String>, document_hash: Option<String> },
    StopsOn { stops: Vec<StopOn> },
    Stop { stop: Option<StopOn>, events: Vec<Event> },
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
    documents: [STOPS],
}
