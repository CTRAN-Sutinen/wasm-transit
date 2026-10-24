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

    #[error("service {service} references unknown pattern {pattern}")]
    UnknownPattern { service: String, pattern: String },

    #[error("service {service} references unknown calendar {calendar}")]
    UnknownCalendar { service: String, calendar: String },

    #[error("block {block} references unknown service {service}")]
    UnknownBlockService { block: String, service: String },

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
    #[error("service {service} has an empty schedule")]
    EmptySchedule { service: String },

    #[error("service {service} headway band {band} must have positive headway")]
    ZeroHeadway { service: String, band: usize },

    #[error("service {service} headway band {band} has start >= end")]
    EmptyHeadwayBand { service: String, band: usize },

    #[error("service {service} headway bands overlap")]
    OverlappingHeadwayBands { service: String },

    #[error("service {service} exact times must be sorted and unique")]
    UnsortedExactTimes { service: String },

    #[error("service {service} trip at {start_secs} falls outside every timing profile of pattern {pattern}")]
    TripOutsideTimingProfiles {
        service: String,
        pattern: String,
        start_secs: u32,
    },

    // -----------------------------------------------------------------
    // Calendar structure
    // -----------------------------------------------------------------
    #[error("calendar {calendar} range {index} has start after end")]
    BadDateRange { calendar: String, index: usize },

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

    #[error("trip {service}@{start_secs} appears in both block {a} and block {b}")]
    DuplicateBlockTrip {
        service: String,
        start_secs: u32,
        a: String,
        b: String,
    },

    // -----------------------------------------------------------------
    // Materialization (total function: these fire only on networks
    // that failed validation)
    // -----------------------------------------------------------------
    #[error("unknown service {service}")]
    UnknownService { service: String },

    #[error("no timing profile of pattern {pattern} covers a trip starting at {start_secs}")]
    NoProfileForTrip { pattern: String, start_secs: u32 },
}
