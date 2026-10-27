//! cap-directory — the directory capability.
//!
//! One question: **what exists in the network?** — the stops (with
//! their walking-group structure), the routes, and one route's
//! patterns with their ordered stops (list → detail). The pickers
//! and the names join read here; they are MODEL questions, never
//! feed reads. Patterns are the model's directions: a route's
//! stop sequences are its patterns, each with its own ordered
//! stops — that is what a diversion editor anchors against — and
//! where each entity is on the map (focus). The planner's lists ask
//! here too: rows of any kind narrowed by a filter (entities), and a
//! route's composition with each schedule's trip count (route_detail).
//!
//! Documents are content-addressed like everywhere on the bus:
//! * **network** — the same identity cap-network's import produces
//!   and cap-analysis consumes (the module is the hash authority).

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use serde::{Deserialize, Serialize};
use ts_rs::TS;
use transit_network::Network;

/// One stop as the directory lists it: identity, name, where it
/// is, and its walking-group structure — the parent flag a station
/// board keys off, the transfer flag that marks a designated
/// change point.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "cap_directory/")]
pub struct DirectoryStop {
    pub stop_id: String,
    pub name: String,
    pub lat: f64,
    pub lon: f64,
    /// The parent station when this stop is a bay — the flag the
    /// station board keys off (a stop with a parent, or with
    /// children, pools; a plain stop stays per-stop).
    pub parent: Option<String>,
    /// A designated transfer point (authored intent — the analysis
    /// ops measure the timed connections it enables).
    pub transfer: bool,
}

/// One route as the directory lists it: identity, display name, its
/// mode (the model's closed set, not GTFS's numeric codes), and its
/// authored color when there is one.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "cap_directory/")]
pub struct DirectoryRoute {
    pub route_id: String,
    pub name: String,
    pub mode: transit_network::Mode,
    pub color: Option<String>,
}

/// One stop on a pattern, in order.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "cap_directory/")]
pub struct StopName {
    pub stop_id: String,
    pub name: String,
}

/// One pattern of a route: its identity, its human-facing name, and
/// its ordered stops — the alignment of service a diversion editor
/// anchors against.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "cap_directory/")]
pub struct PatternStops {
    pub pattern: String,
    pub name: String,
    pub stops: Vec<StopName>,
}

/// One operation call. Tagged like every capability's seam.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "cap_directory/")]
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
    /// The stop list, name-sorted (the pickers' shape): every stop
    /// with its walking-group structure. `query` narrows it the way
    /// a picker's typing does — every whitespace-separated term must
    /// appear in the name or the id, case-insensitive ("fisher
    /// landing" finds Fisher's Landing and its bays). `limit`
    /// truncates after the sort; `None` answers the whole list — a
    /// silently truncated directory would be a lie.
    Stops {
        network_hash: String,
        #[serde(default)]
        #[ts(optional)]
        query: Option<String>,
        #[serde(default)]
        limit: Option<u32>,
    },
    /// Stops by id, in the request's order — the names join hosts
    /// otherwise do by hand when a view (like rt's) carries ids a
    /// human reads as names. Unknown ids are omitted (the caller
    /// asked for stops, and silence is the honest answer for an id
    /// the network does not carry).
    StopsByIds { network_hash: String, ids: Vec<String> },
    /// The route list, name-sorted: every route with its mode.
    Routes { network_hash: String },
    /// One route's patterns with their ordered stops — the model's
    /// directions. An unknown route answers empty, never an error.
    RouteStopSequence { network_hash: String, route_id: String },
    /// Where one entity is on the map: its lines (pattern alignments —
    /// authored, else stop to stop) and the stops it serves. A stop is
    /// its point; a schedule its pattern; a route, a service type, or a
    /// block every pattern it runs. Null for an unknown id.
    Focus { network_hash: String, kind: transit_network::EntityKind, id: String },
    /// Every entity of one kind, in id order, narrowed the way a
    /// filter box's typing does (every term in the name or the id,
    /// case-insensitive): what places it (a pattern's route, a
    /// schedule's pattern and service type) and the trips a schedule
    /// runs or a block chains.
    Entities {
        network_hash: String,
        kind: transit_network::EntityKind,
        #[serde(default)]
        #[ts(optional)]
        query: Option<String>,
    },
    /// One route's composition: its patterns, the stops they visit,
    /// and its schedules grouped by service type, each with the trips
    /// it runs (the materialized day's count). Null for an unknown
    /// route.
    RouteDetail { network_hash: String, route_id: String },
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "cap_directory/")]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Response {
    LoadNetwork { network_hash: String },
    Stops { stops: Vec<DirectoryStop> },
    StopsByIds { stops: Vec<DirectoryStop> },
    Routes { routes: Vec<DirectoryRoute> },
    RouteStopSequence { patterns: Vec<PatternStops> },
    Focus { focus: Option<transit_network::EntityFocus> },
    Entities { rows: Vec<transit_network::EntityRow> },
    RouteDetail { detail: Option<transit_network::RouteDetail> },
}

