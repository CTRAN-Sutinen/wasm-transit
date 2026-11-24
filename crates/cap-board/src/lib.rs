//! cap-board — the board capability.
//!
//! One question: **when does service leave this stop?** — asked at
//! three granularities: one stop (a station pin or bay with nothing
//! of its own answers its POOLED walking group — the model's own
//! concept), realtime-first when a LIVE document is supplied, the
//! same stop grouped by route and destination for its display, and a
//! personal saved set (the WATCHLIST document: a personal multi-stop
//! board is the same question asked over a personal document).
//!
//! Every answer is one MATERIALIZED DAY — `materialize(net,
//! date)`, patched with the edits in force on that date: the kernel's
//! held day (this crate's `day`: `transit_dispatch::day_of`, held under `cap_abi::kernel::DAY`), a pure function of the network,
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
//! * **watchlist** — the personal document, in the version this
//!   build reads (its migration is cap-migrate's).

#![forbid(unsafe_code)]

use cap_abi::kernel::kind;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use std::sync::Arc;

use transit_dispatch::{BoardAt, Day, GroupedBoard, Overnight, ServiceDay, StopBoard};

pub use transit_rider::watchlist::{WatchStop, Watchlist};

/// One watched stop's board, with its label. The board is THE stop
/// board (`stop_board` with the same arguments).
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "cap_board/")]
pub struct WatchBoardStop {
    pub id: String,
    pub label: String,
    pub board: StopBoard,
}

/// The previous service date at the board's moment, as the host's
/// clock reads it: the moment on that date's clock (seconds from its
/// anchor — past 86 400 after midnight) and its length anchor to
/// anchor (86 400; 82 800 or 90 000 across a clock change). A board
/// given it also answers the previous date's after-midnight trips
/// still to leave (GTFS times past 24:00 are the date the trip started
/// on), each row naming its date; before a fall-back day's anchor it
/// is also how long the clamped hour really is.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS)]
#[ts(export, export_to = "cap_board/")]
pub struct Previous {
    pub from_secs: u32,
    pub length_secs: u32,
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
        /// The previous date at the same moment: its after-midnight
        /// trips join the board (each row names its date).
        #[serde(default)]
        #[ts(optional)]
        previous: Option<Previous>,
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
        /// The previous date at the same moment: its after-midnight
        /// trips join the board (each row names its date).
        #[serde(default)]
        #[ts(optional)]
        previous: Option<Previous>,
    },
    /// The board for a watchlist: each watched stop's board on a
    /// service date, labelled per its version — the same question the
    /// per-stop board answers, parameterized by the personal document,
    /// so the same answer: `stop_board` for each stop with the same
    /// edits and live document (realtime-first, skipped visits, a
    /// watched station pin pooling its bays).
    /// What an alert reaches on a day, entity by entity, read as a
    /// rider's app reads the feed: each entity as published, what it
    /// names, and the departures it puts the alert on; and the whole
    /// alert's.
    AlertReach {
        network_hash: String,
        #[serde(default)]
        #[ts(optional)]
        edits_hash: Option<String>,
        date: String,
        alert: transit_alerts::ServiceAlert,
    },
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
        /// The previous date at the same moment: its after-midnight
        /// trips join the board (each row names its date).
        #[serde(default)]
        #[ts(optional)]
        previous: Option<Previous>,
    },
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "cap_board/")]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Response {
    StopBoard { board: StopBoard },
    GroupedBoard { board: GroupedBoard },
    WatchlistBoard { stops: Vec<WatchBoardStop> },
    AlertReach { entities: Vec<EntityReach>, reach: transit_dispatch::Reach },
}

/// One informed entity of an alert, read as the feed reads it.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "cap_board/")]
pub struct EntityReach {
    /// The `informed_entity` the feed carries for it; absent when the
    /// encoder leaves it out (`not_published` says why).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub published: Option<transit_rt::overview::InformedView>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub not_published: Option<String>,
    /// What it names, in the network's words.
    pub names: transit_rt::EntityNames,
    /// The departures it alone puts the alert on.
    pub reach: transit_dispatch::Reach,
}

