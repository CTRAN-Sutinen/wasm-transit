use serde::Serialize;
#[cfg(feature = "bindings")]
use ts_rs::TS;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "bindings", derive(TS, schemars::JsonSchema))]
#[cfg_attr(feature = "bindings", ts(export))]
#[serde(rename_all = "snake_case")]
pub enum NetworkError {
    UnknownParentStop {
        stop: String,
        parent: String,
    },

    UnknownStop {
        pattern: String,
        stop: String,
    },

    UnknownRoute {
        pattern: String,
        route: String,
    },

    UnknownDirection {
        pattern: String,
        route: String,
        direction: String,
    },

    UnknownAlignment {
        pattern: String,
        alignment: String,
    },

    UnknownPattern {
        pattern_trips: String,
        pattern: String,
    },

    UnknownServiceType {
        pattern_trips: String,
        service_type: String,
    },

    UnknownBlockTrip {
        block: String,
        trip: String,
    },

    UnknownLayoverStop {
        block: String,
        stop: String,
    },

    PatternTooShort {
        pattern: String,
    },

    EmptyTimingProfile {
        pattern: String,
        profile: usize,
    },

    TimepointCountMismatch {
        pattern: String,
        profile: usize,
        expected: usize,
        got: usize,
    },

    FirstTimepointNotZero {
        pattern: String,
        profile: usize,
    },

    NonMonotonicTimes {
        pattern: String,
        profile: usize,
    },

    OverlappingTimingProfiles {
        pattern: String,
    },

    DecreasingDistances {
        pattern: String,
    },

    EmptyPatternTrips {
        pattern_trips: String,
    },

    DuplicateTripId {
        trip: String,
        a: String,
        b: String,
    },

    TripTimesMismatch {
        trip: String,
        expected: usize,
        got: usize,
    },

    TripEndpointUnscheduled {
        trip: String,
    },

    TimepointUnscheduled {
        trip: String,
        index: usize,
    },

    FirstOffsetNotZero {
        trip: String,
    },

    TripTimesBackwards {
        trip: String,
    },

    ZeroHeadway {
        trip: String,
        band: usize,
    },

    EmptyHeadwayBand {
        trip: String,
        band: usize,
    },

    OverlappingHeadwayBands {
        trip: String,
    },

    BadDateRange {
        service_type: String,
        index: usize,
    },

    TransferPointWithoutConnections {
        stop: String,
    },

    EmptyBlock {
        block: String,
    },

    BlockOutOfOrder {
        block: String,
        trip: String,
    },

    LayoverAfterLastTrip {
        block: String,
        after_trip: usize,
    },

    /// One vehicle cannot run the block, because an instance arrives after
    /// the next instance departs on a day that both run.
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
    LayoverInfeasible {
        block: String,
        after_trip: usize,
        gap_secs: u32,
        required_secs: u32,
        required_by: RecoverySource,
    },

    DuplicateBlockTrip {
        trip: String,
        a: String,
        b: String,
    },

    UnknownPatternTrips {
        pattern_trips: String,
    },
}

