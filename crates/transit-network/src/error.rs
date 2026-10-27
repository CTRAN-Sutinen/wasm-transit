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

    #[error("pattern trips {pattern_trips} reference unknown pattern {pattern}")]
    UnknownPattern { pattern_trips: String, pattern: String },

    #[error("pattern trips {pattern_trips} reference unknown service type {service_type}")]
    UnknownServiceType { pattern_trips: String, service_type: String },

    #[error("block {block} references unknown pattern trips {pattern_trips}")]
    UnknownBlockPatternTrips { block: String, pattern_trips: String },

    /// The schedule exists but produces no trip at this start: the
    /// runcut and the published schedule would describe different
    /// networks (a retimed schedule left its blocks behind).
    #[error("block {block} references trip {pattern_trips}@{start_secs}, which those pattern trips do not run")]
    UnknownBlockTrip { block: String, pattern_trips: String, start_secs: u32 },

    #[error("layover in block {block} references unknown stop {stop}")]
    UnknownLayoverStop { block: String, stop: String },

    // -----------------------------------------------------------------
    // Pattern structure
    // -----------------------------------------------------------------
    #[error("pattern {pattern} needs at least two stops")]
    PatternTooShort { pattern: String },

    #[error("pattern {pattern} first and last stops must be timepoints")]
    EndpointsMustBeTimepoints { pattern: String },

    #[error("pattern {pattern} has no timing profile")]
    NoTimingProfile { pattern: String },

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

    #[error("pattern trips {pattern_trips} headway band {band} must have positive headway")]
    ZeroHeadway { pattern_trips: String, band: usize },

    #[error("pattern trips {pattern_trips} headway band {band} has start >= end")]
    EmptyHeadwayBand { pattern_trips: String, band: usize },

    #[error("pattern trips {pattern_trips} headway bands overlap")]
    OverlappingHeadwayBands { pattern_trips: String },

    #[error("pattern trips {pattern_trips} exact times must be sorted and unique")]
    UnsortedExactTimes { pattern_trips: String },

    #[error("pattern trips {pattern_trips} trip at {start_secs} falls outside every timing profile of pattern {pattern}")]
    TripOutsideTimingProfiles {
        pattern_trips: String,
        pattern: String,
        start_secs: u32,
    },

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

    #[error("block {block} trips must have strictly increasing start times")]
    BlockOutOfOrder { block: String },

    #[error("block {block} layover after trip {after_trip} is out of range")]
    LayoverAfterLastTrip { block: String, after_trip: usize },

    #[error("block {block} trip {index} arrives at {arrival_secs}, after the next trip departs at {start_secs}")]
    BlockInfeasible {
        block: String,
        index: usize,
        arrival_secs: u32,
        start_secs: u32,
    },

    #[error("block {block} layover after trip {after_trip} does not fit before the next trip")]
    LayoverInfeasible { block: String, after_trip: usize },

    #[error("trip {pattern_trips}@{start_secs} appears in both block {a} and block {b}")]
    DuplicateBlockTrip {
        pattern_trips: String,
        start_secs: u32,
        a: String,
        b: String,
    },

    // -----------------------------------------------------------------
    // Materialization (total function: these fire only on networks
    // that failed validation)
    // -----------------------------------------------------------------
    #[error("unknown pattern trips {pattern_trips}")]
    UnknownPatternTrips { pattern_trips: String },

    #[error("no timing profile of pattern {pattern} covers a trip starting at {start_secs}")]
    NoProfileForTrip { pattern: String, start_secs: u32 },
}

/// A planning concern that does not make the document invalid —
/// materialization still succeeds, export still publishes — but that
/// the planner almost certainly did not intend. Reported beside the
/// issues so nothing about a document goes silent; never a gate.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum NetworkWarning {
    /// Trips of a schedule that no block carries: service with no
    /// vehicle assigned. Reported only once the document has a runcut
    /// at all (a network with no blocks yet is unassigned by design).
    UnassignedTrips { pattern_trips: String, trips: u32 },
    /// A versioned plan whose service types run on no day of its
    /// first week: the pick governs a period it carries no service
    /// for (the calendar ranges a fork inherited end before it).
    NoServiceInPlanPeriod { plan: String, effective_from: String },
}
