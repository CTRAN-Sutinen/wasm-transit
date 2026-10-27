//! Materialization: turning authored intent into concrete trips.
//!
//! `materialize(network, date)` is the bridge from Layer A (the
//! authored model) to Layer B (the service view that RAPTOR and
//! dispatch consume): for every service whose calendar runs on
//! `date`, expand its schedule into trips and compute stop times.
//!
//! The function is **total**: it never panics, and it returns `Err`
//! for any input that fails validation rather than guessing. On a
//! validated network it cannot fail.
//!
//! Stop-time conventions (the single source of truth for these):
//!
//! * The trip *departs* its first stop at `start_secs`.
//! * Timepoint stops get `arrival = start + at_timepoints[k]`.
//! * Non-timepoint stops linearly interpolate by distance between
//!   their bracketing timepoints (equal-distance segments are
//!   tolerated: the fraction is 0).
//! * Departure equals arrival everywhere except the first stop,
//!   where the departure is the trip start; dwells are folded into
//!   the authored running times, not added on top.

use ts_rs::TS;
use chrono::NaiveDate;

use crate::error::NetworkError;
use crate::model::{Network, Pattern, Schedule};

/// One derived trip for one service date.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, TS)]
#[ts(export)]
pub struct MaterializedTrip {
    pub schedule: String,
    pub route: String,
    pub pattern: String,
    /// Seconds after the service-day anchor (see `Network::anchor`).
    pub start_secs: u32,
    pub stop_times: Vec<StopTime>,
}

/// One derived stop visit. Times are seconds after the service-day
/// anchor; departure == arrival except at the first stop.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, TS)]
#[ts(export)]
pub struct StopTime {
    pub stop: String,
    pub arrival_secs: u32,
    pub departure_secs: u32,
    pub timepoint: bool,
}

/// Expand a schedule into its trip start offsets, sorted. Bands are
/// end-exclusive; a zero headway (invalid, flagged by validation)
/// yields no trips rather than looping forever.
pub fn trip_starts(schedule: &Schedule) -> Vec<u32> {
    let mut starts: Vec<u32> = match schedule {
        Schedule::HeadwayBands { bands } => {
            let mut out = Vec::new();
            for band in bands {
                if band.headway_secs == 0 {
                    continue;
                }
                let mut t = band.start_secs;
                while t < band.end_secs {
                    out.push(t);
                    t += band.headway_secs;
                }
            }
            out
        }
        Schedule::ExactTimes { starts } => starts.clone(),
    };
    starts.sort_unstable();
    starts.dedup();
    starts
}

/// Arrival at the final stop of a trip — what the runcut checks
/// feasibility against. On a validated network this cannot fail.
pub fn trip_arrival_end_secs(
    network: &Network,
    schedule_id: &str,
    start_secs: u32,
) -> Result<u32, NetworkError> {
    let service = network.schedules.get(schedule_id).ok_or_else(|| NetworkError::UnknownSchedule {
        schedule: schedule_id.to_string(),
    })?;
    let pattern = network.patterns.get(&service.pattern).ok_or_else(|| NetworkError::UnknownPattern {
        schedule: schedule_id.to_string(),
        pattern: service.pattern.clone(),
    })?;
    let profile = pattern
        .profile_for(start_secs)
        .ok_or_else(|| NetworkError::NoProfileForTrip {
            pattern: service.pattern.clone(),
            start_secs,
        })?;
    profile
        .at_timepoints
        .last()
        .map(|t| start_secs + t)
        .ok_or_else(|| NetworkError::NoProfileForTrip {
            pattern: service.pattern.clone(),
            start_secs,
        })
}

/// Materialize every trip of one service, regardless of date — the
/// schedule expanded against its pattern. Trip *times* don't depend
/// on the date (only whether the service runs at all does), so this
/// is what an authoring UI previews and what the codec loads into
/// the derived-feed view. On a validated network it cannot fail.
pub fn materialize_service(
    network: &Network,
    schedule_id: &str,
) -> Result<Vec<MaterializedTrip>, NetworkError> {
    let service = network
        .schedules
        .get(schedule_id)
        .ok_or_else(|| NetworkError::UnknownSchedule {
            schedule: schedule_id.to_string(),
        })?;
    let pattern = network
        .patterns
        .get(&service.pattern)
        .ok_or_else(|| NetworkError::UnknownPattern {
            schedule: schedule_id.to_string(),
            pattern: service.pattern.clone(),
        })?;
    if network.routes.get(&pattern.route).is_none() {
        return Err(NetworkError::UnknownRoute {
            pattern: service.pattern.clone(),
            route: pattern.route.clone(),
        });
    }
    let mut trips = Vec::new();
    for start in trip_starts(&service.schedule) {
        trips.push(MaterializedTrip {
            schedule: schedule_id.to_string(),
            route: pattern.route.clone(),
            pattern: service.pattern.clone(),
            start_secs: start,
            stop_times: stop_times(&service.pattern, pattern, start)?,
        });
    }
    Ok(trips)
}

