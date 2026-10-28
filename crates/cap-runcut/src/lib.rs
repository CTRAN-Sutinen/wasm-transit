//! cap-runcut — the runcut capability.
//!
//! One question: **what does this vehicle do today?** — at three
//! granularities: the day's blocks LIST (which vehicles are out —
//! the index facet), one block's authored CHAIN with the layover
//! ripple (the detail facet), and one trip instance's EFFECTIVE
//! stops on the date (the trip detail the map's click-through
//! reads). Blocks and layovers are
//! model state GTFS never had: the runcut is authored, and a
//! diverted trip ending later eats into the vehicle's next
//! layover — the connection-health ripple is IN the answer.
//!
//! Every answer is one MATERIALIZED DAY — `materialize(net,
//! date)`, patched with the edits in force on that date. There is
//! no derived registry, no GTFS expansion, and no date-keyed
//! cache: a day is computed per call, and a day is cheap. The
//! domain code lives in the shared transit-dispatch library; this
//! module is the seam.
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
use transit_dispatch::{BlockChainTrip, DayBlock};
use transit_network::Network;

/// One operation call. Tagged like every capability's seam.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "cap_runcut/")]
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
    /// The day's blocks: every authored block with at least one trip
    /// running on the date, with that day's running trip counts. A
    /// block whose every trip is off that day is a vehicle still in
    /// the garage — not offered.
    Blocks {
        network_hash: String,
        #[serde(default)]
        edits_hash: Option<String>,
        date: String,
    },
    /// The day's fleet in one answer (QAF-3): running blocks, the
    /// peak concurrent vehicles and the first time the peak is
    /// reached, and the platform / revenue / layover hours — the
    /// fold that took one block_trips call per block plus
    /// host-side concurrency glue.
    FleetSummary {
        network_hash: String,
        #[serde(default)]
        edits_hash: Option<String>,
        date: String,
    },
    /// One block's chain on a service date: the AUTHORED runcut (the
    /// model's blocks carry layovers GTFS never had), materialized
    /// for the day, with the in-force edits applied — a diverted trip
    /// ending later eats into the vehicle's next layover (the
    /// connection-health ripple, made visible). Trips of the chain
    /// that do not run on the date stay in view, marked; unknown
    /// blocks answer empty, never error.
    BlockTrips {
        network_hash: String,
        #[serde(default)]
        edits_hash: Option<String>,
        block_id: String,
        date: String,
        /// Under this many seconds left, a layover is TIGHT (the
        /// dispatcher's policy; default 300).
        #[serde(default)]
        #[ts(optional)]
        tight_layover_secs: Option<u32>,
    },
    /// One trip instance's stops on a service date — the day's trip
    /// detail, EFFECTIVE (a diverted trip answers its patched
    /// sequence). The instance's identity is the model's: (trip,
    /// start_secs). Its line is `trip_path`. A trip that does not run on the date answers empty, never an
    /// error.
    TripStops {
        network_hash: String,
        #[serde(default)]
        edits_hash: Option<String>,
        trip: String,
        start_secs: u32,
        date: String,
    },
    /// One trip instance's LINE on a service date, over the effective
    /// day: its pattern's authored alignment while it follows the
    /// pattern, its patched stops joined when diverted. A trip that
    /// does not run on the date answers an empty path.
    TripPath {
        network_hash: String,
        #[serde(default)]
        edits_hash: Option<String>,
        trip: String,
        start_secs: u32,
        date: String,
    },
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "cap_runcut/")]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Response {
    LoadNetwork { network_hash: String },
    LoadEdits {
        edits_hash: String,
        reports: Vec<transit_sync::EditReport>,
    },
    Blocks { blocks: Vec<DayBlock> },
    FleetSummary { summary: transit_dispatch::FleetSummary },
    /// The chain, carrying the block it answers for — a session
    /// asking for many blocks groups results by the answer, never by
    /// request order.
    BlockTrips { block_id: String, trips: Vec<BlockChainTrip> },
    TripStops { stops: Vec<transit_dispatch::TripStopView> },
    TripPath { path: Vec<transit_network::Coord> },
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
        Request::Blocks { network_hash, edits_hash, date } => {
let day = effective_day(&network_hash, edits_hash.as_deref(), &date)?;
            let network = network_arc(&network_hash)?;
            Ok(Response::Blocks { blocks: transit_dispatch::day_blocks(&network, &day) })
        }
        Request::FleetSummary { network_hash, edits_hash, date } => {
            let day = effective_day(&network_hash, edits_hash.as_deref(), &date)?;
            let network = network_arc(&network_hash)?;
            Ok(Response::FleetSummary {
                summary: transit_dispatch::fleet_summary(&network, &day),
            })
        }
        Request::BlockTrips { network_hash, edits_hash, block_id, date, tight_layover_secs } => {
let day = effective_day(&network_hash, edits_hash.as_deref(), &date)?;
            let network = network_arc(&network_hash)?;
            Ok(Response::BlockTrips {
                trips: transit_dispatch::block_chain(
                    &network,
                    &day,
                    &block_id,
                    tight_layover_secs.unwrap_or(transit_dispatch::TIGHT_LAYOVER_SECS),
                ),
                block_id,
            })
        }
        Request::TripStops { network_hash, edits_hash, trip, start_secs, date } => {
let day = effective_day(&network_hash, edits_hash.as_deref(), &date)?;
            let network = network_arc(&network_hash)?;
            Ok(Response::TripStops {
                stops: transit_dispatch::trip_stops(&network, &day, &trip, start_secs),
            })
        }
        Request::TripPath { network_hash, edits_hash, trip, start_secs, date } => {
            let day = effective_day(&network_hash, edits_hash.as_deref(), &date)?;
            let network = network_arc(&network_hash)?;
            Ok(Response::TripPath { path: transit_dispatch::trip_path(&network, &day, &trip, start_secs) })
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

/// The runcut module takes JSON documents (network, edits) — no
/// bulk-bytes op exists; the seam answers honestly instead of
/// silently ignoring one.
pub fn dispatch_bytes(op: u32, bytes: &[u8]) -> String {
    match op {
        cap_abi::OP_LOAD_NETWORK => cap_abi::outcome(
            cap_abi::parse_network(bytes).and_then(|network| dispatch(Request::LoadNetwork { network })),
        ),
        _ => serde_json::json!({ "error": format!("unknown bulk op {op}: runcut answers the kernel's load_network bulk op; edits travel as JSON (load_edits)") }).to_string(),
    }
}

cap_abi::capability! {
    json: dispatch_json,
    bytes: dispatch_bytes,
}
