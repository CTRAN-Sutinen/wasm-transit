//! `live` registers the live document where it makes it. In an assembly the
//! board and the router read it by hash, and it never crosses to the host and
//! back. No capability joins rt again.

#![forbid(unsafe_code)]

use std::sync::Arc;

use cap_abi::kernel::kind;
use serde::{Deserialize, Serialize};
use transit_codec::Boundary;
use transit_live::{LiveDay, LiveSummary, OnTimeWindow};
use transit_rt::{LateTrip, RtOverview, TripProgress};
use ts_rs::TS;

/// The runner's `--load` default.
pub const OP_DECODE: u32 = 0;

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "cap_rt/")]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Request {
    Decode { bytes: Vec<u8> },
    TripProgress {
        network_hash: String,
        #[serde(default)]
        #[ts(optional)]
        boundary_hash: Option<String>,
        rt_hash: String,
        trip_id: String,
    },
    LateTrips {
        network_hash: String,
        rt_hash: String,
        #[serde(default)]
        vp_hash: Option<String>,
        min_delay_secs: i32,
    },
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
        #[serde(default)]
        #[ts(optional)]
        late_limit: Option<u32>,
        #[serde(default)]
        #[ts(optional)]
        with_document: Option<bool>,
    },
    EncodeAlerts {
        network_hash: String,
        alerts_hash: String,
        #[ts(type = "number")]
        timestamp: u64,
    },
    ReadAlerts { network_hash: String, bytes: Vec<u8> },
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "cap_rt/")]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Response {
    Decode { overview: RtOverview, rt_hash: String },
    TripProgress { progress: Option<TripProgress> },
    LateTrips { trips: Vec<LateTrip> },
    Live { live_hash: String, live: Option<LiveDay>, summary: LiveSummary },
    EncodeAlerts { rt_hash: String, feed_b64: String, skipped: Vec<transit_rt::SkippedEntity> },
    ReadAlerts {
        alerts_hash: String,
        alerts: transit_alerts::ServiceAlerts,
        skipped: Vec<transit_rt::SkippedEntity>,
        dropped: Vec<transit_rt::DroppedAlert>,
    },
}

fn boundary_arc(hash: Option<&str>) -> Result<Arc<Boundary>, cap_abi::Refusal> {
    match hash {
        Some(hash) => kind::<transit_codec::Boundary>().require(hash),
        None => Ok(Arc::new(Boundary::default())),
    }
}

fn snapshot_arc(rt_hash: &str) -> Result<Arc<RtOverview>, cap_abi::Refusal> {
    kind::<transit_rt::RtOverview>().require(rt_hash)
}

fn decode(bytes: &[u8]) -> Result<Response, cap_abi::Refusal> {
    let (rt_hash, overview) = kind::<transit_rt::RtOverview>().register_bytes(bytes)?;
    Ok(Response::Decode { overview: (*overview).clone(), rt_hash })
}

pub fn dispatch(request: Request) -> Result<Response, cap_abi::Refusal> {
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

pub fn dispatch_json(text: &str) -> String {
    cap_abi::answer(text, dispatch)
}

pub fn dispatch_bytes(op: u32, bytes: &[u8]) -> String {
    match op {
        OP_DECODE => cap_abi::outcome(decode(bytes)),
        other => cap_abi::outcome::<(), String>(Err(format!("unknown bulk op {other}"))),
    }
}

#[cfg(feature = "examples")]
pub mod examples;

cap_abi::capability! {
    id: "rt",
    json: dispatch_json,
    bytes: dispatch_bytes,
    documents: [transit_network::Network, transit_codec::Boundary, transit_rt::RtOverview, transit_live::LiveDay, transit_alerts::ServiceAlerts],
    request: Request,
    consumes: &[("network", "network")],
    // `live` is spoken, not consumed: this module makes the live document and
    // the board reads it, and neither needs the other to answer its own ops.
    speaks: &[
        ("gtfs_rt", &[1]),
        ("network", &[transit_network::SCHEMA_VERSION]),
        ("boundary", &[1]),
        ("live", &[1]),
        ("service_alerts", &[transit_alerts::ALERTS_VERSION]),
    ],
}
