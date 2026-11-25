//! The authored network model. A trip is a stored record with its own id,
//! departure, and time at every pattern stop. A pattern's timing profiles
//! only produce the times of a new trip ([`produce_times`]). A trip instance
//! is addressed by (trip, start).
//!
//! The model covers all of GTFS Schedule in its own words. transit-codec
//! translates at the boundary, and nothing here knows the GTFS format.
//!
//! These conventions differ from GTFS.
//!
//! * Ids are opaque strings that the model owns.
//! * Times are `u32` seconds after the service-day anchor ([`Anchor`]).
//!   Values past 86400 are trips after midnight.
//! * Distances are meters, for each pattern stop.
//! * Maps are `BTreeMap`, so one network serializes to the same bytes on
//!   every host. Per-record LWW sync depends on this.
//! * The crate does no I/O and reads no clock.
//!
//! Other code depends on these rules.
//!
//! * Every date with service is in exactly one service combination, so
//!   types that never share a day are never pooled (D-55, D-94).
//! * `required_recovery` is the only rule for the recovery that a wait
//!   needs (D-9).
//! * `signs::visit_destination` is the only rule for where a bus is going.
//!   Every board row and itinerary leg reads it.
//! * A transfer point is a flag on a stop. It covers the walking group of
//!   the stop (its parent and siblings).
//! * An empty `PlanMeta` identity is an unversioned import.
//! * An edit ([`apply_edits`]) answers the next document and never changes
//!   the last. A kept trip keeps its id and blocks. A new trip is
//!   unassigned. A fork to a period end carries each weekly calendar that
//!   still runs in its last week to that end (D-41, D-112).
//! * Validation issues gate. Warnings never gate.
//! * There are no authored stop aliases, because people would keep them by
//!   hand. A geocoder covers everyday names (D-36, D-48, D-91).

pub mod demand;
pub mod combinations;
pub mod edit;
pub mod error;
pub mod fares;
pub mod geometry;
pub mod listing;
pub mod materialize;
pub mod migrate;
pub mod model;
pub mod operators;
pub mod places;
pub mod plan_set;
pub mod signs;
pub mod timetable_edit;
pub mod validate;
pub mod version;
pub mod load;

pub use edit::{apply_edits, apply_edits_reporting, Applied, CreatedEntity, Day, EditError, NetworkEdit, Redraw, TouchedEntity};
pub use error::{NetworkError, NetworkWarning, RecoverySource};
pub use geometry::{authored_alignment, entity_focus, route_lines, RouteLines, pattern_alignment, pattern_stop_points, stop_point, stops_line, EntityFocus, EntityKind};
pub use combinations::{date_spans, first_service_date, stamp_as_pick, service_combination, service_combinations, DateSpan, ServiceCombination};
pub use listing::{
    entity_rows, route_detail, service_keys, stop_detail, timetable, timetable_against, DirectionGroup, EntityRow, PatternTripsGroup,
    RemovedTrip, RouteDetail, RouteVariant, ServiceKey, ServiceKeyRoute, StopDetail, StopVisit, Neighbour, Timetable, TimetablePattern, TimetableStop, TimetableTrip, VariantStop,
};
pub use materialize::{
    materialize, materialize_service, produce_times, trip_arrival_end_secs, trip_instance, MaterializedTrip, StopTime,
};
pub use version::{migrate_network, migrate_step, migrate_to, schema_version_of, MigrationNote, NetworkKind, NetworkVersion, KIND, SCHEMA_VERSION};
pub use model::{
    Alignment, AlignmentId, AlignmentPoint, Anchor, Block, CalendarException, CalendarRule, Coord, DateRange, DayState, Departs, Direction, DirectionId,
    HeadwayBand, Layover, Mode, Network, Pattern, PatternStop, PatternTrips, PatternTripsId, PlanMeta, Route,
    ServiceType, ServiceTypeId, Stop, StopOffset, StopRule, TimingProfile, Trip, TripId, TripRef, WeekdaySet,
};
pub use plan_set::{PlanSet, PlanSetKind, PLAN_SET_VERSION};
pub use signs::visit_destination;
pub use validate::{
    publish_check, publish_refusals, recovery_floors, required_recovery, validation_issues_under, PublishCheck, RecoveryAt, routes_meeting_at, stop_route_index, validate, validation_issues, validation_warnings,
    walking_group,
};
