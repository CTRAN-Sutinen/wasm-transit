//! cap-runcut wraps transit-dispatch and transit-runs (`OPS.md` has the ops).
//! It reads the same cached day as cap-board.
//!
//! A live prediction of a late end wins over the edits' ripple (D-18).


#![forbid(unsafe_code)]

use cap_abi::kernel::kind;
use serde::{Deserialize, Serialize};
#[cfg(feature = "bindings")]
use ts_rs::TS;
use transit_dispatch::{BlockChainTrip, DayBlock};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS))]
#[cfg_attr(feature = "bindings", ts(export, export_to = "cap_runcut/"))]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Request {
    Blocks {
        network_hash: String,
        #[serde(default)]
        edits_hash: Option<String>,
        date: String,
    },
    FleetSummary {
        network_hash: String,
        #[serde(default)]
        edits_hash: Option<String>,
        date: String,
    },
    BlockTrips {
        network_hash: String,
        #[serde(default)]
        edits_hash: Option<String>,
        block_id: String,
        date: String,
        #[serde(default)]
        #[cfg_attr(feature = "bindings", ts(optional))]
        tight_layover_secs: Option<u32>,
        #[serde(default)]
        #[cfg_attr(feature = "bindings", ts(optional))]
        live_hash: Option<String>,
    },
    TripStops {
        network_hash: String,
        #[serde(default)]
        edits_hash: Option<String>,
        trip: String,
        start_secs: u32,
        date: String,
    },
    Recovery {
        network_hash: String,
        #[serde(default)]
        edits_hash: Option<String>,
        date: String,
        #[serde(default)]
        #[cfg_attr(feature = "bindings", ts(optional))]
        work_rules_hash: Option<String>,
    },
    SuggestBlocks {
        network_hash: String,
        #[serde(default)]
        edits_hash: Option<String>,
        date: String,
        #[serde(default)]
        #[cfg_attr(feature = "bindings", ts(optional))]
        work_rules_hash: Option<String>,
    },
    TripPath {
        network_hash: String,
        #[serde(default)]
        edits_hash: Option<String>,
        trip: String,
        start_secs: u32,
        date: String,
    },
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "bindings", derive(TS))]
#[cfg_attr(feature = "bindings", ts(export, export_to = "cap_runcut/"))]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Response {
    Blocks { blocks: Vec<DayBlock> },
    FleetSummary { summary: transit_dispatch::FleetSummary },
    BlockTrips { block_id: String, trips: Vec<BlockChainTrip> },
    TripStops { stops: Vec<transit_dispatch::TripStopView> },
    TripPath { path: Vec<transit_network::Coord> },
    Recovery { recovery: transit_runs::Recovery },
    SuggestBlocks { suggestion: transit_runs::Suggestion },
}

pub fn dispatch(request: Request) -> Result<Response, cap_abi::Refusal> {
    match request {
        Request::Blocks { network_hash, edits_hash, date } => {
            let day = day(&network_hash, edits_hash.as_deref(), &date)?;
            let network = kind::<transit_network::Network>().require(&network_hash)?;
            Ok(Response::Blocks { blocks: transit_dispatch::day_blocks(&network, &day.trips) })
        }
        Request::FleetSummary { network_hash, edits_hash, date } => {
            let day = day(&network_hash, edits_hash.as_deref(), &date)?;
            let network = kind::<transit_network::Network>().require(&network_hash)?;
            Ok(Response::FleetSummary {
                summary: transit_dispatch::fleet_summary(&network, &day.trips),
            })
        }
        Request::BlockTrips { network_hash, edits_hash, block_id, date, tight_layover_secs, live_hash } => {
            let day = day(&network_hash, edits_hash.as_deref(), &date)?;
            let network = kind::<transit_network::Network>().require(&network_hash)?;
            let live = live_hash.as_deref().map(|h| kind::<transit_live::LiveDay>().require(h)).transpose()?.map(|l| l.on(&date, &date));
            Ok(Response::BlockTrips {
                trips: transit_dispatch::block_chain(
                    &network,
                    &day.trips,
                    &block_id,
                    tight_layover_secs.unwrap_or(transit_dispatch::TIGHT_LAYOVER_SECS),
                    live.as_ref(),
                ),
                block_id,
            })
        }
        Request::TripStops { network_hash, edits_hash, trip, start_secs, date } => {
            let day = day(&network_hash, edits_hash.as_deref(), &date)?;
            let network = kind::<transit_network::Network>().require(&network_hash)?;
            Ok(Response::TripStops {
                stops: transit_dispatch::trip_stops(&network, &day.trips, &trip, start_secs),
            })
        }
        Request::Recovery { network_hash, edits_hash, date, work_rules_hash } => {
            let day = day(&network_hash, edits_hash.as_deref(), &date)?;
            let network = kind::<transit_network::Network>().require(&network_hash)?;
            let rules = work_rules_hash.as_deref().map(|h| kind::<transit_runs::WorkRules>().require(h)).transpose()?;
            Ok(Response::Recovery { recovery: transit_runs::recovery(&network, &day.trips, rules.as_deref()) })
        }
        Request::SuggestBlocks { network_hash, edits_hash, date, work_rules_hash } => {
            let day = day(&network_hash, edits_hash.as_deref(), &date)?;
            let network = kind::<transit_network::Network>().require(&network_hash)?;
            let rules = work_rules_hash.as_deref().map(|h| kind::<transit_runs::WorkRules>().require(h)).transpose()?;
            Ok(Response::SuggestBlocks {
                suggestion: transit_runs::suggest_blocks(&network, &day.trips, rules.as_deref()),
            })
        }
        Request::TripPath { network_hash, edits_hash, trip, start_secs, date } => {
            let day = day(&network_hash, edits_hash.as_deref(), &date)?;
            let network = kind::<transit_network::Network>().require(&network_hash)?;
            let diversions = day.edits.as_ref().map(|e| e.diversions.as_slice()).unwrap_or(&[]);
            Ok(Response::TripPath { path: transit_dispatch::trip_path(&network, &day.trips, diversions, &trip, start_secs) })
        }
    }
}

pub fn dispatch_json(text: &str) -> String {
    cap_abi::answer(text, dispatch)
}

pub fn dispatch_bytes(op: u32, _bytes: &[u8]) -> String {
    cap_abi::outcome::<(), String>(Err(format!("unknown bulk op {op}: the runcut has no bulk op of its own")))
}

#[cfg(feature = "examples")]
pub mod examples;

cap_abi::capability! {
    id: "runcut",
    json: dispatch_json,
    bytes: dispatch_bytes,
    documents: [transit_network::Network, transit_sync::EditDocument, transit_live::LiveDay, transit_runs::WorkRules],
    request: Request,
    consumes: &[("network", "network")],
    speaks: &[
        ("network", &[transit_network::SCHEMA_VERSION]),
        ("operator_edits", &[transit_sync::SCHEMA_VERSION]),
        ("live", &[1]),
        ("work_rules", &[1]),
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
