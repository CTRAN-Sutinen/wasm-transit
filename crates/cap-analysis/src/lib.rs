//! cap-analysis: what do an authored network's numbers say? (OPS.md has
//! the ops.)
//!
//! The seam over transit-analysis. It reads the kernel's networks by
//! canonical hash, and weighted layers by the hash of their bytes.
//! `load_layer` answers a layer's features and the numeric fields that
//! a coverage op can read. A coverage op names the field and the planner's
//! label (D-97). A network another capability made is read by the hash its
//! maker returned.

#![forbid(unsafe_code)]

use std::sync::Arc;

use cap_abi::kernel::kind;
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
    /// The network's numbers on one service date. The optional include
    /// flags shape the response. The UI wants every route and stop inline.
    /// An agent that asks the day-shape question wants five numbers, not
    /// 199 KiB. Both default to true, so the UI shape is unchanged. `false`
    /// empties the corresponding list.
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
    /// Every service type's own numbers: its authored service, on no date.
    /// This is the per-id read under the service groups.
    ServiceTypesOverview { network_hash: String },
    /// The service groups (D-55): one row for each combination of service
    /// types that run together on the same dates. A row lists its members,
    /// the dates it holds on and their spans, and the members' pooled
    /// numbers (the service on each of those dates). Most days first.
    /// The axis a planner designs. The drill-down keys on `members`.
    ServiceGroups { network_hash: String },
    /// One combination (`members`, any order) expanded: its routes, stops,
    /// and schedules, pooled. Null for members that never run together on a
    /// date.
    ServiceTypeDetail { network_hash: String, members: Vec<String> },
    /// A proposal (`network_hash`) against its baseline (`base_hash`),
    /// under one combination. Both sides' totals over its members, with the
    /// dates it holds on each side. Also the routes whose numbers changed.
    /// Null when the members never run together in the proposal.
    ServiceTypeCompare { base_hash: String, network_hash: String, members: Vec<String> },
    /// Coverage of a weighted layer by one service combination
    /// (`members`, any order). A service_groups row gives the members. The
    /// members' pooled authored service stands in for a date. Their dates
    /// are returned beside it (D-55). Null when they never run together.
    /// `field` names the weight. Omitted, it is the layer's one numeric
    /// property. `label` says what it means. Omitted, it is the field.
    /// `window` judges frequency inside a time window ("every 15 min
    /// 07:00–19:00": the worst wait in the window fits frequent_secs).
    /// Omitted, it is the whole-day average headway over departures.
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
    /// One stop's day folded into headway gaps: departures, the max gap and
    /// the departure it follows, and the per-hour profile. The profile holds
    /// departures, and the max and median gap starting in the hour. It is the
    /// service-quality read of "how long until the next bus?". An unknown
    /// stop answers zero/None/empty. It never answers an error.
    StopGaps {
        network_hash: String,
        date: String,
        stop_id: String,
    },
    /// The day's service profile: trips per hour, bucketed by trip start.
    /// Each hour row carries the count for every running service type. Only
    /// hours with trips are rows. This answers the planner's texture question:
    /// where service is thick, and where it tapers.
    HourlyTrips {
        network_hash: String,
        date: String,
    },
    /// One combination's schedule texture: every pattern its members run.
    /// Each pattern gives the headway uniformity of the pooled starts
    /// (min/median/max gap). It also gives clockface alignment: starts on the
    /// quarter-hour grid. Null for members that never run together. Lookups
    /// answer empty, never error.
    ScheduleTexture {
        network_hash: String,
        members: Vec<String>,
    },
    /// The corridor read: every pair of routes that shares at least one stop
    /// on the date. It gives the shared stops and shared segments (same-order
    /// consecutive pairs). It also gives the shared stop where the pair's
    /// departures stack highest.
    RouteOverlap {
        network_hash: String,
        date: String,
    },
    /// The hub's timed-transfer view: arrival→departure pairs across routes
    /// within the connection window. The window is the caller's (`window`,
    /// for a hub with widely separated bays). Otherwise it is the 60–900 s
    /// transfer-point convention. Pairs are pooled over the stop's walking
    /// group: who can make what here. An unknown stop answers empty, never an
    /// error.
    TimedTransfers {
        network_hash: String,
        date: String,
        stop_id: String,
        #[serde(default)]
        #[ts(optional)]
        window: Option<transit_analysis::TransferWindow>,
    },
    /// The hub as a route-pair matrix: `timed_transfers` folded by (from route,
    /// to route). It gives connections, shortest and longest wait, and median
    /// wait (None below four). An unknown stop answers empty, never an error.
    HubConnections {
        network_hash: String,
        date: String,
        stop_id: String,
        #[serde(default)]
        #[ts(optional)]
        window: Option<transit_analysis::TransferWindow>,
    },
    /// The hub's just-missed connections. Over the stop's walking group, take
    /// an arrival of one route and a departure of another. The departure
    /// leaves from `miss_secs` (omitted: 300 s) before the arrival, up to the
    /// window's minimum after it. The rider sees it go. Each one gives that
    /// route's next departure that the rider can make. An unknown stop
    /// answers empty, never an error.
    MissedConnections {
        network_hash: String,
        date: String,
        stop_id: String,
        #[serde(default)]
        #[ts(optional)]
        window: Option<transit_analysis::TransferWindow>,
        #[serde(default)]
        #[ts(optional)]
        miss_secs: Option<u32>,
    },
    /// Where riders wait longest on the date. Every stop that departs is
    /// ranked by its longest wait. That is the whole day's largest gap, or the
    /// coverage reading of `window`. The list is cut to `limit`.
    LongestWaits {
        network_hash: String,
        date: String,
        #[serde(default)]
        #[ts(optional)]
        window: Option<CoverageWindow>,
        #[serde(default)]
        #[ts(optional)]
        limit: Option<u32>,
    },
    /// The day types of a period. These are the service combinations with a
    /// date in [`from`, `through`]. Each bound is optional and inclusive. The
    /// types are sorted by the days of the week their dates fall on. Each
    /// type's regular combination comes first. Then come those running on
    /// fewer of its days.
    DayTypes {
        network_hash: String,
        #[serde(default)]
        #[ts(optional)]
        from: Option<String>,
        #[serde(default)]
        #[ts(optional)]
        through: Option<String>,
    },
    /// The proposal against its baseline, across every day type of the period.
    /// It gives `day_types` of the proposal. Each group carries
    /// `service_type_compare` for its members.
    PlanCompare {
        base_hash: String,
        network_hash: String,
        #[serde(default)]
        #[ts(optional)]
        from: Option<String>,
        #[serde(default)]
        #[ts(optional)]
        through: Option<String>,
    },
    /// The worklist under one combination (`members`). It lists frequency
    /// holes, long waits, and costly near misses at the hubs. Each list is
    /// sorted and cut to `params.limit`. With `base_hash`, each finding comes
    /// with the baseline's finding under the same key. It also lists the
    /// baseline's findings that this version resolved. Null when the members
    /// never run together.
    NetworkFindings {
        network_hash: String,
        #[serde(default)]
        #[ts(optional)]
        base_hash: Option<String>,
        members: Vec<String>,
        #[serde(default)]
        #[ts(optional)]
        params: Option<transit_analysis::FindingParams>,
    },
    /// The service group's frequency grid. It gives every route that departs
    /// under the combination, hour by hour, at its headway stop. Each hour
    /// gives the departures. It also gives the shortest, longest, and median
    /// gap starting in the hour. The gaps are the trips' own, never
    /// regularized. Null for members that never run together.
    RouteFrequency {
        network_hash: String,
        members: Vec<String>,
    },
    /// The unserved where of coverage. It gives every layer point beyond the
    /// radius of every stop that one combination (`members`, any order)
    /// serves. Each point gives the nearest served stop and its distance.
    /// This answers "which part, and where", not just the share. A polygon
    /// answers its unserved part: its share, its centre, and its rings.
    /// Sorted by unserved weight, descending.
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
    /// A proposal's coverage against its base. Each side uses the service
    /// types that the host names: `base_members` of the base, and `members`
    /// of the proposal. Each side pools over its authored service. Each side
    /// gives its served and frequent weight. It also gives what the proposal
    /// gains and loses, the churn that a net change hides. The op never
    /// matches one plan's types to the other's (QAF-20).
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
    MissedConnections { missed: Vec<transit_analysis::MissedConnection> },
    LongestWaits { waits: Vec<transit_analysis::StopWait> },
    DayTypes { day_types: Vec<transit_analysis::DayType> },
    PlanCompare { day_types: Vec<transit_analysis::DayTypeComparison> },
    NetworkFindings { findings: Option<transit_analysis::NetworkFindings> },
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
    f(&*kind::<transit_network::Network>().require(network_hash)?)
}

