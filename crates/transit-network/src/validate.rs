//! Network invariants.
//!
//! `validation_issues` reports every problem a network document can
//! carry — referential integrity, pattern/service/calendar/block
//! structure — so an authoring UI can show the whole list at once.
//! `validate` gates on the first. `Ok(())` is the contract
//! `materialize` relies on: it cannot fail on a validated network.

use std::collections::{BTreeMap, BTreeSet};

use crate::error::NetworkError;
use crate::materialize::{trip_arrival_end_secs, trip_starts};
use crate::model::{Network, Schedule};

/// All issues, in a deterministic order (entity map order, then
/// check order). Empty = valid.
pub fn validation_issues(network: &Network) -> Vec<NetworkError> {
    let mut issues = Vec::new();

    // --- Stops: parent references -------------------------------
    for (stop_id, stop) in &network.stops {
        if let Some(parent) = &stop.parent {
            if !network.stops.contains_key(parent) {
                issues.push(NetworkError::UnknownParentStop {
                    stop: stop_id.clone(),
                    parent: parent.clone(),
                });
            }
        }
    }

    // --- Patterns ------------------------------------------------
    for (pattern_id, pattern) in &network.patterns {
        if network.routes.get(&pattern.route).is_none() {
            issues.push(NetworkError::UnknownRoute {
                pattern: pattern_id.clone(),
                route: pattern.route.clone(),
            });
        }
        if pattern.stops.len() < 2 {
            issues.push(NetworkError::PatternTooShort {
                pattern: pattern_id.clone(),
            });
            continue; // the remaining checks need two stops
        }
        if !pattern.stops.first().map(|s| s.timepoint).unwrap_or(false)
            || !pattern.stops.last().map(|s| s.timepoint).unwrap_or(false)
        {
            issues.push(NetworkError::EndpointsMustBeTimepoints {
                pattern: pattern_id.clone(),
            });
        }
        for ps in &pattern.stops {
            if !network.stops.contains_key(&ps.stop) {
                issues.push(NetworkError::UnknownStop {
                    pattern: pattern_id.clone(),
                    stop: ps.stop.clone(),
                });
            }
        }
        if pattern
            .stops
            .windows(2)
            .any(|w| w[1].distance_m < w[0].distance_m)
        {
            issues.push(NetworkError::DecreasingDistances {
                pattern: pattern_id.clone(),
            });
        }
        if pattern.timing.is_empty() {
            issues.push(NetworkError::NoTimingProfile {
                pattern: pattern_id.clone(),
            });
        }
        let tp_count = pattern.timepoint_positions().len();
        for (pi, profile) in pattern.timing.iter().enumerate() {
            if profile.start_secs >= profile.end_secs {
                issues.push(NetworkError::EmptyTimingProfile {
                    pattern: pattern_id.clone(),
                    profile: pi,
                });
            }
            if profile.at_timepoints.len() != tp_count {
                issues.push(NetworkError::TimepointCountMismatch {
                    pattern: pattern_id.clone(),
                    profile: pi,
                    expected: tp_count,
                    got: profile.at_timepoints.len(),
                });
            } else {
                if profile.at_timepoints.first() != Some(&0) {
                    issues.push(NetworkError::FirstTimepointNotZero {
                        pattern: pattern_id.clone(),
                        profile: pi,
                    });
                }
                if profile
                    .at_timepoints
                    .windows(2)
                    .any(|w| w[1] < w[0])
                {
                    issues.push(NetworkError::NonMonotonicTimes {
                        pattern: pattern_id.clone(),
                        profile: pi,
                    });
                }
            }
        }
        if overlapping(&pattern.timing.iter().map(|p| (p.start_secs, p.end_secs)).collect::<Vec<_>>()) {
            issues.push(NetworkError::OverlappingTimingProfiles {
                pattern: pattern_id.clone(),
            });
        }
    }

    // --- Transfer points ---------------------------------------------
    // A transfer flag is composed intent: what matters is the routes
    // meeting across the stop's WALKING GROUP (see [`walking_group`]
    // — transfers happen across a street or down a block, not only
    // at one pin). Fewer than two routes in the group means the
    // flag is meaningless.
    {
        let index = stop_route_index(network);
        for (stop_id, stop) in &network.stops {
            if !stop.transfer {
                continue;
            }
            if routes_meeting_at(network, &index, stop_id).len() < 2 {
                issues.push(NetworkError::TransferPointWithoutConnections { stop: stop_id.clone() });
            }
        }
    }

    // --- Service types ----------------------------------------------
    for (service_type_id, service_type) in &network.service_types {
        for (i, r) in service_type.rule.ranges.iter().enumerate() {
            if r.start > r.end {
                issues.push(NetworkError::BadDateRange {
                    service_type: service_type_id.clone(),
                    index: i,
                });
            }
        }
    }

    // --- Services --------------------------------------------------
    for (schedule_id, service) in &network.schedules {
        if network.patterns.get(&service.pattern).is_none() {
            issues.push(NetworkError::UnknownPattern {
                schedule: schedule_id.clone(),
                pattern: service.pattern.clone(),
            });
        }
        if network.service_types.get(&service.service_type).is_none() {
            issues.push(NetworkError::UnknownServiceType {
                schedule: schedule_id.clone(),
                service_type: service.service_type.clone(),
            });
        }
        match &service.schedule {
            Schedule::HeadwayBands { bands } => {
                if bands.is_empty() {
                    issues.push(NetworkError::EmptySchedule {
                        schedule: schedule_id.clone(),
                    });
                }
                for (bi, band) in bands.iter().enumerate() {
                    if band.headway_secs == 0 {
                        issues.push(NetworkError::ZeroHeadway {
                            schedule: schedule_id.clone(),
                            band: bi,
                        });
                    }
                    if band.start_secs >= band.end_secs {
                        issues.push(NetworkError::EmptyHeadwayBand {
                            schedule: schedule_id.clone(),
                            band: bi,
                        });
                    }
                }
                if overlapping(&bands.iter().map(|b| (b.start_secs, b.end_secs)).collect::<Vec<_>>()) {
                    issues.push(NetworkError::OverlappingHeadwayBands {
                        schedule: schedule_id.clone(),
                    });
                }
            }
            Schedule::ExactTimes { starts } => {
                if starts.is_empty() {
                    issues.push(NetworkError::EmptySchedule {
                        schedule: schedule_id.clone(),
                    });
                } else if starts.windows(2).any(|w| w[1] <= w[0]) {
                    issues.push(NetworkError::UnsortedExactTimes {
                        schedule: schedule_id.clone(),
                    });
                }
            }
        }
        // Timing coverage: every generated trip start must fall in
        // exactly one timing profile of the pattern (bands are
        // non-overlapping above, so "at least one" is "exactly one").
        if let Some(pattern) = network.patterns.get(&service.pattern) {
            for start in trip_starts(&service.schedule) {
                if pattern.profile_for(start).is_none() {
                    issues.push(NetworkError::TripOutsideTimingProfiles {
                        schedule: schedule_id.clone(),
                        pattern: service.pattern.clone(),
                        start_secs: start,
                    });
                }
            }
        }
    }

    // --- Blocks (the runcut) --------------------------------------
    let mut owner: BTreeMap<(String, u32), String> = BTreeMap::new();
    for (block_id, block) in &network.blocks {
        if block.trips.is_empty() {
            issues.push(NetworkError::EmptyBlock {
                block: block_id.clone(),
            });
            continue;
        }
        if block.trips.windows(2).any(|w| w[1].start_secs <= w[0].start_secs) {
            issues.push(NetworkError::BlockOutOfOrder {
                block: block_id.clone(),
            });
        }
        for t in &block.trips {
            if network.schedules.get(&t.schedule).is_none() {
                issues.push(NetworkError::UnknownBlockSchedule {
                    block: block_id.clone(),
                    schedule: t.schedule.clone(),
                });
            }
            match owner.get(&(t.schedule.clone(), t.start_secs)) {
                Some(prev) => issues.push(NetworkError::DuplicateBlockTrip {
                    schedule: t.schedule.clone(),
                    start_secs: t.start_secs,
                    a: prev.clone(),
                    b: block_id.clone(),
                }),
                None => {
                    owner.insert((t.schedule.clone(), t.start_secs), block_id.clone());
                }
            }
        }
        for layover in &block.layovers {
            if layover.after_trip + 1 >= block.trips.len() {
                issues.push(NetworkError::LayoverAfterLastTrip {
                    block: block_id.clone(),
                    after_trip: layover.after_trip,
                });
            }
            if let Some(at) = &layover.at {
                if !network.stops.contains_key(at) {
                    issues.push(NetworkError::UnknownLayoverStop {
                        block: block_id.clone(),
                        stop: at.clone(),
                    });
                }
            }
        }
        // Feasibility: the vehicle must physically make each trip
        // connection — arrive before the next trip departs, with the
        // layovers in between accounted for.
        for i in 0..block.trips.len().saturating_sub(1) {
            let Ok(arrival) = trip_arrival_end_secs(network, &block.trips[i].schedule, block.trips[i].start_secs)
            else {
                continue; // broken refs are already flagged above
            };
            let next_start = block.trips[i + 1].start_secs;
            let layover: u32 = block
                .layovers
                .iter()
                .filter(|l| l.after_trip == i)
                .map(|l| l.secs)
                .sum();
            if arrival > next_start {
                issues.push(NetworkError::BlockInfeasible {
                    block: block_id.clone(),
                    index: i,
                    arrival_secs: arrival,
                    start_secs: next_start,
                });
            } else if arrival.saturating_add(layover) > next_start {
                issues.push(NetworkError::LayoverInfeasible {
                    block: block_id.clone(),
                    after_trip: i,
                });
            }
        }
    }

    issues
}

