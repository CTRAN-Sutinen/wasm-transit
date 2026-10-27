//! transit-router — earliest-arrival trip planning over the network
//! model.
//!
//! Implements RAPTOR (Round-Based Public Transit Optimized Routing,
//! Delling et al. 2012) in its simplest form: given an origin stop, a
//! departure time and a service date, compute the earliest arrival at
//! every stop using at most `k` transit legs, then reconstruct the
//! itinerary that achieves it.
//!
//! The crate is deliberately pure: it consumes the network document
//! plus a MATERIALIZED DAY (the model's own trips — plain or
//! effective, the caller's composition), builds a routing network,
//! answers queries. No I/O, no clocks, no calendars — times are
//! seconds since service-day midnight, matching every other seam.
//!
//! Layout:
//! * [`network`] — the model's day → routing network: trip patterns
//!   (grouped by actual stop sequence, so edited trips ride their
//!   own patterns), per-pattern time arrays, footpath graph.
//! * [`raptor`] — the round-based search and itinerary reconstruction.

pub mod network;
pub mod raptor;

pub use network::{Footpath, Pattern, PatternTrip, RouterNetwork, StopNode};
pub use raptor::{earliest_arrival, Itinerary, Leg};

use thiserror::Error;

#[derive(Debug, Error)]
pub enum RouterError {
    #[error("unknown stop: {0}")]
    UnknownStop(String),
}

pub type Result<T> = std::result::Result<T, RouterError>;
