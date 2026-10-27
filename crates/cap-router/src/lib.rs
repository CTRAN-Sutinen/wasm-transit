//! cap-router — the trip-planning capability module.
//!
//! Planning composes over the NETWORK document and the EFFECTIVE
//! DAY, like every model-side capability: the network registers
//! under its content hash (the same identity cap-network's import
//! produces and every day view consumes), the edit document
//! registers the same way, and a plan answers by hash pair + date.
//! The day itself is transit-dispatch's composition — materialize,
//! then patch with the edits in force — never a private calendar
//! expansion (Law 2). The RAPTOR graph is built lazily per
//! (network, edits, date) and cached; a rider whose bundle stops at
//! "know when the bus comes" ships gtfs/board and never downloads
//! this module's planning code.
//!
//! The compare facet is the diversion delta: the same query over
//! the patched day and the unpatched one — one materialization
//! apart, no second document.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use ts_rs::TS;
use transit_network::Network;
use transit_sync::EditRecord;

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "cap_router/")]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Request {
    /// Register the NETWORK document under its content hash
    /// (idempotent by hash) — the same document identity
    /// cap-network's import produces and every day view consumes.
    LoadNetwork { network: Network },
    /// Register the OPERATOR EDIT document against a loaded
    /// network, validated per record AGAINST THE NETWORK; the
    /// response carries the reports and the document's content
    /// hash, which plans address. Wholesale replacement, like the
    /// host-owned store.
    LoadEdits {
        network_hash: String,
        records: Vec<EditRecord>,
    },
    /// Earliest-arrival plan from `from` to `to`, leaving at
    /// `depart_secs` on `date` — over the materialized day, with
    /// the in-force edits applied when `edits_hash` names a
    /// registered edit document. `None` in the response means
    /// unreachable within `max_legs`.
    Plan {
        network_hash: String,
        #[serde(default)]
        edits_hash: Option<String>,
        from: String,
        to: String,
        date: String,
        depart_secs: u32,
        max_legs: u32,
    },
    /// The diversion delta: the same plan over the patched day
    /// (active) and the unpatched materialization (baseline). The
    /// baseline is `None` when no edit is in force on the date —
    /// there is no second world to compare against.
    PlanCompare {
        network_hash: String,
        #[serde(default)]
        edits_hash: Option<String>,
        from: String,
        to: String,
        date: String,
        depart_secs: u32,
        max_legs: u32,
    },
    /// Where an itinerary runs, leg by leg, over the same effective
    /// day it was planned on: a transit leg's RIDDEN segment (the
    /// trip's alignment sliced between its board and alight stops
    /// while it follows its pattern; its patched stops when diverted),
    /// a walk through the stops it passes. The map draws the answer.
    ItineraryPaths {
        network_hash: String,
        #[serde(default)]
        edits_hash: Option<String>,
        date: String,
        itinerary: transit_router::Itinerary,
    },
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "cap_router/")]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Response {
    LoadNetwork { network_hash: String },
    LoadEdits {
        edits_hash: String,
        reports: Vec<transit_sync::EditReport>,
    },
    Plan { itinerary: Option<transit_router::Itinerary> },
    PlanCompare {
        active: Option<transit_router::Itinerary>,
        baseline: Option<transit_router::Itinerary>,
    },
    /// One path per leg, in leg order (empty when a leg's stops are
    /// unknown to the network).
    ItineraryPaths { paths: Vec<transit_dispatch::LegPath> },
}

/// Networks by content hash (canonical serialization).
fn networks() -> &'static Mutex<HashMap<String, Arc<Network>>> {
    static REGISTRY: OnceLock<Mutex<HashMap<String, Arc<Network>>>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Edit documents by content hash (canonical serialization).
fn edits() -> &'static Mutex<HashMap<String, Arc<Vec<EditRecord>>>> {
    static REGISTRY: OnceLock<Mutex<HashMap<String, Arc<Vec<EditRecord>>>>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

