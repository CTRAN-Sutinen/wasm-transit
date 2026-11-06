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
//! date)`, patched with the edits in force on that date: the kernel's
//! held day (`cap_abi::kernel::day`), a pure function of the network,
//! the edits, and the date — no GTFS expansion, one materialization
//! per (network, edits, date) however many boards ask. The domain code
//! lives in the shared transit-dispatch library; this module is the
//! seam.
//!
//! The documents it reads are the kernel's, registered by its loads
//! and content-addressed like everywhere on the bus:
//! * **network** — the same identity cap-network's import produces
//!   and cap-analysis consumes (the module is the hash authority);
//! * **edits** — the operator edit document, wholesale-replaced by
//!   the host (a record the network cannot answer is reported by
//!   cap-divert's `reports`, and simply moves nothing here);
//! * **live** — THE live document (transit-live), minted by cap-rt
//!   from an rt snapshot in model vocabulary; the board reads its
//!   predictions and alerts and classifies under its window — no host
//!   joins rt;
//! * **watchlist** — the personal document (v1 notes + colors,
//!   v2 labels; `migrate_watchlist` maps note → label and reports the
//!   dropped color).

#![forbid(unsafe_code)]

use cap_abi::kernel::{LIVE, NETWORK, WATCHLIST};
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use transit_dispatch::{GroupedBoard, StopBoard};

pub use transit_dispatch::watchlist::{migrate as migrate_watchlist, MigrationNote, WatchStopV1, WatchStopV2, Watchlist};

/// One watched stop's board, with the version-appropriate label
/// (v2 labels; v1 notes — the migration maps note → label). The board
/// is THE stop board (`stop_board` with the same arguments).
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "cap_board/")]
pub struct WatchBoardStop {
    pub id: String,
    pub label: String,
    pub board: StopBoard,
}

/// One operation call. Tagged like every capability's seam.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "cap_board/")]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Request {
    /// THE board at one stop on a service date — when does service
    /// leave here: the materialized day with the in-force edits
    /// (`edits_hash`), realtime-first when `live_hash` names a live
    /// document (rows ordered by when the bus actually leaves; a late
    /// bus scheduled before `from_secs` that has not left is still
    /// on the board; each row carries its expected time, delay, and
    /// status). Each row names its route (short and long), the stop it
    /// leaves from, and where it is going as the sign says there. The
    /// departures the in-force edits took away from the stop are
    /// `skipped`, each with the diversion that skips it or the cancel
    /// that took its trip (a cancelled trip is never a row); the live
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
    /// whose last departure has left stays, with none, and says when
    /// it next runs here — the first of the next seven days whose
    /// patched day carries it, never the live document, D-42), each holding
    /// its next `per_group` departures (default 3) by the board's
    /// realtime-first rule and the visits the in-force edits took away
    /// (a detour's skips, a cancelled trip's visits),
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
    /// Migrate a version-1 watchlist to version 2. Pure and
    /// deterministic: label = note, colors dropped and reported.
    /// The caller keeps the old bytes — migrations never destroy.
    MigrateWatchlist { document: Watchlist },
    /// The board for a watchlist: each watched stop's board on a
    /// service date, labelled per its version — the same question the
    /// per-stop board answers, parameterized by the personal document,
    /// so the same answer: `stop_board` for each stop with the same
    /// edits and live document (realtime-first, skipped visits, a
    /// watched station pin pooling its bays).
    WatchlistBoard {
        watchlist_hash: String,
        network_hash: String,
        #[serde(default)]
        edits_hash: Option<String>,
        date: String,
        from_secs: u32,
        limit: u32,
        #[serde(default)]
        #[ts(optional)]
        live_hash: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "cap_board/")]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Response {
    StopBoard { board: StopBoard },
    GroupedBoard { board: GroupedBoard },
    MigratedWatchlist { document: Watchlist, notes: Vec<MigrationNote> },
    WatchlistBoard { stops: Vec<WatchBoardStop> },
}

/// How many departures each group holds when the caller names none —
/// the display's next three.
const PER_GROUP: u32 = 3;

