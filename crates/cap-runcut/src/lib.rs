//! cap-runcut — the runcut capability.
//!
//! One question: **what does this vehicle do today?** — at three
//! granularities: the day's blocks LIST (which vehicles are out —
//! the index facet), one block's authored CHAIN with the layover
//! ripple (the detail facet), and one derived trip's EFFECTIVE
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

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
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
    },
    /// One derived trip's stops on a service date — the day's trip
    /// detail, EFFECTIVE (a diverted trip answers its patched
    /// sequence). The trip's identity is the model's: (schedule,
    /// start_secs). The map's click-through reads this; the answer
    /// carries no geometry (stops carry coordinates in the network
    /// document — the map is the host's view, not the module's).
    /// A trip that does not run on the date answers empty, never an
    /// error.
    TripStops {
        network_hash: String,
        #[serde(default)]
        edits_hash: Option<String>,
        schedule: String,
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
    BlockTrips { trips: Vec<BlockChainTrip> },
    TripStops { stops: Vec<transit_dispatch::TripStopView> },
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
        Request::BlockTrips { network_hash, edits_hash, block_id, date } => {
let day = effective_day(&network_hash, edits_hash.as_deref(), &date)?;
            let network = network_arc(&network_hash)?;
            Ok(Response::BlockTrips {
                trips: transit_dispatch::block_chain(&network, &day, &block_id),
            })
        }
        Request::TripStops { network_hash, edits_hash, schedule, start_secs, date } => {
let day = effective_day(&network_hash, edits_hash.as_deref(), &date)?;
            let network = network_arc(&network_hash)?;
            Ok(Response::TripStops {
                stops: transit_dispatch::trip_stops(&network, &day, &schedule, start_secs),
            })
        }
    }
}

/// The JSON seam: `{"op": ...}` in, `{"ok": ...}` / `{"error": ...}`
/// out — byte-compatible with every other capability module.
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

/// The runcut module takes JSON documents (network, edits) — no
/// bulk-bytes op exists; the seam answers honestly instead of
/// silently ignoring one.
pub fn dispatch_bytes(op: u32, _bytes: &[u8]) -> String {
    serde_json::json!({ "error": format!("unknown bulk op {op}: runcut takes JSON documents (load_network / load_edits)") })
        .to_string()
}

cap_abi::capability! {
    json: dispatch_json,
    bytes: dispatch_bytes,
}
