//! cap-analysis wraps transit-analysis (ops in `OPS.md`). It reads networks by
//! canonical hash and weighted layers by the hash of their bytes. A coverage
//! op names the weight field and the planner's label (D-97).

#![forbid(unsafe_code)]

use std::sync::Arc;

use cap_abi::kernel::kind;
use serde::{Deserialize, Serialize};
use transit_analysis::{
    coverage_compare, network_stats, service_type_compare, service_type_detail,
    service_type_overview, type_coverage, CombinationCoverage, CoverageDelta, CoverageWindow,
    LayerReading, NetworkStats, ServiceTypeComparison, ServiceTypeDetail, ServiceTypeSummary,
};
use transit_network::Network;
#[cfg(feature = "bindings")]
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS))]
#[cfg_attr(feature = "bindings", ts(export, export_to = "cap_analysis/"))]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Request {
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
    ServiceTypesOverview {
        network_hash: String,
    },
    ServiceGroups {
        network_hash: String,
    },
    ServiceTypeDetail {
        network_hash: String,
        members: Vec<String>,
    },
    ServiceTypeCompare {
        base_hash: String,
        network_hash: String,
        members: Vec<String>,
    },
    TypeCoverage {
        network_hash: String,
        layer_hash: String,
        members: Vec<String>,
        #[serde(default)]
        #[cfg_attr(feature = "bindings", ts(optional))]
        field: Option<String>,
        #[serde(default)]
        #[cfg_attr(feature = "bindings", ts(optional))]
        label: Option<String>,
        radius_m: f64,
        frequent_secs: u32,
        min_departures: u32,
        #[serde(default)]
        #[cfg_attr(feature = "bindings", ts(optional))]
        window: Option<CoverageWindow>,
    },
    StopGaps {
        network_hash: String,
        date: String,
        stop_id: String,
    },
    HourlyTrips {
        network_hash: String,
        date: String,
    },
    ScheduleTexture {
        network_hash: String,
        members: Vec<String>,
    },
    RouteOverlap {
        network_hash: String,
        date: String,
    },
    TimedTransfers {
        network_hash: String,
        date: String,
        stop_id: String,
        #[serde(default)]
        #[cfg_attr(feature = "bindings", ts(optional))]
        window: Option<transit_analysis::TransferWindow>,
    },
    HubConnections {
        network_hash: String,
        date: String,
        stop_id: String,
        #[serde(default)]
        #[cfg_attr(feature = "bindings", ts(optional))]
        window: Option<transit_analysis::TransferWindow>,
    },
    MissedConnections {
        network_hash: String,
        date: String,
        stop_id: String,
        #[serde(default)]
        #[cfg_attr(feature = "bindings", ts(optional))]
        window: Option<transit_analysis::TransferWindow>,
        #[serde(default)]
        #[cfg_attr(feature = "bindings", ts(optional))]
        miss_secs: Option<u32>,
    },
    LongestWaits {
        network_hash: String,
        date: String,
        #[serde(default)]
        #[cfg_attr(feature = "bindings", ts(optional))]
        window: Option<CoverageWindow>,
        #[serde(default)]
        #[cfg_attr(feature = "bindings", ts(optional))]
        limit: Option<u32>,
    },
    DayTypes {
        network_hash: String,
        #[serde(default)]
        #[cfg_attr(feature = "bindings", ts(optional))]
        from: Option<String>,
        #[serde(default)]
        #[cfg_attr(feature = "bindings", ts(optional))]
        through: Option<String>,
    },
    PlanCompare {
        base_hash: String,
        network_hash: String,
        #[serde(default)]
        #[cfg_attr(feature = "bindings", ts(optional))]
        from: Option<String>,
        #[serde(default)]
        #[cfg_attr(feature = "bindings", ts(optional))]
        through: Option<String>,
    },
    NetworkFindings {
        network_hash: String,
        #[serde(default)]
        #[cfg_attr(feature = "bindings", ts(optional))]
        base_hash: Option<String>,
        members: Vec<String>,
        #[serde(default)]
        #[cfg_attr(feature = "bindings", ts(optional))]
        params: Option<transit_analysis::FindingParams>,
    },
    RouteFrequency {
        network_hash: String,
        members: Vec<String>,
    },
    UnservedPoints {
        network_hash: String,
        layer_hash: String,
        #[serde(default)]
        #[cfg_attr(feature = "bindings", ts(optional))]
        field: Option<String>,
        #[serde(default)]
        #[cfg_attr(feature = "bindings", ts(optional))]
        label: Option<String>,
        radius_m: f64,
        members: Vec<String>,
    },
    /// The op never matches one plan's service types to the other's (QAF-20).
    CoverageCompare {
        base_hash: String,
        network_hash: String,
        layer_hash: String,
        #[serde(default)]
        #[cfg_attr(feature = "bindings", ts(optional))]
        field: Option<String>,
        #[serde(default)]
        #[cfg_attr(feature = "bindings", ts(optional))]
        label: Option<String>,
        base_members: Vec<String>,
        members: Vec<String>,
        radius_m: f64,
        frequent_secs: u32,
        min_departures: u32,
        #[serde(default)]
        #[cfg_attr(feature = "bindings", ts(optional))]
        window: Option<CoverageWindow>,
    },
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "bindings", derive(TS))]
#[cfg_attr(feature = "bindings", ts(export, export_to = "cap_analysis/"))]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Response {
    NetworkStats {
        stats: NetworkStats,
    },
    ServiceTypesOverview {
        service_types: Vec<ServiceTypeSummary>,
    },
    ServiceGroups {
        groups: Vec<transit_analysis::ServiceGroup>,
    },
    ServiceTypeDetail {
        detail: Option<ServiceTypeDetail>,
    },
    ServiceTypeCompare {
        comparison: Option<ServiceTypeComparison>,
    },
    TypeCoverage {
        layer: LayerReading,
        coverage: Option<CombinationCoverage>,
    },
    StopGaps {
        gaps: transit_analysis::StopGaps,
    },
    HourlyTrips {
        hours: Vec<transit_analysis::HourlyTrips>,
    },
    ScheduleTexture {
        texture: Option<transit_analysis::ScheduleTexture>,
    },
    RouteOverlap {
        overlaps: Vec<transit_analysis::RouteOverlap>,
    },
    TimedTransfers {
        transfers: Vec<transit_analysis::TimedTransfer>,
    },
    HubConnections {
        connections: Vec<transit_analysis::RouteConnections>,
    },
    MissedConnections {
        missed: Vec<transit_analysis::MissedConnection>,
    },
    LongestWaits {
        waits: Vec<transit_analysis::StopWait>,
    },
    DayTypes {
        day_types: Vec<transit_analysis::DayType>,
    },
    PlanCompare {
        day_types: Vec<transit_analysis::DayTypeComparison>,
    },
    NetworkFindings {
        findings: Option<transit_analysis::NetworkFindings>,
    },
    RouteFrequency {
        frequency: Option<transit_analysis::RouteFrequency>,
    },
    UnservedPoints {
        layer: LayerReading,
        unserved: Option<transit_analysis::UnservedPlaces>,
    },
    CoverageCompare {
        layer: LayerReading,
        comparison: CoverageDelta,
    },
}

