//! cap-gtfs — the GTFS capability module.
//!
//! One horizontal capability of the capability-bus PoC: parse a GTFS
//! static zip, hold it in a registry keyed by the CONTENT HASH of the
//! input bytes, and answer read queries over it (departures, stop
//! list). It has no ops that author anything — "only planners write
//! networks, everything else imports" is enforced by the Request enum,
//! not by convention.
//!
//! The hash-keyed registry is the PoC's core seam: capabilities compose
//! through documents addressed by content hash, so the host (JS) can
//! load the same bytes into any set of modules and assert they agree
//! on the identity — "same document, same truth" is a checkable
//! property, not a hope.
//!
//! The ABI is the same flat, import-free C ABI as the monolith
//! (via `cap_abi::capability!`): a capability differs in which ops its
//! dispatch answers, never in how it is called. The TS types that
//! cross the boundary are generated into `types/cap_gtfs/`.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use ts_rs::TS;
use transit_core::{FeedSummary, StopSummary};

/// The bulk-bytes op code (the only op that takes raw feed bytes).
pub const OP_LOAD_FEED: u32 = 0;

// ---------------------------------------------------------------------
// The watchlist document — the PoC's structured, VERSIONED local
// document, so the migration mechanism has something real to move.
//
// v1: stops carry a note and an optional color. v2: stops carry a
// label (from the note); the color is dropped AND REPORTED — never a
// silent loss. The op set speaks both versions, so a rolled-back
// document still queries (the board just loses labels).
// ---------------------------------------------------------------------

/// One stop as version 1 authored it.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "cap_gtfs/")]
pub struct WatchStopV1 {
    pub id: String,
    pub note: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
}

/// One stop as version 2 authors it.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "cap_gtfs/")]
pub struct WatchStopV2 {
    pub id: String,
    pub label: String,
}

/// A watchlist, version-tagged so the seam can dispatch on it.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "cap_gtfs/")]
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
#[ts(export, export_to = "cap_gtfs/")]
pub struct MigrationNote {
    pub kind: String,
    pub field: String,
    pub count: u32,
}

/// One stop's departures on the board, with the version-appropriate
/// label (v2 labels; v1 notes — the migration maps note → label).
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "cap_gtfs/")]
pub struct WatchBoardStop {
    pub id: String,
    pub label: String,
    pub departures: Vec<transit_core::Departure>,
}

/// One operation call. Tagged like the monolith's seam: the serde form
/// is `{"op": "<name>", ...fields}`.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "cap_gtfs/")]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Request {
    /// Parse a GTFS zip from raw bytes; registers the feed under its
    /// content hash. Loading the same bytes twice is idempotent —
    /// same hash, same registry entry.
    LoadFeed { bytes: Vec<u8> },
    /// Upcoming departures at a stop on a service date (schedule
    /// only; GTFS-rt overlay is a separate capability's business).
    Departures {
        feed_hash: String,
        stop_id: String,
        date: String,
        from_secs: u32,
        limit: u32,
    },
    /// Upcoming departures POOLED over a stop's WALKING GROUP — the
    /// station board: the stop, its parent, siblings sharing that
    /// parent, and the parent's children. A station pin (Fisher's
    /// Landing, Main Station) answers no departures of its own; the
    /// bays carry the trips. Each row carries its concrete stop_id,
    /// so callers see which bay a departure leaves from. Any member
    /// of the group answers the same board.
    StationDepartures {
        feed_hash: String,
        stop_id: String,
        date: String,
        from_secs: u32,
        limit: u32,
    },
    /// Stop list for UI pickers: the first `limit` stops by id.
    Stops { feed_hash: String, limit: u32 },
    /// Stop names by id — the join hosts otherwise do by hand when
    /// a view (like rt's) carries ids a human reads as names.
    /// Unknown ids are omitted (the caller asked for names, and
    /// silence is the honest answer for an id the feed does not
    /// carry).
    StopsByIds { feed_hash: String, ids: Vec<String> },
    /// Service keys running on a date — often several at once
    /// (regular patterns plus exception-added services). The
    /// dispatcher's first question: what is running today?
    ActiveServices { feed_hash: String, date: String },
    /// All blocks in the feed; with `date`, only the blocks RUNNING
    /// that day, with that day's trip counts — the vehicle picker
    /// shows buses in service, not parked buses.
    Blocks { feed_hash: String, date: Option<String> },
    /// One block's trip chain on a service date: where the vehicle
    /// starts, where it ends, the layover between trips — the
    /// dispatcher's block view.
    BlockTrips { feed_hash: String, block_id: String, date: String },
    /// `feed_info.txt` metadata (version, publisher) — the identity
    /// question a dispatcher asks first: which feed am I dispatching?
    FeedInfo { feed_hash: String },
    /// Register a watchlist document under its content hash
    /// (idempotent by hash, like feeds).
    LoadWatchlist { document: Watchlist },
    /// Migrate a version-1 watchlist to version 2. Pure and
    /// deterministic: label = note, colors dropped and reported.
    /// The caller keeps the old bytes — migrations never destroy.
    MigrateWatchlist { document: Watchlist },
    /// The board for a watchlist: each watched stop's upcoming
    /// departures on a service date, labelled per its version.
    WatchlistBoard {
        watchlist_hash: String,
        feed_hash: String,
        date: String,
        from_secs: u32,
        limit: u32,
    },
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "cap_gtfs/")]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Response {
    LoadFeed { feed_hash: String, summary: FeedSummary },
    Departures { departures: Vec<transit_core::Departure> },
    StationDepartures { departures: Vec<transit_core::Departure> },
    Stops { stops: Vec<StopSummary> },
    StopsByIds { stops: Vec<StopSummary> },
    ActiveServices { services: Vec<String> },
    Blocks { blocks: Vec<transit_core::BlockSummary> },
    BlockTrips { trips: Vec<transit_core::BlockTrip> },
    FeedInfo { info: Option<transit_core::FeedInfo> },
    LoadWatchlist { watchlist_hash: String, version: u32 },
    MigratedWatchlist { document: Watchlist, notes: Vec<MigrationNote> },
    WatchlistBoard { stops: Vec<WatchBoardStop> },
}