impl std::fmt::Display for NetworkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NetworkError::UnknownParentStop { stop, parent } => write!(f, "stop {stop} references unknown parent stop {parent}"),
            NetworkError::UnknownStop { pattern, stop } => write!(f, "pattern {pattern} references unknown stop {stop}"),
            NetworkError::UnknownRoute { pattern, route } => write!(f, "pattern {pattern} references unknown route {route}"),
            NetworkError::UnknownDirection { pattern, route, direction } => write!(f, "pattern {pattern} runs in direction {direction}, which route {route} does not have"),
            NetworkError::UnknownAlignment { pattern, alignment } => write!(f, "pattern {pattern} runs unknown alignment {alignment}"),
            NetworkError::UnknownPattern { pattern_trips, pattern } => write!(f, "pattern trips {pattern_trips} reference unknown pattern {pattern}"),
            NetworkError::UnknownServiceType { pattern_trips, service_type } => write!(f, "pattern trips {pattern_trips} reference unknown service type {service_type}"),
            NetworkError::UnknownBlockTrip { block, trip } => write!(f, "block {block} references trip {trip}, which the network does not have"),
            NetworkError::UnknownLayoverStop { block, stop } => write!(f, "layover in block {block} references unknown stop {stop}"),
            NetworkError::PatternTooShort { pattern } => write!(f, "pattern {pattern} needs at least two stops"),
            NetworkError::EmptyTimingProfile { pattern, profile } => write!(f, "pattern {pattern} timing profile {profile} has an empty time range"),
            NetworkError::TimepointCountMismatch { pattern, profile, expected, got } => write!(f, "pattern {pattern} timing profile {profile} has {got} entries, expected {expected} (the timepoint count)"),
            NetworkError::FirstTimepointNotZero { pattern, profile } => write!(f, "pattern {pattern} timing profile {profile} must start at 0"),
            NetworkError::NonMonotonicTimes { pattern, profile } => write!(f, "pattern {pattern} timing profile {profile} times must be non-decreasing"),
            NetworkError::OverlappingTimingProfiles { pattern } => write!(f, "pattern {pattern} timing profiles overlap"),
            NetworkError::DecreasingDistances { pattern } => write!(f, "pattern {pattern} stop distances must be non-decreasing"),
            NetworkError::EmptyPatternTrips { pattern_trips } => write!(f, "pattern trips {pattern_trips} run no trips"),
            NetworkError::DuplicateTripId { trip, a, b } => write!(f, "trip {trip} appears in both pattern trips {a} and {b}"),
            NetworkError::TripTimesMismatch { trip, expected, got } => write!(f, "trip {trip} has {got} stop times, expected {expected} (its pattern's stops)"),
            NetworkError::TripEndpointUnscheduled { trip } => write!(f, "trip {trip} must carry both times at its first and last stops"),
            NetworkError::TimepointUnscheduled { trip, index } => write!(f, "trip {trip} must carry both times at timepoint stop {index}"),
            NetworkError::FirstOffsetNotZero { trip } => write!(f, "trip {trip} must arrive at its first stop at offset 0 (its start)"),
            NetworkError::TripTimesBackwards { trip } => write!(f, "trip {trip} times run backwards"),
            NetworkError::ZeroHeadway { trip, band } => write!(f, "trip {trip} headway band {band} must have positive headway"),
            NetworkError::EmptyHeadwayBand { trip, band } => write!(f, "trip {trip} headway band {band} has start >= end"),
            NetworkError::OverlappingHeadwayBands { trip } => write!(f, "trip {trip} headway bands overlap"),
            NetworkError::BadDateRange { service_type, index } => write!(f, "service type {service_type} range {index} has start after end"),
            NetworkError::TransferPointWithoutConnections { stop } => write!(f, "stop {stop} is flagged as a transfer point but fewer than two routes meet across its walking group"),
            NetworkError::EmptyBlock { block } => write!(f, "block {block} is empty"),
            NetworkError::BlockOutOfOrder { block, trip } => write!(f, "block {block} trip {trip} does not start after the trip before it on a day both run"),
            NetworkError::LayoverAfterLastTrip { block, after_trip } => write!(f, "block {block} layover after trip {after_trip} is out of range"),
            NetworkError::BlockInfeasible { block, trip, arrival_secs, next, start_secs } => write!(f, "block {block} trip {trip} arrives at {arrival_secs}, after trip {next} departs at {start_secs}"),
            NetworkError::LayoverInfeasible { block, after_trip, gap_secs, required_secs, .. } => write!(f, "block {block}: {gap_secs} s after trip {after_trip}, under the {required_secs} s recovery required there"),
            NetworkError::DuplicateBlockTrip { trip, a, b } => write!(f, "trip {trip} appears in both block {a} and block {b}"),
            NetworkError::UnknownPatternTrips { pattern_trips } => write!(f, "unknown pattern trips {pattern_trips}"),
        }
    }
}

impl std::error::Error for NetworkError {}

/// A concern that does not make the document invalid. It never gates.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "bindings", derive(TS, schemars::JsonSchema))]
#[cfg_attr(feature = "bindings", ts(export))]
#[serde(rename_all = "snake_case")]
pub enum NetworkWarning {
    /// Trip instances that no block carries. A network with no blocks yet
    /// does not report it.
    UnassignedTrips { pattern_trips: String, trips: u32 },
    /// The calendar ranges that a fork inherited end before the pick starts.
    /// `plan_name` is the name, not the generated id.
    NoServiceInPlanPeriod {
        plan_name: String,
        effective_from: String,
    },
    /// The stops document records the status. `status` is none when
    /// the stop is not there yet. A suspension counts only when it covers the
    /// whole period of the pick or has no end.
    StopNotInService {
        stop: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        #[cfg_attr(feature = "bindings", ts(optional))]
        status: Option<String>,
        from: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        #[cfg_attr(feature = "bindings", ts(optional))]
        until: Option<String>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, serde::Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS, schemars::JsonSchema))]
#[cfg_attr(feature = "bindings", ts(export))]
#[serde(rename_all = "snake_case")]
pub enum RecoverySource {
    Policy,
    /// The smallest authored layover at that terminal.
    Terminal,
}