fn default_true() -> bool {
    true
}

fn with_network<T>(
    network_hash: &str,
    f: impl FnOnce(&Network) -> Result<T, String>,
) -> Result<T, cap_abi::Refusal> {
    Ok(f(
        &*kind::<transit_network::Network>().require(network_hash)?
    )?)
}

fn network_arc(network_hash: &str) -> Result<Arc<Network>, cap_abi::Refusal> {
    kind::<transit_network::Network>().require(network_hash)
}

pub fn dispatch(request: Request) -> Result<Response, cap_abi::Refusal> {
    match request {
        Request::NetworkStats {
            network_hash,
            date,
            include_routes,
            include_stops,
            include_service_types,
        } => {
            let parsed = transit_date::Date::read(&date)?;
            with_network(&network_hash, |network| {
                let mut stats = network_stats(network, parsed);
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
        Request::ServiceTypeDetail {
            network_hash,
            members,
        } => with_network(&network_hash, |network| {
            Ok(Response::ServiceTypeDetail {
                detail: service_type_detail(network, &members),
            })
        }),
        Request::ServiceTypeCompare {
            base_hash,
            network_hash,
            members,
        } => {
            let base = network_arc(&base_hash)?;
            let proposal = network_arc(&network_hash)?;
            let comparison = service_type_compare(&base, &proposal, &members);
            Ok(Response::ServiceTypeCompare { comparison })
        }
        Request::TypeCoverage {
            network_hash,
            layer_hash,
            members,
            field,
            label,
            radius_m,
            frequent_secs,
            min_departures,
            window,
        } => {
            let network_arc = network_arc(&network_hash)?;
            let layer_arc = kind::<transit_analysis::WeightedLayer>().require(&layer_hash)?;
            let weights = layer_arc.weights(field.as_deref(), label.as_deref())?;
            Ok(Response::TypeCoverage {
                coverage: type_coverage(
                    &network_arc,
                    &weights,
                    &members,
                    radius_m,
                    frequent_secs,
                    min_departures,
                    window,
                ),
                layer: weights.reading,
            })
        }
        Request::StopGaps {
            network_hash,
            date,
            stop_id,
        } => {
            let parsed = transit_date::Date::read(&date)?;
            let network_arc = network_arc(&network_hash)?;
            Ok(Response::StopGaps {
                gaps: transit_analysis::stop_gaps(&network_arc, parsed, &stop_id),
            })
        }
        Request::HourlyTrips { network_hash, date } => {
            let parsed = transit_date::Date::read(&date)?;
            let network_arc = network_arc(&network_hash)?;
            Ok(Response::HourlyTrips {
                hours: transit_analysis::hourly_trips(&network_arc, parsed),
            })
        }
        Request::ScheduleTexture {
            network_hash,
            members,
        } => {
            let network_arc = network_arc(&network_hash)?;
            Ok(Response::ScheduleTexture {
                texture: transit_analysis::schedule_texture(&network_arc, &members),
            })
        }
        Request::RouteOverlap { network_hash, date } => {
            let parsed = transit_date::Date::read(&date)?;
            let network_arc = network_arc(&network_hash)?;
            Ok(Response::RouteOverlap {
                overlaps: transit_analysis::route_overlap(&network_arc, parsed),
            })
        }
        Request::HubConnections {
            network_hash,
            date,
            stop_id,
            window,
        } => {
            let parsed = transit_date::Date::read(&date)?;
            let network_arc = network_arc(&network_hash)?;
            Ok(Response::HubConnections {
                connections: transit_analysis::hub_connections(
                    &network_arc,
                    parsed,
                    &stop_id,
                    window,
                ),
            })
        }
        Request::MissedConnections {
            network_hash,
            date,
            stop_id,
            window,
            miss_secs,
        } => {
            let parsed = transit_date::Date::read(&date)?;
            let network_arc = network_arc(&network_hash)?;
            Ok(Response::MissedConnections {
                missed: transit_analysis::missed_connections(
                    &network_arc,
                    parsed,
                    &stop_id,
                    window,
                    miss_secs,
                ),
            })
        }
        Request::LongestWaits {
            network_hash,
            date,
            window,
            limit,
        } => {
            let parsed = transit_date::Date::read(&date)?;
            let network_arc = network_arc(&network_hash)?;
            Ok(Response::LongestWaits {
                waits: transit_analysis::longest_waits(&network_arc, parsed, window, limit),
            })
        }
        Request::DayTypes {
            network_hash,
            from,
            through,
        } => {
            let (from, through) = (
                from.as_deref().map(transit_date::Date::read).transpose()?,
                through
                    .as_deref()
                    .map(transit_date::Date::read)
                    .transpose()?,
            );
            let network_arc = network_arc(&network_hash)?;
            Ok(Response::DayTypes {
                day_types: transit_analysis::day_types(&network_arc, from, through),
            })
        }
        Request::PlanCompare {
            base_hash,
            network_hash,
            from,
            through,
        } => {
            let (from, through) = (
                from.as_deref().map(transit_date::Date::read).transpose()?,
                through
                    .as_deref()
                    .map(transit_date::Date::read)
                    .transpose()?,
            );
            let base = network_arc(&base_hash)?;
            let proposal = network_arc(&network_hash)?;
            Ok(Response::PlanCompare {
                day_types: transit_analysis::plan_compare(&base, &proposal, from, through),
            })
        }
        Request::NetworkFindings {
            network_hash,
            base_hash,
            members,
            params,
        } => {
            let base = base_hash.as_deref().map(network_arc).transpose()?;
            let net = network_arc(&network_hash)?;
            Ok(Response::NetworkFindings {
                findings: transit_analysis::network_findings(
                    &net,
                    base.as_deref(),
                    &members,
                    &params.unwrap_or_default(),
                ),
            })
        }
        Request::RouteFrequency {
            network_hash,
            members,
        } => {
            let network_arc = network_arc(&network_hash)?;
            Ok(Response::RouteFrequency {
                frequency: transit_analysis::route_frequency(&network_arc, &members),
            })
        }
        Request::TimedTransfers {
            network_hash,
            date,
            stop_id,
            window,
        } => {
            let parsed = transit_date::Date::read(&date)?;
            let network_arc = network_arc(&network_hash)?;
            Ok(Response::TimedTransfers {
                transfers: transit_analysis::timed_transfers(
                    &network_arc,
                    parsed,
                    &stop_id,
                    window,
                ),
            })
        }
        Request::UnservedPoints {
            network_hash,
            layer_hash,
            field,
            label,
            radius_m,
            members,
        } => {
            let network_arc = network_arc(&network_hash)?;
            let layer_arc = kind::<transit_analysis::WeightedLayer>().require(&layer_hash)?;
            let weights = layer_arc.weights(field.as_deref(), label.as_deref())?;
            Ok(Response::UnservedPoints {
                unserved: transit_analysis::unserved_points(
                    &network_arc,
                    &weights,
                    &members,
                    radius_m,
                ),
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

pub fn dispatch_json(text: &str) -> String {
    cap_abi::answer(text, dispatch)
}

pub fn dispatch_bytes(op: u32, _bytes: &[u8]) -> String {
    cap_abi::outcome::<(), String>(Err(format!(
        "unknown bulk op {op}: analysis has no bulk op of its own"
    )))
}

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
