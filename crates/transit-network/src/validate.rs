//! Network invariants.
//!
//! `validation_issues` reports every problem a network document can
//! carry — referential integrity, pattern/service/calendar/block
//! structure — so an authoring UI can show the whole list at once.
//! `validate` gates on the first. `Ok(())` is the contract
//! `materialize` relies on: it cannot fail on a validated network.

use std::collections::BTreeMap;

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

    // --- Calendars ------------------------------------------------
    for (calendar_id, calendar) in &network.calendars {
        for (i, r) in calendar.ranges.iter().enumerate() {
            if r.start > r.end {
                issues.push(NetworkError::BadDateRange {
                    calendar: calendar_id.clone(),
                    index: i,
                });
            }
        }
    }

    // --- Services --------------------------------------------------
    for (service_id, service) in &network.services {
        if network.patterns.get(&service.pattern).is_none() {
            issues.push(NetworkError::UnknownPattern {
                service: service_id.clone(),
                pattern: service.pattern.clone(),
            });
        }
        if network.calendars.get(&service.calendar).is_none() {
            issues.push(NetworkError::UnknownCalendar {
                service: service_id.clone(),
                calendar: service.calendar.clone(),
            });
        }
        match &service.schedule {
            Schedule::HeadwayBands { bands } => {
                if bands.is_empty() {
                    issues.push(NetworkError::EmptySchedule {
                        service: service_id.clone(),
                    });
                }
                for (bi, band) in bands.iter().enumerate() {
                    if band.headway_secs == 0 {
                        issues.push(NetworkError::ZeroHeadway {
                            service: service_id.clone(),
                            band: bi,
                        });
                    }
                    if band.start_secs >= band.end_secs {
                        issues.push(NetworkError::EmptyHeadwayBand {
                            service: service_id.clone(),
                            band: bi,
                        });
                    }
                }
                if overlapping(&bands.iter().map(|b| (b.start_secs, b.end_secs)).collect::<Vec<_>>()) {
                    issues.push(NetworkError::OverlappingHeadwayBands {
                        service: service_id.clone(),
                    });
                }
            }
            Schedule::ExactTimes { starts } => {
                if starts.is_empty() {
                    issues.push(NetworkError::EmptySchedule {
                        service: service_id.clone(),
                    });
                } else if starts.windows(2).any(|w| w[1] <= w[0]) {
                    issues.push(NetworkError::UnsortedExactTimes {
                        service: service_id.clone(),
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
                        service: service_id.clone(),
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
            if network.services.get(&t.service).is_none() {
                issues.push(NetworkError::UnknownBlockService {
                    block: block_id.clone(),
                    service: t.service.clone(),
                });
            }
            match owner.get(&(t.service.clone(), t.start_secs)) {
                Some(prev) => issues.push(NetworkError::DuplicateBlockTrip {
                    service: t.service.clone(),
                    start_secs: t.start_secs,
                    a: prev.clone(),
                    b: block_id.clone(),
                }),
                None => {
                    owner.insert((t.service.clone(), t.start_secs), block_id.clone());
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
            let Ok(arrival) = trip_arrival_end_secs(network, &block.trips[i].service, block.trips[i].start_secs)
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

/// Do any of the (start, end) ranges overlap? Sort by start; any pair
/// where the next start is before the current end overlaps.
fn overlapping(ranges: &[(u32, u32)]) -> bool {
    let mut sorted = ranges.to_vec();
    sorted.sort_unstable();
    sorted.windows(2).any(|w| w[1].0 < w[0].1)
}
