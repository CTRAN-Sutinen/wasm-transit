//! cap-board wraps transit-dispatch (`OPS.md` has the ops). It reads the
//! watchlist in v1 or v2 (cap-migrate carries the chain).
//!
//! * The grouped board's look-ahead for a stopped group materializes the
//!   later days itself and does not cache them. A week of days would push
//!   the cached days out.

#![forbid(unsafe_code)]

use cap_abi::kernel::kind;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use std::sync::Arc;

use transit_dispatch::{BoardAt, Day, GroupedBoard, Overnight, ServiceDay, StopBoard};

pub use transit_rider::watchlist::{WatchStop, Watchlist};

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "cap_board/")]
pub struct WatchBoardStop {
    pub id: String,
    pub label: String,
    pub board: StopBoard,
}

/// `from_secs` is the board's moment on the previous date's clock, past
/// 86400 after midnight. `length_secs` is that date's length from anchor to
/// anchor: 86400, or 82800 or 90000 across a clock change. GTFS times past
/// 24:00 belong to the date the trip started on.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS)]
#[ts(export, export_to = "cap_board/")]
pub struct Previous {
    pub from_secs: u32,
    pub length_secs: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "cap_board/")]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Request {
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
        #[serde(default)]
        #[ts(optional)]
        previous: Option<Previous>,
    },
    /// The next-run look-ahead never uses the live document.
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
        #[serde(default)]
        #[ts(optional)]
        previous: Option<Previous>,
    },
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
        #[ts(optional)]
        edits_hash: Option<String>,
        date: String,
        from_secs: u32,
        limit: u32,
        #[serde(default)]
        #[ts(optional)]
        live_hash: Option<String>,
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

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "cap_board/")]
pub struct EntityReach {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub published: Option<transit_rt::overview::InformedView>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub not_published: Option<String>,
    pub names: transit_rt::EntityNames,
    pub reach: transit_dispatch::Reach,
}

const PER_GROUP: u32 = 3;

pub fn dispatch(request: Request) -> Result<Response, cap_abi::Refusal> {
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

struct Days {
    today: Arc<Day>,
    previous: Option<(String, Arc<Day>, Previous)>,
}

impl Days {
    fn read(network_hash: &str, edits_hash: Option<&str>, date: &str, previous: Option<Previous>) -> Result<Days, cap_abi::Refusal> {
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

pub fn dispatch_json(text: &str) -> String {
    cap_abi::answer(text, dispatch)
}

pub fn dispatch_bytes(op: u32, _bytes: &[u8]) -> String {
    cap_abi::outcome::<(), String>(Err(format!("unknown bulk op {op}: the board has no bulk op of its own")))
}

#[cfg(feature = "examples")]
pub mod examples;

cap_abi::capability! {
    id: "board",
    json: dispatch_json,
    bytes: dispatch_bytes,
    documents: [transit_network::Network, transit_sync::EditDocument, transit_live::LiveDay, transit_rider::watchlist::Watchlist],
    request: Request,
    consumes: &[("network", "network")],
    speaks: &[
        ("network", &[transit_network::SCHEMA_VERSION]),
        ("operator_edits", &[transit_sync::SCHEMA_VERSION]),
        ("watchlist", &[transit_rider::watchlist::SCHEMA_VERSION]),
        ("live", &[1]),
    ],
}

fn day(network_hash: &str, edits_hash: Option<&str>, date: &str) -> Result<std::sync::Arc<transit_dispatch::Day>, cap_abi::Refusal> {
    let on = transit_dispatch::day_date(date)?;
    let network = kind::<transit_network::Network>().require(network_hash)?;
    let edits = edits_hash.map(|hash| kind::<transit_sync::EditDocument>().require(hash)).transpose()?;
    let records = edits.as_deref().map(|d| d.records.as_slice()).unwrap_or(&[]);
    let inputs: Vec<&str> = std::iter::once(network_hash).chain(edits_hash).collect();
    let key = transit_dispatch::day_key(network_hash, edits_hash, date);
    cap_abi::kernel::derive(&cap_abi::kernel::DAY, &key, &inputs, || transit_dispatch::day_of(&network, records, on))
}
