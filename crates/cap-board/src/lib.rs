//! cap-board: when does service leave this stop? (OPS.md has the ops.)
//!
//! The seam over transit-dispatch. It answers at one stop. A stop with
//! nothing of its own answers its pooled walking group. Answers come grouped
//! by route and destination for a stop's display, or over a rider's
//! watchlist (v1 or v2; cap-migrate carries the chain). It reads the
//! kernel's network, edits, live document (cap-rt's, by hash), and watchlist.
//!
//! * Every answer is computed here, never in the host: pooling, order,
//!   status, names, the sign's destination, skipped visits, and alerts.
//! * The board ops read the kernel's held day (`transit_dispatch::day_of`
//!   under `cap_abi::kernel::DAY`). One materialization serves each
//!   (network, edits, date), however many boards ask. There is no GTFS
//!   expansion. The arch gate forbids a GTFS layer.
//! * The grouped board's look-ahead for a stopped group materializes the
//!   later days itself. It does not hold them, because a week of days would
//!   push the held days out.
//! * An edit record the network cannot answer moves nothing here
//!   (cap-divert's `reports` names it).
//! * Each row carries its trip instance's address (trip, start, and its
//!   date when asked at a moment), so a click-through needs no join.

#![forbid(unsafe_code)]

use cap_abi::kernel::kind;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use std::sync::Arc;

use transit_dispatch::{BoardAt, Day, GroupedBoard, Overnight, ServiceDay, StopBoard};

pub use transit_rider::watchlist::{WatchStop, Watchlist};

/// One watched stop's board, with its label. The board is the stop board
/// (`stop_board` with the same arguments).
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "cap_board/")]
pub struct WatchBoardStop {
    pub id: String,
    pub label: String,
    pub board: StopBoard,
}

