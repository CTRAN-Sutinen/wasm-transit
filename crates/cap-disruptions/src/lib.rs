//! cap-disruptions: what is disrupted on a day, and does an alert explain
//! it? (D-213, D-218; OPS.md has the ops.)
//!
//! The seam over transit-disruptions. It reads the network, the edit
//! document, the live document, the stops, the service alerts, and the
//! notes, each by hash, and writes nothing.

#![forbid(unsafe_code)]

use std::sync::Arc;

use cap_abi::kernel::kind;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

pub use transit_disruptions::{AlertGap, AlertStart, Disruption, Filter, NamedStop, Origin, RouteDay, Today};

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
    /// What a new alert starts from (D-228): with a `source` that is a
    /// gap, the gap's effect, informed riders, and what it explains; else
    /// a blank alert that explains `source`, if one is given.
    StartAlert {
        #[serde(flatten)]
        #[ts(flatten)]
        documents: Documents,
        #[serde(default)]
        #[ts(optional)]
        source: Option<transit_alerts::AlertSource>,
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
    StartAlert {
        #[serde(flatten)]
        #[ts(flatten)]
        start: AlertStart,
    },
}

/// The documents, held: every hash the request names is required.
struct Held {
    network: Arc<transit_network::Network>,
    edits: Vec<transit_sync::EditRecord>,
    live: Option<Arc<transit_live::LiveDay>>,
    stops: Arc<transit_stops::Stops>,
    alerts: Arc<transit_alerts::ServiceAlerts>,
    notes: Arc<transit_notes::Notes>,
    date: chrono::NaiveDate,
    now: u64,
}

impl Held {
    fn of(d: &Documents) -> Result<Held, String> {
        Ok(Held {
            network: kind::<transit_network::Network>().require(&d.network_hash)?,
            edits: match &d.edits_hash {
                Some(h) => kind::<transit_sync::EditDocument>().require(h)?.records.clone(),
                None => Vec::new(),
            },
            live: d.live_hash.as_deref().map(|h| kind::<transit_live::LiveDay>().require(h)).transpose()?,
            stops: match &d.stops_hash {
                Some(h) => kind::<transit_stops::Stops>().require(h)?,
                None => Arc::new(transit_stops::Stops::empty()),
            },
            alerts: match &d.alerts_hash {
                Some(h) => kind::<transit_alerts::ServiceAlerts>().require(h)?,
                None => Arc::new(Default::default()),
            },
            notes: match &d.notes_hash {
                Some(h) => kind::<transit_notes::Notes>().require(h)?,
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
        Request::StartAlert { documents, source } => {
            let held = Held::of(&documents)?;
            Ok(Response::StartAlert { start: transit_disruptions::start_alert(&held.inputs(), source.as_ref()) })
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

/// The worked examples, which the generator runs (no module build turns
/// the feature on).
#[cfg(feature = "examples")]
pub mod examples;

cap_abi::capability! {
    id: "disruptions",
    json: dispatch_json,
    bytes: dispatch_bytes,
    documents: [transit_network::Network, transit_sync::EditDocument, transit_live::LiveDay, transit_stops::Stops, transit_alerts::ServiceAlerts, transit_notes::Notes],
    request: Request,
    speaks: &[
        ("network", &[transit_network::SCHEMA_VERSION]),
        ("operator_edits", &[transit_sync::SCHEMA_VERSION]),
        ("live", &[2]),
        ("stops", &[transit_stops::SCHEMA_VERSION]),
        ("service_alerts", &[transit_alerts::ALERTS_VERSION]),
        ("notes", &[transit_notes::SCHEMA_VERSION]),
    ],
}
