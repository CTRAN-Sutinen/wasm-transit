//! cap-router wraps transit-router (ops in `OPS.md`). It plans over the
//! network and the kernel's cached day, which is transit-dispatch's
//! composition: materialize, then patch with the edits in force. It has no
//! calendar expansion of its own.
//!
//! * The RAPTOR graph is a derived entry (`cap_abi::kernel::GRAPH`) for each
//!   (network, edits, live, date, walks), built on first use. Only the graphs
//!   for the newest live document stay.

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
        #[serde(default)]
        walks_hash: Option<String>,
        #[serde(default)]
        direct: Option<DirectWalk>,
    },
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
        #[serde(default)]
        walks_hash: Option<String>,
        #[serde(default)]
        direct: Option<DirectWalk>,
    },
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
        #[serde(default)]
        walks_hash: Option<String>,
        #[serde(default)]
        direct: Option<DirectWalk>,
    },
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
    PlanArriveBy {
        arrive_by_secs: u32,
        itinerary: Option<transit_router::Itinerary>,
    },
    PlanCompare {
        active: Option<transit_router::Itinerary>,
        baseline: Option<transit_router::Itinerary>,
        notice: Option<transit_router::PlanNotice>,
    },
    ItineraryPaths { paths: Vec<transit_dispatch::LegPath> },
}

/// A walk table belongs to one network, and its stops mean nothing in another.
fn walks_arc(walks_hash: &str, network_hash: &str) -> Result<Arc<WalkTable>, cap_abi::Refusal> {
    let table = kind::<transit_streets::walking::WalkTable>().require(walks_hash)?;
    if table.network_hash != network_hash {
        return Err(format!(
            "walk table {walks_hash} walks network {}, not {network_hash}",
            table.network_hash
        ).into());
    }
    Ok(table)
}

fn parse_date(date: &str) -> Result<NaiveDate, String> {
    NaiveDate::parse_from_str(date, "%Y-%m-%d")
        .map_err(|e| format!("invalid date {date:?}: {e}"))
}

fn graph(
    network_hash: &str,
    edits_hash: Option<&str>,
    live_hash: Option<&str>,
    walks_hash: Option<&str>,
    date_str: &str,
) -> Result<Arc<transit_router::RouterNetwork>, cap_abi::Refusal> {
    // Parse the date first, so a bad date is a caller error and not a
    // missing-document error.
    let date = parse_date(date_str)?;
    let network = kind::<transit_network::Network>().require(network_hash)?;
    let live = live_hash.map(|h| kind::<transit_live::LiveDay>().require(h)).transpose()?;
    let walks = walks_hash.map(|h| walks_arc(h, network_hash)).transpose()?;
    // A live document lasts one rt moment, so only the newest one's graphs
    // stay. Otherwise every snapshot would leave a graph behind. The key leads
    // with the live hash ("" for none), and this rule reads it back.
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
        let live = live.map(|l| l.on(date_str, date_str));
        let mut built = transit_router::RouterNetwork::build(&network, &day.trips, date, live.as_ref());
        if let Some(walks) = &walks {
            built = built.with_walks(walks);
        }
        Ok::<_, cap_abi::Refusal>(built)
    })
}

pub fn dispatch(request: Request) -> Result<Response, cap_abi::Refusal> {
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
            parse_date(&date)?;
            let legs = max_legs.max(1) as usize;
            let net = graph(&network_hash, edits_hash.as_deref(), live_hash.as_deref(), walks_hash.as_deref(), &date)?;
            let direct_secs = direct.map(|d| d.walk_secs);
            let active = transit_router::earliest_arrival_between(&net, &from, &to, direct_secs, depart_secs, legs)
                .map_err(|e| e.to_string())?;
            let network = kind::<transit_network::Network>().require(&network_hash)?;
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

pub fn dispatch_json(text: &str) -> String {
    cap_abi::answer(text, dispatch)
}

pub fn dispatch_bytes(op: u32, _bytes: &[u8]) -> String {
    cap_abi::outcome::<(), String>(Err(format!("unknown bulk op {op}: the router has no bulk op of its own")))
}

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

fn day(network_hash: &str, edits_hash: Option<&str>, date: &str) -> Result<std::sync::Arc<transit_dispatch::Day>, cap_abi::Refusal> {
    let on = transit_dispatch::day_date(date)?;
    let network = kind::<transit_network::Network>().require(network_hash)?;
    let edits = edits_hash.map(|hash| kind::<transit_sync::EditDocument>().require(hash)).transpose()?;
    let records = edits.as_deref().map(|d| d.records.as_slice()).unwrap_or(&[]);
    let inputs: Vec<&str> = std::iter::once(network_hash).chain(edits_hash).collect();
    let key = transit_dispatch::day_key(network_hash, edits_hash, date);
    cap_abi::kernel::derive(&cap_abi::kernel::DAY, &key, &inputs, || transit_dispatch::day_of(&network, records, on))
}