/// How many departures each group holds when the caller names none —
/// the display's next three.
const PER_GROUP: u32 = 3;

/// The typed dispatch (native hosts call this directly). Failures
/// are `{"error": ...}`, never panics — a total seam.
pub fn dispatch(request: Request) -> Result<Response, String> {
    match request {
        Request::StopBoard { network_hash, edits_hash, stop_id, date, from_secs, limit, live_hash, previous } => {
            let days = Days::read(&network_hash, edits_hash.as_deref(), &date, previous)?;
            let network = kind::<transit_network::Network>().require(&network_hash)?;
            let live = live_hash.as_deref().map(|h| kind::<transit_live::LiveDay>().require(h)).transpose()?;
            Ok(Response::StopBoard {
                board: transit_dispatch::stop_board_at(&network, &days.at(&date, from_secs), &stop_id, limit as usize, live.as_deref()),
            })
        }
        Request::AlertReach { network_hash, edits_hash, date, alert } => {
            let days = Days::read(&network_hash, edits_hash.as_deref(), &date, None)?;
            let network = kind::<transit_network::Network>().require(&network_hash)?;
            let mut all = Vec::new();
            let entities = alert
                .informed
                .iter()
                .map(|e| {
                    let reading = transit_rt::entity_reading(&network, e);
                    all.extend(reading.scopes.iter().cloned());
                    let (published, not_published) = match reading.published {
                        Ok(v) => (Some(v), None),
                        Err(why) => (None, Some(why)),
                    };
                    EntityReach { published, not_published, names: reading.names, reach: transit_dispatch::alert_reach_day(&network, &days.today, &reading.scopes, &date) }
                })
                .collect();
            Ok(Response::AlertReach { entities, reach: transit_dispatch::alert_reach_day(&network, &days.today, &all, &date) })
        }
        Request::GroupedBoard { network_hash, edits_hash, stop_id, date, from_secs, per_group, live_hash, previous } => {
            let days = Days::read(&network_hash, edits_hash.as_deref(), &date, previous)?;
            let network = kind::<transit_network::Network>().require(&network_hash)?;
            let live = live_hash.as_deref().map(|h| kind::<transit_live::LiveDay>().require(h)).transpose()?;
            let mut board = transit_dispatch::grouped_board_at(
                &network,
                &days.at(&date, from_secs),
                &stop_id,
                per_group.unwrap_or(PER_GROUP) as usize,
                live.as_deref(),
            );
            // A stopped group's next run (D-42): the later days are
            // materialized here, not held — the kernel keeps a few days
            // per derivation, and a week of them would push out the
            // day every other op reads. Asked only while a stopped
            // group has no answer; never the live document.
            if board.groups.iter().any(|g| g.departures.is_empty()) {
                let records = edits_hash.as_deref().map(|h| cap_abi::kernel::kind::<transit_sync::EditDocument>().require(h)).transpose()?;
                let records = records.as_deref().map(|d| d.records.as_slice()).unwrap_or(&[]);
                let date = chrono::NaiveDate::parse_from_str(&date, "%Y-%m-%d").map_err(|e| format!("invalid date {date:?}: {e}"))?;
                transit_dispatch::with_next_service(&network, &mut board, &stop_id, date, from_secs, |on| {
                    transit_dispatch::day(&network, on, records)
                        .map(Box::new)
                        .map_err(|e| format!("the network does not materialize on {on}: {e}"))
                })?;
            }
            Ok(Response::GroupedBoard { board })
        }
        Request::WatchlistBoard { watchlist_hash, network_hash, edits_hash, date, from_secs, limit, live_hash, previous } => {
            let watchlist = kind::<transit_rider::watchlist::Watchlist>().require(&watchlist_hash)?;
            let days = Days::read(&network_hash, edits_hash.as_deref(), &date, previous)?;
            let at = days.at(&date, from_secs);
            let network = kind::<transit_network::Network>().require(&network_hash)?;
            let live = live_hash.as_deref().map(|h| kind::<transit_live::LiveDay>().require(h)).transpose()?;
            let stops = watchlist
                .stop_ids()
                .into_iter()
                .map(|stop_id| WatchBoardStop {
                    label: watchlist.label(&stop_id),
                    board: transit_dispatch::stop_board_at(&network, &at, &stop_id, limit as usize, live.as_deref()),
                    id: stop_id,
                })
                .collect();
            Ok(Response::WatchlistBoard { stops })
        }
    }
}

