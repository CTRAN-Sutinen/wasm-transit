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
//! validated per record; derived caches (effective feed, routing
//! networks) are managed here and invalidated on any document change.
//! Hosts own I/O, files, and the clock — ops receive dates as
//! "YYYY-MM-DD" strings and times as seconds since midnight.

pub use transit_analysis as analysis; // the analysis module moved to its own crate — the bus shares it as a library
pub use transit_codec as codec; // the network codec moved to its own crate — the bus shares it as a library
pub mod geojson;
pub mod plans;
pub mod state;

pub use plans::{PlanInfo, PlanIssue};
pub use geojson::{
    FeatureCollection, LineGeometry, PointGeometry, StopFeature, StopProps, TripLineFeature,
    TripProps,
};
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
    /// All stops as a GeoJSON FeatureCollection.
    StopsGeojson { feed_id: u32 },
    /// One trip's path as a GeoJSON LineString Feature (date scopes
    /// operator edits: diverted trips draw patched shapes).
    TripPathGeojson { feed_id: u32, trip_id: String, date: String },
    /// The route's alignment as a GeoJSON LineString Feature: the
    /// path of the representative trip (the same trip the diversion
    /// editor's anchors come from), shape polyline when the feed has
    /// one. The date scopes operator edits like `trip_path_geojson`.
    RoutePathGeojson {
        feed_id: u32,
        route_id: String,
        direction_id: Option<u8>,
        date: String,
    },
    /// One leg of a trip (board → alight) as a GeoJSON LineString
    /// Feature — the ridden segment only.
    TripSegmentGeojson {
        feed_id: u32,
        trip_id: String,
        from_stop_id: String,
        to_stop_id: String,
        date: String,
    },
    /// One stop as a GeoJSON point Feature (or null).
    StopPoint { feed_id: u32, stop_id: String },
    /// Earliest-arrival plan against the effective feed.
    PlanTrip {
        feed_id: u32,
        from: String,
        to: String,
        date: String,
        depart_secs: u32,
        max_legs: u32,
    },
    /// The plan twice: with in-force edits, and against the base feed
    /// (baseline null when no edit covers the date).
    PlanTripCompare {
        feed_id: u32,
        from: String,
        to: String,
        date: String,
        depart_secs: u32,
        max_legs: u32,
    },
    /// Replace the whole edit document; returns per-record validation
    /// reports. Invalid records are reported, not rejected.
    SetDiversions { feed_id: u32, records: Vec<EditRecord> },
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
    StopsGeojson { collection: FeatureCollection },
    TripPathGeojson { feature: Option<TripLineFeature> },
    RoutePathGeojson { feature: Option<TripLineFeature> },
    TripSegmentGeojson { feature: Option<TripLineFeature> },
    StopPoint { feature: Option<StopFeature> },
    PlanTrip { itinerary: Option<transit_router::Itinerary> },
    PlanTripCompare {
        active: Option<transit_router::Itinerary>,
        baseline: Option<transit_router::Itinerary>,
    },
    SetDiversions { reports: Vec<transit_sync::EditReport> },
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
        Request::StopsGeojson { feed_id } => state::with_feed(feed_id, |f| {
            Ok(Response::StopsGeojson { collection: geojson::stops_geojson(&f.gtfs) })
        }),
        Request::TripPathGeojson { feed_id, trip_id, date } => state::with_feed(feed_id, |f| {
            // with_effective borrows the state, so collect the path
            // first and build the feature from the base feed's names.
            let path = f.with_effective(&date, |gtfs| gtfs.trip_path(&trip_id))?;
            Ok(Response::TripPathGeojson {
                feature: geojson::trip_line_feature(&f.gtfs, &trip_id, &path),
            })
        }),
        Request::TripSegmentGeojson { feed_id, trip_id, from_stop_id, to_stop_id, date } => {
            state::with_feed(feed_id, |f| {
                let path = f.with_effective(&date, |gtfs| {
                    gtfs.trip_path_segment(&trip_id, &from_stop_id, &to_stop_id)
                })?;
                Ok(Response::TripSegmentGeojson {
                    feature: geojson::trip_line_feature(&f.gtfs, &trip_id, &path),
                })
            })
        }
        Request::RoutePathGeojson { feed_id, route_id, direction_id, date } => {
            state::with_feed(feed_id, |f| {
                let trip_id = f
                    .gtfs
                    .representative_trip_id(&route_id, direction_id)
                    .map(str::to_string);
                let Some(trip_id) = trip_id else {
                    return Ok(Response::RoutePathGeojson { feature: None });
                };
                let path = f.with_effective(&date, |gtfs| gtfs.trip_path(&trip_id))?;
                Ok(Response::RoutePathGeojson {
                    feature: geojson::trip_line_feature(&f.gtfs, &trip_id, &path),
                })
            })
        }
        Request::StopPoint { feed_id, stop_id } => state::with_feed(feed_id, |f| {
            Ok(Response::StopPoint { feature: geojson::stop_point(&f.gtfs, &stop_id) })
        }),
        Request::PlanTrip { feed_id, from, to, date, depart_secs, max_legs } => {
            state::with_feed(feed_id, |f| {
                f.ensure_router(&date)?; // parses and validates the date
                let (_, cache) = f.router.as_ref().expect("router built");
                let itinerary = transit_router::earliest_arrival(
                    &cache.effective,
                    &from,
                    &to,
                    depart_secs,
                    max_legs.max(1) as usize,
                )
                .map_err(|e| OpError::new(e.to_string()))?;
                Ok(Response::PlanTrip { itinerary })
            })
        }
        Request::PlanTripCompare { feed_id, from, to, date, depart_secs, max_legs } => {
            let parsed = parse_date(&date)?;
            let legs = max_legs.max(1) as usize;
            state::with_feed(feed_id, |f| {
                f.ensure_router(&date)?;
                let active = {
                    let (_, cache) = f.router.as_ref().expect("router built");
                    transit_router::earliest_arrival(
                        &cache.effective, &from, &to, depart_secs, legs,
                    )
                    .map_err(|e| OpError::new(e.to_string()))?
                };
                let baseline = if f.applicable_diversions(parsed).is_empty() {
                    None
                } else {
                    // Lazily build the base-feed network on first comparison.
                    let (_, cache) = f.router.as_mut().expect("router built");
                    if cache.baseline.is_none() {
                        cache.baseline =
                            Some(transit_router::RouterNetwork::build(&f.gtfs, parsed));
                    }
                    transit_router::earliest_arrival(
                        cache.baseline.as_ref().expect("baseline network just built"),
                        &from,
                        &to,
                        depart_secs,
                        legs,
                    )
                    .map_err(|e| OpError::new(e.to_string()))?
                };
                Ok(Response::PlanTripCompare { active, baseline })
            })
        }
        Request::SetDiversions { feed_id, records } => state::with_feed(feed_id, |f| {
            let reports = transit_core::report_all(&f.gtfs, &records);
            f.edits = records;
            f.invalidate();
            Ok(Response::SetDiversions { reports })
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
