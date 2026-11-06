//! cap-rt — the realtime capability module.
//!
//! The third capability extracted from the MONOLITH (after analysis
//! and the network codec): decode a GTFS-rt protobuf payload into
//! the overview view (delays, vehicles) a dispatcher or rider reads.
//! The domain code lives in the shared `transit-rt` library — the
//! monolith keeps no copy; every host (web, runner, a future server
//! leg) decodes through the same bytes.
//!
//! It also owns the JOIN the dispatcher actually asks for: over the
//! NETWORK document (the kernel's, the same identity every network
//! consumer holds) and, for an imported network, its `Boundary` (the
//! published stop_sequence numbering rt updates name),
//! `trip_progress` / `late_trips` / `live` answer over the network's
//! own trips — an rt trip id IS a model trip id.
//!
//! The rt payload is EPHEMERAL: snapshots are polled every few
//! seconds, never persisted; decode registers each snapshot in the
//! kernel under its content hash (idempotent — the same bytes are the
//! same snapshot) so later ops can address it, and `live` registers
//! the live document it mints, so in an assembly the board and the
//! router read it by hash without it crossing to the host and back.

#![forbid(unsafe_code)]

use std::sync::Arc;

use cap_abi::kernel::{BOUNDARY, LIVE, NETWORK, RT};
use serde::{Deserialize, Serialize};
use transit_codec::Boundary;
use transit_live::{LiveDay, LiveSummary, OnTimeWindow};
use transit_rt::{LateTrip, RtOverview, TripProgress};
use ts_rs::TS;

/// The bulk-bytes op code that decodes an rt payload (the runner's
/// `--load` default); the network arrives on the kernel's
/// load_network op.
pub const OP_DECODE: u32 = 0;

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "cap_rt/")]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Request {
    /// Decode a GTFS-rt protobuf payload (the bytes an agency serves,
    /// typically polled every 15–30 seconds) into the overview, and
    /// register the snapshot under its content hash (idempotent —
    /// the same bytes are the same snapshot) so the joins can
    /// address it. Bulk-bytes op 0.
    Decode { bytes: Vec<u8> },
    /// One trip's joined board: every scheduled stop of the instance
    /// the snapshot's update names, with the live delay and the
    /// PREDICTED time. Null when the snapshot has no update for the
    /// trip or the update names no instance of it.
    TripProgress {
        network_hash: String,
        #[serde(default)]
        #[ts(optional)]
        boundary_hash: Option<String>,
        rt_hash: String,
        trip_id: String,
    },
    /// The ranked "what's late right now" view: every trip whose
    /// worst stop delay reaches `min_delay_secs`, worst first, each
    /// carrying the vehicle's position when one names it (or another
    /// trip of its block) — and the DELAYED-BUT-DARK flag when none
    /// does. `vp_hash` names the VehiclePositions snapshot agencies
    /// serve as a SEPARATE feed (omitted for a combined feed).
    LateTrips {
        network_hash: String,
        rt_hash: String,
        #[serde(default)]
        vp_hash: Option<String>,
        min_delay_secs: i32,
    },
    /// THE LIVE DOCUMENT: one rt moment resolved into model
    /// vocabulary — delays per trip instance (trip, start) and stop
    /// (propagated downstream), vehicles on their trip instances,
    /// every status classified under `window` (recorded in the
    /// document), plus the summary a status panel reads (vehicles by
    /// status, the late list worst first — late trips when there are
    /// no positions), and the SERVICE ALERTS in force at the snapshot's
    /// moment, their informed entities resolved into routes, stops, and
    /// trip instances. `rt_hash` names trip updates, `vp_hash` vehicle
    /// positions, `alerts_hash` service alerts (alerts are gathered from
    /// every snapshot given); a combined feed passes only `rt_hash`. At
    /// least one.
    Live {
        network_hash: String,
        #[serde(default)]
        #[ts(optional)]
        boundary_hash: Option<String>,
        #[serde(default)]
        #[ts(optional)]
        rt_hash: Option<String>,
        #[serde(default)]
        #[ts(optional)]
        vp_hash: Option<String>,
        #[serde(default)]
        #[ts(optional)]
        alerts_hash: Option<String>,
        #[serde(default)]
        #[ts(optional)]
        window: Option<OnTimeWindow>,
        /// Late-list length (default 15).
        #[serde(default)]
        #[ts(optional)]
        late_limit: Option<u32>,
        /// Whether the answer carries the document itself (default
        /// true). It is registered where it was minted either way: a
        /// host whose board and router are linked beside rt reads it by
        /// `live_hash` and need not receive it (QAF-49).
        #[serde(default)]
        #[ts(optional)]
        with_document: Option<bool>,
    },
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "cap_rt/")]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Response {
    Decode { overview: RtOverview, rt_hash: String },
    TripProgress { progress: Option<TripProgress> },
    LateTrips { trips: Vec<LateTrip> },
    /// `live`: the document, unless the request declined it.
    Live { live_hash: String, live: Option<LiveDay>, summary: LiveSummary },
}

