//! cap-analysis — the planner-analysis capability module.
//!
//! The first capability extracted from the MONOLITH: the analysis
//! functions (network stats, service-type overview and detail,
//! coverage of a weighted layer) live in `transit-analysis`, shared as a
//! library under this module. A planner's bundle ships it; a
//! rider's never hears of it.
//!
//! The documents it reads are the kernel's, content-addressed like
//! everywhere on the bus: an authored network (the JSON document)
//! under its canonical hash, a weighted layer under the hash of its
//! bytes (the kernel's `load_layer` answers its features and the
//! numeric fields it can be read by; a coverage op names the field and
//! the planner's label), and every op addresses them
//! by hash — never by carrying the whole document per request.
//! Composition is through documents: a network produced by the network
//! capability is read here by the SAME hash the producer returned.

#![forbid(unsafe_code)]

use std::sync::Arc;

use cap_abi::kernel::{LAYER, NETWORK};
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use transit_analysis::{
    coverage_compare, network_stats, service_type_compare, service_type_detail, service_type_overview,
    type_coverage, CoverageDelta, CoverageWindow, LayerReading, NetworkStats, ServiceTypeComparison,
    ServiceTypeDetail, ServiceTypeSummary, CombinationCoverage,
};
use transit_network::Network;

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "cap_analysis/")]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Request {
    /// The network's numbers on one service date. The optional
    /// include flags shape the response: the UI wants every route
    /// and stop inline; an agent asking the day-shape question wants
    /// five numbers, not 199 KiB. Both default TRUE (the UI's shape
    /// is unchanged); `false` empties the corresponding list.
    NetworkStats {
        network_hash: String,
        date: String,
        #[serde(default = "default_true")]
        include_routes: bool,
        #[serde(default = "default_true")]
        include_stops: bool,
        #[serde(default = "default_true")]
        include_service_types: bool,
    },
    /// Every service type's own numbers: its authored service, on no
    /// date (the per-id read under the service groups).
    ServiceTypesOverview { network_hash: String },
    /// The service groups (D-55): one row per COMBINATION of service
    /// types that run together on the same dates — its members, the
    /// dates it holds on and their spans, and the members' pooled
    /// numbers (the service on each of those dates). Most days first.
    /// The axis a planner designs; the drill-down keys on `members`.
    ServiceGroups { network_hash: String },
    /// One combination (`members`, any order) expanded: its routes,
    /// stops and schedules pooled. Null for members that never run
    /// together on a date.
    ServiceTypeDetail { network_hash: String, members: Vec<String> },
    /// A proposal (`network_hash`) against its baseline (`base_hash`)
    /// under one combination: both sides' totals over its members
    /// (with the dates it holds on each side) and the routes whose
    /// numbers changed. Null when the members never run together in
    /// the proposal.
    ServiceTypeCompare { base_hash: String, network_hash: String, members: Vec<String> },
    /// Coverage of a weighted layer by one service COMBINATION
    /// (`members`, any order — a service_groups row's): its members'
    /// pooled authored service with no date standing in, its dates
    /// beside it (D-55); null when they never run together. `field` names
    /// the weight (omitted: the layer's one numeric property) and
    /// `label` what it means (omitted: the field). `window` judges
    /// frequency inside a time window ("every 15 min 07:00–19:00":
    /// the worst wait in the window fits frequent_secs); omitted, it
    /// is the whole-day average headway over departures.
    TypeCoverage {
        network_hash: String,
        layer_hash: String,
        members: Vec<String>,
        #[serde(default)]
        #[ts(optional)]
        field: Option<String>,
        #[serde(default)]
        #[ts(optional)]
        label: Option<String>,
        radius_m: f64,
        frequent_secs: u32,
        min_departures: u32,
        #[serde(default)]
        #[ts(optional)]
        window: Option<CoverageWindow>,
    },
    /// One stop's day folded into headway gaps: departures, the max
    /// gap and the departure it follows, and the per-hour profile
    /// (departures, max and median gap starting in the hour) — the
    /// service-quality read of "how long until the next bus?". An
    /// unknown stop answers zero/None/empty, never an error.
    StopGaps {
        network_hash: String,
        date: String,
        stop_id: String,
    },
    /// The day's service profile: trips per hour bucketed by trip
    /// start, each hour row carrying every running service type's
    /// count. Only hours with trips are rows — the planner's
    /// texture question (where service is thick, where it tapers).
    HourlyTrips {
        network_hash: String,
        date: String,
    },
    /// One combination's schedule texture: every pattern its members
    /// run, with the pooled starts' headway uniformity (min/median/max
    /// gap) and clockface alignment (starts on the quarter-hour grid).
    /// Null for members that never run together — lookups answer
    /// empty, never error.
    ScheduleTexture {
        network_hash: String,
        members: Vec<String>,
    },
    /// The corridor read: every pair of routes that shares at least
    /// one stop on the date — shared stops, shared segments (same-
    /// order consecutive pairs), and the shared stop where the
    /// pair's departures stack highest.
    RouteOverlap {
        network_hash: String,
        date: String,
    },
    /// The hub's timed-transfer view: arrival→departure pairs
    /// across routes within the connection window — the caller's
    /// (`window`, for a hub with widely separated bays), else the
    /// 60–900 s transfer-point convention — pooled over the stop's
    /// walking group: who can make what here. An unknown stop
    /// answers empty, never an error.
    TimedTransfers {
        network_hash: String,
        date: String,
        stop_id: String,
        #[serde(default)]
        #[ts(optional)]
        window: Option<transit_analysis::TransferWindow>,
    },
    /// The hub as a route-pair matrix: `timed_transfers` folded by
    /// (from route, to route) — connections, shortest and longest
    /// wait, median wait (None below four). An unknown stop answers
    /// empty, never an error.
    HubConnections {
        network_hash: String,
        date: String,
        stop_id: String,
        #[serde(default)]
        #[ts(optional)]
        window: Option<transit_analysis::TransferWindow>,
    },
    /// The SERVICE GROUP's frequency grid: every route that departs
    /// under the combination, hour by hour at its headway stop —
    /// departures and the shortest, longest and median gap starting
    /// in the hour, the trips' own (never regularized). Null for
    /// members that never run together.
    RouteFrequency {
        network_hash: String,
        members: Vec<String>,
    },
    /// The unserved WHERE of coverage: every layer point beyond the
    /// radius of every stop one combination (`members`, any order)
    /// serves, with the nearest served stop and its distance — "which part, and
    /// where", not just the share. A polygon answers its unserved
    /// part (its share, its centre, its rings). Sorted by unserved
    /// weight descending.
    UnservedPoints {
        network_hash: String,
        layer_hash: String,
        #[serde(default)]
        #[ts(optional)]
        field: Option<String>,
        #[serde(default)]
        #[ts(optional)]
        label: Option<String>,
        radius_m: f64,
        members: Vec<String>,
    },
    /// A proposal's coverage against its base, each side the service
    /// types the host names (`base_members` of the base, `members` of
    /// the proposal, each pooled over its authored service): each
    /// side's served and frequent weight, and what the proposal gains
    /// and loses — the churn a net change hides. The op never matches
    /// one plan's types to the other's (QAF-20).
    CoverageCompare {
        base_hash: String,
        network_hash: String,
        layer_hash: String,
        #[serde(default)]
        #[ts(optional)]
        field: Option<String>,
        #[serde(default)]
        #[ts(optional)]
        label: Option<String>,
        base_members: Vec<String>,
        members: Vec<String>,
        radius_m: f64,
        frequent_secs: u32,
        min_departures: u32,
        #[serde(default)]
        #[ts(optional)]
        window: Option<CoverageWindow>,
    },
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "cap_analysis/")]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Response {
    NetworkStats { stats: NetworkStats },
    ServiceTypesOverview { service_types: Vec<ServiceTypeSummary> },
    ServiceGroups { groups: Vec<transit_analysis::ServiceGroup> },
    ServiceTypeDetail { detail: Option<ServiceTypeDetail> },
    ServiceTypeCompare { comparison: Option<ServiceTypeComparison> },
    TypeCoverage { layer: LayerReading, coverage: Option<CombinationCoverage> },
    StopGaps { gaps: transit_analysis::StopGaps },
    HourlyTrips { hours: Vec<transit_analysis::HourlyTrips> },
    ScheduleTexture { texture: Option<transit_analysis::ScheduleTexture> },
    RouteOverlap { overlaps: Vec<transit_analysis::RouteOverlap> },
    TimedTransfers { transfers: Vec<transit_analysis::TimedTransfer> },
    HubConnections { connections: Vec<transit_analysis::RouteConnections> },
    RouteFrequency { frequency: Option<transit_analysis::RouteFrequency> },
    UnservedPoints { layer: LayerReading, unserved: Option<transit_analysis::UnservedPlaces> },
    CoverageCompare { layer: LayerReading, comparison: CoverageDelta },
}

