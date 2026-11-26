//! These rules set the stop times.
//!
//! * The times of an instance are its start plus the offsets of the trip.
//!   They never come from a profile.
//! * A stop scheduled on one side only uses that time for both.
//! * An unscheduled stop gets one time, interpolated by distance between its
//!   scheduled neighbours (the departure of the previous one and the arrival
//!   of the next one). Equal distances give the earlier time.
//!
//! The functions return `Err` for any input that fails validation. They
//! cannot fail on a validated network.

#[cfg(feature = "bindings")]
use ts_rs::TS;
use transit_date::Date;

use crate::error::NetworkError;
use crate::model::{Departs, Network, Pattern, Trip};

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS, schemars::JsonSchema))]
#[cfg_attr(feature = "bindings", ts(export))]
pub struct MaterializedTrip {
    /// The instances of a frequency trip share it.
    pub trip: String,
    pub pattern_trips: String,
    pub route: String,
    pub pattern: String,
    /// Seconds after the service-day anchor.
    pub start_secs: u32,
    pub stop_times: Vec<StopTime>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS, schemars::JsonSchema))]
#[cfg_attr(feature = "bindings", ts(export))]
pub struct StopTime {
    pub stop: String,
    pub arrival_secs: u32,
    pub departure_secs: u32,
    pub timepoint: bool,
}

/// The runcut checks feasibility against it.
pub fn trip_arrival_end_secs(trip: &Trip, start_secs: u32) -> Option<u32> {
    trip.run_secs().map(|run| start_secs + run)
}

/// Trip times do not depend on the date, so an authoring UI previews this.
/// Instances sort by start, then id.
pub fn materialize_service(
    network: &Network,
    pattern_trips_id: &str,
) -> Result<Vec<MaterializedTrip>, NetworkError> {
    let service = network
        .pattern_trips
        .get(pattern_trips_id)
        .ok_or_else(|| NetworkError::UnknownPatternTrips {
            pattern_trips: pattern_trips_id.to_string(),
        })?;
    let pattern = network
        .patterns
        .get(&service.pattern)
        .ok_or_else(|| NetworkError::UnknownPattern {
            pattern_trips: pattern_trips_id.to_string(),
            pattern: service.pattern.clone(),
        })?;
    if network.routes.get(&pattern.route).is_none() {
        return Err(NetworkError::UnknownRoute {
            pattern: service.pattern.clone(),
            route: pattern.route.clone(),
        });
    }
    let mut trips = Vec::new();
    for trip in &service.trips {
        let offsets = offsets(pattern, trip)?;
        for start in trip.departs.instances() {
            trips.push(MaterializedTrip {
                trip: trip.id.clone(),
                pattern_trips: pattern_trips_id.to_string(),
                route: pattern.route.clone(),
                pattern: service.pattern.clone(),
                start_secs: start,
                stop_times: pattern
                    .stops
                    .iter()
                    .zip(&offsets)
                    .map(|(ps, &(a, d))| StopTime {
                        stop: ps.stop.clone(),
                        arrival_secs: start + a,
                        departure_secs: start + d,
                        timepoint: ps.timepoint,
                    })
                    .collect(),
            });
        }
    }
    trips.sort_by(|a, b| a.start_secs.cmp(&b.start_secs).then_with(|| a.trip.cmp(&b.trip)));
    Ok(trips)
}

/// A start inside the band of a non-exact frequency trip is an instance,
/// because riders get the headway, not the minutes.
pub fn trip_instance(network: &Network, trip: &str, start_secs: u32) -> Option<MaterializedTrip> {
    let (pattern_trips_id, service, t) = network.trip(trip)?;
    let runs = match &t.departs {
        Departs::At { start_secs: s } => *s == start_secs,
        Departs::Every { exact: true, .. } => t.departs.instances().contains(&start_secs),
        Departs::Every { bands, exact: false } => bands.iter().any(|b| b.start_secs <= start_secs && start_secs < b.end_secs),
    };
    if !runs {
        return None;
    }
    let pattern = network.patterns.get(&service.pattern)?;
    let offsets = offsets(pattern, t).ok()?;
    Some(MaterializedTrip {
        trip: t.id.clone(),
        pattern_trips: pattern_trips_id.to_string(),
        route: pattern.route.clone(),
        pattern: service.pattern.clone(),
        start_secs,
        stop_times: pattern
            .stops
            .iter()
            .zip(&offsets)
            .map(|(ps, &(a, d))| StopTime {
                stop: ps.stop.clone(),
                arrival_secs: start_secs + a,
                departure_secs: start_secs + d,
                timepoint: ps.timepoint,
            })
            .collect(),
    })
}

