use ts_rs::TS;
use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Error, Serialize, TS, schemars::JsonSchema)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum NetworkError {
    #[error("stop {stop} references unknown parent stop {parent}")]
    UnknownParentStop { stop: String, parent: String },

    #[error("pattern {pattern} references unknown stop {stop}")]
    UnknownStop { pattern: String, stop: String },

    #[error("pattern {pattern} references unknown route {route}")]
    UnknownRoute { pattern: String, route: String },

    #[error("pattern {pattern} runs in direction {direction}, which route {route} does not have")]
    UnknownDirection { pattern: String, route: String, direction: String },

    #[error("pattern {pattern} runs unknown alignment {alignment}")]
    UnknownAlignment { pattern: String, alignment: String },

    #[error("pattern trips {pattern_trips} reference unknown pattern {pattern}")]
    UnknownPattern { pattern_trips: String, pattern: String },

    #[error("pattern trips {pattern_trips} reference unknown service type {service_type}")]
    UnknownServiceType { pattern_trips: String, service_type: String },

    #[error("block {block} references trip {trip}, which the network does not have")]
    UnknownBlockTrip { block: String, trip: String },

    #[error("layover in block {block} references unknown stop {stop}")]
    UnknownLayoverStop { block: String, stop: String },

    #[error("pattern {pattern} needs at least two stops")]
    PatternTooShort { pattern: String },

    #[error("pattern {pattern} timing profile {profile} has an empty time range")]
    EmptyTimingProfile { pattern: String, profile: usize },

    #[error(
        "pattern {pattern} timing profile {profile} has {got} entries, expected {expected} (the timepoint count)"
    )]
    TimepointCountMismatch {
        pattern: String,
        profile: usize,
        expected: usize,
        got: usize,
    },

    #[error("pattern {pattern} timing profile {profile} must start at 0")]
    FirstTimepointNotZero { pattern: String, profile: usize },

    #[error("pattern {pattern} timing profile {profile} times must be non-decreasing")]
    NonMonotonicTimes { pattern: String, profile: usize },

    #[error("pattern {pattern} timing profiles overlap")]
    OverlappingTimingProfiles { pattern: String },

    #[error("pattern {pattern} stop distances must be non-decreasing")]
    DecreasingDistances { pattern: String },

    #[error("pattern trips {pattern_trips} run no trips")]
    EmptyPatternTrips { pattern_trips: String },

    #[error("trip {trip} appears in both pattern trips {a} and {b}")]
    DuplicateTripId { trip: String, a: String, b: String },

    #[error("trip {trip} has {got} stop times, expected {expected} (its pattern's stops)")]
    TripTimesMismatch { trip: String, expected: usize, got: usize },

    #[error("trip {trip} must carry both times at its first and last stops")]
    TripEndpointUnscheduled { trip: String },

    #[error("trip {trip} must carry both times at timepoint stop {index}")]
    TimepointUnscheduled { trip: String, index: usize },

    #[error("trip {trip} must arrive at its first stop at offset 0 (its start)")]
    FirstOffsetNotZero { trip: String },

    #[error("trip {trip} times run backwards")]
    TripTimesBackwards { trip: String },

    #[error("trip {trip} headway band {band} must have positive headway")]
    ZeroHeadway { trip: String, band: usize },

    #[error("trip {trip} headway band {band} has start >= end")]
    EmptyHeadwayBand { trip: String, band: usize },

    #[error("trip {trip} headway bands overlap")]
    OverlappingHeadwayBands { trip: String },

    #[error("service type {service_type} range {index} has start after end")]
    BadDateRange { service_type: String, index: usize },

    #[error("stop {stop} is flagged as a transfer point but fewer than two routes meet across its walking group")]
    TransferPointWithoutConnections { stop: String },

    #[error("block {block} is empty")]
    EmptyBlock { block: String },

    #[error("block {block} trip {trip} does not start after the trip before it on a day both run")]
    BlockOutOfOrder { block: String, trip: String },

    #[error("block {block} layover after trip {after_trip} is out of range")]
    LayoverAfterLastTrip { block: String, after_trip: usize },

    /// One vehicle cannot run the block, because an instance arrives after
    /// the next instance departs on a day that both run.
    #[error("block {block} trip {trip} arrives at {arrival_secs}, after trip {next} departs at {start_secs}")]
    BlockInfeasible {
        block: String,
        trip: String,
        arrival_secs: u32,
        next: String,
        start_secs: u32,
    },

    /// The wait after trip `after_trip` is shorter than the recovery required
    /// where the vehicle waits. An authored layover longer than the
    /// requirement is slack.
    #[error("block {block}: {gap_secs} s after trip {after_trip}, under the {required_secs} s recovery required there")]
    LayoverInfeasible {
        block: String,
        after_trip: usize,
        gap_secs: u32,
        required_secs: u32,
        required_by: RecoverySource,
    },

    #[error("trip {trip} appears in both block {a} and block {b}")]
    DuplicateBlockTrip { trip: String, a: String, b: String },

    #[error("unknown pattern trips {pattern_trips}")]
    UnknownPatternTrips { pattern_trips: String },
}

/// A concern that does not make the document invalid. It never gates.
#[derive(Debug, Clone, PartialEq, Serialize, TS, schemars::JsonSchema)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum NetworkWarning {
    /// Trip instances that no block carries. A network with no blocks yet
    /// does not report it.
    UnassignedTrips { pattern_trips: String, trips: u32 },
    /// The calendar ranges that a fork inherited end before the pick starts.
    /// `plan_name` is the name, not the generated id.
    NoServiceInPlanPeriod { plan_name: String, effective_from: String },
    /// The stops document records the status. `status` is none when
    /// the stop is not there yet. A suspension counts only when it covers the
    /// whole period of the pick or has no end.
    StopNotInService {
        stop: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        status: Option<String>,
        from: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        until: Option<String>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, serde::Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum RecoverySource {
    Policy,
    /// The smallest authored layover at that terminal.
    Terminal,
}
