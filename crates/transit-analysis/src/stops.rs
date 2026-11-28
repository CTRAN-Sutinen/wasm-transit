use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use transit_date::Date;
#[cfg(feature = "bindings")]
use ts_rs::TS;

use crate::Network;

/// The hour is the service-day hour, 0..47. A gap belongs to the hour it starts
/// in.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "bindings", derive(TS))]
#[cfg_attr(feature = "bindings", ts(export))]
pub struct StopHourGaps {
    pub hour: u32,
    pub departures: u32,
    pub max_gap_secs: Option<u32>,
    pub median_gap_secs: Option<u32>,
}

/// Two routes that leave together make no gap, and `departures` counts both.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "bindings", derive(TS))]
#[cfg_attr(feature = "bindings", ts(export))]
pub struct StopGaps {
    pub stop_id: String,
    pub name: String,
    pub departures: u32,
    pub first_departure_secs: Option<u32>,
    pub last_departure_secs: Option<u32>,
    /// None below two distinct departures.
    pub max_gap_secs: Option<u32>,
    pub max_gap_after_secs: Option<u32>,
    pub hourly: Vec<StopHourGaps>,
}

pub fn stop_gaps(net: &Network, date: Date, stop_id: &str) -> StopGaps {
    let pooled: BTreeSet<&str> = std::iter::once(stop_id)
        .chain(
            net.stops
                .iter()
                .filter(|(_, s)| s.parent.as_deref() == Some(stop_id))
                .map(|(id, _)| id.as_str()),
        )
        .collect();
    let mut departures: Vec<u32> = Vec::new();
    for (service_id, service) in &net.pattern_trips {
        if !net
            .service_types
            .get(&service.service_type)
            .is_some_and(|st| st.rule.runs_on(date))
        {
            continue;
        }
        let Ok(materialized) = transit_network::materialize_service(net, service_id) else {
            continue;
        };
        for trip in materialized {
            for st in &trip.stop_times[..trip.stop_times.len() - 1] {
                if pooled.contains(st.stop.as_str()) {
                    departures.push(st.departure_secs);
                }
            }
        }
    }
    let distinct: BTreeSet<u32> = departures.iter().copied().collect();
    let spine: Vec<u32> = distinct.into_iter().collect();
    let mut max_gap = None;
    let mut max_after = None;
    let mut gaps_by_hour: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
    for w in spine.windows(2) {
        let gap = w[1] - w[0];
        if max_gap.map_or(true, |m| gap > m) {
            max_gap = Some(gap);
            max_after = Some(w[0]);
        }
        gaps_by_hour.entry(w[0] / 3600).or_default().push(gap);
    }
    let departures_by_hour: BTreeMap<u32, u32> =
        departures
            .iter()
            .copied()
            .fold(BTreeMap::new(), |mut m, d| {
                *m.entry(d / 3600).or_default() += 1;
                m
            });
    let mut hours: BTreeSet<u32> = departures_by_hour.keys().copied().collect();
    hours.extend(gaps_by_hour.keys().copied());
    let hourly = hours
        .into_iter()
        .map(|hour| {
            let mut gaps = gaps_by_hour.get(&hour).cloned().unwrap_or_default();
            gaps.sort_unstable();
            StopHourGaps {
                hour,
                departures: departures_by_hour.get(&hour).copied().unwrap_or(0),
                max_gap_secs: gaps.last().copied(),
                median_gap_secs: (gaps.len() >= 4).then(|| gaps[gaps.len() / 2]),
            }
        })
        .collect();
    StopGaps {
        stop_id: stop_id.to_string(),
        name: net
            .stops
            .get(stop_id)
            .map(|s| s.name.clone())
            .unwrap_or_default(),
        departures: departures.len() as u32,
        first_departure_secs: spine.first().copied(),
        last_departure_secs: spine.last().copied(),
        max_gap_secs: max_gap,
        max_gap_after_secs: max_after,
        hourly,
    }
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "bindings", derive(TS))]
#[cfg_attr(feature = "bindings", ts(export))]
pub struct StopWait {
    pub stop_id: String,
    pub name: String,
    pub departures: u32,
    pub longest_wait_secs: u32,
    pub wait_from_secs: u32,
}
