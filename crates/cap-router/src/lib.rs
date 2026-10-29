//! cap-router — the trip-planning capability module.
//!
//! Planning composes over the NETWORK document and the EFFECTIVE
//! DAY, like every model-side capability: the network registers
//! under its content hash (the same identity cap-network's import
//! produces and every day view consumes), the edit document
//! registers the same way, and a plan answers by hash pair + date.
//! The day itself is transit-dispatch's composition — materialize,
//! then patch with the edits in force — never a private calendar
//! expansion (Law 2). A LIVE document (cap-rt's, registered by
//! hash like cap-board's) makes the plan ride each trip instance's
//! predicted times where the document states them. The RAPTOR graph
//! is built lazily per (network, edits, live, date) and cached; a
//! rider whose bundle stops at "know when the bus comes" ships
//! gtfs/board and never downloads this module's planning code.
//!
//! The question is one — how do I get from here to there — at its
//! facets: leaving at a time (`plan`), arriving by a time
//! (`plan_arrive_by`: the latest departure; with no deadline, the
//! last bus home), and the diversion delta (`plan_compare`: the same
//! query over the patched day and the unpatched one — one
//! materialization apart, no second document — with the rider's
//! notice whenever the diversion changes the trip).

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use chrono::NaiveDate;
use cap_abi::canonical_hash;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use transit_live::LiveDay;
use transit_network::Network;
use transit_streets::walking::{DirectWalk, WalkTable};
use transit_sync::EditRecord;

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "cap_router/")]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Request {
    /// Register the NETWORK document under its content hash
    /// (idempotent by hash) — the same document identity
    /// cap-network's import produces and every day view consumes.
    /// Decoded by `cap_abi::load_network_json` on either lane, never
    /// through the tagged envelope (whose buffered decode would cost
    /// ~8× the document in peak memory and a second decoder).
    #[serde(skip_deserializing)]
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
    /// Register a LIVE document (minted by cap-rt's `live`) under its
    /// content hash — the same identity cap-rt answered (assert at
    /// the host). Ephemeral like the snapshot it came from.
    LoadLive { live: LiveDay },
    /// Register a WALK TABLE (minted by cap-streets' `walks` from a
    /// streets document and this network) under its content hash —
    /// the same identity cap-streets answered (assert at the host).
    /// A plan naming it walks its transfers along the streets.
    LoadWalks { walks: WalkTable },
    /// Earliest-arrival plan from `from` to `to`, leaving at
    /// `depart_secs` on `date`. Each end is a stop id (a bare string)
    /// or a PLACE off the network — `{id, name, walks}`, its walks to
    /// the stops near it (cap-streets `access` answers them; a place's
    /// id is never a stop's): the trip walks from the place to one of
    /// its stops and from one of its stops to the place, or straight
    /// there when the `direct` walk arrives as early. Over the
    /// materialized day, with
    /// the in-force edits applied when `edits_hash` names a
    /// registered edit document, and at the predicted times when
    /// `live_hash` names a registered live document, its transfers
    /// along the streets when `walks_hash` names a registered walk
    /// table. `None` in the response means unreachable within
    /// `max_legs`.
    Plan {
        network_hash: String,
        #[serde(default)]
        edits_hash: Option<String>,
        from: transit_router::End,
        to: transit_router::End,
        date: String,
        depart_secs: u32,
        max_legs: u32,
        #[serde(default)]
        live_hash: Option<String>,
        /// The walk table the transfers walk (`load_walks`); with
        /// none, transfers are straight lines (D-32).
        #[serde(default)]
        walks_hash: Option<String>,
        /// The walk straight from `from` to `to`, when one joins them
        /// — cap-streets `access`'s `direct`, as it is.
        #[serde(default)]
        direct: Option<DirectWalk>,
    },
    /// Arrive-by: the LATEST departure from `from` that reaches `to`
    /// by `arrive_by_secs` on `date` within `max_legs` — with no
    /// deadline, the latest that reaches `to` at all on the service
    /// day (the last bus home; the deadline is then the day's last
    /// arrival). The same day as `plan` (edits, live).
    PlanArriveBy {
        network_hash: String,
        #[serde(default)]
        edits_hash: Option<String>,
        from: transit_router::End,
        to: transit_router::End,
        date: String,
        #[serde(default)]
        arrive_by_secs: Option<u32>,
        max_legs: u32,
        #[serde(default)]
        live_hash: Option<String>,
        /// The walk table the transfers walk (`load_walks`); with
        /// none, transfers are straight lines (D-32).
        #[serde(default)]
        walks_hash: Option<String>,
        /// The walk straight from `from` to `to`, when one joins them
        /// — cap-streets `access`'s `direct`, as it is.
        #[serde(default)]
        direct: Option<DirectWalk>,
    },
    /// The diversion delta: the same plan over the patched day
    /// (active) and the unpatched materialization (baseline), both
    /// at the live document's predictions when `live_hash` is given.
    /// The baseline is `None` when no edit is in force on the date
    /// (or no edits hash is given) — there is no second world to
    /// compare against. The notice is what the rider is told
    /// whenever the diversion changes the trip: no trip, a later
    /// arrival, or a ride on a diverted trip (and that no trip exists
    /// within `max_legs`). One op whether or not edits exist.
    PlanCompare {
        network_hash: String,
        #[serde(default)]
        edits_hash: Option<String>,
        from: transit_router::End,
        to: transit_router::End,
        date: String,
        depart_secs: u32,
        max_legs: u32,
        #[serde(default)]
        live_hash: Option<String>,
        /// The walk table the transfers walk (`load_walks`); with
        /// none, transfers are straight lines (D-32).
        #[serde(default)]
        walks_hash: Option<String>,
        /// The walk straight from `from` to `to`, when one joins them
        /// — cap-streets `access`'s `direct`, as it is.
        #[serde(default)]
        direct: Option<DirectWalk>,
    },
    /// Where an itinerary runs, leg by leg, over the same effective
    /// day it was planned on: a transit leg's RIDDEN segment (the
    /// trip's alignment sliced between its board and alight stops
    /// while it follows its pattern; its patched stops when diverted),
    /// a walk through the stops it passes — and, given the plan's
    /// ends and direct walk, along the streets from or to a place
    /// (its walk's path; between two places, the direct walk's). The
    /// map draws the answer.
    ItineraryPaths {
        network_hash: String,
        #[serde(default)]
        edits_hash: Option<String>,
        date: String,
        itinerary: transit_router::Itinerary,
        #[serde(default)]
        from: Option<transit_router::End>,
        #[serde(default)]
        to: Option<transit_router::End>,
        #[serde(default)]
        direct: Option<DirectWalk>,
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
    LoadLive { live_hash: String },
    LoadWalks { walks_hash: String },
    Plan { itinerary: Option<transit_router::Itinerary> },
    /// The deadline answered (the request's, else the day's end) and
    /// the itinerary leaving as late as possible.
    PlanArriveBy {
        arrive_by_secs: u32,
        itinerary: Option<transit_router::Itinerary>,
    },
    PlanCompare {
        active: Option<transit_router::Itinerary>,
        baseline: Option<transit_router::Itinerary>,
        notice: Option<transit_router::PlanNotice>,
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

/// Live documents by content hash (canonical serialization).
fn lives() -> &'static Mutex<HashMap<String, Arc<LiveDay>>> {
    static REGISTRY: OnceLock<Mutex<HashMap<String, Arc<LiveDay>>>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Walk tables by content hash (canonical serialization).
fn walk_tables() -> &'static Mutex<HashMap<String, Arc<WalkTable>>> {
    static REGISTRY: OnceLock<Mutex<HashMap<String, Arc<WalkTable>>>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

/// A graph's identity: (network, edits, live, date, walks).
type GraphKey = (String, Option<String>, Option<String>, NaiveDate, Option<String>);

/// RAPTOR graphs by (network, edits, live, date, walks), built on first
/// use — the graph is a pure function of the day and its
/// predictions, so it caches per day.
fn graphs() -> &'static Mutex<HashMap<GraphKey, Arc<transit_router::RouterNetwork>>> {
    static GRAPHS: OnceLock<Mutex<HashMap<GraphKey, Arc<transit_router::RouterNetwork>>>> = OnceLock::new();
    GRAPHS.get_or_init(|| Mutex::new(HashMap::new()))
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

fn live_arc(live_hash: &str) -> Result<Arc<LiveDay>, String> {
    lives()
        .lock()
        .expect("live registry lock")
        .get(live_hash)
        .cloned()
        .ok_or_else(|| format!("unknown live document {live_hash}: load_live first"))
}

/// A registered walk table, refused when it was derived for another
/// network: its stops would be another network's.
fn walks_arc(walks_hash: &str, network_hash: &str) -> Result<Arc<WalkTable>, String> {
    let table = walk_tables()
        .lock()
        .expect("walk table registry lock")
        .get(walks_hash)
        .cloned()
        .ok_or_else(|| format!("unknown walk table {walks_hash}: load_walks first"))?;
    if table.network_hash != network_hash {
        return Err(format!(
            "walk table {walks_hash} walks network {}, not {network_hash}",
            table.network_hash
        ));
    }
    Ok(table)
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

/// The RAPTOR graph for (network, edits, live, date, walks), built
/// on first use. No edits hash is the unpatched materialization — the
/// compare facet's baseline; no walks hash, straight-line transfers.
fn graph(
    network_hash: &str,
    edits_hash: Option<&str>,
    live_hash: Option<&str>,
    walks_hash: Option<&str>,
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
    let live = live_hash.map(live_arc).transpose()?;
    let walks = walks_hash.map(|h| walks_arc(h, network_hash)).transpose()?;
    let key = (
        network_hash.to_string(),
        edits_hash.map(str::to_string),
        live_hash.map(str::to_string),
        date,
        walks_hash.map(str::to_string),
    );
    if let Some(cached) = graphs().lock().expect("graph cache lock").get(&key) {
        return Ok(cached.clone());
    }
    let day = effective_day(&network, &records, date)?;
    let mut built = transit_router::RouterNetwork::build(&network, &day, date, live.as_deref());
    if let Some(walks) = &walks {
        built = built.with_walks(walks);
    }
    let built = Arc::new(built);
    let mut cache = graphs().lock().expect("graph cache lock");
    // Live documents are ephemeral (one per rt moment): only the
    // newest live document's graphs stay cached, or every snapshot
    // would leave a graph behind in linear memory that never shrinks.
    if let Some(current) = &key.2 {
        cache.retain(|k, _| k.2.as_ref().is_none_or(|h| h == current));
    }
    cache.insert(key, built.clone());
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
        Request::LoadLive { live } => {
            let hash = canonical_hash(&live)?;
            lives().lock().expect("live registry lock").entry(hash.clone()).or_insert_with(|| Arc::new(live));
            Ok(Response::LoadLive { live_hash: hash })
        }
        Request::LoadWalks { walks } => {
            let hash = canonical_hash(&walks)?;
            walk_tables().lock().expect("walk table registry lock").entry(hash.clone()).or_insert_with(|| Arc::new(walks));
            Ok(Response::LoadWalks { walks_hash: hash })
        }
        Request::PlanArriveBy {
            network_hash, edits_hash, from, to, date, arrive_by_secs, max_legs, live_hash, walks_hash, direct,
        } => {
            let net = graph(&network_hash, edits_hash.as_deref(), live_hash.as_deref(), walks_hash.as_deref(), &date)?;
            let legs = max_legs.max(1) as usize;
            let direct_secs = direct.map(|d| d.walk_secs);
            let answer = transit_router::latest_departure_between(&net, &from, &to, direct_secs, arrive_by_secs, legs)
                .map_err(|e| e.to_string())?;
            Ok(Response::PlanArriveBy { arrive_by_secs: answer.arrive_by_secs, itinerary: answer.itinerary })
        }
        Request::Plan {
            network_hash, edits_hash, from, to, date, depart_secs, max_legs, live_hash, walks_hash, direct,
        } => {
            let net = graph(&network_hash, edits_hash.as_deref(), live_hash.as_deref(), walks_hash.as_deref(), &date)?;
            let legs = max_legs.max(1) as usize;
            let direct_secs = direct.map(|d| d.walk_secs);
            let itinerary = transit_router::earliest_arrival_between(&net, &from, &to, direct_secs, depart_secs, legs)
                .map_err(|e| e.to_string())?;
            Ok(Response::Plan { itinerary })
        }
        Request::ItineraryPaths { network_hash, edits_hash, date, itinerary, from, to, direct } => {
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
                    transit_router::Leg::Transit { trip, start_secs, board_stop_id, alight_stop_id, .. } => {
                        transit_dispatch::LegPath {
                            kind: transit_dispatch::LegKind::Transit,
                            path: transit_dispatch::trip_segment(
                                &network, &day, trip, *start_secs, board_stop_id, alight_stop_id,
                            ),
                        }
                    }
                    transit_router::Leg::Walk { from_stop_id, via_stop_ids, to_stop_id, .. } => {
                        transit_dispatch::LegPath {
                            kind: transit_dispatch::LegKind::Walk,
                            path: transit_router::walk_path(
                                &network,
                                from_stop_id,
                                via_stop_ids,
                                to_stop_id,
                                [from.as_ref(), to.as_ref()],
                                direct.as_ref(),
                            ),
                        }
                    }
                })
                .collect();
            Ok(Response::ItineraryPaths { paths })
        }
        Request::PlanCompare {
            network_hash, edits_hash, from, to, date, depart_secs, max_legs, live_hash, walks_hash, direct,
        } => {
            // The request's own fields validate before any registry
            // lookup, so both worlds fail identically on bad input.
            let parsed = parse_date(&date)?;
            let legs = max_legs.max(1) as usize;
            let net = graph(&network_hash, edits_hash.as_deref(), live_hash.as_deref(), walks_hash.as_deref(), &date)?;
            let direct_secs = direct.map(|d| d.walk_secs);
            let active = transit_router::earliest_arrival_between(&net, &from, &to, direct_secs, depart_secs, legs)
                .map_err(|e| e.to_string())?;
            let network = network_arc(&network_hash)?;
            // The baseline is the unpatched materialization — one
            // materialization apart, never a second document — read
            // at the same predictions; the diverted trips are the
            // instances the patch moved.
            let (baseline, diverted) = match edits_hash.as_deref() {
                Some(hash) => {
                    let records = edits_arc(hash)?;
                    if transit_sync::in_force_on(&records, parsed).is_empty() {
                        (None, transit_router::DivertedTrips::default())
                    } else {
                        let base = graph(&network_hash, None, live_hash.as_deref(), walks_hash.as_deref(), &date)?;
                        let baseline =
                            transit_router::earliest_arrival_between(&base, &from, &to, direct_secs, depart_secs, legs)
                            .map_err(|e| e.to_string())?;
                        let unpatched = transit_network::materialize(&network, parsed)
                            .map_err(|e| format!("the network does not materialize: {e}"))?;
                        let patched = effective_day(&network, &records, parsed)?;
                        (baseline, transit_router::diverted_instances(&unpatched, &patched))
                    }
                }
                None => (None, transit_router::DivertedTrips::default()),
            };
            let notice = transit_router::plan_notice(
                &network,
                active.as_ref(),
                baseline.as_ref(),
                max_legs.max(1),
                &diverted,
            );
            Ok(Response::PlanCompare { active, baseline, notice })
        }
    }
}

/// The JSON seam: `{\"op\": ...}` in, `{\"ok\": ...}` /
/// `{\"error\": ...}` out — byte-compatible with every other
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

/// The router module takes JSON documents (network, edits, live) — no
/// bulk-bytes op exists; the seam answers honestly instead of
/// silently ignoring one.
pub fn dispatch_bytes(op: u32, bytes: &[u8]) -> String {
    match op {
        cap_abi::OP_LOAD_NETWORK => cap_abi::outcome(
            cap_abi::parse_network(bytes).and_then(|network| dispatch(Request::LoadNetwork { network })),
        ),
        _ => serde_json::json!({ "error": format!("unknown bulk op {op}: router answers the kernel's load_network bulk op; edits and live documents travel as JSON (load_edits, load_live)") }).to_string(),
    }
}

cap_abi::capability! {
    json: dispatch_json,
    bytes: dispatch_bytes,
}
