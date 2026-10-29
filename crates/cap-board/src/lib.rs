//! cap-board — the board capability.
//!
//! One question: **when does service leave this stop?** — asked at
//! three granularities: one stop (a station pin or bay with nothing
//! of its own answers its POOLED walking group — the model's own
//! concept), realtime-first when a LIVE document is supplied, the
//! same stop grouped by route and destination for its display, and a
//! personal saved set
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
//! * **live** — THE live document (transit-live), minted by cap-rt
//!   from an rt snapshot in model vocabulary; the board reads its
//!   predictions and alerts and classifies under its window — no host
//!   joins rt;
//! * **watchlist** — the personal document (v1 notes + colors,
//!   v2 labels; the migration maps note → label and reports the
//!   dropped color).

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use cap_abi::canonical_hash;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use transit_dispatch::{BoardDeparture, GroupedBoard, StopBoard};
use transit_live::LiveDay;
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
    /// Register a LIVE document (minted by cap-rt's `live`) under its
    /// content hash — the same identity cap-rt answered (assert at
    /// the host). Ephemeral like the snapshot it came from.
    LoadLive { live: LiveDay },
    /// THE board at one stop on a service date — when does service
    /// leave here: the materialized day with the in-force edits
    /// (`edits_hash`), realtime-first when `live_hash` names a live
    /// document (rows ordered by when the bus actually leaves; a late
    /// bus scheduled before `from_secs` that has not left is still
    /// on the board; each row carries its expected time, delay, and
    /// status). Each row names its route (short and long), the stop it
    /// leaves from, and where it is going as the sign says there. The
    /// departures the in-force edits took away from the stop are
    /// `skipped`, each with the diversion that skips it; the live
    /// document's service alerts about the board ride along (the board
    /// carries them, each row names the ones about it). A stop with
    /// nothing of its own — nothing leaving, nothing skipped — that
    /// belongs to a walking group (a station pin) answers the group's
    /// board, `pooled`, each row naming its bay.
    StopBoard {
        network_hash: String,
        #[serde(default)]
        #[ts(optional)]
        edits_hash: Option<String>,
        stop_id: String,
        date: String,
        from_secs: u32,
        limit: u32,
        #[serde(default)]
        #[ts(optional)]
        live_hash: Option<String>,
    },
    /// THE board grouped for a stop's display: one group per route
    /// and destination the stop is served by on the date (a group
    /// whose last departure has left stays, with none), each holding
    /// its next `per_group` departures (default 3) by the board's
    /// realtime-first rule and the visits the in-force edits skip,
    /// in the route's display order (`route_sort_order`, then id),
    /// then by destination. Every row says how long until it leaves
    /// (`wait`). A stop with nothing of its own all day that belongs
    /// to a walking group (a station pin) pools it, each row naming
    /// its bay.
    GroupedBoard {
        network_hash: String,
        #[serde(default)]
        #[ts(optional)]
        edits_hash: Option<String>,
        stop_id: String,
        date: String,
        from_secs: u32,
        #[serde(default)]
        #[ts(optional)]
        per_group: Option<u32>,
        #[serde(default)]
        #[ts(optional)]
        live_hash: Option<String>,
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
    LoadLive { live_hash: String },
    StopBoard { board: StopBoard },
    GroupedBoard { board: GroupedBoard },
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

/// Live documents by content hash (canonical serialization).
fn lives() -> &'static Mutex<HashMap<String, Arc<LiveDay>>> {
    static REGISTRY: OnceLock<Mutex<HashMap<String, Arc<LiveDay>>>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

fn live_arc(live_hash: &str) -> Result<Arc<LiveDay>, String> {
    lives()
        .lock()
        .expect("live registry lock")
        .get(live_hash)
        .cloned()
        .ok_or_else(|| format!("unknown live document {live_hash}: load_live first"))
}

/// Watchlist documents by content hash (canonical serialization).
fn watchlists() -> &'static Mutex<HashMap<String, Arc<Watchlist>>> {
    static WATCHLISTS: OnceLock<Mutex<HashMap<String, Arc<Watchlist>>>> = OnceLock::new();
    WATCHLISTS.get_or_init(|| Mutex::new(HashMap::new()))
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

/// The departures the in-force edits took away on the date — `None`
/// with no edit document, or none in force.
fn day_edits(
    network: &Network,
    edits_hash: Option<&str>,
    date: &str,
) -> Result<Option<transit_dispatch::DayEdits>, String> {
    match edits_hash {
        Some(hash) => transit_dispatch::day_edits(network, parse_date(date)?, &edits_arc(hash)?)
            .map_err(|e| format!("the network does not materialize: {e}")),
        None => Ok(None),
    }
}

/// How many departures each group holds when the caller names none —
/// the display's next three.
const PER_GROUP: u32 = 3;

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
        Request::LoadLive { live } => {
            let hash = canonical_hash(&live)?;
            lives().lock().expect("live registry lock").entry(hash.clone()).or_insert_with(|| Arc::new(live));
            Ok(Response::LoadLive { live_hash: hash })
        }
        Request::StopBoard { network_hash, edits_hash, stop_id, date, from_secs, limit, live_hash } => {
            let day = effective_day(&network_hash, edits_hash.as_deref(), &date)?;
            let network = network_arc(&network_hash)?;
            let live = live_hash.as_deref().map(live_arc).transpose()?;
            let edits = day_edits(&network, edits_hash.as_deref(), &date)?;
            Ok(Response::StopBoard {
                board: transit_dispatch::stop_board(
                    &network,
                    &day,
                    &stop_id,
                    from_secs,
                    limit as usize,
                    live.as_deref(),
                    edits.as_ref(),
                ),
            })
        }
        Request::GroupedBoard { network_hash, edits_hash, stop_id, date, from_secs, per_group, live_hash } => {
            let day = effective_day(&network_hash, edits_hash.as_deref(), &date)?;
            let network = network_arc(&network_hash)?;
            let live = live_hash.as_deref().map(live_arc).transpose()?;
            let edits = day_edits(&network, edits_hash.as_deref(), &date)?;
            Ok(Response::GroupedBoard {
                board: transit_dispatch::grouped_board(
                    &network,
                    &day,
                    &stop_id,
                    from_secs,
                    per_group.unwrap_or(PER_GROUP) as usize,
                    live.as_deref(),
                    edits.as_ref(),
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

/// The board module takes JSON documents (network, edits, the
/// watchlist) — no bulk-bytes op exists; the seam answers honestly
/// instead of silently ignoring one.
pub fn dispatch_bytes(op: u32, bytes: &[u8]) -> String {
    match op {
        cap_abi::OP_LOAD_NETWORK => cap_abi::outcome(
            cap_abi::parse_network(bytes).and_then(|network| dispatch(Request::LoadNetwork { network })),
        ),
        _ => serde_json::json!({ "error": format!("unknown bulk op {op}: board answers the kernel's load_network bulk op; edits and watchlists travel as JSON") }).to_string(),
    }
}

cap_abi::capability! {
    json: dispatch_json,
    bytes: dispatch_bytes,
}
