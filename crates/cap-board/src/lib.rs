//! cap-board — the board capability.
//!
//! One question: **when does service leave this stop?** — asked at
//! three granularities: one stop, a station's POOLED bays (the
//! model's own walking-group concept), and a personal saved set
//! (the WATCHLIST document, version-tagged and migrated here — a
//! personal multi-stop board is the same question parameterized by
//! a personal document).
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
//!   contract);
//! * **watchlist** — the personal document (v1 notes + colors,
//!   v2 labels; the migration maps note → label and reports the
//!   dropped color).

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use ts_rs::TS;
use transit_dispatch::BoardDeparture;
use transit_network::Network;

// ---------------------------------------------------------------------
// The watchlist document — the bus's structured, VERSIONED local
// document, so the migration mechanism has something real to move.
//
// v1: stops carry a note and an optional color. v2: stops carry a
// label (from the note); the color is dropped AND REPORTED — never a
// silent loss. The op set speaks both versions, so a rolled-back
// document still queries (the board just loses labels).
// ---------------------------------------------------------------------

/// One stop as version 1 authored it.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "cap_board/")]
pub struct WatchStopV1 {
    pub id: String,
    pub note: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
}

/// One stop as version 2 authors it.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "cap_board/")]
pub struct WatchStopV2 {
    pub id: String,
    pub label: String,
}

/// A watchlist, version-tagged so the seam can dispatch on it.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "cap_board/")]
#[serde(tag = "version", rename_all = "snake_case")]
pub enum Watchlist {
    V1 { stops: Vec<WatchStopV1> },
    V2 { stops: Vec<WatchStopV2> },
}

impl Watchlist {
    pub fn version(&self) -> u32 {
        match self {
            Watchlist::V1 { .. } => 1,
            Watchlist::V2 { .. } => 2,
        }
    }

    /// Stop ids in authored order (the migration identity law: the
    /// id sequence is invariant across versions).
    pub fn stop_ids(&self) -> Vec<&str> {
        match self {
            Watchlist::V1 { stops } => stops.iter().map(|s| s.id.as_str()).collect(),
            Watchlist::V2 { stops } => stops.iter().map(|s| s.id.as_str()).collect(),
        }
    }
}

/// Every loss a migration applies to document data, reported — the
/// ImportNote convention, restated for migrations.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "cap_board/")]
pub struct MigrationNote {
    pub kind: String,
    pub field: String,
    pub count: u32,
}

/// One watched stop's board, with the version-appropriate label
/// (v2 labels; v1 notes — the migration maps note → label).
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "cap_board/")]
pub struct WatchBoardStop {
    pub id: String,
    pub label: String,
    pub departures: Vec<BoardDeparture>,
}

