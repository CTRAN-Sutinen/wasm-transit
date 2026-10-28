//! cap-calendar — the calendar capability.
//!
//! One question: **which service keys run on this date?** — often
//! several at once (a regular pattern plus an exception-added
//! service) — and its reverse, **when does one service type
//! run?**. Both answers come from the EFFECTIVE day, not the
//! calendar rules alone: a key whose every trip is skipped by
//! edits (or was never representable in the model) is not running,
//! however its calendar reads — the same honesty as the runcut
//! view's empty hole.
//!
//! The day is `materialize(net, date)`, patched with the edits in
//! force on that date. No derived registry, no GTFS expansion, no
//! date-keyed cache: a day is computed per call, and a day is
//! cheap. The domain code lives in the shared transit-dispatch
//! library; this module is the seam.
//!
//! Documents are content-addressed like everywhere on the bus:
//! * **network** — the same identity cap-network's import produces
//!   and cap-analysis consumes (the module is the hash authority);
//! * **edits** — the operator edit document, wholesale-replaced by
//!   the host, validated per record against the NETWORK at load
//!   (invalid records are REPORTED, not rejected — the sync
//!   contract).

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use cap_abi::canonical_hash;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use transit_network::Network;

/// One operation call. Tagged like every capability's seam.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "cap_calendar/")]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Request {
    /// Register the NETWORK document under its content hash
    /// (idempotent by hash) — the same document identity
    /// cap-network's import produces and cap-analysis consumes.
    /// Decoded by `cap_abi::load_network_json` on either lane, never
    /// through the tagged envelope (whose buffered decode would cost
    /// ~8× the document in peak memory and a second decoder).
    #[serde(skip_deserializing)]
    LoadNetwork { network: Network },
    /// Register the OPERATOR EDIT document against a loaded network,
    /// validated per record AGAINST THE NETWORK (model vocabulary —
    /// no feed is expanded): the response carries the reports
    /// (invalid records are REPORTED, not rejected) and the
    /// document's content hash, which the day views address.
    /// Wholesale replacement, like the host-owned store: the whole
    /// document is pushed on every change.
    LoadEdits {
        network_hash: String,
        records: Vec<transit_sync::EditRecord>,
    },
    /// The service keys running on a date: the distinct service types
    /// the effective day's trips run under, sorted. Schedule-only
    /// when `edits_hash` is omitted.
    ActiveServices {
        network_hash: String,
        #[serde(default)]
        edits_hash: Option<String>,
        date: String,
    },
    /// The dates one service type runs on within a window — the same
    /// question `active_services` answers in the other direction
    /// ("when does 1-WKDY/31 run?"). Inclusive "YYYY-MM-DD" window;
    /// an unknown type or an inverted window answers empty.
    /// Schedule-only when `edits_hash` is omitted.
    ServiceDates {
        network_hash: String,
        #[serde(default)]
        edits_hash: Option<String>,
        service_type: String,
        from: String,
        to: String,
    },
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "cap_calendar/")]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Response {
    LoadNetwork { network_hash: String },
    LoadEdits {
        edits_hash: String,
        reports: Vec<transit_sync::EditReport>,
    },
    ActiveServices { services: Vec<String> },
    ServiceDates { dates: Vec<String> },
}

/// Networks by content hash (canonical serialization).
fn networks() -> &'static Mutex<HashMap<String, Arc<Network>>> {
    static REGISTRY: OnceLock<Mutex<HashMap<String, Arc<Network>>>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Operator edit documents by content hash (canonical serialization).
fn edits() -> &'static Mutex<HashMap<String, Arc<Vec<transit_sync::EditRecord>>>> {
    static REGISTRY: OnceLock<Mutex<HashMap<String, Arc<Vec<transit_sync::EditRecord>>>>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

fn network_arc(network_hash: &str) -> Result<Arc<Network>, String> {
    networks()
        .lock()
        .expect("network registry lock")
        .get(network_hash)
        .cloned()
        .ok_or_else(|| format!("unknown network {network_hash}: load_network first"))
}

fn edits_arc(edits_hash: &str) -> Result<Arc<Vec<transit_sync::EditRecord>>, String> {
    edits()
        .lock()
        .expect("edits registry lock")
        .get(edits_hash)
        .cloned()
        .ok_or_else(|| format!("unknown edits {edits_hash}: load_edits first"))
}

fn parse_date(date: &str) -> Result<chrono::NaiveDate, String> {
    chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d")
        .map_err(|e| format!("invalid date {date:?}: {e}"))
}

