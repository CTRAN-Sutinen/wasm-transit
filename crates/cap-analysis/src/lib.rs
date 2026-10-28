//! cap-analysis — the planner-analysis capability module.
//!
//! The first capability extracted from the MONOLITH: the analysis
//! functions (network stats, service-type overview and detail,
//! population coverage) live in `transit-analysis`, shared as a
//! library under this module. A planner's bundle ships it; a
//! rider's never hears of it.
//!
//! Documents are content-addressed like everywhere on the bus: an
//! authored network (the JSON document) registers under its canonical
//! hash, a population layer under the hash of its bytes, and every
//! op addresses them by hash — never by carrying the whole document
//! per request (the fix for the monolith's per-op serialization).
//! Composition is through documents: a network produced by a future
//! network capability would be loaded here by the SAME hash the
//! producer returned, asserted at the host.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use serde::{Deserialize, Serialize};
use cap_abi::content_hash;
use ts_rs::TS;
use transit_analysis::{
    coverage, network_stats, parse_pop_layer, service_type_compare, service_type_detail, service_type_overview,
    type_coverage, CoverageWindow, NetworkStats, PopLayer, ServiceTypeComparison, ServiceTypeDetail,
    ServiceTypeSummary, TypeCoverage,
};
use transit_network::Network;

