//! transit-router plans trips with RAPTOR over a materialized day. It does no
//! I/O, clock, or calendar work.
//!
//! * With a live document, an instance rides its predicted times where stated.
//!   The times are clamped so that a trip never runs backwards. A cancelled
//!   trip or a skipped stop is not in the planned world.
//! * A direct walk between the ends wins ties (D-33).
//! * Only `Itinerary::new` derives the door-to-door values (D-92), so no
//!   host does arithmetic over legs.
//! * Transfers are straight-line footpaths until `with_walks` replaces them
//!   with the walks of a walk table (D-32).
//! * The legs of a plan bound its rides and not its walks. With the arrival
//!   and the rides fixed, the plan walks the least it can (D-35).

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
