//! transit-router: RAPTOR trip planning over the network model's day.
//!
//! Pure and GTFS-free: it takes the network, a materialized day (plain or
//! effective, the caller's choice), and optionally the live document. No
//! I/O, clock, or calendar. [`network`] builds the routing network (an
//! edited trip rides its own pattern), [`raptor`] runs the rounds,
//! [`walk_path`] draws walk legs, and [`notice`] is the one place that
//! tells a rider about a compared plan.
//!
//! Rules:
//!
//! * With a live document, an instance rides its predicted times where
//!   stated, clamped so a trip never runs backwards. A cancelled trip or
//!   a skipped stop is not in the world planned over.
//! * A plan's end is a stop or a place off the network (named by its walks
//!   to nearby stops). A direct walk between the ends wins ties (D-33).
//! * Arrive-by runs the same rounds backward; with no deadline it answers
//!   the last bus home.
//! * An itinerary is door to door (D-40, D-92): it leaves at the first
//!   boarding minus the walk before it, and "leave in N min" is the
//!   board's `wait` rule. Only `Itinerary::new` derives these, because
//!   host arithmetic over legs would put an answer outside the module.
//! * Transfers are straight-line footpaths (300 m at 1.3 m/s) until
//!   `with_walks` replaces them with exactly a walk table's walks (D-32).
//! * A plan's legs bound rides, never walks. With its arrival and rides
//!   fixed, it then walks the least it can (D-35).

pub mod network;
pub mod notice;
pub mod raptor;
pub mod walk_path;

pub use network::{live_times, Footpath, LiveVisit, Pattern, PatternTrip, RouterNetwork, StopNode};
pub use walk_path::walk_path;
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
