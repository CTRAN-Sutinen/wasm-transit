//! Errors for the network model: every invariant violation a network
//! document can carry, plus the failure modes of materialization.
//! One enum so callers (and the wasm seam) match on a single type.

use ts_rs::TS;
use serde::Serialize;
use thiserror::Error;

/// Every invariant violation a network document can carry, plus the
/// failure modes of materialization. One enum so callers (and the
/// wasm seam) match on a single type. Serializes as the payload of
/// the `network_validate` op.
#[derive(Debug, Clone, PartialEq, Error, Serialize, TS, schemars::JsonSchema)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum NetworkError {
    // -----------------------------------------------------------------
    // Referential integrity
    // -----------------------------------------------------------------
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

    /// The runcut and the trips would describe different networks.
    #[error("block {block} references trip {trip}, which the network does not have")]
    UnknownBlockTrip { block: String, trip: String },

    #[error("layover in block {block} references unknown stop {stop}")]
    UnknownLayoverStop { block: String, stop: String },

    // -----------------------------------------------------------------
    // Pattern structure
    // -----------------------------------------------------------------
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

    // -----------------------------------------------------------------
    // Service structure
    // -----------------------------------------------------------------
    #[error("pattern trips {pattern_trips} run no trips")]
    EmptyPatternTrips { pattern_trips: String },

    // -----------------------------------------------------------------
    // Trip structure
    // -----------------------------------------------------------------
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

    // -----------------------------------------------------------------
    // Calendar structure
    // -----------------------------------------------------------------
    #[error("service type {service_type} range {index} has start after end")]
    BadDateRange { service_type: String, index: usize },

    // -----------------------------------------------------------------
    // Transfer points
    // -----------------------------------------------------------------
    #[error("stop {stop} is flagged as a transfer point but fewer than two routes meet across its walking group")]
    TransferPointWithoutConnections { stop: String },

    // -----------------------------------------------------------------
    // Runcut structure
    // -----------------------------------------------------------------
    #[error("block {block} is empty")]
    EmptyBlock { block: String },

    /// On a day the block runs, its trips (a frequency trip's
    /// instances included) must run in its order.
    #[error("block {block} trip {trip} does not start after the trip before it on a day both run")]
    BlockOutOfOrder { block: String, trip: String },

    #[error("block {block} layover after trip {after_trip} is out of range")]
    LayoverAfterLastTrip { block: String, after_trip: usize },

    /// One vehicle cannot run it. An instance arrives after the next one (the
    /// next trip, or the frequency trip's next instance) departs, on a day
    /// both run.
    #[error("block {block} trip {trip} arrives at {arrival_secs}, after trip {next} departs at {start_secs}")]
    BlockInfeasible {
        block: String,
        trip: String,
        arrival_secs: u32,
        next: String,
        start_secs: u32,
    },

    /// The wait after trip `after_trip` is shorter than the recovery required
    /// where the vehicle waits (D-9). A policy's requirement wins when one
    /// applies. Otherwise it is the smallest authored layover at that terminal.
    /// The authored length of a layover is not the requirement. What is above
    /// the requirement is slack.
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

    // -----------------------------------------------------------------
    // Materialization (total function: these fire only on networks
    // that failed validation)
    // -----------------------------------------------------------------
    #[error("unknown pattern trips {pattern_trips}")]
    UnknownPatternTrips { pattern_trips: String },
}

/// A planning concern that does not make the document invalid.
/// Materialization still succeeds, and export still publishes. The planner
/// almost certainly did not intend it. It is reported beside the issues, so
/// nothing about a document goes silent. It is never a gate.
#[derive(Debug, Clone, PartialEq, Serialize, TS, schemars::JsonSchema)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum NetworkWarning {
    /// Trip instances of a pattern's trips that no block carries. This is
    /// service with no vehicle assigned. Reported only once the document has a
    /// runcut at all. A network with no blocks yet is unassigned.
    UnassignedTrips { pattern_trips: String, trips: u32 },
    /// A versioned plan whose service types run on no day of its first week.
    /// The pick governs a period that the plan carries no service for. The
    /// calendar ranges that a fork inherited end before it. The plan carries
    /// its own name. A reader knows it by that name, not by its (often minted)
    /// id.
    NoServiceInPlanPeriod { plan_name: String, effective_from: String },
    /// A stop that the pick's patterns visit is not in service from `from`
    /// until `until`. Without `until`, the stop stays open-ended. The stops
    /// document records the status (D-157): planned, permitted, built, retired,
    /// or not there yet (`status` none). It is suspended when the suspension
    /// covers the pick's whole period or has no end. A shorter suspension is
    /// not the pick's to say. Real time says it.
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

/// Where a required recovery came from (D-9).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, serde::Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum RecoverySource {
    /// The work-rule policy the caller supplied.
    Policy,
    /// The smallest authored layover at that terminal (no policy
    /// applies).
    Terminal,
}