/// The registry: content hash -> parsed feed. One instance per module
/// (wasm modules never share state); the mutex only guards the map.
fn registry() -> &'static Mutex<HashMap<String, Arc<transit_core::Gtfs>>> {
    static REGISTRY: OnceLock<Mutex<HashMap<String, Arc<transit_core::Gtfs>>>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

/// The watchlist registry: content hash -> document. Hashed over the
/// CANONICAL serialization, so byte-variants of one logical document
/// land on one entry.
fn watchlists() -> &'static Mutex<HashMap<String, Arc<Watchlist>>> {
    static WATCHLISTS: OnceLock<Mutex<HashMap<String, Arc<Watchlist>>>> = OnceLock::new();
    WATCHLISTS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// "sha256:<hex>" — the manifest's document-hash format everywhere.
pub fn content_hash(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    format!("sha256:{:x}", digest)
}

fn watchlist_hash(document: &Watchlist) -> Result<String, String> {
    let canonical = serde_json::to_string(document).map_err(|e| e.to_string())?;
    Ok(content_hash(canonical.as_bytes()))
}

fn with_feed<T>(
    feed_hash: &str,
    f: impl FnOnce(&transit_core::Gtfs) -> Result<T, String>,
) -> Result<T, String> {
    let guard = registry().lock().expect("registry lock");
    let feed = guard
        .get(feed_hash)
        .ok_or_else(|| format!("unknown feed {feed_hash}: load_feed first"))?;
    f(&feed)
}

fn parse_date(date: &str) -> Result<chrono::NaiveDate, String> {
    chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d")
        .map_err(|e| format!("invalid date {date:?}: {e}"))
}

/// Load a feed into the registry; idempotent by content hash.
fn load_feed(bytes: &[u8]) -> Result<Response, String> {
    let hash = content_hash(bytes);
    {
        let guard = registry().lock().expect("registry lock");
        if let Some(feed) = guard.get(&hash) {
            return Ok(Response::LoadFeed {
                feed_hash: hash,
                summary: feed.summary(),
            });
        }
    }
    let feed =
        transit_core::Gtfs::from_zip_bytes(bytes).map_err(|e| format!("parse: {e}"))?;
    let summary = feed.summary();
    registry()
        .lock()
        .expect("registry lock")
        .insert(hash.clone(), Arc::new(feed));
    Ok(Response::LoadFeed { feed_hash: hash, summary })
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
    let hash = watchlist_hash(document)?;
    watchlists()
        .lock()
        .expect("watchlist lock")
        .entry(hash.clone())
        .or_insert_with(|| Arc::new(document.clone()));
    Ok(Response::LoadWatchlist { watchlist_hash: hash, version: document.version() })
}

