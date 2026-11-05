//! cap-gtfs — the GTFS capability module.
//!
//! One subject: the FEED document. The kernel registers a GTFS
//! static zip under the CONTENT HASH of its bytes (`load_feed`, which
//! answers what the feed contains at a glance — its summary); this
//! capability answers the feed's own question: its identity
//! (`feed_info.txt` — version, publisher). It has no ops that author
//! anything — "only planners write networks, everything else
//! imports" is enforced by the Request enum, not by convention — and
//! no day views, no pickers, no names join: those are MODEL questions
//! and answer over the NETWORK document (cap-board / cap-runcut /
//! cap-calendar for the day, cap-directory for what exists — the
//! bus's one-answer law: a duplicate feed-side answer is deleted, not
//! kept).
//!
//! The TS types that cross the boundary are generated into
//! `types/cap_gtfs/`.

#![forbid(unsafe_code)]

use cap_abi::kernel::FEED;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use transit_codec::FeedInfo;

/// One operation call. Tagged like the monolith's seam: the serde form
/// is `{"op": "<name>", ...fields}`.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "cap_gtfs/")]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Request {
    /// `feed_info.txt` metadata (version, publisher) — the identity
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
        Request::FeedInfo { feed_hash } => Ok(Response::FeedInfo { info: FEED.require(&feed_hash)?.info() }),
    }
}

/// The JSON seam: `{"op": ...}` in, `{"ok": ...}` / `{"error": ...}`
/// out — byte-compatible with every other capability module.
pub fn dispatch_json(text: &str) -> String {
    cap_abi::answer(text, dispatch)
}

/// The feed capability has no bulk op of its own (a zip registers on
/// the kernel's `load_feed`); the seam answers honestly instead of
/// silently ignoring one.
pub fn dispatch_bytes(op: u32, _bytes: &[u8]) -> String {
    cap_abi::outcome::<()>(Err(format!("unknown bulk op {op}: the feed capability has no bulk op of its own")))
}

cap_abi::capability! {
    id: "gtfs",
    json: dispatch_json,
    bytes: dispatch_bytes,
    documents: [FEED],
}
