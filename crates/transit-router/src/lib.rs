//! transit-router — trip planning over the network model.
//!
//! Implements RAPTOR (Round-Based Public Transit Optimized Routing,
//! Delling et al. 2012) in its simplest form: given an origin, a
//! departure time and a service date, compute the earliest arrival at
//! every stop using at most `k` transit legs, then reconstruct the
//! itinerary that achieves it — and the same rounds backward in time
//! for arrive-by: the latest departure that still arrives by a
//! deadline (by the service day's end when none is given: the last
//! bus home). An origin or destination is a stop or a place off the
//! network, named by its walks to the stops near it.
//!
//! The crate is deliberately pure: it consumes the network document
//! plus a MATERIALIZED DAY (the model's own trips — plain or
//! effective, the caller's composition) and, optionally, the LIVE
//! document (each trip instance then rides at its predicted times
//! where the document states them), builds a routing network,
//! answers queries. No I/O, no clocks, no calendars — times are
//! seconds since service-day midnight, matching every other seam.
//!
//! Layout:
//! * [`network`] — the model's day (+ live predictions) → routing
//!   network: trip patterns (grouped by actual stop sequence, so
//!   edited trips ride their own patterns), per-pattern time arrays
//!   with each visit's delay and headsign, footpath graph.
//! * [`raptor`] — a plan's ends (stops, places), the round-based
//!   searches, and itinerary reconstruction (legs carry the route's names, the sign at the
//!   boarding visit, and which times are predicted).
//! * [`notice`] — what a rider is told about a compared plan (the
//!   diverted day against the unpatched one).

pub mod network;
pub mod notice;
pub mod raptor;

pub use network::{live_times, Footpath, LiveVisit, Pattern, PatternTrip, RouterNetwork, StopNode};
pub use notice::{diverted_instances, plan_notice, DivertedTrips, NoticeKind, PlanNotice};
pub use raptor::{
    earliest_arrival, earliest_arrival_between, latest_departure, latest_departure_between, ArriveBy, End,
    Itinerary, Leg, Place, PlaceWalk,
};

use thiserror::Error;

#[derive(Debug, Error)]
pub enum RouterError {
    #[error("unknown stop: {0}")]
    UnknownStop(String),
    #[error("a place's id names a stop: {0}")]
    PlaceIsStop(String),
}

pub type Result<T> = std::result::Result<T, RouterError>;
