//! cap-router: how do I get from here to there? (OPS.md has the ops.)
//!
//! The seam over transit-router. It plans over the network and the kernel's
//! held day (transit-dispatch's composition: materialize, then patch with the
//! edits in force). It never uses a private calendar expansion. A rider bundle
//! that only shows departures never links this module.
//!
//! * A live document (cap-rt's) makes a plan ride predicted times.
//! * A walk table (cap-streets' `walks`) makes transfers walk the streets. A
//!   table derived for another network is refused. Without one, transfers are
//!   straight lines.
//! * The RAPTOR graph is a derived entry (`cap_abi::kernel::GRAPH`) for each
//!   (network, edits, live, date, walks). It is built on first use. Only the
//!   graphs for the newest live document stay.
//! * `plan_compare` reads the unpatched day at the same predictions, one
//!   materialization apart. It gives the rider's notice. The browser plans
//!   only through it. The host shows the notice's text.

#![forbid(unsafe_code)]

use std::sync::Arc;

use cap_abi::kernel::{kind, GRAPH};
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use transit_streets::walking::{DirectWalk, WalkTable};

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "cap_router/")]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Request {
    /// Earliest-arrival plan from `from` to `to`, leaving at `depart_secs` on
    /// `date`. Each end is a stop id (a bare string), or a PLACE off the
    /// network. A place is `{id, name, walks}`, with its walks to the stops
    /// near it (cap-streets `access` answers them). A place's id is never a
    /// stop's. The trip walks from the place to one of its stops, and from one
    /// of its stops to the place. It goes straight there when the `direct` walk
    /// arrives as early. The plan runs over the materialized day. The in-force
    /// edits apply when `edits_hash` names a registered edit document. It rides
    /// predicted times when `live_hash` names a registered live document. Its
    /// transfers walk the streets when `walks_hash` names a registered walk
    /// table. `None` in the response means unreachable within `max_legs`.
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
        /// The walk table the transfers walk (`load_walks`). With none,
        /// transfers are straight lines (D-32).
        #[serde(default)]
        walks_hash: Option<String>,
        /// The walk straight from `from` to `to`, when one joins them. It is
        /// cap-streets `access`'s `direct`, as it is.
        #[serde(default)]
        direct: Option<DirectWalk>,
    },
    /// Arrive-by: the latest departure from `from` that reaches `to` by
    /// `arrive_by_secs` on `date` within `max_legs`. With no deadline, it is
    /// the latest that reaches `to` at all on the service day (the last bus
    /// home). The deadline is then the day's last arrival. It uses the same
    /// day as `plan` (edits, live).
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
        /// The walk table the transfers walk (`load_walks`). With none,
        /// transfers are straight lines (D-32).
        #[serde(default)]
        walks_hash: Option<String>,
        /// The walk straight from `from` to `to`, when one joins them. It is
        /// cap-streets `access`'s `direct`, as it is.
        #[serde(default)]
        direct: Option<DirectWalk>,
    },
    /// The diversion delta: the same plan over the patched day (active) and the
    /// unpatched materialization (baseline). Both use the live document's
    /// predictions when `live_hash` is given. The baseline is `None` when no
    /// edit is in force on the date, or when no edits hash is given. There is
    /// no second world to compare against then. The notice is what the rider is
    /// told whenever the diversion changes the trip: no trip, a later arrival,
    /// or a ride on a diverted trip. It also applies when no trip exists within
    /// `max_legs`. One op serves whether or not edits exist.
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
        /// The walk table the transfers walk (`load_walks`). With none,
        /// transfers are straight lines (D-32).
        #[serde(default)]
        walks_hash: Option<String>,
        /// The walk straight from `from` to `to`, when one joins them. It is
        /// cap-streets `access`'s `direct`, as it is.
        #[serde(default)]
        direct: Option<DirectWalk>,
    },
    /// Where an itinerary runs, leg by leg, over the same effective day it was
    /// planned on. A transit leg gives its RIDDEN segment. That is the trip's
    /// alignment, sliced between its board and alight stops while it follows
    /// its pattern. When diverted, it gives the patched stops. A walk leg gives
    /// the stops it passes. Given the plan's ends and direct walk, it also runs
    /// along the streets from or to a place. That is the walk's path. Between
    /// two places, it is the direct walk's path. The map draws the answer.
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

