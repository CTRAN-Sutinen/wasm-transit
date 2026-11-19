//! cap-disruptions: the disruptions capability.
//!
//! One question: **what is disrupted on a day, and does an alert explain
//! it?** (D-213, D-218). transit-disruptions is the library. A
//! disruption comes from the edit document (a diversion or a trip
//! cancel), the live feed (a trip it cancels), or the stops document (a
//! suspended stop), and each is listed as its document gives it.
//!
//! - `today` answers the day by route: each route with a disruption or a
//!   late trip, each disruption with the names a rider uses, the alerts
//!   that explain it, and its latest note; and how many routes have
//!   neither. A filter answers one route or one stop.
//! - `gaps` answers what no alert explains, with what an alert about
//!   each starts from.
//!
//! The module writes nothing.

#![forbid(unsafe_code)]

use std::sync::Arc;

use cap_abi::kernel::{EDITS, LIVE, NETWORK, NOTES, SERVICE_ALERTS, STOPS};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

pub use transit_disruptions::{AlertGap, Disruption, Filter, NamedStop, Origin, RouteDay, Today};

/// The documents a question reads, by hash. Each one absent is an empty
/// one, and no live document means no feed cancels and no late trips.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "cap_disruptions/")]
pub struct Documents {
    pub network_hash: String,
    #[serde(default)]
    #[ts(optional)]
    pub edits_hash: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    pub live_hash: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    pub stops_hash: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    pub alerts_hash: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    pub notes_hash: Option<String>,
    /// The service date, "YYYY-MM-DD".
    pub date: String,
    /// The host's clock, Unix seconds: what an alert explains now.
    #[ts(type = "number")]
    pub now: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "cap_disruptions/")]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Request {
    /// The day by route. `route` answers one route; `stop`, the
    /// disruptions that touch one stop.
    Today {
        #[serde(flatten)]
        #[ts(flatten)]
        documents: Documents,
        #[serde(default)]
        #[ts(optional)]
        route: Option<String>,
        #[serde(default)]
        #[ts(optional)]
        stop: Option<String>,
    },
    /// What no alert explains on the day, with what an alert about each
    /// starts from.
    Gaps {
        #[serde(flatten)]
        #[ts(flatten)]
        documents: Documents,
    },
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "cap_disruptions/")]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Response {
    Today {
        #[serde(flatten)]
        #[ts(flatten)]
        today: Today,
    },
    Gaps { gaps: Vec<AlertGap> },
}

/// The documents, held: every hash the request names is required.
struct Held {
    network: Arc<cap_abi::kernel::Network>,
    edits: Vec<transit_sync::EditRecord>,
    live: Option<Arc<transit_live::LiveDay>>,
    stops: Arc<cap_abi::kernel::Stops>,
    alerts: Arc<transit_alerts::ServiceAlerts>,
    notes: Arc<transit_notes::Notes>,
    date: chrono::NaiveDate,
    now: u64,
}

impl Held {
    fn of(d: &Documents) -> Result<Held, String> {
        Ok(Held {
            network: NETWORK.require(&d.network_hash)?,
            edits: match &d.edits_hash {
                Some(h) => EDITS.require(h)?.records.clone(),
                None => Vec::new(),
            },
            live: d.live_hash.as_deref().map(|h| LIVE.require(h)).transpose()?,
            stops: match &d.stops_hash {
                Some(h) => STOPS.require(h)?,
                None => Arc::new(cap_abi::kernel::Stops::empty()),
            },
            alerts: match &d.alerts_hash {
                Some(h) => SERVICE_ALERTS.require(h)?,
                None => Arc::new(Default::default()),
            },
            notes: match &d.notes_hash {
                Some(h) => NOTES.require(h)?,
                None => Arc::new(Default::default()),
            },
            date: chrono::NaiveDate::parse_from_str(&d.date, "%Y-%m-%d").map_err(|e| format!("invalid date {:?}: {e}", d.date))?,
            now: d.now,
        })
    }

    fn inputs(&self) -> transit_disruptions::Inputs<'_> {
        transit_disruptions::Inputs {
            network: &self.network,
            edits: &self.edits,
            live: self.live.as_deref(),
            stops: &self.stops,
            alerts: &self.alerts,
            notes: &self.notes,
            date: self.date,
            now: self.now,
        }
    }
}

/// The typed dispatch (native hosts call this directly). Failures are
/// `{"error": ...}`, never panics: a total seam.
pub fn dispatch(request: Request) -> Result<Response, String> {
    match request {
        Request::Today { documents, route, stop } => {
            let held = Held::of(&documents)?;
            Ok(Response::Today { today: transit_disruptions::today(&held.inputs(), &Filter { route, stop }) })
        }
        Request::Gaps { documents } => {
            let held = Held::of(&documents)?;
            Ok(Response::Gaps { gaps: transit_disruptions::gaps(&held.inputs()) })
        }
    }
}

/// The JSON seam: `{"op": ...}` in, `{"ok": ...}` / `{"error": ...}`
/// out, byte-compatible with every other capability module.
pub fn dispatch_json(text: &str) -> String {
    cap_abi::answer(text, dispatch)
}

/// The disruptions take their questions inline; each document registers
/// on its kernel load.
pub fn dispatch_bytes(op: u32, _bytes: &[u8]) -> String {
    serde_json::json!({ "error": format!("unknown bulk op {op}: the disruptions read documents that register on the kernel's loads") }).to_string()
}

cap_abi::capability! {
    id: "disruptions",
    json: dispatch_json,
    bytes: dispatch_bytes,
    documents: [NETWORK, EDITS, LIVE, STOPS, SERVICE_ALERTS, NOTES],
}
