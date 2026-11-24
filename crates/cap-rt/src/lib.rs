//! cap-rt: GTFS-realtime decoding and its join to the network (OPS.md has
//! the ops).
//!
//! The seam over transit-rt. An rt trip id is a model trip id. An imported
//! network's `Boundary` gives the published stop_sequence numbering. It is
//! optional.
//!
//! * Snapshots are ephemeral. They are polled every few seconds and never
//!   persisted. Each registers in the kernel under its content hash, so the
//!   same bytes are the same snapshot.
//! * `live` registers the live document where it mints it. In an assembly,
//!   the board and the router read it by hash, and it never crosses to the
//!   host and back. No capability joins rt again.
//! * GTFS-rt is also the agency's alerts' serialization. `encode_alerts`
//!   registers the feed it writes as a snapshot, so `live` reads the agency's
//!   alerts as a rider's app does.

#![forbid(unsafe_code)]

use std::sync::Arc;

use cap_abi::kernel::kind;
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
    /// Decode a GTFS-rt protobuf payload into the overview. The bytes an agency
    /// serves are typically polled every 15–30 seconds. The snapshot registers
    /// under its content hash, so the joins can address it. Registering is
    /// idempotent: the same bytes are the same snapshot. Bulk-bytes op 0.
    Decode { bytes: Vec<u8> },
    /// One trip's joined board. It gives every scheduled stop of the instance
    /// that the snapshot's update names, with the live delay and the PREDICTED
    /// time. Null when the snapshot has no update for the trip, or the update
    /// names no instance of it.
    TripProgress {
        network_hash: String,
        #[serde(default)]
        #[ts(optional)]
        boundary_hash: Option<String>,
        rt_hash: String,
        trip_id: String,
    },
    /// The ranked "what's late right now" view. It gives every trip whose worst
    /// stop delay reaches `min_delay_secs`, worst first. Each trip carries the
    /// vehicle's position when one names it, or another trip of its block. The
    /// DELAYED-BUT-DARK flag is set when none does. `vp_hash` names the
    /// VehiclePositions snapshot that agencies serve as a separate feed. It is
    /// omitted for a combined feed.
    LateTrips {
        network_hash: String,
        rt_hash: String,
        #[serde(default)]
        vp_hash: Option<String>,
        min_delay_secs: i32,
    },
    /// The live document: one rt moment, resolved into model vocabulary. It
    /// gives delays per trip instance (trip, start) and stop, propagated
    /// downstream. It gives vehicles on their trip instances. Every status is
    /// classified under `window`, and the document records that window. The
    /// answer also gives the summary that a status panel reads: vehicles by
    /// status, and the late list, worst first. With no positions, the late list
    /// holds late trips. It gives the SERVICE ALERTS in force at the snapshot's
    /// moment. Their informed entities are resolved into routes, stops, and
    /// trip instances. `rt_hash` names trip updates. `vp_hash` names vehicle
    /// positions. `alerts_hash` names service alerts. Alerts are gathered from
    /// every snapshot given. A combined feed passes only `rt_hash`. At least
    /// one snapshot is required.
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
        /// Whether the answer carries the document itself (default true). It is
        /// registered where it was minted either way. A host whose board and
        /// router are linked beside rt reads it by `live_hash`, and need not
        /// receive it (QAF-49).
        #[serde(default)]
        #[ts(optional)]
        with_document: Option<bool>,
    },
    /// The service alerts document, written as a GTFS-rt Alerts feed
    /// (`FULL_DATASET`). It holds the published alerts not yet ended at
    /// `timestamp` (Unix seconds, from the host's clock). Each alert gives one
    /// entity, under the alert's id. The feed registers as an rt snapshot under
    /// the content hash of its bytes, so `live` reads it at once.
    EncodeAlerts {
        network_hash: String,
        alerts_hash: String,
        #[ts(type = "number")]
        timestamp: u64,
    },
    /// A GTFS-rt Alerts feed (`bytes`) read into the service alerts document
    /// over the network. This is the inverse of `encode_alerts`. The document
    /// registers under its canonical hash. Each entity, and each alert that
    /// cannot be read as written, is named.
    ReadAlerts { network_hash: String, bytes: Vec<u8> },
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
    /// The feed's bytes (base64), under their content hash. That hash is also
    /// the rt snapshot's hash. The answer names each informed entity it could
    /// not write.
    EncodeAlerts { rt_hash: String, feed_b64: String, skipped: Vec<transit_rt::SkippedEntity> },
    ReadAlerts {
        alerts_hash: String,
        alerts: transit_alerts::ServiceAlerts,
        skipped: Vec<transit_rt::SkippedEntity>,
        dropped: Vec<transit_rt::DroppedAlert>,
    },
}