/// The bulk-bytes op code (population layer GeoJSON).
pub const OP_LOAD_LAYER: u32 = 0;

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "cap_analysis/")]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Request {
    /// Register an authored network document under its content hash
    /// (idempotent by hash). The network is the PLANNER'S document —
    /// this capability only ever reads it.
    /// Decoded by `cap_abi::load_network_json` on either lane, never
    /// through the tagged envelope (whose buffered decode would cost
    /// ~8× the document in peak memory and a second decoder).
    #[serde(skip_deserializing)]
    LoadNetwork { network: Network },
    /// Register a population layer (GeoJSON points) under its
    /// content hash. The handle is the whole answer the import UI
    /// shows — the field the parse picked and the people it found —
    /// so the layer's owner never needs a second parse elsewhere.
    LoadPopulationLayer { bytes: Vec<u8>, pop_field: Option<String> },
    /// The network's numbers on one service date. The optional
    /// include flags shape the response: the UI wants every route
    /// and stop inline; an agent asking the day-shape question wants
    /// five numbers, not 199 KiB. Both default TRUE (the UI's shape
    /// is unchanged); `false` empties the corresponding list.
    NetworkStats {
        network_hash: String,
        date: String,
        #[serde(default = "default_true")]
        include_routes: bool,
        #[serde(default = "default_true")]
        include_stops: bool,
        #[serde(default = "default_true")]
        include_service_types: bool,
    },
    /// Every service type's numbers, each on its representative day —
    /// the planner's design axis.
    ServiceTypesOverview { network_hash: String },
    /// One service type expanded on its representative day.
    ServiceTypeDetail { network_hash: String, service_type: String },
    /// A proposal (`network_hash`) against its baseline (`base_hash`)
    /// under one service type: both sides' totals and the routes
    /// whose numbers changed.
    ServiceTypeCompare { base_hash: String, network_hash: String, service_type: String },
    /// Population coverage per service type. `window` judges
    /// frequency inside a time window ("every 15 min 07:00–19:00":
    /// the worst wait in the window fits frequent_secs); omitted, it
    /// is the whole-day average headway.
    TypeCoverage {
        network_hash: String,
        layer_hash: String,
        radius_m: f64,
        frequent_secs: u32,
        min_departures: u32,
        #[serde(default)]
        #[ts(optional)]
        window: Option<CoverageWindow>,
    },
    /// One stop's day folded into headway gaps: departures, the max
    /// gap and the departure it follows, and the per-hour profile
    /// (departures, max and median gap starting in the hour) — the
    /// service-quality read of "how long until the next bus?". An
    /// unknown stop answers zero/None/empty, never an error.
    StopGaps {
        network_hash: String,
        date: String,
        stop_id: String,
    },
    /// The day's service profile: trips per hour bucketed by trip
    /// start, each hour row carrying every running service type's
    /// count. Only hours with trips are rows — the planner's
    /// texture question (where service is thick, where it tapers).
    HourlyTrips {
        network_hash: String,
        date: String,
    },
    /// One service type's schedule texture on its representative
    /// day: every pattern trips entry that runs under the type, with
    /// headway uniformity (min/median/max gap between starts) and
    /// clockface alignment (starts on the quarter-hour grid). Null
    /// for an unknown type — lookups answer empty, never error.
    ScheduleTexture {
        network_hash: String,
        service_type: String,
    },
    /// The corridor read: every pair of routes that shares at least
    /// one stop on the date — shared stops, shared segments (same-
    /// order consecutive pairs), and the shared stop where the
    /// pair's departures stack highest.
    RouteOverlap {
        network_hash: String,
        date: String,
    },
    /// The hub's timed-transfer view: arrival→departure pairs
    /// across routes within the connection window — the caller's
    /// (`window`, for a hub with widely separated bays), else the
    /// 60–900 s transfer-point convention — pooled over the stop's
    /// walking group: who can make what here. An unknown stop
    /// answers empty, never an error.
    TimedTransfers {
        network_hash: String,
        date: String,
        stop_id: String,
        #[serde(default)]
        #[ts(optional)]
        window: Option<transit_analysis::TransferWindow>,
    },
    /// The unserved WHERE of coverage: every layer point beyond the
    /// radius of every stop with service on the date, with the
    /// nearest served stop and its distance — "which part, and
    /// where", not just the share. Sorted by population descending.
    UnservedPoints {
        network_hash: String,
        layer_hash: String,
        radius_m: f64,
        date: String,
    },
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "cap_analysis/")]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Response {
    LoadNetwork { network_hash: String },
    LoadPopulationLayer {
        layer_hash: String,
        points: u64,
        pop_field: String,
        total_pop: f64,
    },
    NetworkStats { stats: NetworkStats },
    ServiceTypesOverview { service_types: Vec<ServiceTypeSummary> },
    ServiceTypeDetail { detail: Option<ServiceTypeDetail> },
    ServiceTypeCompare { comparison: Option<ServiceTypeComparison> },
    TypeCoverage { coverage: Vec<TypeCoverage> },
    StopGaps { gaps: transit_analysis::StopGaps },
    HourlyTrips { hours: Vec<transit_analysis::HourlyTrips> },
    ScheduleTexture { texture: Option<transit_analysis::ScheduleTexture> },
    RouteOverlap { overlaps: Vec<transit_analysis::RouteOverlap> },
    TimedTransfers { transfers: Vec<transit_analysis::TimedTransfer> },
    UnservedPoints { points: Vec<transit_analysis::UnservedPoint> },
}

/// Networks by content hash — canonical serialization, so byte
/// variants of one logical document land on one entry.
fn networks() -> &'static Mutex<HashMap<String, Arc<Network>>> {
    static REGISTRY: OnceLock<Mutex<HashMap<String, Arc<Network>>>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Population layers by content hash of the input bytes.
fn layers() -> &'static Mutex<HashMap<String, Arc<PopLayer>>> {
    static REGISTRY: OnceLock<Mutex<HashMap<String, Arc<PopLayer>>>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

fn default_true() -> bool {
    true
}

fn network_hash(network: &Network) -> Result<String, String> {
    cap_abi::canonical_hash(network)
}

fn parse_date(date: &str) -> Result<chrono::NaiveDate, String> {
    chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d")
        .map_err(|e| format!("invalid date {date:?}: {e}"))
}

fn with_network<T>(
    network_hash: &str,
    f: impl FnOnce(&Network) -> Result<T, String>,
) -> Result<T, String> {
    let guard = networks().lock().expect("network registry lock");
    let network = guard
        .get(network_hash)
        .ok_or_else(|| format!("unknown network {network_hash}: load_network first"))?;
    f(&network)
}