fn default_true() -> bool {
    true
}

fn parse_date(date: &str) -> Result<chrono::NaiveDate, String> {
    chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d")
        .map_err(|e| format!("invalid date {date:?}: {e}"))
}

fn with_network<T>(
    network_hash: &str,
    f: impl FnOnce(&Network) -> Result<T, String>,
) -> Result<T, String> {
    f(&*NETWORK.require(network_hash)?)
}

/// The registered document itself (for ops that compose across
/// registries).
fn network_arc(network_hash: &str) -> Result<Arc<Network>, String> {
    NETWORK.require(network_hash)
}

/// The typed dispatch (native hosts call this directly). Failures
/// are `{"error": ...}`, never panics — analysis is best-effort on
/// mid-edit documents, and unknown hashes are errors, not crashes.
pub fn dispatch(request: Request) -> Result<Response, String> {
    match request {
        Request::NetworkStats { network_hash, date, include_routes, include_stops, include_service_types } => {
            let parsed = parse_date(&date)?;
            with_network(&network_hash, |network| {
                let mut stats = network_stats(network, parsed);
                // Query shaping: the flags right-size the response AT
                // the op (the founding dogfood session read a 199 KiB
                // answer to a five-number question through a
                // hand-rolled summarizer — the missing piece was this
                // parameter, not a script).
                if !include_routes {
                    stats.routes.clear();
                }
                if !include_stops {
                    stats.stops.clear();
                }
                if !include_service_types {
                    stats.service_types.clear();
                }
                Ok(Response::NetworkStats { stats })
            })
        }
        Request::ServiceTypesOverview { network_hash } => with_network(&network_hash, |network| {
            Ok(Response::ServiceTypesOverview {
                service_types: service_type_overview(network),
            })
        }),
        Request::ServiceGroups { network_hash } => with_network(&network_hash, |network| {
            Ok(Response::ServiceGroups {
                groups: transit_analysis::service_groups(network),
            })
        }),
        Request::ServiceTypeDetail { network_hash, members } => {
            with_network(&network_hash, |network| {
                // Unknown ids answer empty (QAF-19's convention);
                // only hash addresses error.
                Ok(Response::ServiceTypeDetail { detail: service_type_detail(network, &members) })
            })
        }
        Request::ServiceTypeCompare { base_hash, network_hash, members } => {
            let base = network_arc(&base_hash)?;
            let proposal = network_arc(&network_hash)?;
            // Unknown ids answer empty (QAF-19's convention);
            // only hash addresses error.
            let comparison = service_type_compare(&base, &proposal, &members);
            Ok(Response::ServiceTypeCompare { comparison })
        }
        Request::TypeCoverage { network_hash, layer_hash, members, field, label, radius_m, frequent_secs, min_departures, window } => {
            let network_arc = network_arc(&network_hash)?;
            let layer_arc = LAYER.require(&layer_hash)?;
            let weights = layer_arc.weights(field.as_deref(), label.as_deref())?;
            Ok(Response::TypeCoverage {
                // Members that never run together answer null, as the
                // other pooled reads do.
                coverage: type_coverage(&network_arc, &weights, &members, radius_m, frequent_secs, min_departures, window),
                layer: weights.reading,
            })
        }
        Request::StopGaps { network_hash, date, stop_id } => {
            let parsed = parse_date(&date)?;
            let network_arc = network_arc(&network_hash)?;
            Ok(Response::StopGaps {
                gaps: transit_analysis::stop_gaps(&network_arc, parsed, &stop_id),
            })
        }
        Request::HourlyTrips { network_hash, date } => {
            let parsed = parse_date(&date)?;
            let network_arc = network_arc(&network_hash)?;
            Ok(Response::HourlyTrips {
                hours: transit_analysis::hourly_trips(&network_arc, parsed),
            })
        }
        Request::ScheduleTexture { network_hash, members } => {
            let network_arc = network_arc(&network_hash)?;
            Ok(Response::ScheduleTexture {
                texture: transit_analysis::schedule_texture(&network_arc, &members),
            })
        }
        Request::RouteOverlap { network_hash, date } => {
            let parsed = parse_date(&date)?;
            let network_arc = network_arc(&network_hash)?;
            Ok(Response::RouteOverlap {
                overlaps: transit_analysis::route_overlap(&network_arc, parsed),
            })
        }
        Request::HubConnections { network_hash, date, stop_id, window } => {
            let parsed = parse_date(&date)?;
            let network_arc = network_arc(&network_hash)?;
            Ok(Response::HubConnections {
                connections: transit_analysis::hub_connections(&network_arc, parsed, &stop_id, window),
            })
        }
        Request::RouteFrequency { network_hash, members } => {
            let network_arc = network_arc(&network_hash)?;
            Ok(Response::RouteFrequency {
                frequency: transit_analysis::route_frequency(&network_arc, &members),
            })
        }
        Request::TimedTransfers { network_hash, date, stop_id, window } => {
            let parsed = parse_date(&date)?;
            let network_arc = network_arc(&network_hash)?;
            Ok(Response::TimedTransfers {
                transfers: transit_analysis::timed_transfers(&network_arc, parsed, &stop_id, window),
            })
        }
        Request::UnservedPoints { network_hash, layer_hash, field, label, radius_m, members } => {
            let network_arc = network_arc(&network_hash)?;
            let layer_arc = LAYER.require(&layer_hash)?;
            let weights = layer_arc.weights(field.as_deref(), label.as_deref())?;
            Ok(Response::UnservedPoints {
                unserved: transit_analysis::unserved_points(&network_arc, &weights, &members, radius_m),
                layer: weights.reading,
            })
        }
        Request::CoverageCompare {
            base_hash,
            network_hash,
            layer_hash,
            field,
            label,
            base_members,
            members,
            radius_m,
            frequent_secs,
            min_departures,
            window,
        } => {
            let base = network_arc(&base_hash)?;
            let proposal = network_arc(&network_hash)?;
            let layer_arc = LAYER.require(&layer_hash)?;
            let weights = layer_arc.weights(field.as_deref(), label.as_deref())?;
            Ok(Response::CoverageCompare {
                comparison: coverage_compare(
                    &base,
                    &proposal,
                    &weights,
                    &base_members,
                    &members,
                    radius_m,
                    frequent_secs,
                    min_departures,
                    window,
                ),
                layer: weights.reading,
            })
        }
    }
}

/// The JSON seam: `{"op": ...}` in, `{"ok": ...}` / `{"error": ...}`
/// out — byte-compatible with the monolith and every other
/// capability module.
pub fn dispatch_json(text: &str) -> String {
    cap_abi::answer(text, dispatch)
}

/// The analysis capability has no bulk op of its own (a weighted
/// layer registers on the kernel's `load_layer`); the seam
/// answers honestly instead of silently ignoring one.
pub fn dispatch_bytes(op: u32, _bytes: &[u8]) -> String {
    cap_abi::outcome::<()>(Err(format!("unknown bulk op {op}: analysis has no bulk op of its own")))
}

cap_abi::capability! {
    id: "analysis",
    json: dispatch_json,
    bytes: dispatch_bytes,
    documents: [NETWORK, LAYER],
}