/// The previous service date at the board's moment, as the host's clock
/// reads it. `from_secs` is the moment on that date's clock: seconds from
/// its anchor, past 86 400 after midnight. `length_secs` is the length from
/// anchor to anchor (86 400, or 82 800 or 90 000 across a clock change). A
/// board given it also answers the previous date's after-midnight trips
/// still to leave. GTFS times past 24:00 belong to the date the trip started
/// on. Each row names its date. Before a fall-back day's anchor, it also
/// gives how long the clamped hour really is.
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
    /// The board at one stop on a service date. It answers when service
    /// leaves here. It gives the materialized day with the in-force edits
    /// (`edits_hash`). When `live_hash` names a live document, the board is
    /// realtime-first: rows are ordered by when the bus actually leaves. A
    /// late bus scheduled before `from_secs` that has not left is still on
    /// the board. Each row carries its expected time, delay, and status. Each
    /// row names its route (short and long), the stop it leaves from, and
    /// where it is going as the sign says. The departures that the in-force
    /// edits took away from the stop are `skipped`. Each one gives the
    /// diversion that skips it, or the cancel that took its trip. A cancelled
    /// trip is never a row. The live document's service alerts about the
    /// board ride along. The board carries them, and each row names the ones
    /// about it. A stop with nothing of its own (nothing leaving, nothing
    /// skipped) that belongs to a walking group, such as a station pin,
    /// answers the group's board. It is marked `pooled`, and each row names
    /// its bay.
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
        /// The previous date at the same moment. Its after-midnight trips
        /// join the board. Each row names its date.
        #[serde(default)]
        #[ts(optional)]
        previous: Option<Previous>,
    },
    /// The board grouped for a stop's display. It gives one group per route
    /// and destination that serves the stop on the date. A group whose last
    /// departure has left stays, with none. It says when it next runs here:
    /// the first of the next seven days whose patched day carries it. This
    /// never uses the live document (D-42). Each group holds its next
    /// `per_group` departures (default 3). These follow the board's
    /// realtime-first rule. They include the visits that the in-force edits
    /// took away (a detour's skips, a cancelled trip's visits). Groups sort
    /// by the route's display order (`route_sort_order`, then id), then by
    /// destination. Every row says how long until it leaves (`wait`). A stop
    /// with nothing of its own all day, that belongs to a walking group (a
    /// station pin), pools it. Each row names its bay.
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
        /// The previous date at the same moment. Its after-midnight trips
        /// join the board. Each row names its date.
        #[serde(default)]
        #[ts(optional)]
        previous: Option<Previous>,
    },
    /// What an alert reaches on a day, entity by entity. It reads the feed
    /// as a rider's app does: each entity as published, what it names, and
    /// the departures it puts the alert on. It also gives the whole alert's
    /// reach.
    AlertReach {
        network_hash: String,
        #[serde(default)]
        #[ts(optional)]
        edits_hash: Option<String>,
        date: String,
        alert: transit_alerts::ServiceAlert,
    },
    /// The board for a watchlist: each watched stop's board on a service date,
    /// labelled per its version. It asks the per-stop board's question, with
    /// the personal document as a parameter. So the answer is `stop_board`
    /// for each stop, with the same edits and live document. That gives
    /// realtime-first rows, skipped visits, and a watched station pin pooling
    /// its bays.
    WatchlistBoard {
        watchlist_hash: String,
        network_hash: String,
        #[serde(default)]
        #[ts(optional)]
        edits_hash: Option<String>,
        date: String,
        from_secs: u32,
        limit: u32,
        #[serde(default)]
        #[ts(optional)]
        live_hash: Option<String>,
        /// The previous date at the same moment. Its after-midnight trips
        /// join the board. Each row names its date.
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

/// How many departures each group holds when the caller names none: the
/// display's next three.
const PER_GROUP: u32 = 3;

/// The typed dispatch. Native hosts call this directly. Failures are
/// `{"error": ...}`, and never panic. The seam is total.
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
            // A stopped group's next run (D-42). The later days are
            // materialized here, not held. The kernel keeps a few days per
            // derivation, and a week of them would push out the day that
            // every other op reads. This runs only while a stopped group
            // has no answer. It never uses the live document.
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
/// date's when the host gives the moment on that clock. Both are the
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

/// The JSON seam: `{"op": ...}` in, and `{"ok": ...}` or
/// `{"error": ...}` out. Its bytes match every other capability module.
pub fn dispatch_json(text: &str) -> String {
    cap_abi::answer(text, dispatch)
}

/// The board has no bulk op of its own. Its documents register on the
/// kernel's loads. The seam answers with an error, and does not silently
/// ignore a bulk op.
pub fn dispatch_bytes(op: u32, _bytes: &[u8]) -> String {
    cap_abi::outcome::<()>(Err(format!("unknown bulk op {op}: the board has no bulk op of its own")))
}

/// The worked examples. The generator runs them. No module build turns
/// the feature on.
#[cfg(feature = "examples")]
pub mod examples;

cap_abi::capability! {
    id: "board",
    json: dispatch_json,
    bytes: dispatch_bytes,
    documents: [transit_network::Network, transit_sync::EditDocument, transit_live::LiveDay, transit_rider::watchlist::Watchlist],
    request: Request,
    // The network capability makes the network document. The host owns the
    // operator edit document and replaces it whole (the sync contract). The
    // watchlist is the personal document. It is read in the current version,
    // and cap-migrate migrates it.
    consumes: &[("network", "network")],
    speaks: &[
        ("network", &[transit_network::SCHEMA_VERSION]),
        ("operator_edits", &[transit_sync::SCHEMA_VERSION]),
        ("watchlist", &[transit_rider::watchlist::SCHEMA_VERSION]),
        ("live", &[1]),
    ],
}

/// The day for (network, edits, date). The kernel holds it in its derived
/// registry under `transit_dispatch::day_key`. It has the effective trips
/// and, when edits are in force, what they took away. The date is read
/// first, so a date that does not parse is the caller's error.
fn day(network_hash: &str, edits_hash: Option<&str>, date: &str) -> Result<std::sync::Arc<transit_dispatch::Day>, String> {
    let on = transit_dispatch::day_date(date)?;
    let network = kind::<transit_network::Network>().require(network_hash)?;
    let edits = edits_hash.map(|hash| kind::<transit_sync::EditDocument>().require(hash)).transpose()?;
    let records = edits.as_deref().map(|d| d.records.as_slice()).unwrap_or(&[]);
    let inputs: Vec<&str> = std::iter::once(network_hash).chain(edits_hash).collect();
    let key = transit_dispatch::day_key(network_hash, edits_hash, date);
    cap_abi::kernel::derive(&cap_abi::kernel::DAY, &key, &inputs, || transit_dispatch::day_of(&network, records, on))
}
