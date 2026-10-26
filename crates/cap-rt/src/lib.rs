//! cap-rt — the realtime capability module.
//!
//! The third capability extracted from the MONOLITH (after analysis
//! and the network codec): decode a GTFS-rt protobuf payload into
//! the overview view (delays, vehicles) a dispatcher or rider reads.
//! The domain code lives in the shared `transit-rt` library — the
//! monolith keeps no copy; every host (web, runner, a future server
//! leg) decodes through the same bytes.
//!
//! The payload is EPHEMERAL: an rt snapshot is polled every few
//! seconds, never persisted, and is addressed by content hash only
//! for identity checks (two modules handed the same bytes must agree
//! they are the same snapshot). The capability writes nothing — it
//! is a pure decoder by construction.

use serde::{Deserialize, Serialize};
use ts_rs::TS;
use transit_rt::RtOverview;

/// The bulk-bytes op code (the only op that takes raw protobuf).
pub const OP_DECODE: u32 = 0;

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "cap_rt/")]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Request {
    /// Decode a GTFS-rt protobuf payload (the bytes an agency serves,
    /// typically polled every 15–30 seconds). Bulk-bytes op 0; the
    /// JSON form exists for hosts that already hold the bytes as
    /// numbers.
    Decode { bytes: Vec<u8> },
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "cap_rt/")]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Response {
    Decode { overview: RtOverview },
}

/// The typed dispatch (native hosts call this directly). Failures
/// are `{"error": ...}`, never panics — a total seam: garbage bytes
/// are an error outcome, not a trap.
pub fn dispatch(request: Request) -> Result<Response, String> {
    match request {
        Request::Decode { bytes } => Ok(Response::Decode {
            overview: transit_rt::decode_overview(&bytes)?,
        }),
    }
}

/// The JSON seam: `{"op": ...}` in, `{"ok": ...}` / `{"error": ...}`
/// out — byte-compatible with the monolith and every other
/// capability module.
pub fn dispatch_json(text: &str) -> String {
    let outcome = match serde_json::from_str::<Request>(text) {
        Ok(request) => match dispatch(request) {
            Ok(response) => serde_json::json!({ "ok": response }),
            Err(error) => serde_json::json!({ "error": error }),
        },
        Err(error) => serde_json::json!({ "error": format!("invalid request: {error}") }),
    };
    serde_json::to_string(&outcome).expect("dispatch outcomes serialize")
}

/// The bulk-bytes seam: op 0 decodes a payload.
pub fn dispatch_bytes(op: u32, bytes: &[u8]) -> String {
    let outcome = match op {
        OP_DECODE => match dispatch(Request::Decode { bytes: bytes.to_vec() }) {
            Ok(response) => serde_json::json!({ "ok": response }),
            Err(error) => serde_json::json!({ "error": error }),
        },
        other => serde_json::json!({ "error": format!("unknown bulk op {other}") }),
    };
    serde_json::to_string(&outcome).expect("dispatch outcomes serialize")
}

cap_abi::capability! {
    json: dispatch_json,
    bytes: dispatch_bytes,
}