/// Networks by content hash (canonical serialization).
fn networks() -> &'static Mutex<HashMap<String, Arc<Network>>> {
    static REGISTRY: OnceLock<Mutex<HashMap<String, Arc<Network>>>> = OnceLock::new();
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

fn directory_stop(network: &Network, id: &str) -> Option<DirectoryStop> {
    network.stops.get(id).map(|s| DirectoryStop {
        stop_id: id.to_string(),
        name: s.name.clone(),
        lat: s.lat,
        lon: s.lon,
        parent: s.parent.clone(),
        transfer: s.transfer,
    })
}

/// The typed dispatch (native hosts call this directly). Failures
/// are `{"error": ...}`, never panics — a total seam.
pub fn dispatch(request: Request) -> Result<Response, String> {
    match request {
        Request::LoadNetwork { network } => {
            let hash = cap_abi::canonical_hash(&network)?;
            networks()
                .lock()
                .expect("network registry lock")
                .entry(hash.clone())
                .or_insert_with(|| Arc::new(network));
            Ok(Response::LoadNetwork { network_hash: hash })
        }
        Request::Stops { network_hash, query, limit } => {
            let network = network_arc(&network_hash)?;
            let terms: Vec<String> = query
                .as_deref()
                .unwrap_or("")
                .split_whitespace()
                .map(str::to_lowercase)
                .collect();
            let mut stops: Vec<DirectoryStop> = network
                .stops
                .keys()
                .filter_map(|id| directory_stop(&network, id))
                .filter(|stop| {
                    let haystack = format!("{} {}", stop.name, stop.stop_id).to_lowercase();
                    terms.iter().all(|term| haystack.contains(term.as_str()))
                })
                .collect();
            // Name-sorted for pickers; the id breaks ties so the
            // order is total and deterministic.
            stops.sort_by(|a, b| a.name.cmp(&b.name).then_with(|| a.stop_id.cmp(&b.stop_id)));
            if let Some(limit) = limit {
                stops.truncate(limit as usize);
            }
            Ok(Response::Stops { stops })
        }
        Request::StopsByIds { network_hash, ids } => {
            let network = network_arc(&network_hash)?;
            let stops = ids
                .iter()
                .filter_map(|id| directory_stop(&network, id))
                .collect();
            Ok(Response::StopsByIds { stops })
        }
        Request::Routes { network_hash } => {
            let network = network_arc(&network_hash)?;
            let mut routes: Vec<DirectoryRoute> = network
                .routes
                .iter()
                .map(|(id, r)| DirectoryRoute {
                    route_id: id.clone(),
                    name: r.name.clone(),
                    mode: r.mode,
                    color: r.color.clone(),
                })
                .collect();
            routes.sort_by(|a, b| a.name.cmp(&b.name).then_with(|| a.route_id.cmp(&b.route_id)));
            Ok(Response::Routes { routes })
        }
        Request::RouteStopSequence { network_hash, route_id } => {
            let network = network_arc(&network_hash)?;
            let patterns = network
                .patterns
                .iter()
                .filter(|(_, p)| p.route == route_id)
                .map(|(id, p)| PatternStops {
                    pattern: id.clone(),
                    name: p.name.clone(),
                    stops: p
                        .stops
                        .iter()
                        .map(|ps| StopName {
                            stop_id: ps.stop.clone(),
                            name: network
                                .stops
                                .get(&ps.stop)
                                .map(|s| s.name.clone())
                                .unwrap_or_else(|| ps.stop.clone()),
                        })
                        .collect(),
                })
                .collect();
            Ok(Response::RouteStopSequence { patterns })
        }
        Request::Focus { network_hash, kind, id } => {
            let network = network_arc(&network_hash)?;
            Ok(Response::Focus { focus: transit_network::entity_focus(&network, kind, &id) })
        }
        Request::Entities { network_hash, kind, query } => {
            let network = network_arc(&network_hash)?;
            Ok(Response::Entities {
                rows: transit_network::entity_rows(&network, kind, query.as_deref().unwrap_or("")),
            })
        }
        Request::RouteDetail { network_hash, route_id } => {
            let network = network_arc(&network_hash)?;
            Ok(Response::RouteDetail { detail: transit_network::route_detail(&network, &route_id) })
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

/// The directory module takes a JSON document (the network) — no
/// bulk-bytes op exists; the seam answers honestly instead of
/// silently ignoring one.
pub fn dispatch_bytes(op: u32, bytes: &[u8]) -> String {
    match op {
        cap_abi::OP_LOAD_NETWORK => cap_abi::outcome(
            cap_abi::parse_document(bytes).and_then(|network| dispatch(Request::LoadNetwork { network })),
        ),
        _ => serde_json::json!({ "error": format!("unknown bulk op {op}: directory answers only the kernel's load_network bulk op") }).to_string(),
    }
}

cap_abi::capability! {
    json: dispatch_json,
    bytes: dispatch_bytes,
}