/// One effective day: the network materialized for the date, patched
/// with the edits in force on it. Computed per call — no cache, no
/// calendar expansion, one date.
fn effective_day(
    network_hash: &str,
    edits_hash: Option<&str>,
    date_str: &str,
) -> Result<Vec<transit_network::MaterializedTrip>, String> {
    // The request's own fields validate before any registry lookup —
    // a garbage date is a caller error, not a missing-document error.
    let date = parse_date(date_str)?;
    let network = network_arc(network_hash)?;
    let records = match edits_hash {
        Some(hash) => edits_arc(hash)?,
        None => Arc::new(Vec::new()),
    };
    transit_dispatch::effective_day(&network, date, &records)
        .map_err(|e| format!("the network does not materialize: {e}"))
}

/// The typed dispatch (native hosts call this directly). Failures
/// are `{"error": ...}`, never panics — a total seam.
pub fn dispatch(request: Request) -> Result<Response, String> {
    match request {
        Request::LoadNetwork { network } => {
            let hash = canonical_hash(&network)?;
            networks()
                .lock()
                .expect("network registry lock")
                .entry(hash.clone())
                .or_insert_with(|| Arc::new(network));
            Ok(Response::LoadNetwork { network_hash: hash })
        }
        Request::LoadEdits { network_hash, records } => {
            // Validation is against the NETWORK — model vocabulary;
            // no feed is expanded to answer it.
            let network = network_arc(&network_hash)?;
            let reports = transit_dispatch::report_against_network(&network, &records);
            let hash = canonical_hash(&records)?;
            edits()
                .lock()
                .expect("edits registry lock")
                .insert(hash.clone(), Arc::new(records));
            Ok(Response::LoadEdits { edits_hash: hash, reports })
        }
        Request::ActiveServices { network_hash, edits_hash, date } => {
let day = effective_day(&network_hash, edits_hash.as_deref(), &date)?;
            let network = network_arc(&network_hash)?;
            Ok(Response::ActiveServices {
                services: transit_dispatch::active_services(&network, &day),
            })
        }
        Request::ServiceDates { network_hash, edits_hash, service_type, from, to } => {
            // The request's own fields validate before any registry
            // lookup — a garbage date is a caller error, not a
            // missing-document error.
            let from = parse_date(&from)?;
            let to = parse_date(&to)?;
            let network = network_arc(&network_hash)?;
            let records = match edits_hash.as_deref() {
                Some(hash) => edits_arc(hash)?,
                None => Arc::new(Vec::new()),
            };
            let dates = transit_dispatch::service_dates(&network, &records, &service_type, from, to)
                .map_err(|e| format!("the network does not materialize: {e}"))?;
            Ok(Response::ServiceDates {
                dates: dates.iter().map(|d| d.format("%Y-%m-%d").to_string()).collect(),
            })
        }
    }
}

/// The JSON seam: `{"op": ...}` in, `{"ok": ...}` / `{"error": ...}`
/// out — byte-compatible with every other capability module.
pub fn dispatch_json(text: &str) -> String {
    if let Some(answer) = cap_abi::load_network_json(text, |doc| dispatch_bytes(cap_abi::OP_LOAD_NETWORK, doc)) {
        return answer;
    }
    let outcome = match serde_json::from_str::<Request>(text) {
        Ok(request) => match dispatch(request) {
            Ok(response) => serde_json::json!({ "ok": response }),
            Err(error) => serde_json::json!({ "error": error }),
        },
        Err(error) => serde_json::json!({ "error": format!("invalid request: {error}") }),
    };
    serde_json::to_string(&outcome).expect("dispatch outcomes serialize")
}

/// The calendar module takes JSON documents (network, edits) — no
/// bulk-bytes op exists; the seam answers honestly instead of
/// silently ignoring one.
pub fn dispatch_bytes(op: u32, bytes: &[u8]) -> String {
    match op {
        cap_abi::OP_LOAD_NETWORK => cap_abi::outcome(
            cap_abi::parse_network(bytes).and_then(|network| dispatch(Request::LoadNetwork { network })),
        ),
        _ => serde_json::json!({ "error": format!("unknown bulk op {op}: calendar answers the kernel's load_network bulk op; edits travel as JSON (load_edits)") }).to_string(),
    }
}

cap_abi::capability! {
    json: dispatch_json,
    bytes: dispatch_bytes,
}
