//! transit-network: the authored network model (the planner's authority).
//!
//! Patterns, service types, trips, alignments, and the runcut are the
//! authored truth. A trip is a stored record with its own id, departure,
//! and time at every pattern stop. A pattern's timing profiles only
//! produce a new trip's times; they are never the schedule. A day's
//! instances are materialized per date ([`materialize`]), and a trip
//! instance is addressed (trip, start) everywhere.
//!
//! GTFS is not this model or its shape. transit-codec translates at the
//! boundary, and nothing here knows the format. The model still covers all
//! of GTFS Schedule in its own words (`operators`, `places`, `fares`,
//! `demand`). transit-stops reads this crate; this crate reads no stops
//! document.
//!
//! Conventions, deliberately not GTFS's:
//!
//! * Ids are opaque strings owned by the model.
//! * Times are `u32` seconds after the service-day anchor ([`Anchor`]).
//!   Values past 86400 are trips after midnight.
//! * Distances are meters, per pattern stop.
//! * Maps are `BTreeMap`, so one network serializes to the same bytes on
//!   every host (per-record LWW sync depends on it).
//! * Everything is pure: no I/O, no clock.
//!
//! Rules other code depends on:
//!
//! * A service combination is the set of service types that run together
//!   on a date. Every date with service is in exactly one combination, so
//!   types that never share a day are never pooled. Its dates fold into
//!   spans with a break after a gap of more than seven days (D-55, D-94;
//!   tests/combination_law_test).
//! * A block runs each trip whole and is judged day by day. The recovery
//!   a wait needs is one rule, `required_recovery` (D-9): a work rule the
//!   caller holds wins; else the smallest authored layover at that
//!   terminal. A layover's own length is slack, never the requirement
//!   (tests/recovery_law_test).
//! * Patterns run an alignment by reference, and many patterns may share
//!   one. A pattern's line is its alignment, else its stops joined. A
//!   redraw names whether it moves one pattern or every pattern on the
//!   alignment, and reports what it touched (D-11).
//! * Where a bus is going is one rule (`signs::visit_destination`): the
//!   visit's headsign, else the trip's, else the final stop's place.
//!   Every board row and itinerary leg reads it.
//! * A transfer point is a flag on a stop, and it covers the stop's
//!   walking group (its parent and siblings).
//! * Every document is a service plan (`PlanMeta`); an empty identity is
//!   an unversioned import. The page's plan set is [`plan_set`].
//! * An edit ([`apply_edits`]) answers the next document and never
//!   changes the last. A removal refuses while something references the
//!   entity. A gesture is one edit per planner act and keeps its own
//!   coherence rule. Timetable gestures act on a route and direction
//!   across its patterns: a kept trip keeps its id and blocks, a new trip
//!   is unassigned. A fork to a period end carries each weekly calendar
//!   still running in its last week to that end (D-41, D-112).
//! * Validation issues gate; warnings never gate.
//! * The timetable's rows are `materialize_service`'s trips, never a
//!   second derivation.
//! * Stop search is fuzzy on the feed's own names: exact, then a sign
//!   abbreviation, then a typo. There are no authored aliases, because
//!   they would be a hand-kept dataset; a geocoder covers everyday names
//!   (D-36, D-48, D-91).
//! * The document is `"kind": "network"`. The version chain (`version`)
//!   is kept forever and brings any older document forward.

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
