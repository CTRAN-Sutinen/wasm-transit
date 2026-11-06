//! cap-runcut — the runcut capability.
//!
//! One question: **what does this vehicle do today?** — at three
//! granularities: the day's blocks LIST (which vehicles are out —
//! the index facet), one block's authored CHAIN with the layover
//! ripple (the detail facet), and one trip instance's EFFECTIVE
//! stops on the date (the trip detail the map's click-through
//! reads). Blocks and layovers are
//! model state GTFS never had: the runcut is authored, and a
//! diverted trip ending later eats into the vehicle's next
//! layover, as does a trip the live document predicts ends late —
//! the connection-health ripple is IN the answer.
//!
//! Every answer is one MATERIALIZED DAY — `materialize(net,
//! date)`, patched with the edits in force on that date: the kernel's
//! held day (`cap_abi::kernel::day`); no GTFS expansion. The domain
//! code lives in the shared transit-dispatch library; this module is
//! the seam.
//!
//! The documents it reads are the kernel's, registered by its loads
//! and content-addressed like everywhere on the bus:
//! * **network** — the same identity cap-network's import produces
//!   and cap-analysis consumes (the module is the hash authority);
//! * **edits** — the operator edit document, wholesale-replaced by
//!   the host;
//! * **live** — cap-rt's live document (the snapshot in model
//!   vocabulary), which the chain's ripple reads;
//! * **work_rules** — the rules work is cut under (transit-runs): the
//!   recovery a wait requires and the route pairs a vehicle may
//!   interline, which the recovery view and the suggested blocking
//!   read. Blocking is where cutting the work begins (D-46): the
//!   suggestion answers which vehicles the trips no block carries
//!   need, as blocks the planner adopts (author `apply` over its
//!   edits) or edits.

#![forbid(unsafe_code)]

use cap_abi::kernel::{LIVE, NETWORK, WORK_RULES};
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use transit_dispatch::{BlockChainTrip, DayBlock};

/// One operation call. Tagged like every capability's seam.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "cap_runcut/")]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Request {
    /// The day's blocks: every authored block with at least one trip
    /// running on the date, with that day's running trip counts. A
    /// block whose every trip is off that day is a vehicle still in
    /// the garage — not offered.
    Blocks {
        network_hash: String,
        #[serde(default)]
        edits_hash: Option<String>,
        date: String,
    },
    /// The day's fleet in one answer (QAF-3): running blocks, the
    /// peak concurrent vehicles and the first time the peak is
    /// reached, the platform / revenue / layover hours, and each
    /// running block's outline (span, trips, longest internal gap) —
    /// the fold that took one block_trips call per block plus
    /// host-side concurrency glue.
    FleetSummary {
        network_hash: String,
        #[serde(default)]
        edits_hash: Option<String>,
        date: String,
    },
    /// One block's chain on a service date: the AUTHORED runcut (the
    /// model's blocks carry layovers GTFS never had), materialized
    /// for the day, with the in-force edits applied — a diverted trip
    /// ending later eats into the vehicle's next layover (the
    /// connection-health ripple, made visible), and with `live_hash`
    /// so does a trip the live document predicts ends late (its delay
    /// at its last stop, against the published time). Trips of the
    /// chain that do not run on the date stay in view, marked; unknown
    /// blocks answer empty, never error.
    BlockTrips {
        network_hash: String,
        #[serde(default)]
        edits_hash: Option<String>,
        block_id: String,
        date: String,
        /// Under this many seconds left, a layover is TIGHT (the
        /// dispatcher's policy; default 300).
        #[serde(default)]
        #[ts(optional)]
        tight_layover_secs: Option<u32>,
        #[serde(default)]
        #[ts(optional)]
        live_hash: Option<String>,
    },
    /// One trip instance's stops on a service date — the day's trip
    /// detail, EFFECTIVE (a diverted trip answers its patched
    /// sequence). The instance's identity is the model's: (trip,
    /// start_secs). Its line is `trip_path`. A trip that does not run on the date answers empty, never an
    /// error.
    TripStops {
        network_hash: String,
        #[serde(default)]
        edits_hash: Option<String>,
        trip: String,
        start_secs: u32,
        date: String,
    },
    /// Every wait between a block's adjacent running trips on a date:
    /// the recovery required there (D-9: the work rules' when one
    /// applies, else the smallest authored layover at that terminal)
    /// and the slack above it, folded per pattern into
    /// layover-to-runtime.
    Recovery {
        network_hash: String,
        #[serde(default)]
        edits_hash: Option<String>,
        date: String,
        #[serde(default)]
        #[ts(optional)]
        work_rules_hash: Option<String>,
    },
    /// The trips running on a date that no block carries, chained into
    /// suggested blocks (D-46): each follows the vehicle that leaves it
    /// the least slack where it starts (no deadheads), on a route the
    /// work rules let it interline with, after the recovery required.
    /// The answer carries each wait's slack, the peak with the
    /// suggestion adopted against the authored peak, and the edits
    /// that adopt it.
    SuggestBlocks {
        network_hash: String,
        #[serde(default)]
        edits_hash: Option<String>,
        date: String,
        #[serde(default)]
        #[ts(optional)]
        work_rules_hash: Option<String>,
    },
    /// One trip instance's LINE on a service date, over the effective
    /// day: its pattern's authored alignment, and for a trip a
    /// diversion in force reroutes, the alignment to the first kept
    /// stop, the diversion's traced detour, and the alignment from the
    /// last kept stop (a skip-only diversion keeps the alignment; with
    /// no alignment, the patched stops joined around the detour). A
    /// trip that does not run on the date answers an empty path.
    TripPath {
        network_hash: String,
        #[serde(default)]
        edits_hash: Option<String>,
        trip: String,
        start_secs: u32,
        date: String,
    },
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "cap_runcut/")]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Response {
    Blocks { blocks: Vec<DayBlock> },
    FleetSummary { summary: transit_dispatch::FleetSummary },
    /// The chain, carrying the block it answers for — a session
    /// asking for many blocks groups results by the answer, never by
    /// request order.
    BlockTrips { block_id: String, trips: Vec<BlockChainTrip> },
    TripStops { stops: Vec<transit_dispatch::TripStopView> },
    TripPath { path: Vec<transit_network::Coord> },
    Recovery { recovery: transit_runs::Recovery },
    SuggestBlocks { suggestion: transit_runs::Suggestion },
}