/// RAPTOR graphs by (network, edits, date), built on first use —
/// the graph is a pure function of the day, so it caches per day.
fn graphs() -> &'static Mutex<HashMap<(String, Option<String>, NaiveDate), Arc<transit_router::RouterNetwork>>> {
    static GRAPHS: OnceLock<Mutex<HashMap<(String, Option<String>, NaiveDate), Arc<transit_router::RouterNetwork>>>> =
        OnceLock::new();
    GRAPHS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn content_hash(bytes: &[u8]) -> String {
    format!("sha256:{:x}", Sha256::digest(bytes))
}

fn canonical_hash<T: Serialize>(value: &T) -> Result<String, String> {
    let canonical =
        serde_json::to_string(value).map_err(|e| format!("document does not serialize: {e}"))?;
    Ok(content_hash(canonical.as_bytes()))
}

fn network_arc(network_hash: &str) -> Result<Arc<Network>, String> {
    networks()
        .lock()
        .expect("network registry lock")
        .get(network_hash)
        .cloned()
        .ok_or_else(|| format!("unknown network {network_hash}: load_network first"))
}

fn edits_arc(edits_hash: &str) -> Result<Arc<Vec<EditRecord>>, String> {
    edits()
        .lock()
        .expect("edits registry lock")
        .get(edits_hash)
        .cloned()
        .ok_or_else(|| format!("unknown edits {edits_hash}: load_edits first"))
}

fn parse_date(date: &str) -> Result<NaiveDate, String> {
    NaiveDate::parse_from_str(date, "%Y-%m-%d")
        .map_err(|e| format!("invalid date {date:?}: {e}"))
}

/// One effective day: the network materialized for the date,
/// patched with the edits in force on it — transit-dispatch's one
/// composition, never a private expansion.
fn effective_day(
    network: &Network,
    records: &[EditRecord],
    date: NaiveDate,
) -> Result<Vec<transit_network::MaterializedTrip>, String> {
    transit_dispatch::effective_day(network, date, records)
        .map_err(|e| format!("the network does not materialize: {e}"))
}

/// The RAPTOR graph for (network, edits, date), built on first use.
fn graph(
    network_hash: &str,
    edits_hash: Option<&str>,
    date_str: &str,
) -> Result<Arc<transit_router::RouterNetwork>, String> {
    // The request's own fields validate before any registry lookup —
    // a garbage date is a caller error, not a missing-document error.
    let date = parse_date(date_str)?;
    let network = network_arc(network_hash)?;
    let records = match edits_hash {
        Some(hash) => edits_arc(hash)?,
        None => Arc::new(Vec::new()),
    };
    let key = (network_hash.to_string(), edits_hash.map(str::to_string), date);
    let mut graphs = graphs().lock().expect("graph cache lock");
    if let Some(cached) = graphs.get(&key) {
        return Ok(cached.clone());
    }
    let day = effective_day(&network, &records, date)?;
    let built = Arc::new(transit_router::RouterNetwork::build_from_day(&network, &day, date));
    graphs.insert(key, built.clone());
    Ok(built)
}

