//! transit-ops — the host-agnostic operations layer.
//!
//! Every host (wasm-bindgen in the browser, wasmtime in the sync
//! server, plain cargo tests) shares this one dispatch seam. The
//! contract is the [`Request`]/[`Response`] pair — plain serde types,
//! serializable to tagged JSON (`{"op": "stops_geojson", ...}`)
//! whenever a host wants a textual form, constructible directly in
//! Rust when it doesn't.
//!
//! Ownership inside the module: feeds live in a registry keyed by
//! `feed_id` (hosts never hold typed handles); the operator-edit
//! document arrives whole via [`Request::SetDiversions`] and is
//! validated per record; derived caches (the effective feed) are
//! managed here and invalidated on any document change.
//! Hosts own I/O, files, and the clock — ops receive dates as
//! "YYYY-MM-DD" strings and times as seconds since midnight.

pub use transit_analysis as analysis; // the analysis module moved to its own crate — the bus shares it as a library
pub use transit_codec as codec; // the network codec moved to its own crate — the bus shares it as a library
pub mod plans;
pub mod state;

pub use plans::{PlanInfo, PlanIssue};
pub use state::FeedState;

use ts_rs::TS;
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

use transit_sync::EditRecord;

/// One operation call. Tagged enums, so the serde form is
/// `{"op": "<name>", ...fields}` — one shape, native or JSON.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Request {
    /// Parse a GTFS static zip from raw bytes; registers the feed.
    LoadFeed { bytes: Vec<u8> },
    /// The service-plan chain: which plan is in force on a date (the
    /// latest effective date at or before it). Unversioned plans
    /// never resolve.
    PlansResolve { plans: Vec<PlanInfo>, date: String },
    /// Chain validity: unique plan ids, versioned plans
    /// effective-dated, effective dates unique.
    PlansValidate { plans: Vec<PlanInfo> },
    /// Every chain member's governing period — [effective_from,
    /// until the next pick) — derived, never stored: pick dates are
    /// targets that move during planning.
    PlansPeriods { plans: Vec<PlanInfo> },
    /// Release a feed and its caches. Hosts use this when a
    /// republished artifact replaces the one they pinned.
    DropFeed { feed_id: u32 },
    /// Sync: merge a client document with a server document (LWW per
    /// id, see transit-sync), transition drafts to submitted (the
    /// clock is passed by the host — hosts own time), and — when a
    /// feed is given — validate the merged document against it, so
    /// the response carries authoritative reports. This is the op
    /// behind POST /edits.
    SyncEdits {
        feed_id: Option<u32>,
        client: Vec<EditRecord>,
        server: Vec<EditRecord>,
        #[ts(type = "number")]
        now_ms: u64,
    },
}

/// The result of one operation. Same tagging as [`Request`].
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Response {
    LoadFeed { feed_id: u32, summary: transit_core::FeedSummary },
    PlansResolve { plan: Option<String> },
    PlansValidate { issues: Vec<PlanIssue> },
    PlansPeriods { periods: Vec<plans::PlanPeriod> },
    DropFeed {},
    SyncEdits { records: Vec<EditRecord>, reports: Vec<transit_sync::EditReport> },
}

/// Operation failure: a message hosts can surface or serialize.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct OpError {
    pub message: String,
}

impl OpError {
    pub fn new(message: impl Into<String>) -> OpError {
        OpError { message: message.into() }
    }
}

impl std::fmt::Display for OpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for OpError {}

pub(crate) fn parse_date(s: &str) -> Result<NaiveDate, OpError> {
    NaiveDate::parse_from_str(s, "%Y-%m-%d")
        .map_err(|e| OpError::new(format!("invalid date {s:?}, expected YYYY-MM-DD: {e}")))
}

/// The dispatch seam: every host calls this, nothing else.
pub fn dispatch(req: Request) -> Result<Response, OpError> {
    match req {
        Request::LoadFeed { bytes } => {
            let gtfs = transit_core::Gtfs::from_zip_bytes(&bytes)
                .map_err(|e| OpError::new(e.to_string()))?;
            let summary = gtfs.summary();
            let feed_id = state::register_feed(gtfs);
            Ok(Response::LoadFeed { feed_id, summary })
        }
        Request::PlansResolve { plans, date } => {
            let parsed = parse_date(&date)?;
            Ok(Response::PlansResolve {
                plan: plans::resolve(&plans, parsed)?,
            })
        }
        Request::PlansValidate { plans } => Ok(Response::PlansValidate {
            issues: plans::validate(&plans),
        }),
        Request::PlansPeriods { plans } => Ok(Response::PlansPeriods {
            periods: plans::periods(&plans),
        }),
        Request::DropFeed { feed_id } => {
            state::drop_feed(feed_id);
            Ok(Response::DropFeed {})
        }
        Request::SyncEdits { feed_id, client, server, now_ms } => {
            let records = transit_sync::submit(transit_sync::merge(&client, &server), now_ms);
            let reports = match feed_id {
                Some(feed_id) => state::with_feed(feed_id, |f| {
                    Ok(transit_core::report_all(&f.gtfs, &records))
                })?,
                None => Vec::new(),
            };
            Ok(Response::SyncEdits { records, reports })
        }
    }
}

/// Binary-op codes for [`dispatch_bytes`] — the ops whose payloads
/// are megabytes of raw bytes (feed zips), where a JSON array of
/// numbers would be absurd. (The rt protobuf payload moved to cap-rt
/// with its decoder, the street GeoJSON to cap-streets with the
/// detour editor's graph, and the population layers to cap-analysis
/// with the coverage that reads them.)
pub const OP_LOAD_FEED: u32 = 0;

/// Outcome shape: `{"ok": <Response>}` on success, `{"error": "<msg>"}`
/// on failure — one tagged wrapper, so hosts have exactly two paths.
fn dispatch_json_outcome(outcome: Result<Response, OpError>) -> String {
    #[derive(Serialize)]
    #[serde(untagged)]
    enum Outcome {
        Ok { ok: Response },
        Error { error: String },
    }
    let outcome = match outcome {
        Ok(response) => Outcome::Ok { ok: response },
        Err(e) => Outcome::Error { error: e.message },
    };
    serde_json::to_string(&outcome)
        .unwrap_or_else(|_| r#"{"error":"internal: response serialization failed"}"#.to_string())
}

/// The textual form of the seam: a serialized [`Request`] in, a
/// serialized outcome out. Hosts with linear-memory ABIs (the flat
/// C ABI, any wasmtime/wazero embed) move these strings across the
/// boundary as bytes.
pub fn dispatch_json(input: &str) -> String {
    match serde_json::from_str::<Request>(input) {
        Err(e) => dispatch_json_outcome(Err(OpError::new(format!("invalid request: {e}")))),
        Ok(req) => dispatch_json_outcome(dispatch(req)),
    }
}

/// Bytes-op form of the seam for [`OP_LOAD_FEED`]. Same outcome
/// shape as [`dispatch_json`].
pub fn dispatch_bytes(op: u32, bytes: &[u8]) -> String {
    let req = match op {
        OP_LOAD_FEED => Request::LoadFeed { bytes: bytes.to_vec() },
        other => {
            return dispatch_json_outcome(Err(OpError::new(format!(
                "unknown bytes op code {other}"
            ))));
        }
    };
    dispatch_json_outcome(dispatch(req))
}