/// One operation call. Tagged like every capability's seam.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "cap_board/")]
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
    /// The board at one stop on a service date: the materialized day,
    /// with the in-force edits applied when `edits_hash` names a
    /// registered edit document (schedule-only when omitted).
    Departures {
        network_hash: String,
        #[serde(default)]
        edits_hash: Option<String>,
        stop_id: String,
        date: String,
        from_secs: u32,
        limit: u32,
    },
    /// The STATION board: the same day pooled over the stop's WALKING
    /// GROUP (the stop, its parent, siblings sharing that parent,
    /// the parent's children — the model's own transfer-group
    /// concept). A station pin answers no departures of its own;
    /// its bays carry the trips. Each row carries its concrete
    /// stop_id, so callers see which bay a departure leaves from.
    StationDepartures {
        network_hash: String,
        #[serde(default)]
        edits_hash: Option<String>,
        stop_id: String,
        date: String,
        from_secs: u32,
        limit: u32,
    },
    /// Register a watchlist document under its content hash
    /// (idempotent by hash, like every document on the bus).
    LoadWatchlist { document: Watchlist },
    /// Migrate a version-1 watchlist to version 2. Pure and
    /// deterministic: label = note, colors dropped and reported.
    /// The caller keeps the old bytes — migrations never destroy.
    MigrateWatchlist { document: Watchlist },
    /// The board for a watchlist: each watched stop's departures on
    /// a service date, labelled per its version — the same question
    /// the per-stop board answers, parameterized by the personal
    /// document.
    WatchlistBoard {
        watchlist_hash: String,
        network_hash: String,
        #[serde(default)]
        edits_hash: Option<String>,
        date: String,
        from_secs: u32,
        limit: u32,
    },
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "cap_board/")]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Response {
    LoadNetwork { network_hash: String },
    LoadEdits {
        edits_hash: String,
        reports: Vec<transit_sync::EditReport>,
    },
    Departures { departures: Vec<BoardDeparture> },
    StationDepartures { departures: Vec<BoardDeparture> },
    LoadWatchlist { watchlist_hash: String, version: u32 },
    MigratedWatchlist { document: Watchlist, notes: Vec<MigrationNote> },
    WatchlistBoard { stops: Vec<WatchBoardStop> },
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

/// Watchlist documents by content hash (canonical serialization).
fn watchlists() -> &'static Mutex<HashMap<String, Arc<Watchlist>>> {
    static WATCHLISTS: OnceLock<Mutex<HashMap<String, Arc<Watchlist>>>> = OnceLock::new();
    WATCHLISTS.get_or_init(|| Mutex::new(HashMap::new()))
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

/// The v1 → v2 migration: pure, deterministic, loss reported. Label
/// comes from the note; the color goes and says so.
pub fn migrate_watchlist(document: &Watchlist) -> Result<(Watchlist, Vec<MigrationNote>), String> {
    match document {
        Watchlist::V1 { stops } => {
            let mut dropped_colors = 0u32;
            let stops = stops
                .iter()
                .map(|s| {
                    if s.color.is_some() {
                        dropped_colors += 1;
                    }
                    WatchStopV2 { id: s.id.clone(), label: s.note.clone() }
                })
                .collect();
            let mut notes = Vec::new();
            if dropped_colors > 0 {
                notes.push(MigrationNote {
                    kind: "dropped_field".into(),
                    field: "color".into(),
                    count: dropped_colors,
                });
            }
            Ok((Watchlist::V2 { stops }, notes))
        }
        Watchlist::V2 { .. } => Err("document is already version 2".into()),
    }
}

fn load_watchlist(document: &Watchlist) -> Result<Response, String> {
    let hash = canonical_hash(document)?;
    watchlists()
        .lock()
        .expect("watchlist lock")
        .entry(hash.clone())
        .or_insert_with(|| Arc::new(document.clone()));
    Ok(Response::LoadWatchlist { watchlist_hash: hash, version: document.version() })
}

/// The label a watched stop carries under its document version.
fn watch_label(watchlist: &Watchlist, stop_id: &str) -> String {
    match watchlist {
        Watchlist::V1 { stops } => stops
            .iter()
            .find(|s| s.id == stop_id)
            .map(|s| s.note.clone())
            .unwrap_or_default(),
        Watchlist::V2 { stops } => stops
            .iter()
            .find(|s| s.id == stop_id)
            .map(|s| s.label.clone())
            .unwrap_or_default(),
    }
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
        Request::Departures { network_hash, edits_hash, stop_id, date, from_secs, limit } => {
let day = effective_day(&network_hash, edits_hash.as_deref(), &date)?;
            let network = network_arc(&network_hash)?;
            Ok(Response::Departures {
                departures: transit_dispatch::board(&network, &day, &stop_id, from_secs, limit as usize),
            })
        }
        Request::StationDepartures { network_hash, edits_hash, stop_id, date, from_secs, limit } => {
let day = effective_day(&network_hash, edits_hash.as_deref(), &date)?;
            let network = network_arc(&network_hash)?;
            Ok(Response::StationDepartures {
                departures: transit_dispatch::station_board(
                    &network,
                    &day,
                    &stop_id,
                    from_secs,
                    limit as usize,
                ),
            })
        }
        Request::LoadWatchlist { document } => load_watchlist(&document),
        Request::MigrateWatchlist { document } => {
            let (migrated, notes) = migrate_watchlist(&document)?;
            Ok(Response::MigratedWatchlist { document: migrated, notes })
        }
        Request::WatchlistBoard { watchlist_hash, network_hash, edits_hash, date, from_secs, limit } => {
            let watchlist = watchlists()
                .lock()
                .expect("watchlist lock")
                .get(&watchlist_hash)
                .cloned()
                .ok_or_else(|| format!("unknown watchlist {watchlist_hash}: load_watchlist first"))?;
let day = effective_day(&network_hash, edits_hash.as_deref(), &date)?;
            let network = network_arc(&network_hash)?;
            let stops = watchlist
                .stop_ids()
                .into_iter()
                .map(|stop_id| WatchBoardStop {
                    id: stop_id.to_string(),
                    label: watch_label(&watchlist, stop_id),
                    departures: transit_dispatch::board(
                        &network,
                        &day,
                        stop_id,
                        from_secs,
                        limit as usize,
                    ),
                })
                .collect();
            Ok(Response::WatchlistBoard { stops })
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

/// The board module takes JSON documents (network, edits, the
/// watchlist) — no bulk-bytes op exists; the seam answers honestly
/// instead of silently ignoring one.
pub fn dispatch_bytes(op: u32, _bytes: &[u8]) -> String {
    serde_json::json!({ "error": format!("unknown bulk op {op}: board takes JSON documents (load_network / load_edits / load_watchlist)") })
        .to_string()
}

cap_abi::capability! {
    json: dispatch_json,
    bytes: dispatch_bytes,
}