/// The registered document itself (for ops that compose across
/// registries).
fn network_arc(network_hash: &str) -> Result<Arc<Network>, String> {
    networks()
        .lock()
        .expect("network registry lock")
        .get(network_hash)
        .cloned()
        .ok_or_else(|| format!("unknown network {network_hash}: load_network first"))
}

/// The typed dispatch (native hosts call this directly). Failures
/// are `{"error": ...}`, never panics — analysis is best-effort on
/// mid-edit documents, and unknown hashes are errors, not crashes.
pub fn dispatch(request: Request) -> Result<Response, String> {
    match request {
        Request::LoadNetwork { network } => {
            let hash = network_hash(&network)?;
            networks()
                .lock()
                .expect("network registry lock")
                .entry(hash.clone())
                .or_insert_with(|| Arc::new(network));
            Ok(Response::LoadNetwork { network_hash: hash })
        }
        Request::LoadPopulationLayer { bytes, pop_field } => {
            let hash = content_hash(&bytes);
            let mut registry = layers().lock().expect("layer lock");
            if !registry.contains_key(&hash) {
                let layer = parse_pop_layer(&bytes, pop_field.as_deref())
                    .map_err(|e| format!("population layer: {e}"))?;
                registry.insert(hash.clone(), Arc::new(layer));
            }
            let layer = registry
                .get(&hash)
                .expect("layer was just inserted under its hash");
            Ok(Response::LoadPopulationLayer {
                layer_hash: hash,
                points: layer.points.len() as u64,
                pop_field: layer.pop_field.clone(),
                total_pop: layer.points.iter().map(|p| p.pop).sum(),
            })
        }
        Request::NetworkStats { network_hash, date, include_routes, include_stops, include_service_types } => {
            let parsed = parse_date(&date)?;
            with_network(&network_hash, |network| {
                let mut stats = network_stats(network, parsed);
                // Query shaping: the flags right-size the response AT
                // the op (the founding dogfood session read a 199 KiB
                // answer to a five-number question through a
                // hand-rolled summarizer — the missing piece was this
                // parameter, not a script).
                if !include_routes {
                    stats.routes.clear();
                }
                if !include_stops {
                    stats.stops.clear();
                }
                if !include_service_types {
                    stats.service_types.clear();
                }
                Ok(Response::NetworkStats { stats })
            })
        }
        Request::ServiceTypesOverview { network_hash } => with_network(&network_hash, |network| {
            Ok(Response::ServiceTypesOverview {
                service_types: service_type_overview(network),
            })
        }),
        Request::ServiceTypeDetail { network_hash, service_type } => {
            with_network(&network_hash, |network| {
                // Unknown ids answer empty (QAF-19's convention);
                // only hash addresses error.
                Ok(Response::ServiceTypeDetail { detail: service_type_detail(network, &service_type) })
            })
        }
        Request::ServiceTypeCompare { base_hash, network_hash, service_type } => {
            let base = network_arc(&base_hash)?;
            let proposal = network_arc(&network_hash)?;
            // Unknown ids answer empty (QAF-19's convention);
            // only hash addresses error.
            let comparison = service_type_compare(&base, &proposal, &service_type);
            Ok(Response::ServiceTypeCompare { comparison })
        }
        Request::TypeCoverage { network_hash, layer_hash, radius_m, frequent_secs, min_departures, window } => {
            let network_arc = network_arc(&network_hash)?;
            let layer_arc = {
                let guard = layers().lock().expect("layer lock");
                guard
                    .get(&layer_hash)
                    .cloned()
                    .ok_or_else(|| format!("unknown layer {layer_hash}: load_population_layer first"))?
            };
            Ok(Response::TypeCoverage {
                coverage: type_coverage(
                    &network_arc,
                    &layer_arc,
                    radius_m,
                    frequent_secs,
                    min_departures,
                    window,
                ),
            })
        }
        Request::StopGaps { network_hash, date, stop_id } => {
            let parsed = parse_date(&date)?;
            let network_arc = network_arc(&network_hash)?;
            Ok(Response::StopGaps {
                gaps: transit_analysis::stop_gaps(&network_arc, parsed, &stop_id),
            })
        }
        Request::HourlyTrips { network_hash, date } => {
            let parsed = parse_date(&date)?;
            let network_arc = network_arc(&network_hash)?;
            Ok(Response::HourlyTrips {
                hours: transit_analysis::hourly_trips(&network_arc, parsed),
            })
        }
        Request::ScheduleTexture { network_hash, service_type } => {
            let network_arc = network_arc(&network_hash)?;
            Ok(Response::ScheduleTexture {
                texture: transit_analysis::schedule_texture(&network_arc, &service_type),
            })
        }
        Request::RouteOverlap { network_hash, date } => {
            let parsed = parse_date(&date)?;
            let network_arc = network_arc(&network_hash)?;
            Ok(Response::RouteOverlap {
                overlaps: transit_analysis::route_overlap(&network_arc, parsed),
            })
        }
        Request::TimedTransfers { network_hash, date, stop_id, window } => {
            let parsed = parse_date(&date)?;
            let network_arc = network_arc(&network_hash)?;
            Ok(Response::TimedTransfers {
                transfers: transit_analysis::timed_transfers(&network_arc, parsed, &stop_id, window),
            })
        }
        Request::UnservedPoints { network_hash, layer_hash, radius_m, date } => {
            let parsed = parse_date(&date)?;
            let network_arc = network_arc(&network_hash)?;
            let layer_arc = {
                let guard = layers().lock().expect("layer lock");
                guard
                    .get(&layer_hash)
                    .cloned()
                    .ok_or_else(|| format!("unknown layer {layer_hash}: load_population_layer first"))?
            };
            Ok(Response::UnservedPoints {
                points: transit_analysis::unserved_points(&network_arc, parsed, &layer_arc, radius_m),
            })
        }
    }
}