/// The typed dispatch (native hosts call this directly; the ABI shell
/// wraps it in JSON). Failures are `{"error": ...}`, never panics.
pub fn dispatch(request: Request) -> Result<Response, String> {
    match request {
        Request::LoadFeed { bytes } => load_feed(&bytes),
        Request::Departures { feed_hash, stop_id, date, from_secs, limit } => {
            let parsed = parse_date(&date)?;
            with_feed(&feed_hash, |feed| {
                Ok(Response::Departures {
                    departures: feed.next_departures(&stop_id, parsed, from_secs, limit as usize),
                })
            })
        }
        Request::StationDepartures { feed_hash, stop_id, date, from_secs, limit } => {
            let parsed = parse_date(&date)?;
            with_feed(&feed_hash, |feed| {
                // Pool every walking-group member's board, merged by
                // time — the union the station's dispatcher reads off
                // the wall, each row tagged with its concrete bay.
                let mut pooled: Vec<transit_core::Departure> = Vec::new();
                for member in feed.walking_group(&stop_id) {
                    pooled.extend(feed.next_departures(&member, parsed, from_secs, usize::MAX));
                }
                pooled.sort_by_key(|d| (d.departure_time.unwrap_or(0), d.trip_id.clone()));
                pooled.truncate(limit as usize);
                Ok(Response::StationDepartures { departures: pooled })
            })
        }
        Request::Stops { feed_hash, limit } => with_feed(&feed_hash, |feed| {
            Ok(Response::Stops {
                stops: feed.stop_summaries().into_iter().take(limit as usize).collect(),
            })
        }),
        Request::StopsByIds { feed_hash, ids } => with_feed(&feed_hash, |feed| {
            let mut out = Vec::with_capacity(ids.len());
            for id in &ids {
                if let Some(stop) = feed.stops.get(id) {
                    out.push(StopSummary {
                        stop_id: stop.stop_id.clone(),
                        name: stop.stop_name.clone(),
                        parent_station: stop.parent_station.clone(),
                    });
                }
            }
            Ok(Response::StopsByIds { stops: out })
        }),
        Request::ActiveServices { feed_hash, date } => {
            let parsed = parse_date(&date)?;
            with_feed(&feed_hash, |feed| {
                Ok(Response::ActiveServices { services: feed.active_services(parsed) })
            })
        }
        Request::Blocks { feed_hash, date } => with_feed(&feed_hash, |feed| {
            let blocks = match date {
                Some(d) => feed.blocks_on(parse_date(&d)?),
                None => feed.blocks(),
            };
            Ok(Response::Blocks { blocks })
        }),
        Request::BlockTrips { feed_hash, block_id, date } => {
            let parsed = parse_date(&date)?;
            with_feed(&feed_hash, |feed| {
                Ok(Response::BlockTrips { trips: feed.block_trips(&block_id, parsed) })
            })
        }
        Request::FeedInfo { feed_hash } => with_feed(&feed_hash, |feed| {
            Ok(Response::FeedInfo { info: feed.feed_info.clone() })
        }),
        Request::LoadWatchlist { document } => load_watchlist(&document),
        Request::MigrateWatchlist { document } => {
            let (migrated, notes) = migrate_watchlist(&document)?;
            Ok(Response::MigratedWatchlist { document: migrated, notes })
        }
        Request::WatchlistBoard { watchlist_hash, feed_hash, date, from_secs, limit } => {
            let parsed = parse_date(&date)?;
            let watchlist = {
                let guard = watchlists().lock().expect("watchlist lock");
                guard
                    .get(&watchlist_hash)
                    .cloned()
                    .ok_or_else(|| format!("unknown watchlist {watchlist_hash}: load_watchlist first"))?
            };
            with_feed(&feed_hash, |feed| {
                let stops = watchlist
                    .stop_ids()
                    .into_iter()
                    .map(|stop_id| {
                        let label = match watchlist.as_ref() {
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
                        };
                        Ok(WatchBoardStop {
                            id: stop_id.to_string(),
                            label,
                            departures: feed.next_departures(stop_id, parsed, from_secs, limit as usize),
                        })
                    })
                    .collect::<Result<Vec<_>, String>>()?;
                Ok(Response::WatchlistBoard { stops })
            })
        }
    }
}

/// The JSON seam: `{"op": ...}` in, `{"ok": ...}` / `{"error": ...}`
/// out — byte-compatible with the monolith's dispatch_json.
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

/// The bulk-bytes seam: op 0 loads a feed zip.
pub fn dispatch_bytes(op: u32, bytes: &[u8]) -> String {
    let outcome = match op {
        OP_LOAD_FEED => match load_feed(bytes) {
            Ok(response) => serde_json::json!({ "ok": response }),
            Err(error) => serde_json::json!({ "error": error }),
        },
        other => serde_json::json!({ "error": format!("unknown bulk op {other}") }),
    };
    serde_json::to_string(&outcome).expect("dispatch outcomes serialize")
}

cap_abi::capability! {
    json: dispatch_json,
    bytes: dispatch_bytes,
}