/// The typed dispatch (native hosts call this directly). Failures
/// are `{"error": ...}`, never panics — a total seam.
pub fn dispatch(request: Request) -> Result<Response, String> {
    match request {
        Request::StopBoard { network_hash, edits_hash, stop_id, date, from_secs, limit, live_hash } => {
            let day = cap_abi::kernel::day(&network_hash, edits_hash.as_deref(), &date)?;
            let network = NETWORK.require(&network_hash)?;
            let live = live_hash.as_deref().map(|h| LIVE.require(h)).transpose()?;
            Ok(Response::StopBoard {
                board: transit_dispatch::stop_board(
                    &network,
                    &day.trips,
                    &stop_id,
                    from_secs,
                    limit as usize,
                    live.as_deref(),
                    day.edits.as_ref(),
                ),
            })
        }
        Request::GroupedBoard { network_hash, edits_hash, stop_id, date, from_secs, per_group, live_hash } => {
            let day = cap_abi::kernel::day(&network_hash, edits_hash.as_deref(), &date)?;
            let network = NETWORK.require(&network_hash)?;
            let live = live_hash.as_deref().map(|h| LIVE.require(h)).transpose()?;
            let mut board = transit_dispatch::grouped_board(
                &network,
                &day.trips,
                &stop_id,
                from_secs,
                per_group.unwrap_or(PER_GROUP) as usize,
                live.as_deref(),
                day.edits.as_ref(),
            );
            // A stopped group's next run (D-42): the later days are
            // materialized here, not held — the kernel keeps a few days
            // per derivation, and a week of them would push out the
            // day every other op reads. Asked only while a stopped
            // group has no answer; never the live document.
            if board.groups.iter().any(|g| g.departures.is_empty()) {
                let records = edits_hash.as_deref().map(|h| cap_abi::kernel::EDITS.require(h)).transpose()?;
                let records = records.as_deref().map(Vec::as_slice).unwrap_or(&[]);
                let date = chrono::NaiveDate::parse_from_str(&date, "%Y-%m-%d").map_err(|e| format!("invalid date {date:?}: {e}"))?;
                transit_dispatch::with_next_service(&network, &mut board, &stop_id, date, from_secs, |on| {
                    transit_dispatch::day(&network, on, records)
                        .map(Box::new)
                        .map_err(|e| format!("the network does not materialize on {on}: {e}"))
                })?;
            }
            Ok(Response::GroupedBoard { board })
        }
        Request::MigrateWatchlist { document } => {
            let (migrated, notes) = migrate_watchlist(&document)?;
            Ok(Response::MigratedWatchlist { document: migrated, notes })
        }
        Request::WatchlistBoard { watchlist_hash, network_hash, edits_hash, date, from_secs, limit, live_hash } => {
            let watchlist = WATCHLIST.require(&watchlist_hash)?;
            let day = cap_abi::kernel::day(&network_hash, edits_hash.as_deref(), &date)?;
            let network = NETWORK.require(&network_hash)?;
            let live = live_hash.as_deref().map(|h| LIVE.require(h)).transpose()?;
            let stops = watchlist
                .stop_ids()
                .into_iter()
                .map(|stop_id| WatchBoardStop {
                    id: stop_id.to_string(),
                    label: watchlist.label(stop_id),
                    board: transit_dispatch::stop_board(
                        &network,
                        &day.trips,
                        stop_id,
                        from_secs,
                        limit as usize,
                        live.as_deref(),
                        day.edits.as_ref(),
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
    cap_abi::answer(text, dispatch)
}

/// The board has no bulk op of its own (its documents register on the
/// kernel's loads); the seam answers honestly instead of silently
/// ignoring one.
pub fn dispatch_bytes(op: u32, _bytes: &[u8]) -> String {
    cap_abi::outcome::<()>(Err(format!("unknown bulk op {op}: the board has no bulk op of its own")))
}

cap_abi::capability! {
    id: "board",
    json: dispatch_json,
    bytes: dispatch_bytes,
    documents: [NETWORK, EDITS, LIVE, WATCHLIST],
}