/// The typed dispatch (native hosts call this directly). Failures
/// are `{"error": ...}`, never panics — a total seam.
pub fn dispatch(request: Request) -> Result<Response, String> {
    match request {
        Request::Blocks { network_hash, edits_hash, date } => {
            let day = cap_abi::kernel::day(&network_hash, edits_hash.as_deref(), &date)?;
            let network = NETWORK.require(&network_hash)?;
            Ok(Response::Blocks { blocks: transit_dispatch::day_blocks(&network, &day.trips) })
        }
        Request::FleetSummary { network_hash, edits_hash, date } => {
            let day = cap_abi::kernel::day(&network_hash, edits_hash.as_deref(), &date)?;
            let network = NETWORK.require(&network_hash)?;
            Ok(Response::FleetSummary {
                summary: transit_dispatch::fleet_summary(&network, &day.trips),
            })
        }
        Request::BlockTrips { network_hash, edits_hash, block_id, date, tight_layover_secs, live_hash } => {
            let day = cap_abi::kernel::day(&network_hash, edits_hash.as_deref(), &date)?;
            let network = NETWORK.require(&network_hash)?;
            let live = live_hash.as_deref().map(|h| LIVE.require(h)).transpose()?;
            Ok(Response::BlockTrips {
                trips: transit_dispatch::block_chain(
                    &network,
                    &day.trips,
                    &block_id,
                    tight_layover_secs.unwrap_or(transit_dispatch::TIGHT_LAYOVER_SECS),
                    live.as_deref(),
                ),
                block_id,
            })
        }
        Request::TripStops { network_hash, edits_hash, trip, start_secs, date } => {
            let day = cap_abi::kernel::day(&network_hash, edits_hash.as_deref(), &date)?;
            let network = NETWORK.require(&network_hash)?;
            Ok(Response::TripStops {
                stops: transit_dispatch::trip_stops(&network, &day.trips, &trip, start_secs),
            })
        }
        Request::Recovery { network_hash, edits_hash, date, work_rules_hash } => {
            let day = cap_abi::kernel::day(&network_hash, edits_hash.as_deref(), &date)?;
            let network = NETWORK.require(&network_hash)?;
            let rules = work_rules_hash.as_deref().map(|h| WORK_RULES.require(h)).transpose()?;
            Ok(Response::Recovery { recovery: transit_runs::recovery(&network, &day.trips, rules.as_deref()) })
        }
        Request::SuggestBlocks { network_hash, edits_hash, date, work_rules_hash } => {
            let day = cap_abi::kernel::day(&network_hash, edits_hash.as_deref(), &date)?;
            let network = NETWORK.require(&network_hash)?;
            let rules = work_rules_hash.as_deref().map(|h| WORK_RULES.require(h)).transpose()?;
            Ok(Response::SuggestBlocks {
                suggestion: transit_runs::suggest_blocks(&network, &day.trips, rules.as_deref()),
            })
        }
        Request::TripPath { network_hash, edits_hash, trip, start_secs, date } => {
            let day = cap_abi::kernel::day(&network_hash, edits_hash.as_deref(), &date)?;
            let network = NETWORK.require(&network_hash)?;
            let diversions = day.edits.as_ref().map(|e| e.diversions.as_slice()).unwrap_or(&[]);
            Ok(Response::TripPath { path: transit_dispatch::trip_path(&network, &day.trips, diversions, &trip, start_secs) })
        }
    }
}

/// The JSON seam: `{"op": ...}` in, `{"ok": ...}` / `{"error": ...}`
/// out — byte-compatible with every other capability module.
pub fn dispatch_json(text: &str) -> String {
    cap_abi::answer(text, dispatch)
}

/// The runcut has no bulk op of its own (its documents register on
/// the kernel's loads); the seam answers honestly instead of silently
/// ignoring one.
pub fn dispatch_bytes(op: u32, _bytes: &[u8]) -> String {
    cap_abi::outcome::<()>(Err(format!("unknown bulk op {op}: the runcut has no bulk op of its own")))
}

cap_abi::capability! {
    id: "runcut",
    json: dispatch_json,
    bytes: dispatch_bytes,
    documents: [NETWORK, EDITS, LIVE, WORK_RULES],
}
