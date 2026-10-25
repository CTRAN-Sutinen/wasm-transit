//! transit-ops — the host-agnostic operations layer.
//!
//! Every host (wasm-bindgen in the browser, wasmtime in the sync
//! server, plain cargo tests) shares this one dispatch seam. The
//! contract is the [`Request`]/[`Response`] pair — plain serde types,
//! serializable to tagged JSON (`{"op": "departures", ...}`) whenever
//! a host wants a textual form, constructible directly in Rust when
//! it doesn't.
//!
//! Ownership inside the module: feeds live in a registry keyed by
//! `feed_id` (hosts never hold typed handles); the operator-edit
//! document arrives whole via [`Request::SetDiversions`] and is
//! validated per record; derived caches (effective feed, routing
//! networks) are managed here and invalidated on any document change.
//! Hosts own I/O, files, and the clock — ops receive dates as
//! "YYYY-MM-DD" strings and times as seconds since midnight.

pub mod geojson;
pub mod network;
pub mod rt;
pub mod state;

pub use geojson::{
    FeatureCollection, LineGeometry, PointGeometry, StopFeature, StopProps, TripLineFeature,
    TripProps,
};
pub use network::network_to_gtfs;
pub use rt::{RtOverview, StopUpdateView, TripUpdateView, VehicleView};
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
    /// Validate an authored network document without loading it.
    /// Returns every issue — the authoring UI's feedback loop.
    NetworkValidate { network: transit_network::Network },
    /// Infer an authored network document from a loaded feed — the
    /// planner's bootstrap path: the network the agency already runs,
    /// clustered into patterns. Deliberately lossy; deterministic.
    NetworkImport { feed_id: u32 },
    /// Materialize an authored network document into the derived-feed
    /// view and register it in the same registry a parsed zip lands
    /// in, so every other op (departures, planning, blocks, geojson)
    /// works on a planner's document unchanged. Invalid documents are
    /// rejected; the error carries the first issue (validate first
    /// when the whole list is wanted).
    NetworkLoad { network: transit_network::Network },
    /// Entity counts of a feed.
    Summary { feed_id: u32 },
    /// Name-sorted `[{ stop_id, name }]` for UI pickers.
    Stops { feed_id: u32 },
    /// Name-sorted route list for pickers.
    Routes { feed_id: u32 },
    /// The stop pattern of a route (+direction) as served by its
    /// longest matching trip: what the diversion editor anchors
    /// against.
    RouteStopSequence { feed_id: u32, route_id: String, direction_id: Option<u8> },
    /// Upcoming departures at a stop on a service date.
    Departures {
        feed_id: u32,
        stop_id: String,
        date: String,
        from_secs: u32,
        limit: u32,
    },
    /// Service keys running on a date (often several).
    ActiveServices { feed_id: u32, date: String },
    /// All blocks in the feed. With `date`: only the blocks running
    /// that day, with that day's trip counts — the dispatcher's
    /// block picker shows vehicles in service, not parked buses.
    Blocks { feed_id: u32, date: Option<String> },
    /// The trip chain of one block on a service date.
    BlockTrips { feed_id: u32, block_id: String, date: String },
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
    /// Load a street network (GeoJSON lines) for detour authoring;
    /// returns its id. Street data is reference geometry, keyed by
    /// its own registry — it never mixes with feed state.
    StreetsLoad { bytes: Vec<u8> },
    /// The nearest street position to a point (within the snap
    /// neighborhood), or null when no street is close.
    StreetSnap { streets_id: u32, lat: f64, lon: f64 },
    /// The street geometry between two positions (shortest path).
    /// Empty when the positions sit on disconnected parts.
    StreetRoute {
        streets_id: u32,
        from: transit_streets::StreetPosition,
        to: transit_streets::StreetPosition,
    },
    /// Decode a GTFS-rt protobuf payload into the overview view.
    DecodeRt { bytes: Vec<u8> },
    /// `feed_info.txt` metadata of a feed (version, publisher), when
    /// the feed provides the file.
    FeedInfo { feed_id: u32 },
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
    NetworkValidate { issues: Vec<transit_network::NetworkError> },
    NetworkImport { network: transit_network::Network },
    NetworkLoad { feed_id: u32, summary: transit_core::FeedSummary },
    Summary { summary: transit_core::FeedSummary },
    Stops { stops: Vec<transit_core::StopSummary> },
    Routes { routes: Vec<transit_core::RouteSummary> },
    RouteStopSequence { stops: Vec<transit_core::StopSummary> },
    Departures { departures: Vec<transit_core::Departure> },
    ActiveServices { services: Vec<String> },
    Blocks { blocks: Vec<transit_core::BlockSummary> },
    BlockTrips { trips: Vec<transit_core::BlockTrip> },
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
    StreetsLoad { streets_id: u32, segments: u32, nodes: u32 },
    StreetSnap { position: Option<transit_streets::StreetPosition> },
    StreetRoute { points: Vec<transit_streets::StreetPoint> },
    DecodeRt { overview: RtOverview },
    FeedInfo { info: Option<transit_core::FeedInfo> },
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
        Request::NetworkValidate { network } => Ok(Response::NetworkValidate {
            issues: transit_network::validation_issues(&network),
        }),
        Request::NetworkImport { feed_id } => state::with_feed(feed_id, |f| {
            Ok(Response::NetworkImport {
                network: network::gtfs_to_network(&f.gtfs),
            })
        }),
        Request::NetworkLoad { network } => {
            let gtfs = network::network_to_gtfs(&network)?;
            let summary = gtfs.summary();
            let feed_id = state::register_feed(gtfs);
            Ok(Response::NetworkLoad { feed_id, summary })
        }
        Request::DecodeRt { bytes } => {
            Ok(Response::DecodeRt { overview: rt::decode_rt_overview(&bytes)? })
        }
        Request::Summary { feed_id } => {
            state::with_feed(feed_id, |f| Ok(Response::Summary { summary: f.gtfs.summary() }))
        }
        Request::Stops { feed_id } => state::with_feed(feed_id, |f| {
            Ok(Response::Stops { stops: f.gtfs.stop_summaries() })
        }),
        Request::Routes { feed_id } => state::with_feed(feed_id, |f| {
            Ok(Response::Routes { routes: f.gtfs.route_summaries() })
        }),
        Request::RouteStopSequence { feed_id, route_id, direction_id } => {
            state::with_feed(feed_id, |f| {
                Ok(Response::RouteStopSequence {
                    stops: f.gtfs.route_stop_sequence(&route_id, direction_id),
                })
            })
        }
        Request::Departures { feed_id, stop_id, date, from_secs, limit } => {
            let parsed = parse_date(&date)?;
            state::with_feed(feed_id, |f| {
                let departures = f.with_effective(&date, |gtfs| {
                    gtfs.next_departures(&stop_id, parsed, from_secs, limit as usize)
                })?;
                Ok(Response::Departures { departures })
            })
        }
        Request::ActiveServices { feed_id, date } => {
            let parsed = parse_date(&date)?;
            state::with_feed(feed_id, |f| {
                Ok(Response::ActiveServices { services: f.gtfs.active_services(parsed) })
            })
        }
        Request::Blocks { feed_id, date } => state::with_feed(feed_id, |f| {
            let blocks = match date {
                Some(d) => f.gtfs.blocks_on(parse_date(&d)?),
                None => f.gtfs.blocks(),
            };
            Ok(Response::Blocks { blocks })
        }),
        Request::BlockTrips { feed_id, block_id, date } => {
            let parsed = parse_date(&date)?;
            state::with_feed(feed_id, |f| {
                let trips = f.with_effective(&date, |gtfs| gtfs.block_trips(&block_id, parsed))?;
                Ok(Response::BlockTrips { trips })
            })
        }
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
        Request::StreetsLoad { bytes } => {
            let net = transit_streets::StreetNetwork::parse(&bytes)
                .map_err(|e| OpError::new(format!("street network: {}", e.0)))?;
            let segments = net.segment_count();
            let nodes = net.node_count();
            let streets_id = state::register_streets(net);
            Ok(Response::StreetsLoad { streets_id, segments, nodes })
        }
        Request::StreetSnap { streets_id, lat, lon } => state::with_streets(streets_id, |net| {
            Ok(Response::StreetSnap { position: net.snap(lat, lon) })
        }),
        Request::StreetRoute { streets_id, from, to } => {
            state::with_streets(streets_id, |net| {
                Ok(Response::StreetRoute {
                    points: net.route(from, to).unwrap_or_default(),
                })
            })
        }
        Request::SetDiversions { feed_id, records } => state::with_feed(feed_id, |f| {
            let reports = transit_sync::report_all(&f.gtfs, &records);
            f.edits = records;
            f.invalidate();
            Ok(Response::SetDiversions { reports })
        }),
        Request::FeedInfo { feed_id } => state::with_feed(feed_id, |f| {
            Ok(Response::FeedInfo { info: f.gtfs.feed_info.clone() })
        }),
        Request::DropFeed { feed_id } => {
            state::drop_feed(feed_id);
            Ok(Response::DropFeed {})
        }
        Request::SyncEdits { feed_id, client, server, now_ms } => {
            let records = transit_sync::submit(transit_sync::merge(&client, &server), now_ms);
            let reports = match feed_id {
                Some(feed_id) => state::with_feed(feed_id, |f| {
                    Ok(transit_sync::report_all(&f.gtfs, &records))
                })?,
                None => Vec::new(),
            };
            Ok(Response::SyncEdits { records, reports })
        }
    }
}

/// Binary-op codes for [`dispatch_bytes`] — the ops whose payloads
/// are megabytes of raw bytes (feed zips, rt protobuf, street
/// GeoJSON), where a JSON array of numbers would be absurd.
pub const OP_LOAD_FEED: u32 = 0;
pub const OP_DECODE_RT: u32 = 1;
pub const OP_STREETS_LOAD: u32 = 2;

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

/// Bytes-op form of the seam for [`OP_LOAD_FEED`] / [`OP_DECODE_RT`]
/// / [`OP_STREETS_LOAD`]. Same outcome shape as [`dispatch_json`].
pub fn dispatch_bytes(op: u32, bytes: &[u8]) -> String {
    let req = match op {
        OP_LOAD_FEED => Request::LoadFeed { bytes: bytes.to_vec() },
        OP_DECODE_RT => Request::DecodeRt { bytes: bytes.to_vec() },
        OP_STREETS_LOAD => Request::StreetsLoad { bytes: bytes.to_vec() },
        other => {
            return dispatch_json_outcome(Err(OpError::new(format!(
                "unknown bytes op code {other}"
            ))));
        }
    };
    dispatch_json_outcome(dispatch(req))
}