/// A registered walk table. It is refused when it was derived for another
/// network, since its stops would be another network's.
fn walks_arc(walks_hash: &str, network_hash: &str) -> Result<Arc<WalkTable>, String> {
    let table = kind::<transit_streets::walking::WalkTable>().require(walks_hash)?;
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

/// The RAPTOR graph for (network, edits, live, date, walks). It is built on
/// first use and held as a derived entry. No edits hash means the unpatched
/// materialization, the compare facet's baseline. No walks hash means
/// straight-line transfers.
fn graph(
    network_hash: &str,
    edits_hash: Option<&str>,
    live_hash: Option<&str>,
    walks_hash: Option<&str>,
    date_str: &str,
) -> Result<Arc<transit_router::RouterNetwork>, String> {
    // The request's own fields validate before any registry lookup. A garbage
    // date is a caller error, not a missing-document error.
    let date = parse_date(date_str)?;
    let network = kind::<transit_network::Network>().require(network_hash)?;
    let live = live_hash.map(|h| kind::<transit_live::LiveDay>().require(h)).transpose()?;
    let walks = walks_hash.map(|h| walks_arc(h, network_hash)).transpose()?;
    // Live documents are ephemeral (one per rt moment): only the
    // newest live document's graphs stay held, or every snapshot
    // would leave a graph behind. The key leads with the live hash
    // ("" for none), which this rule reads back.
    if let Some(current) = live_hash {
        cap_abi::kernel::retain_derived(&GRAPH, |key, _| {
            key.split('|').next().is_none_or(|live| live.is_empty() || live == current)
        });
    }
    let key = format!(
        "{}|{network_hash}|{}|{date}|{}",
        live_hash.unwrap_or(""),
        edits_hash.unwrap_or(""),
        walks_hash.unwrap_or("")
    );
    let inputs: Vec<&str> = [Some(network_hash), edits_hash, live_hash, walks_hash].into_iter().flatten().collect();
    cap_abi::kernel::derive(&GRAPH, &key, &inputs, || {
        let day = day(network_hash, edits_hash, date_str)?;
        // The date's live instances only. An instance that states no date
        // belongs to the planned date (LiveDay::on).
        let live = live.map(|l| l.on(date_str, date_str));
        let mut built = transit_router::RouterNetwork::build(&network, &day.trips, date, live.as_ref());
        if let Some(walks) = &walks {
            built = built.with_walks(walks);
        }
        Ok(built)
    })
}

/// The typed dispatch. Native hosts call this directly. Failures are
/// `{\"error\": ...}`, and never panic.
pub fn dispatch(request: Request) -> Result<Response, String> {
    match request {
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
            let day = day(&network_hash, edits_hash.as_deref(), &date)?;
            let network = kind::<transit_network::Network>().require(&network_hash)?;
            let diversions = day.edits.as_ref().map(|e| e.diversions.as_slice()).unwrap_or(&[]);
            let paths = itinerary
                .legs
                .iter()
                .map(|leg| match leg {
                    transit_router::Leg::Transit { trip, start_secs, board_stop_id, alight_stop_id, .. } => {
                        transit_dispatch::LegPath {
                            kind: transit_dispatch::LegKind::Transit,
                            path: transit_dispatch::trip_segment(
                                &network, &day.trips, diversions, trip, *start_secs, board_stop_id, alight_stop_id,
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
            parse_date(&date)?;
            let legs = max_legs.max(1) as usize;
            let net = graph(&network_hash, edits_hash.as_deref(), live_hash.as_deref(), walks_hash.as_deref(), &date)?;
            let direct_secs = direct.map(|d| d.walk_secs);
            let active = transit_router::earliest_arrival_between(&net, &from, &to, direct_secs, depart_secs, legs)
                .map_err(|e| e.to_string())?;
            let network = kind::<transit_network::Network>().require(&network_hash)?;
            // The baseline is the unpatched materialization: one
            // materialization apart, never a second document. It is read at
            // the same predictions. The diverted trips are the instances the
            // patch moved.
            let day = day(&network_hash, edits_hash.as_deref(), &date)?;
            let (baseline, diverted) = match &day.edits {
                Some(edits) => {
                    let base = graph(&network_hash, None, live_hash.as_deref(), walks_hash.as_deref(), &date)?;
                    let baseline =
                        transit_router::earliest_arrival_between(&base, &from, &to, direct_secs, depart_secs, legs)
                            .map_err(|e| e.to_string())?;
                    (baseline, transit_router::diverted_instances(&edits.base, &day.trips))
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

/// The JSON seam: `{\"op\": ...}` in, and `{\"ok\": ...}` or
/// `{\"error\": ...}` out. Its bytes match every other capability module.
pub fn dispatch_json(text: &str) -> String {
    cap_abi::answer(text, dispatch)
}

/// The router has no bulk op of its own. Its documents register on the
/// kernel's loads. The seam answers with an error, and does not silently
/// ignore a bulk op.
pub fn dispatch_bytes(op: u32, _bytes: &[u8]) -> String {
    cap_abi::outcome::<()>(Err(format!("unknown bulk op {op}: the router has no bulk op of its own")))
}

/// The worked examples. The generator runs them. No module build turns
/// the feature on.
#[cfg(feature = "examples")]
pub mod examples;

cap_abi::capability! {
    id: "router",
    json: dispatch_json,
    bytes: dispatch_bytes,
    documents: [transit_network::Network, transit_sync::EditDocument, transit_live::LiveDay, transit_streets::walking::WalkTable],
    request: Request,
    consumes: &[("network", "network")],
    speaks: &[("network", &[transit_network::SCHEMA_VERSION]), ("live", &[1]), ("walks", &[1])],
}

/// The day for (network, edits, date). The kernel holds it in its derived
/// registry under `transit_dispatch::day_key`. It has the effective trips and,
/// when edits are in force, what they took away. The date is read first, so
/// a date that does not parse is the caller's error.
fn day(network_hash: &str, edits_hash: Option<&str>, date: &str) -> Result<std::sync::Arc<transit_dispatch::Day>, String> {
    let on = transit_dispatch::day_date(date)?;
    let network = kind::<transit_network::Network>().require(network_hash)?;
    let edits = edits_hash.map(|hash| kind::<transit_sync::EditDocument>().require(hash)).transpose()?;
    let records = edits.as_deref().map(|d| d.records.as_slice()).unwrap_or(&[]);
    let inputs: Vec<&str> = std::iter::once(network_hash).chain(edits_hash).collect();
    let key = transit_dispatch::day_key(network_hash, edits_hash, date);
    cap_abi::kernel::derive(&cap_abi::kernel::DAY, &key, &inputs, || transit_dispatch::day_of(&network, records, on))
}
