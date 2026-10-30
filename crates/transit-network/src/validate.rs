//! Network invariants.
//!
//! `validation_issues` reports every problem a network document can
//! carry — referential integrity, pattern/service/calendar/block
//! structure — so an authoring UI can show the whole list at once.
//! `validate` gates on the first. `Ok(())` is the contract
//! `materialize` relies on: it cannot fail on a validated network.

use std::collections::{BTreeMap, BTreeSet};

use chrono::NaiveDate;

use crate::error::{NetworkError, NetworkWarning};
use crate::model::{Departs, Network, Trip};

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
        match network.routes.get(&pattern.route) {
            None => issues.push(NetworkError::UnknownRoute {
                pattern: pattern_id.clone(),
                route: pattern.route.clone(),
            }),
            Some(route) => {
                if let Some(direction) = &pattern.direction {
                    if !route.directions.contains_key(direction) {
                        issues.push(NetworkError::UnknownDirection {
                            pattern: pattern_id.clone(),
                            route: pattern.route.clone(),
                            direction: direction.clone(),
                        });
                    }
                }
            }
        }
        if let Some(alignment) = pattern.alignment.as_ref().filter(|a| !network.alignments.contains_key(*a)) {
            issues.push(NetworkError::UnknownAlignment {
                pattern: pattern_id.clone(),
                alignment: alignment.clone(),
            });
        }
        if pattern.stops.len() < 2 {
            issues.push(NetworkError::PatternTooShort {
                pattern: pattern_id.clone(),
            });
            continue; // the remaining checks need two stops
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

    // --- Pattern trips and their trips --------------------------------
    let mut homes: BTreeMap<&str, &str> = BTreeMap::new();
    for (pattern_trips_id, service) in &network.pattern_trips {
        let pattern = network.patterns.get(&service.pattern);
        if pattern.is_none() {
            issues.push(NetworkError::UnknownPattern {
                pattern_trips: pattern_trips_id.clone(),
                pattern: service.pattern.clone(),
            });
        }
        if network.service_types.get(&service.service_type).is_none() {
            issues.push(NetworkError::UnknownServiceType {
                pattern_trips: pattern_trips_id.clone(),
                service_type: service.service_type.clone(),
            });
        }
        if service.trips.is_empty() {
            issues.push(NetworkError::EmptyPatternTrips {
                pattern_trips: pattern_trips_id.clone(),
            });
        }
        for trip in &service.trips {
            match homes.get(trip.id.as_str()) {
                Some(a) => issues.push(NetworkError::DuplicateTripId {
                    trip: trip.id.clone(),
                    a: a.to_string(),
                    b: pattern_trips_id.clone(),
                }),
                None => {
                    homes.insert(&trip.id, pattern_trips_id);
                }
            }
            trip_issues(trip, pattern, &mut issues);
        }
    }

    // --- Blocks (the runcut) --------------------------------------
    let index: BTreeMap<&str, (&str, &Trip)> = network
        .pattern_trips
        .values()
        .flat_map(|pt| pt.trips.iter().map(move |t| (t.id.as_str(), (pt.service_type.as_str(), t))))
        .collect();
    let mut days = DaySignatures::new(network);
    let mut owner: BTreeMap<&str, &str> = BTreeMap::new();
    for (block_id, block) in &network.blocks {
        if block.trips.is_empty() {
            issues.push(NetworkError::EmptyBlock {
                block: block_id.clone(),
            });
            continue;
        }
        let mut resolved = true;
        for t in &block.trips {
            if !index.contains_key(t.trip.as_str()) {
                resolved = false;
                issues.push(NetworkError::UnknownBlockTrip {
                    block: block_id.clone(),
                    trip: t.trip.clone(),
                });
            }
            match owner.get(t.trip.as_str()) {
                Some(prev) => issues.push(NetworkError::DuplicateBlockTrip {
                    trip: t.trip.clone(),
                    a: prev.to_string(),
                    b: block_id.clone(),
                }),
                None => {
                    owner.insert(&t.trip, block_id);
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
        if !resolved {
            continue; // feasibility needs every trip
        }
        // Feasibility, day by day: on each combination of service types
        // that run together on some date, the block's trips running that
        // day — every instance of a frequency trip — must follow one
        // another in block order, each arriving before the next departs,
        // with the layover between adjacent trips accounted for.
        let types: BTreeSet<&str> = block.trips.iter().map(|t| index[t.trip.as_str()].0).collect();
        'days: for day in days.signatures(&types) {
            // (block index, trip, start, arrival) of the previous instance.
            let mut prev: Option<(usize, &str, u32, u32)> = None;
            for (i, r) in block.trips.iter().enumerate() {
                let (service_type, trip) = index[r.trip.as_str()];
                if !day.contains(service_type) {
                    continue;
                }
                let run = trip.run_secs().unwrap_or(0);
                let mut starts = trip.departs.instances();
                starts.sort_unstable();
                for start in starts {
                    if let Some((pi, ptrip, pstart, arrival)) = prev {
                        let layover: u32 = if pi + 1 == i {
                            block.layovers.iter().filter(|l| l.after_trip == pi).map(|l| l.secs).sum()
                        } else {
                            0
                        };
                        let issue = if start <= pstart {
                            Some(NetworkError::BlockOutOfOrder { block: block_id.clone(), trip: r.trip.clone() })
                        } else if arrival > start {
                            Some(NetworkError::BlockInfeasible {
                                block: block_id.clone(),
                                trip: ptrip.to_string(),
                                arrival_secs: arrival,
                                next: r.trip.clone(),
                                start_secs: start,
                            })
                        } else if pi != i && arrival.saturating_add(layover) > start {
                            Some(NetworkError::LayoverInfeasible { block: block_id.clone(), after_trip: pi })
                        } else {
                            None
                        };
                        if let Some(issue) = issue {
                            issues.push(issue);
                            break 'days;
                        }
                    }
                    prev = Some((i, r.trip.as_str(), start, start + run));
                }
            }
        }
    }

    issues
}

/// One trip's own invariants, against its pattern when it resolves.
fn trip_issues(trip: &Trip, pattern: Option<&crate::model::Pattern>, issues: &mut Vec<NetworkError>) {
    if let Departs::Every { bands, .. } = &trip.departs {
        if bands.is_empty() {
            issues.push(NetworkError::EmptyHeadwayBand { trip: trip.id.clone(), band: 0 });
        }
        for (bi, band) in bands.iter().enumerate() {
            if band.headway_secs == 0 {
                issues.push(NetworkError::ZeroHeadway { trip: trip.id.clone(), band: bi });
            }
            if band.start_secs >= band.end_secs {
                issues.push(NetworkError::EmptyHeadwayBand { trip: trip.id.clone(), band: bi });
            }
        }
        if overlapping(&bands.iter().map(|b| (b.start_secs, b.end_secs)).collect::<Vec<_>>()) {
            issues.push(NetworkError::OverlappingHeadwayBands { trip: trip.id.clone() });
        }
    }
    let Some(pattern) = pattern else { return };
    if trip.times.len() != pattern.stops.len() {
        issues.push(NetworkError::TripTimesMismatch {
            trip: trip.id.clone(),
            expected: pattern.stops.len(),
            got: trip.times.len(),
        });
        return;
    }
    let both = |i: usize| trip.times[i].arrival_secs.is_some() && trip.times[i].departure_secs.is_some();
    let last = trip.times.len().saturating_sub(1);
    if trip.times.is_empty() || !both(0) || !both(last) {
        issues.push(NetworkError::TripEndpointUnscheduled { trip: trip.id.clone() });
    }
    for (i, ps) in pattern.stops.iter().enumerate() {
        if ps.timepoint && i != 0 && i != last && !both(i) {
            issues.push(NetworkError::TimepointUnscheduled { trip: trip.id.clone(), index: i });
        }
    }
    if trip.times.first().and_then(|o| o.arrival_secs).is_some_and(|a| a != 0) {
        issues.push(NetworkError::FirstOffsetNotZero { trip: trip.id.clone() });
    }
    let scheduled: Vec<u32> =
        trip.times.iter().flat_map(|o| o.arrival_secs.into_iter().chain(o.departure_secs)).collect();
    if scheduled.windows(2).any(|w| w[1] < w[0]) {
        issues.push(NetworkError::TripTimesBackwards { trip: trip.id.clone() });
    }
}

/// The combinations of service types that run together on some date,
/// restricted to a block's types: each type alone, plus every set
/// observed on a date one of them runs. Per-type date sets are
/// computed once per network.
struct DaySignatures<'a> {
    network: &'a Network,
    dates: BTreeMap<&'a str, BTreeSet<NaiveDate>>,
}

impl<'a> DaySignatures<'a> {
    fn new(network: &'a Network) -> Self {
        Self { network, dates: BTreeMap::new() }
    }

    fn dates_of(&mut self, id: &'a str) -> &BTreeSet<NaiveDate> {
        let network = self.network;
        self.dates.entry(id).or_insert_with(|| {
            let Some(st) = network.service_types.get(id) else { return BTreeSet::new() };
            let mut candidates: BTreeSet<NaiveDate> =
                st.rule.exceptions.iter().map(|e| e.date).collect();
            for r in &st.rule.ranges {
                let mut d = r.start;
                while d <= r.end {
                    candidates.insert(d);
                    d = match d.succ_opt() {
                        Some(next) => next,
                        None => break,
                    };
                }
            }
            candidates.into_iter().filter(|d| st.rule.runs_on(*d)).collect()
        })
    }

    fn signatures(&mut self, types: &BTreeSet<&'a str>) -> Vec<BTreeSet<&'a str>> {
        let mut out: BTreeSet<BTreeSet<&'a str>> = types.iter().map(|t| [*t].into_iter().collect()).collect();
        if types.len() > 1 {
            let mut by_date: BTreeMap<NaiveDate, BTreeSet<&'a str>> = BTreeMap::new();
            for t in types {
                for d in self.dates_of(t).clone() {
                    by_date.entry(d).or_default().insert(t);
                }
            }
            out.extend(by_date.into_values());
        }
        out.into_iter().collect()
    }
}

/// Every planning concern that is not an invalidity (see
/// [`NetworkWarning`]), in deterministic order: unassigned trips by
/// schedule id, then the plan-period check.
pub fn validation_warnings(network: &Network) -> Vec<NetworkWarning> {
    let mut warnings = Vec::new();

    if !network.blocks.is_empty() {
        let blocked: BTreeSet<&str> =
            network.blocks.values().flat_map(|b| b.trips.iter().map(|t| t.trip.as_str())).collect();
        for (pattern_trips_id, pattern_trips) in &network.pattern_trips {
            let unassigned = pattern_trips
                .trips
                .iter()
                .filter(|t| !blocked.contains(t.id.as_str()))
                .map(|t| t.departs.instances().len())
                .sum::<usize>() as u32;
            if unassigned > 0 {
                warnings.push(NetworkWarning::UnassignedTrips {
                    pattern_trips: pattern_trips_id.clone(),
                    trips: unassigned,
                });
            }
        }
    }

    if let (false, Some(from)) = (network.plan.id.is_empty(), network.plan.effective_from) {
        let scheduled: BTreeSet<&str> =
            network.pattern_trips.values().map(|s| s.service_type.as_str()).collect();
        let runs_in_first_week = (0..7).any(|d| {
            let date = from + chrono::Duration::days(d);
            network
                .service_types
                .iter()
                .any(|(id, st)| scheduled.contains(id.as_str()) && st.rule.runs_on(date))
        });
        if !runs_in_first_week {
            warnings.push(NetworkWarning::NoServiceInPlanPeriod {
                plan_name: network.plan.name.clone(),
                effective_from: from.format("%Y-%m-%d").to_string(),
            });
        }
    }

    warnings
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
