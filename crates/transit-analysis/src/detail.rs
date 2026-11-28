use std::collections::BTreeSet;

use serde::Serialize;
#[cfg(feature = "bindings")]
use ts_rs::TS;

use crate::{PatternTripsSummary, StopStats, RouteStats, Network, family_stats};

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "bindings", derive(TS))]
#[cfg_attr(feature = "bindings", ts(export))]
pub struct ServiceTypeDetail {
    pub group: String,
    pub members: Vec<String>,
    pub trips: u32,
    pub revenue_hours: f64,
    pub revenue_miles: f64,
    pub transfer_stops: u32,
    pub routes: Vec<RouteStats>,
    pub stops: Vec<StopStats>,
    pub pattern_trips: Vec<PatternTripsSummary>,
}

/// `None` when the members never run together on a date.
pub fn service_type_detail(net: &Network, members: &[String]) -> Option<ServiceTypeDetail> {
    let combination = transit_network::service_combination(net, members)?;
    Some(pooled_detail(net, &combination.members))
}

/// A baseline that the combination is new to still runs what its members run
/// there.
pub(crate) fn pooled_detail(net: &Network, members: &[String]) -> ServiceTypeDetail {
    let group: BTreeSet<String> = members.iter().cloned().collect();
    let stats = family_stats(net, &group);

    let mut pattern_trips = Vec::new();
    for (id, entry) in &net.pattern_trips {
        if !group.contains(&entry.service_type) {
            continue;
        }
        pattern_trips.push(PatternTripsSummary {
            pattern_trips: id.clone(),
            pattern: entry.pattern.clone(),
            pattern_name: net
                .patterns
                .get(&entry.pattern)
                .map(|p| p.name.clone())
                .unwrap_or_else(|| entry.pattern.clone()),
            trips: entry.trips.iter().map(|t| t.departs.instances().len()).sum::<usize>() as u32,
        });
    }

    ServiceTypeDetail {
        group: group.iter().cloned().collect::<Vec<_>>().join(" + "),
        members: group.into_iter().collect(),
        trips: stats.trips,
        revenue_hours: stats.revenue_hours,
        revenue_miles: stats.revenue_miles,
        transfer_stops: stats.transfer_stops,
        routes: stats.routes.into_iter().filter(|r| r.trips > 0).collect(),
        stops: stats.stops.into_iter().filter(|s| s.departures > 0).collect(),
        pattern_trips,
    }
}

