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
#[derive(Debug, Clone, PartialEq, Error, Serialize, TS)]
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

    /// One vehicle cannot run it: an instance arrives after the next
    /// one (the next trip, or the frequency trip's next instance) on a
    /// day both run departs.
    #[error("block {block} trip {trip} arrives at {arrival_secs}, after trip {next} departs at {start_secs}")]
    BlockInfeasible {
        block: String,
        trip: String,
        arrival_secs: u32,
        next: String,
        start_secs: u32,
    },

    #[error("block {block} layover after trip {after_trip} does not fit before the next trip")]
    LayoverInfeasible { block: String, after_trip: usize },

    #[error("trip {trip} appears in both block {a} and block {b}")]
    DuplicateBlockTrip { trip: String, a: String, b: String },

    // -----------------------------------------------------------------
    // Materialization (total function: these fire only on networks
    // that failed validation)
    // -----------------------------------------------------------------
    #[error("unknown pattern trips {pattern_trips}")]
    UnknownPatternTrips { pattern_trips: String },
}

/// A planning concern that does not make the document invalid —
/// materialization still succeeds, export still publishes — but that
/// the planner almost certainly did not intend. Reported beside the
/// issues so nothing about a document goes silent; never a gate.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum NetworkWarning {
    /// Trip instances of a pattern trips that no block carries: service with no
    /// vehicle assigned. Reported only once the document has a runcut
    /// at all (a network with no blocks yet is unassigned by design).
    UnassignedTrips { pattern_trips: String, trips: u32 },
    /// A versioned plan whose service types run on no day of its
    /// first week: the pick governs a period it carries no service
    /// for (the calendar ranges a fork inherited end before it). The
    /// plan is the document's own, named as a reader knows it — its
    /// name, never its (often minted) id.
    NoServiceInPlanPeriod { plan_name: String, effective_from: String },
}
