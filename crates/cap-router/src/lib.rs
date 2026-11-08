//! cap-router — the trip-planning capability module.
//!
//! Planning composes over the NETWORK document and the EFFECTIVE
//! DAY, like every model-side capability: the network and the edit
//! document are the kernel's (registered by its loads, under their
//! content hashes), and a plan answers by hash pair + date. The day
//! itself is the kernel's held day — transit-dispatch's composition,
//! materialize then patch with the edits in force — never a private
//! calendar expansion (Law 2). A LIVE document (cap-rt's) makes the
//! plan ride each trip instance's predicted times where the document
//! states them. The RAPTOR graph is a derived entry
//! (`cap_abi::kernel::GRAPH`) per (network, edits, live, date, walks),
//! built on first use; a rider whose bundle stops at "know when the
//! bus comes" never downloads this module's planning code.
//!
//! The question is one — how do I get from here to there — at its
//! facets: leaving at a time (`plan`), arriving by a time
//! (`plan_arrive_by`: the latest departure; with no deadline, the
//! last bus home), and the diversion delta (`plan_compare`: the same
//! query over the patched day and the unpatched one — one
//! materialization apart, no second document — with the rider's
//! notice whenever the diversion changes the trip).

#![forbid(unsafe_code)]

use std::sync::Arc;

use cap_abi::kernel::{GRAPH, LIVE, NETWORK, WALKS};
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use transit_streets::walking::{DirectWalk, WalkTable};

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "cap_router/")]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Request {
    /// Earliest-arrival plan from `from` to `to`, leaving at
    /// `depart_secs` on `date`. Each end is a stop id (a bare string)
    /// or a PLACE off the network — `{id, name, walks}`, its walks to
    /// the stops near it (cap-streets `access` answers them; a place's
    /// id is never a stop's): the trip walks from the place to one of
    /// its stops and from one of its stops to the place, or straight
    /// there when the `direct` walk arrives as early. Over the
    /// materialized day, with
    /// the in-force edits applied when `edits_hash` names a
    /// registered edit document, and at the predicted times when
    /// `live_hash` names a registered live document, its transfers
    /// along the streets when `walks_hash` names a registered walk
    /// table. `None` in the response means unreachable within
    /// `max_legs`.
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
        /// The walk table the transfers walk (`load_walks`); with
        /// none, transfers are straight lines (D-32).
        #[serde(default)]
        walks_hash: Option<String>,
        /// The walk straight from `from` to `to`, when one joins them
        /// — cap-streets `access`'s `direct`, as it is.
        #[serde(default)]
        direct: Option<DirectWalk>,
    },
    /// Arrive-by: the LATEST departure from `from` that reaches `to`
    /// by `arrive_by_secs` on `date` within `max_legs` — with no
    /// deadline, the latest that reaches `to` at all on the service
    /// day (the last bus home; the deadline is then the day's last
    /// arrival). The same day as `plan` (edits, live).
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
        /// The walk table the transfers walk (`load_walks`); with
        /// none, transfers are straight lines (D-32).
        #[serde(default)]
        walks_hash: Option<String>,
        /// The walk straight from `from` to `to`, when one joins them
        /// — cap-streets `access`'s `direct`, as it is.
        #[serde(default)]
        direct: Option<DirectWalk>,
    },
    /// The diversion delta: the same plan over the patched day
    /// (active) and the unpatched materialization (baseline), both
    /// at the live document's predictions when `live_hash` is given.
    /// The baseline is `None` when no edit is in force on the date
    /// (or no edits hash is given) — there is no second world to
    /// compare against. The notice is what the rider is told
    /// whenever the diversion changes the trip: no trip, a later
    /// arrival, or a ride on a diverted trip (and that no trip exists
    /// within `max_legs`). One op whether or not edits exist.
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
        /// The walk table the transfers walk (`load_walks`); with
        /// none, transfers are straight lines (D-32).
        #[serde(default)]
        walks_hash: Option<String>,
        /// The walk straight from `from` to `to`, when one joins them
        /// — cap-streets `access`'s `direct`, as it is.
        #[serde(default)]
        direct: Option<DirectWalk>,
    },
    /// Where an itinerary runs, leg by leg, over the same effective
    /// day it was planned on: a transit leg's RIDDEN segment (the
    /// trip's alignment sliced between its board and alight stops
    /// while it follows its pattern; its patched stops when diverted),
    /// a walk through the stops it passes — and, given the plan's
    /// ends and direct walk, along the streets from or to a place
    /// (its walk's path; between two places, the direct walk's). The
    /// map draws the answer.
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

/// A registered walk table, refused when it was derived for another
/// network: its stops would be another network's.
fn walks_arc(walks_hash: &str, network_hash: &str) -> Result<Arc<WalkTable>, String> {
    let table = WALKS.require(walks_hash)?;
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

/// The RAPTOR graph for (network, edits, live, date, walks), built
/// on first use and held as a derived entry. No edits hash is the
/// unpatched materialization — the compare facet's baseline; no walks
/// hash, straight-line transfers.
fn graph(
    network_hash: &str,
    edits_hash: Option<&str>,
    live_hash: Option<&str>,
    walks_hash: Option<&str>,
    date_str: &str,
) -> Result<Arc<transit_router::RouterNetwork>, String> {
    // The request's own fields validate before any registry lookup —
    // a garbage date is a caller error, not a missing-document error.
    let date = parse_date(date_str)?;
    let network = NETWORK.require(network_hash)?;
    let live = live_hash.map(|h| LIVE.require(h)).transpose()?;
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
        let day = cap_abi::kernel::day(network_hash, edits_hash, date_str)?;
        // The date's live instances only (an instance stating no date
        // is the planned date's — LiveDay::on).
        let live = live.map(|l| l.on(date_str, date_str));
        let mut built = transit_router::RouterNetwork::build(&network, &day.trips, date, live.as_ref());
        if let Some(walks) = &walks {
            built = built.with_walks(walks);
        }
        Ok(built)
    })
}

/// The typed dispatch (native hosts call this directly). Failures
/// are `{\"error\": ...}`, never panics.
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
            let day = cap_abi::kernel::day(&network_hash, edits_hash.as_deref(), &date)?;
            let network = NETWORK.require(&network_hash)?;
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
            let network = NETWORK.require(&network_hash)?;
            // The baseline is the unpatched materialization — one
            // materialization apart, never a second document — read
            // at the same predictions; the diverted trips are the
            // instances the patch moved.
            let day = cap_abi::kernel::day(&network_hash, edits_hash.as_deref(), &date)?;
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

/// The JSON seam: `{\"op\": ...}` in, `{\"ok\": ...}` /
/// `{\"error\": ...}` out — byte-compatible with every other
/// capability module.
pub fn dispatch_json(text: &str) -> String {
    cap_abi::answer(text, dispatch)
}

/// The router has no bulk op of its own (its documents register on
/// the kernel's loads); the seam answers honestly instead of silently
/// ignoring one.
pub fn dispatch_bytes(op: u32, _bytes: &[u8]) -> String {
    cap_abi::outcome::<()>(Err(format!("unknown bulk op {op}: the router has no bulk op of its own")))
}

cap_abi::capability! {
    id: "router",
    json: dispatch_json,
    bytes: dispatch_bytes,
    documents: [NETWORK, EDITS, LIVE, WALKS],
}