/// Sorted by start time, then pattern trips id, then trip id.
pub fn materialize(
    network: &Network,
    date: Date,
) -> Result<Vec<MaterializedTrip>, NetworkError> {
    let mut trips = Vec::new();
    for (pattern_trips_id, service) in &network.pattern_trips {
        let service_type = network
            .service_types
            .get(&service.service_type)
            .ok_or_else(|| NetworkError::UnknownServiceType {
                pattern_trips: pattern_trips_id.clone(),
                service_type: service.service_type.clone(),
            })?;
        if !service_type.rule.runs_on(date) {
            continue;
        }
        trips.extend(materialize_service(network, pattern_trips_id)?);
    }
    trips.sort_by(|a, b| {
        a.start_secs
            .cmp(&b.start_secs)
            .then_with(|| a.pattern_trips.cmp(&b.pattern_trips))
            .then_with(|| a.trip.cmp(&b.trip))
    });
    Ok(trips)
}

pub(crate) fn offsets(pattern: &Pattern, trip: &Trip) -> Result<Vec<(u32, u32)>, NetworkError> {
    if trip.times.len() != pattern.stops.len() {
        return Err(NetworkError::TripTimesMismatch {
            trip: trip.id.clone(),
            expected: pattern.stops.len(),
            got: trip.times.len(),
        });
    }
    let side = |i: usize| -> Option<(u32, u32)> {
        let o = &trip.times[i];
        match (o.arrival_secs, o.departure_secs) {
            (Some(a), Some(d)) => Some((a, d)),
            (Some(a), None) => Some((a, a)),
            (None, Some(d)) => Some((d, d)),
            (None, None) => None,
        }
    };
    let n = pattern.stops.len();
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        if let Some(t) = side(i) {
            out.push(t);
            continue;
        }
        let left = (0..i).rev().find(|&j| side(j).is_some());
        let right = (i + 1..n).find(|&j| side(j).is_some());
        let (Some(left), Some(right)) = (left, right) else {
            return Err(NetworkError::TripEndpointUnscheduled { trip: trip.id.clone() });
        };
        let from = side(left).expect("scheduled").1;
        let to = side(right).expect("scheduled").0;
        let (d0, d1) = (pattern.stops[left].distance_m, pattern.stops[right].distance_m);
        let fraction = if d1 > d0 { ((pattern.stops[i].distance_m - d0) / (d1 - d0)).clamp(0.0, 1.0) } else { 0.0 };
        let t = (from as f64 + fraction * to.saturating_sub(from) as f64).round() as u32;
        out.push((t, t));
    }
    Ok(out)
}

/// Timepoints carry the offsets of the covering profile (arrival equals
/// departure), and other stops stay unscheduled. `None` when no profile
/// covers the start, the profile does not fit the pattern, or an endpoint is
/// not a timepoint.
pub fn produce_times(pattern: &Pattern, start_secs: u32) -> Option<Vec<crate::model::StopOffset>> {
    let profile = pattern.profile_for(start_secs)?;
    let timepoints = pattern.timepoint_positions();
    let last = pattern.stops.len().checked_sub(1)?;
    if profile.at_timepoints.len() != timepoints.len()
        || timepoints.first() != Some(&0)
        || timepoints.last() != Some(&last)
    {
        return None;
    }
    let mut k = 0;
    Some(
        pattern
            .stops
            .iter()
            .map(|ps| {
                if ps.timepoint {
                    let t = profile.at_timepoints[k];
                    k += 1;
                    crate::model::StopOffset { arrival_secs: Some(t), departure_secs: Some(t) }
                } else {
                    crate::model::StopOffset::default()
                }
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use crate::model::{Departs, HeadwayBand};

    #[test]
    fn headway_bands_generate_end_exclusive() {
        let departs = Departs::Every {
            bands: vec![HeadwayBand { start_secs: 6 * 3600, end_secs: 9 * 3600, headway_secs: 15 * 60, extra: Default::default() }],
            exact: true,
        };
        let starts = departs.instances();
        assert_eq!(starts.first(), Some(&(6 * 3600)));
        assert_eq!(starts.last(), Some(&(9 * 3600 - 15 * 60)));
        assert_eq!(starts.len(), 12);
    }

    #[test]
    fn zero_headway_yields_no_trips() {
        let departs = Departs::Every {
            bands: vec![HeadwayBand { start_secs: 0, end_secs: 3600, headway_secs: 0, extra: Default::default() }],
            exact: false,
        };
        assert!(departs.instances().is_empty());
    }
}