/// The boundary a join pairs rt sequences by: the registered one, or
/// 1..n numbering.
fn boundary_arc(hash: Option<&str>) -> Result<Arc<Boundary>, String> {
    match hash {
        Some(hash) => BOUNDARY.require(hash),
        None => Ok(Arc::new(Boundary::default())),
    }
}

fn snapshot_arc(rt_hash: &str) -> Result<Arc<RtOverview>, String> {
    RT.require(rt_hash)
}

fn decode(bytes: &[u8]) -> Result<Response, String> {
    let (rt_hash, overview) = RT.register_bytes(bytes)?;
    Ok(Response::Decode { overview: (*overview).clone(), rt_hash })
}

/// The typed dispatch (native hosts call this directly). Failures
/// are `{"error": ...}`, never panics — a total seam: garbage bytes
/// are an error outcome, not a trap.
pub fn dispatch(request: Request) -> Result<Response, String> {
    match request {
        Request::Decode { bytes } => decode(&bytes),
        Request::TripProgress { network_hash, boundary_hash, rt_hash, trip_id } => {
            let network = NETWORK.require(&network_hash)?;
            let boundary = boundary_arc(boundary_hash.as_deref())?;
            let snapshot = snapshot_arc(&rt_hash)?;
            Ok(Response::TripProgress { progress: transit_rt::trip_progress(&network, &boundary, &snapshot, &trip_id) })
        }
        Request::LateTrips { network_hash, rt_hash, vp_hash, min_delay_secs } => {
            let network = NETWORK.require(&network_hash)?;
            let snapshot = snapshot_arc(&rt_hash)?;
            let positions = vp_hash.as_deref().map(snapshot_arc).transpose()?;
            Ok(Response::LateTrips {
                trips: transit_rt::late_trips(&network, &snapshot, positions.as_deref(), min_delay_secs),
            })
        }
        Request::Live { network_hash, boundary_hash, rt_hash, vp_hash, alerts_hash, window, late_limit, with_document } => {
            if rt_hash.is_none() && vp_hash.is_none() && alerts_hash.is_none() {
                return Err(
                    "live needs a snapshot: rt_hash (trip updates), vp_hash (vehicle positions), and/or alerts_hash (service alerts)".into(),
                );
            }
            let network = NETWORK.require(&network_hash)?;
            let boundary = boundary_arc(boundary_hash.as_deref())?;
            let delays = rt_hash.as_deref().map(snapshot_arc).transpose()?;
            let positions = vp_hash.as_deref().map(snapshot_arc).transpose()?;
            let alerts = alerts_hash.as_deref().map(snapshot_arc).transpose()?;
            let live = transit_rt::live_day(
                &network,
                &boundary,
                delays.as_deref(),
                positions.as_deref(),
                alerts.as_deref(),
                window.unwrap_or_default(),
            );
            let summary = transit_live::summarize(&live, late_limit.unwrap_or(15) as usize);
            // The live document registers where it was minted: in an
            // assembly, the board and the router read it by hash.
            let (live_hash, live) = LIVE.register(live)?;
            let live = with_document.unwrap_or(true).then(|| (*live).clone());
            Ok(Response::Live { live_hash, live, summary })
        }
    }
}

/// The JSON seam: `{"op": ...}` in, `{"ok": ...}` / `{"error": ...}`
/// out — byte-compatible with every other capability module.
pub fn dispatch_json(text: &str) -> String {
    cap_abi::answer(text, dispatch)
}

/// The bulk-bytes seam: op 0 decodes a payload (the overview and its
/// hash; the kernel's `load_rt` registers one answering the hash only).
pub fn dispatch_bytes(op: u32, bytes: &[u8]) -> String {
    match op {
        OP_DECODE => cap_abi::outcome(decode(bytes)),
        other => cap_abi::outcome::<()>(Err(format!("unknown bulk op {other}"))),
    }
}

cap_abi::capability! {
    id: "rt",
    json: dispatch_json,
    bytes: dispatch_bytes,
    documents: [NETWORK, BOUNDARY, RT, LIVE],
}