/// The days a board at a moment reads: its date's, and the previous
/// date's when the host gives the moment on that clock — both the
/// kernel's held days.
struct Days {
    today: Arc<Day>,
    previous: Option<(String, Arc<Day>, Previous)>,
}

impl Days {
    fn read(network_hash: &str, edits_hash: Option<&str>, date: &str, previous: Option<Previous>) -> Result<Days, String> {
        let today = day(network_hash, edits_hash, date)?;
        let previous = match previous {
            Some(p) => {
                let on = chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d")
                    .ok()
                    .and_then(|d| d.pred_opt())
                    .ok_or_else(|| format!("invalid date {date:?}"))?
                    .format("%Y-%m-%d")
                    .to_string();
                let day = day(network_hash, edits_hash, &on)?;
                Some((on, day, p))
            }
            None => None,
        };
        Ok(Days { today, previous })
    }

    fn at<'a>(&'a self, date: &'a str, from_secs: u32) -> BoardAt<'a> {
        BoardAt {
            today: ServiceDay { date, trips: &self.today.trips, edits: self.today.edits.as_ref() },
            from_secs,
            previous: self.previous.as_ref().map(|(on, day, p)| Overnight {
                day: ServiceDay { date: on, trips: &day.trips, edits: day.edits.as_ref() },
                from_secs: p.from_secs,
                length_secs: p.length_secs,
            }),
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

/// The worked examples, which the generator runs (no module build turns
/// the feature on).
#[cfg(feature = "examples")]
pub mod examples;

cap_abi::capability! {
    id: "board",
    json: dispatch_json,
    bytes: dispatch_bytes,
    documents: [transit_network::Network, transit_sync::EditDocument, transit_live::LiveDay, transit_rider::watchlist::Watchlist],
    request: Request,
    // The network capability makes the network document. The host owns
    // the operator edit document and replaces it whole (the sync
    // contract). The watchlist is the personal document, read in the
    // current version (cap-migrate migrates it).
    consumes: &[("network", "network")],
    speaks: &[
        ("network", &[transit_network::SCHEMA_VERSION]),
        ("operator_edits", &[transit_sync::SCHEMA_VERSION]),
        ("watchlist", &[transit_rider::watchlist::SCHEMA_VERSION]),
        ("live", &[1]),
    ],
}

/// The day for (network, edits, date), held in the kernel's derived
/// registry under `transit_dispatch::day_key`: the effective trips and,
/// when edits are in force, what they took away. The date is read first,
/// so a date that does not parse is the caller's error.
fn day(network_hash: &str, edits_hash: Option<&str>, date: &str) -> Result<std::sync::Arc<transit_dispatch::Day>, String> {
    let on = transit_dispatch::day_date(date)?;
    let network = kind::<transit_network::Network>().require(network_hash)?;
    let edits = edits_hash.map(|hash| kind::<transit_sync::EditDocument>().require(hash)).transpose()?;
    let records = edits.as_deref().map(|d| d.records.as_slice()).unwrap_or(&[]);
    let inputs: Vec<&str> = std::iter::once(network_hash).chain(edits_hash).collect();
    let key = transit_dispatch::day_key(network_hash, edits_hash, date);
    cap_abi::kernel::derive(&cap_abi::kernel::DAY, &key, &inputs, || transit_dispatch::day_of(&network, records, on))
}