/// The JSON seam: `{"op": ...}` in, `{"ok": ...}` / `{"error": ...}`
/// out — byte-compatible with the monolith and every other
/// capability module.
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

/// The bulk-bytes seam: op 0 loads a population layer.
pub fn dispatch_bytes(op: u32, bytes: &[u8]) -> String {
    let outcome = match op {
        OP_LOAD_LAYER => match dispatch(Request::LoadPopulationLayer { bytes: bytes.to_vec(), pop_field: None }) {
            Ok(response) => serde_json::json!({ "ok": response }),
            Err(error) => serde_json::json!({ "error": error }),
        },
        cap_abi::OP_LOAD_NETWORK => {
            return cap_abi::outcome(
                cap_abi::parse_document(bytes).and_then(|network| dispatch(Request::LoadNetwork { network })),
            )
        }
        other => serde_json::json!({ "error": format!("unknown bulk op {other}") }),
    };
    serde_json::to_string(&outcome).expect("dispatch outcomes serialize")
}

/// Coverage for a single network on a single date (the calendar
/// view, as the monolith's `coverage` op computes it) — exposed so
/// the composition can answer "what share of the population does
/// this network cover on THIS date".
pub fn coverage_on(
    network_hash: &str,
    layer_hash: &str,
    date: &str,
    radius_m: f64,
    frequent_secs: u32,
    min_departures: u32,
) -> Result<transit_analysis::CoverageStats, String> {
    let parsed = parse_date(date)?;
    let network_arc = network_arc(network_hash)?;
    let layer_arc = {
        let guard = layers().lock().expect("layer lock");
        guard
            .get(layer_hash)
            .cloned()
            .ok_or_else(|| format!("unknown layer {layer_hash}: load_population_layer first"))?
    };
    Ok(coverage(&network_arc, parsed, &layer_arc, radius_m, frequent_secs, min_departures, None))
}

cap_abi::capability! {
    json: dispatch_json,
    bytes: dispatch_bytes,
}