/// First-issue gate: `Ok` iff the network is valid.
pub fn validate(network: &Network) -> Result<(), NetworkError> {
    network.validate()
}

/// The walking group of a stop: itself, its siblings sharing a
/// parent, its children, and its parent. The places a rider at this
/// stop can walk to for a connection — transfers happen across a
/// street or down a block, not only at one pin. Parents are how
/// GTFS models transfer centers; the walking group is the model's
/// word for the same reality.
pub fn walking_group(network: &Network, stop_id: &str) -> BTreeMap<String, ()> {
    let mut group: BTreeMap<String, ()> = BTreeMap::new();
    let Some(stop) = network.stops.get(stop_id) else {
        return group;
    };
    for (id, s) in &network.stops {
        let in_group = id == stop_id
            || (stop.parent.is_some() && s.parent == stop.parent)
            || s.parent.as_deref() == Some(stop_id)
            || stop.parent.as_deref() == Some(id.as_str());
        if in_group {
            group.insert(id.clone(), ());
        }
    }
    group
}

/// stop → routes serving it, across every pattern. The base index
/// for transfer-point composition.
pub fn stop_route_index(network: &Network) -> BTreeMap<String, BTreeSet<String>> {
    let mut index: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for pattern in network.patterns.values() {
        for ps in &pattern.stops {
            index
                .entry(ps.stop.clone())
                .or_default()
                .insert(pattern.route.clone());
        }
    }
    index
}

/// The routes meeting at a stop, pooled across its walking group.
pub fn routes_meeting_at(
    network: &Network,
    index: &BTreeMap<String, BTreeSet<String>>,
    stop_id: &str,
) -> BTreeSet<String> {
    let mut routes: BTreeSet<String> = BTreeSet::new();
    for id in walking_group(network, stop_id).keys() {
        if let Some(rs) = index.get(id) {
            routes.extend(rs.iter().cloned());
        }
    }
    routes
}

/// Do any of the (start, end) ranges overlap? Sort by start; any pair
/// where the next start is before the current end overlaps.
fn overlapping(ranges: &[(u32, u32)]) -> bool {
    let mut sorted = ranges.to_vec();
    sorted.sort_unstable();
    sorted.windows(2).any(|w| w[1].0 < w[0].1)
}
