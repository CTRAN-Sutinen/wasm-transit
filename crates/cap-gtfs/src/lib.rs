//! cap-gtfs: the feed document's own question, its identity
//! (`feed_info`; OPS.md has the op).
//!
//! The kernel registers a GTFS zip under the hash of its bytes
//! (`load_feed`, which answers its summary). This capability authors
//! nothing; the `Request` enum holds that. Day views, pickers, and the
//! names join answer over the network document in other capabilities,
//! because the bus has one answer for each question. Its charter is
//! closed: parse and give the identity over transit-codec's `Feed`.

#![forbid(unsafe_code)]

use cap_abi::kernel::kind;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use transit_codec::FeedInfo;

/// One operation call. Tagged like every capability's seam: the serde form
/// is `{"op": "<name>", ...fields}`.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "cap_gtfs/")]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Request {
    /// `feed_info.txt` metadata (version, publisher). This is the identity
    /// question: which feed is this?
    FeedInfo { feed_hash: String },
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "cap_gtfs/")]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Response {
    FeedInfo { info: Option<FeedInfo> },
}

/// The typed dispatch (native hosts call this directly; the ABI shell
/// wraps it in JSON). Failures are `{"error": ...}`, never panics.
pub fn dispatch(request: Request) -> Result<Response, String> {
    match request {
        Request::FeedInfo { feed_hash } => Ok(Response::FeedInfo { info: kind::<transit_codec::Feed>().require(&feed_hash)?.info() }),
    }
}

/// The JSON seam: `{"op": ...}` in, `{"ok": ...}` or `{"error": ...}` out.
/// The bytes match every other capability module.
pub fn dispatch_json(text: &str) -> String {
    cap_abi::answer(text, dispatch)
}

/// The feed capability has no bulk op of its own. A zip registers on the
/// kernel's `load_feed`. The seam answers with an error instead of
/// silently ignoring the bulk bytes.
pub fn dispatch_bytes(op: u32, _bytes: &[u8]) -> String {
    cap_abi::outcome::<()>(Err(format!("unknown bulk op {op}: the feed capability has no bulk op of its own")))
}

/// The worked examples. The generator runs them. No module build turns
/// the feature on.
#[cfg(feature = "examples")]
pub mod examples;

cap_abi::capability! {
    id: "gtfs",
    json: dispatch_json,
    bytes: dispatch_bytes,
    documents: [transit_codec::Feed],
    request: Request,
    speaks: &[("feed", &[1])],
}