/// The registered document itself (for ops that compose across
/// registries).
fn network_arc(network_hash: &str) -> Result<Arc<Network>, String> {
    kind::<transit_network::Network>().require(network_hash)
}

/// The typed dispatch. Native hosts call this directly. Failures are
/// `{"error": ...}`, and never panic. Analysis is best-effort on mid-edit
/// documents. Unknown hashes are errors, not crashes.
pub fn dispatch(request: Request) -> Result<Response, String> {
    match request {
        Request::NetworkStats { network_hash, date, include_routes, include_stops, include_service_types } => {
            let parsed = parse_date(&date)?;
            with_network(&network_hash, |network| {
                let mut stats = network_stats(network, parsed);
                // Query shaping: the flags right-size the response at the op.
                // A dogfood session read a 199 KiB answer to a five-number
                // question. It used a hand-rolled summarizer. The missing
                // piece was this parameter, not a script.
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
                // Unknown ids answer empty (QAF-19's convention). Only
                // hash addresses error.
                Ok(Response::ServiceTypeDetail { detail: service_type_detail(network, &members) })
            })
        }
        Request::ServiceTypeCompare { base_hash, network_hash, members } => {
            let base = network_arc(&base_hash)?;
            let proposal = network_arc(&network_hash)?;
            // Unknown ids answer empty (QAF-19's convention). Only
            // hash addresses error.
            let comparison = service_type_compare(&base, &proposal, &members);
            Ok(Response::ServiceTypeCompare { comparison })
        }
        Request::TypeCoverage { network_hash, layer_hash, members, field, label, radius_m, frequent_secs, min_departures, window } => {
            let network_arc = network_arc(&network_hash)?;
            let layer_arc = kind::<transit_analysis::WeightedLayer>().require(&layer_hash)?;
            let weights = layer_arc.weights(field.as_deref(), label.as_deref())?;
            Ok(Response::TypeCoverage {
                // Members that never run together answer null, as other
                // pooled reads do.
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
        Request::MissedConnections { network_hash, date, stop_id, window, miss_secs } => {
            let parsed = parse_date(&date)?;
            let network_arc = network_arc(&network_hash)?;
            Ok(Response::MissedConnections {
                missed: transit_analysis::missed_connections(&network_arc, parsed, &stop_id, window, miss_secs),
            })
        }
        Request::LongestWaits { network_hash, date, window, limit } => {
            let parsed = parse_date(&date)?;
            let network_arc = network_arc(&network_hash)?;
            Ok(Response::LongestWaits {
                waits: transit_analysis::longest_waits(&network_arc, parsed, window, limit),
            })
        }
        Request::DayTypes { network_hash, from, through } => {
            let (from, through) = (from.as_deref().map(parse_date).transpose()?, through.as_deref().map(parse_date).transpose()?);
            let network_arc = network_arc(&network_hash)?;
            Ok(Response::DayTypes { day_types: transit_analysis::day_types(&network_arc, from, through) })
        }
        Request::PlanCompare { base_hash, network_hash, from, through } => {
            let (from, through) = (from.as_deref().map(parse_date).transpose()?, through.as_deref().map(parse_date).transpose()?);
            let base = network_arc(&base_hash)?;
            let proposal = network_arc(&network_hash)?;
            Ok(Response::PlanCompare { day_types: transit_analysis::plan_compare(&base, &proposal, from, through) })
        }
        Request::NetworkFindings { network_hash, base_hash, members, params } => {
            let base = base_hash.as_deref().map(network_arc).transpose()?;
            let net = network_arc(&network_hash)?;
            Ok(Response::NetworkFindings {
                findings: transit_analysis::network_findings(&net, base.as_deref(), &members, &params.unwrap_or_default()),
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
            let layer_arc = kind::<transit_analysis::WeightedLayer>().require(&layer_hash)?;
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
            let layer_arc = kind::<transit_analysis::WeightedLayer>().require(&layer_hash)?;
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

/// The JSON seam: `{"op": ...}` in, and `{"ok": ...}` or
/// `{"error": ...}` out. Its bytes match the monolith and every other
/// capability module.
pub fn dispatch_json(text: &str) -> String {
    cap_abi::answer(text, dispatch)
}

/// The analysis capability has no bulk op of its own. A weighted layer
/// registers on the kernel's `load_layer`. The seam answers with an error,
/// and does not silently ignore a bulk op.
pub fn dispatch_bytes(op: u32, _bytes: &[u8]) -> String {
    cap_abi::outcome::<()>(Err(format!("unknown bulk op {op}: analysis has no bulk op of its own")))
}

/// The worked examples. The generator runs them. No module build turns
/// the feature on.
#[cfg(feature = "examples")]
pub mod examples;

cap_abi::capability! {
    id: "analysis",
    json: dispatch_json,
    bytes: dispatch_bytes,
    documents: [transit_network::Network, transit_analysis::WeightedLayer],
    request: Request,
    consumes: &[("network", "network")],
    speaks: &[("network", &[transit_network::SCHEMA_VERSION])],
}
