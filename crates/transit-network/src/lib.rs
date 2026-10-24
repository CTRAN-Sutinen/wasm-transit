//! transit-network — the authored network model.
//!
//! The planner's data model, designed from planner intent downward:
//! patterns, service rules, and the runcut are the authored truth;
//! trips and stop times are *derived*, materialized per date. A
//! planner does not author "60 trips" — they author a pattern, a
//! schedule, and a calendar, and the trips fall out.
//!
//! GTFS is neither this model nor its shape. It is an optional codec
//! at the boundary (import by inference: backlog; export: backlog).
//! Nothing in this crate knows the format exists.
//!
//! Conventions, deliberately not GTFS's:
//!
//! * **IDs are opaque strings owned by this model.** Any GTFS ids
//!   live in the codec layer as a translation concern, not here.
//! * **Times are `u32` seconds after the service-day anchor** of the
//!   materialized date ([`Anchor`]). Values past 86400 are
//!   legitimate after-midnight trips.
//! * **Distances are meters** (`f64`), per pattern stop.
//! * **Maps are `BTreeMap`** so serialization is canonical — the
//!   same network serializes to the same bytes on every host, which
//!   is what per-entity LWW sync needs later.
//!
//! Everything here is pure: types, validation, materialization. No
//! I/O, no clocks.

pub mod error;
pub mod materialize;
pub mod model;
pub mod validate;

pub use error::NetworkError;
pub use materialize::{materialize, trip_arrival_end_secs, trip_starts, MaterializedTrip, StopTime};
pub use model::{
    Anchor, Block, CalendarException, CalendarRule, DateRange, DayState, HeadwayBand, Layover,
    Mode, Network, Pattern, PatternStop, Route, Schedule, Service, Stop, TimingProfile, TripRef,
    WeekdaySet,
};
pub use validate::{validate, validation_issues};