/// The typed dispatch (native hosts call this directly). Failures
/// are `{\"error\": ...}`, never panics.
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
            // Validation is against the NETWORK — model vocabulary.
            let network = network_arc(&network_hash)?;
            let reports = transit_dispatch::report_against_network(&network, &records);
            let hash = canonical_hash(&records)?;
            edits()
                .lock()
                .expect("edits registry lock")
                .insert(hash.clone(), Arc::new(records));
            Ok(Response::LoadEdits { edits_hash: hash, reports })
        }
        Request::Plan { network_hash, edits_hash, from, to, date, depart_secs, max_legs } => {
            let net = graph(&network_hash, edits_hash.as_deref(), &date)?;
            let itinerary = transit_router::earliest_arrival(
                &net,
                &from,
                &to,
                depart_secs,
                max_legs.max(1) as usize,
            )
            .map_err(|e| e.to_string())?;
            Ok(Response::Plan { itinerary })
        }
        Request::ItineraryPaths { network_hash, edits_hash, date, itinerary } => {
            let parsed = parse_date(&date)?;
            let network = network_arc(&network_hash)?;
            let records = match edits_hash.as_deref() {
                Some(hash) => edits_arc(hash)?,
                None => Arc::new(Vec::new()),
            };
            let day = effective_day(&network, &records, parsed)?;
            let paths = itinerary
                .legs
                .iter()
                .map(|leg| match leg {
                    transit_router::Leg::Transit { schedule, start_secs, board_stop_id, alight_stop_id, .. } => {
                        transit_dispatch::LegPath {
                            kind: transit_dispatch::LegKind::Transit,
                            path: transit_dispatch::trip_segment(
                                &network, &day, schedule, *start_secs, board_stop_id, alight_stop_id,
                            ),
                        }
                    }
                    transit_router::Leg::Walk { from_stop_id, via_stop_ids, to_stop_id, .. } => {
                        transit_dispatch::LegPath {
                            kind: transit_dispatch::LegKind::Walk,
                            path: transit_network::stops_line(
                                &network,
                                std::iter::once(from_stop_id)
                                    .chain(via_stop_ids)
                                    .chain(std::iter::once(to_stop_id))
                                    .map(String::as_str),
                            ),
                        }
                    }
                })
                .collect();
            Ok(Response::ItineraryPaths { paths })
        }
        Request::PlanCompare { network_hash, edits_hash, from, to, date, depart_secs, max_legs } => {
            // The request's own fields validate before any registry
            // lookup, so both worlds fail identically on bad input.
            let parsed = parse_date(&date)?;
            let legs = max_legs.max(1) as usize;
            let net = graph(&network_hash, edits_hash.as_deref(), &date)?;
            let active = transit_router::earliest_arrival(&net, &from, &to, depart_secs, legs)
                .map_err(|e| e.to_string())?;
            // The baseline is the unpatched materialization — one
            // materialization apart, never a second document.
            let baseline = match edits_hash.as_deref() {
                Some(hash) => {
                    let network = network_arc(&network_hash)?;
                    let records = edits_arc(hash)?;
                    if transit_sync::in_force_on(&records, parsed).is_empty() {
                        None
                    } else {
                        let day = transit_network::materialize(&network, parsed)
                            .map_err(|e| format!("the network does not materialize: {e}"))?;
                        let base = transit_router::RouterNetwork::build_from_day(&network, &day, parsed);
                        transit_router::earliest_arrival(&base, &from, &to, depart_secs, legs)
                            .map_err(|e| e.to_string())?
                    }
                }
                None => None,
            };
            Ok(Response::PlanCompare { active, baseline })
        }
    }
}

/// The JSON seam: `{\"op\": ...}` in, `{\"ok\": ...}` /
/// `{\"error\": ...}` out — byte-compatible with every other
/// capability module.
pub fn dispatch_json(text: &str) -> String {
    let outcome = match serde_json::from_str::<Request>(text) {
        Ok(request) => match dispatch(request) {
            Ok(response) => serde_json::json!({ "ok": response }),
            Err(error) => serde_json::json!({ "error": error }),
        },
        Err(error) => serde_json::json!({ "error": format!("invalid request: {error}") }),
    };
    serde_json::to_string(&outcome).expect("dispatch outcomes serialize")
}

/// The router module takes JSON documents (network, edits) — no
/// bulk-bytes op exists; the seam answers honestly instead of
/// silently ignoring one.
pub fn dispatch_bytes(op: u32, _bytes: &[u8]) -> String {
    serde_json::json!({ "error": format!("unknown bulk op {op}: router takes JSON documents (load_network / load_edits)") })
        .to_string()
}

cap_abi::capability! {
    json: dispatch_json,
    bytes: dispatch_bytes,
}
