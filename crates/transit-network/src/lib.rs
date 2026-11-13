//! transit-network — the authored network model.
//!
//! The planner's data model, designed from planner intent downward:
//! patterns, service types, trips, and the runcut are the authored
//! truth. A trip is a stored record — its own id, its own departure,
//! its own times — produced from a pattern's timing profile and then
//! adjusted trip by trip; a day's instances are materialized per date.
//!
//! GTFS is neither this model nor its shape. It is a codec at the
//! boundary (transit-codec). Nothing in this crate knows the format
//! exists.
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
//! Everything here is pure: types, validation, materialization, and
//! authoring ([`apply_edits`] — a change answers the next document,
//! never mutating the last). No I/O, no clocks.

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
pub mod signs;
pub mod stop_record;
pub mod timetable_edit;
pub mod validate;
pub mod version;

pub use edit::{apply_edits, apply_edits_reporting, Applied, CreatedEntity, Day, EditError, NetworkEdit, Redraw, TouchedEntity};
pub use error::{NetworkError, NetworkWarning, RecoverySource};
pub use geometry::{authored_alignment, entity_focus, route_lines, RouteLines, pattern_alignment, pattern_stop_points, stop_point, stops_line, EntityFocus, EntityKind};
pub use combinations::{date_spans, first_service_date, service_combination, service_combinations, DateSpan, ServiceCombination};
pub use listing::{
    entity_rows, route_detail, service_keys, stop_detail, timetable, timetable_against, timetable_on, DirectionGroup, EntityRow, PatternTripsGroup,
    RemovedTrip, RouteDetail, RouteVariant, ServiceKey, ServiceKeyRoute, StopDetail, StopVisit, Timetable, TimetablePattern, TimetableStop, TimetableTrip, VariantStop,
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
pub use signs::visit_destination;
pub use validate::{
    publish_refusals, recovery_floors, required_recovery, validation_issues_under, RecoveryAt, routes_meeting_at, stop_route_index, validate, validation_issues, validation_warnings,
    walking_group,
};