/// The boundary a join pairs rt sequences by: the registered one, or
/// 1..n numbering.
fn boundary_arc(hash: Option<&str>) -> Result<Arc<Boundary>, String> {
    match hash {
        Some(hash) => kind::<transit_codec::Boundary>().require(hash),
        None => Ok(Arc::new(Boundary::default())),
    }
}

fn snapshot_arc(rt_hash: &str) -> Result<Arc<RtOverview>, String> {
    kind::<transit_rt::RtOverview>().require(rt_hash)
}

fn decode(bytes: &[u8]) -> Result<Response, String> {
    let (rt_hash, overview) = kind::<transit_rt::RtOverview>().register_bytes(bytes)?;
    Ok(Response::Decode { overview: (*overview).clone(), rt_hash })
}

/// The typed dispatch. Native hosts call this directly. Failures are
/// `{"error": ...}`, and never panic. The seam is total. Garbage bytes are an
/// error outcome, not a trap.
pub fn dispatch(request: Request) -> Result<Response, String> {
    match request {
        Request::Decode { bytes } => decode(&bytes),
        Request::TripProgress { network_hash, boundary_hash, rt_hash, trip_id } => {
            let network = kind::<transit_network::Network>().require(&network_hash)?;
            let boundary = boundary_arc(boundary_hash.as_deref())?;
            let snapshot = snapshot_arc(&rt_hash)?;
            Ok(Response::TripProgress { progress: transit_rt::trip_progress(&network, &boundary, &snapshot, &trip_id) })
        }
        Request::LateTrips { network_hash, rt_hash, vp_hash, min_delay_secs } => {
            let network = kind::<transit_network::Network>().require(&network_hash)?;
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
            let network = kind::<transit_network::Network>().require(&network_hash)?;
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
            let (live_hash, live) = kind::<transit_live::LiveDay>().register(live)?;
            let live = with_document.unwrap_or(true).then(|| (*live).clone());
            Ok(Response::Live { live_hash, live, summary })
        }
        Request::EncodeAlerts { network_hash, alerts_hash, timestamp } => {
            let network = kind::<transit_network::Network>().require(&network_hash)?;
            let alerts = kind::<transit_alerts::ServiceAlerts>().require(&alerts_hash)?;
            let encoded = transit_rt::encode_alerts(&network, &alerts, timestamp);
            let (rt_hash, _) = kind::<transit_rt::RtOverview>().register_bytes(&encoded.bytes)?;
            Ok(Response::EncodeAlerts { rt_hash, feed_b64: transit_codec::base64_encode(&encoded.bytes), skipped: encoded.skipped })
        }
        Request::ReadAlerts { network_hash, bytes } => {
            let network = kind::<transit_network::Network>().require(&network_hash)?;
            let read = transit_rt::read_alerts(&network, &bytes).map_err(|e| e.to_string())?;
            let (alerts_hash, alerts) = kind::<transit_alerts::ServiceAlerts>().register(read.alerts)?;
            Ok(Response::ReadAlerts { alerts_hash, alerts: (*alerts).clone(), skipped: read.skipped, dropped: read.dropped })
        }
    }
}

/// The JSON seam: `{"op": ...}` in, and `{"ok": ...}` or `{"error": ...}`
/// out. Its bytes match every other capability module.
pub fn dispatch_json(text: &str) -> String {
    cap_abi::answer(text, dispatch)
}

/// The bulk-bytes seam. Op 0 decodes a payload, and answers the overview and
/// its hash. The kernel's `load_rt` registers one, answering the hash only.
pub fn dispatch_bytes(op: u32, bytes: &[u8]) -> String {
    match op {
        OP_DECODE => cap_abi::outcome(decode(bytes)),
        other => cap_abi::outcome::<()>(Err(format!("unknown bulk op {other}"))),
    }
}

/// The worked examples, which the generator runs (no module build turns
/// the feature on).
#[cfg(feature = "examples")]
pub mod examples;

cap_abi::capability! {
    id: "rt",
    json: dispatch_json,
    bytes: dispatch_bytes,
    documents: [transit_network::Network, transit_codec::Boundary, transit_rt::RtOverview, transit_live::LiveDay, transit_alerts::ServiceAlerts],
    request: Request,
    consumes: &[("network", "network")],
    // The GTFS-rt payload is a short-lived document, polled every few seconds.
    // It is versioned by the protobuf schema it speaks. It joins against the
    // network, since an rt trip id is a model trip id. It uses the import's
    // boundary for the published stop_sequence numbering, when there is one.
    // The live document is made here, and the board reads it. Neither module
    // needs the other to answer its own ops, so live is spoken, not consumed.
    // encode_alerts writes the service alerts document as an Alerts feed.
    // GTFS-rt is its serialization.
    speaks: &[
        ("gtfs_rt", &[1]),
        ("network", &[transit_network::SCHEMA_VERSION]),
        ("boundary", &[1]),
        ("live", &[1]),
        ("service_alerts", &[transit_alerts::ALERTS_VERSION]),
    ],
}