/// Materialize every trip that runs on `date`, sorted by start time
/// (ties by service id). Empty result means "nothing runs today".
pub fn materialize(
    network: &Network,
    date: NaiveDate,
) -> Result<Vec<MaterializedTrip>, NetworkError> {
    let mut trips = Vec::new();
    for (schedule_id, service) in &network.schedules {
        let service_type = network
            .service_types
            .get(&service.service_type)
            .ok_or_else(|| NetworkError::UnknownServiceType {
                schedule: schedule_id.clone(),
                service_type: service.service_type.clone(),
            })?;
        if !service_type.rule.runs_on(date) {
            continue;
        }
        trips.extend(materialize_service(network, schedule_id)?);
    }
    trips.sort_by(|a, b| {
        a.start_secs
            .cmp(&b.start_secs)
            .then_with(|| a.schedule.cmp(&b.schedule))
    });
    Ok(trips)
}

/// Stop times for one trip of a pattern departing at `start_secs`.
fn stop_times(
    pattern_id: &str,
    pattern: &Pattern,
    start_secs: u32,
) -> Result<Vec<StopTime>, NetworkError> {
    let profile = pattern
        .profile_for(start_secs)
        .ok_or_else(|| NetworkError::NoProfileForTrip {
            pattern: pattern_id.to_string(),
            start_secs,
        })?;
    let times = &profile.at_timepoints;
    let tp_pos = pattern.timepoint_positions();
    if times.is_empty() || times.len() != tp_pos.len() {
        // Validation catches this; stay total anyway.
        return Err(NetworkError::TimepointCountMismatch {
            pattern: pattern_id.to_string(),
            profile: 0,
            expected: tp_pos.len(),
            got: times.len(),
        });
    }

    let base = pattern.stops.first().map(|s| s.distance_m).unwrap_or(0.0);
    let mut out = Vec::with_capacity(pattern.stops.len());
    // `k` is the index of the timepoint that governs the upcoming
    // stops; stops between timepoint k and k+1 interpolate by
    // distance against times[k]..times[k+1].
    let mut k = 0usize;
    for (i, ps) in pattern.stops.iter().enumerate() {
        let arrival = if ps.timepoint {
            // Position i is timepoint index k (invariant: timepoints
            // appear in pattern order, so a simple cursor suffices).
            while k < tp_pos.len() && tp_pos[k] < i {
                k += 1;
            }
            start_secs + times[k.min(times.len() - 1)]
        } else {
            // Bracketing timepoints: the last one at or before i and
            // the first one after it.
            let left = tp_pos.iter().rposition(|&p| p < i).unwrap_or(0);
            let right = tp_pos.iter().position(|&p| p > i).unwrap_or(tp_pos.len() - 1);
            let d0 = pattern.stops[tp_pos[left]].distance_m - base;
            let d1 = pattern.stops[tp_pos[right]].distance_m - base;
            let di = ps.distance_m - base;
            let fraction = if d1 > d0 { ((di - d0) / (d1 - d0)).clamp(0.0, 1.0) } else { 0.0 };
            let span = times[right].saturating_sub(times[left]) as f64;
            (start_secs as f64 + times[left] as f64 + fraction * span).round() as u32
        };
        let departure = if i == 0 { start_secs } else { arrival };
        out.push(StopTime {
            stop: ps.stop.clone(),
            arrival_secs: arrival,
            departure_secs: departure,
            timepoint: ps.timepoint,
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn headway_bands_generate_end_exclusive() {
        let schedule = Schedule::HeadwayBands {
            bands: vec![crate::model::HeadwayBand {
                start_secs: 6 * 3600,
                end_secs: 9 * 3600,
                headway_secs: 15 * 60,
            }],
        };
        let starts = trip_starts(&schedule);
        assert_eq!(starts.first(), Some(&(6 * 3600)));
        assert_eq!(starts.last(), Some(&(9 * 3600 - 15 * 60)));
        assert_eq!(starts.len(), 12);
    }

    #[test]
    fn zero_headway_yields_no_trips() {
        let schedule = Schedule::HeadwayBands {
            bands: vec![crate::model::HeadwayBand {
                start_secs: 0,
                end_secs: 3600,
                headway_secs: 0,
            }],
        };
        assert!(trip_starts(&schedule).is_empty());
    }

    #[test]
    fn exact_times_pass_through_sorted() {
        let schedule = Schedule::ExactTimes {
            starts: vec![3600, 1800, 7200],
        };
        assert_eq!(trip_starts(&schedule), vec![1800, 3600, 7200]);
    }
}
