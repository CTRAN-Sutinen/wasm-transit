//! transit-dispatch: the materialized day's views, in model vocabulary.
//!
//! The day's questions run over the network document, with no GTFS expansion.
//! An arch gate forbids a GTFS layer. The day is
//! [`transit_network::materialize`] for one date, patched by the operator's
//! edits. [`day_of`] is a pure function of the network, the edits, and the
//! date. The library keeps no cache. The kernel holds each day under
//! [`day_key`] (`cap_abi::kernel::DAY`), so capabilities that read one day
//! share one copy.
//! Times are seconds after the service-day anchor. Past 86400 is after
//! midnight, not an error.
//!
//! Rules other code depends on:
//!
//! * Edits act on the day, never the calendar. A cancel takes out one trip
//!   instance and leaves every other trip whole. The trip must exist and run
//!   that instance.
//! * The board is realtime-first, ordered by when the bus leaves. A visit the
//!   live document cancels or skips is no row (D-73). Edits that take a
//!   departure away list it as `skipped` (D-100). A stop with nothing of its
//!   own pools its walking group.
//! * The grouped board keeps every group serving the stop that day, in route
//!   sort order, then id, then destination. A stopped group shows its next
//!   departure within seven days, patched by each day's own edits, never the
//!   live feed (D-42;
//!   tests/next_service_law_test). The days come through a closure, so the
//!   capability chooses where they come from.
//! * The board at a moment (`stop_board_at`) also lists the previous date's
//!   departures still to leave. GTFS times past 24:00 belong to the date the
//!   trip started. Each date reads its own edits and live instances. Rows name
//!   their date. The instant counts the date's true length (23, 24, or 25 h)
//!   (tests/overnight_law_test).
//! * An alert reaches a departure by its scope (`LiveAlert::touches`,
//!   `touches_stop`). Over a held day it also reaches the skipped departures
//!   (tests/alert_reach_law_test).
//! * A diverted trip's line runs the alignment to the first kept stop, the
//!   traced detour, then the alignment from the last kept stop. A skip-only
//!   diversion keeps the alignment.
//! * `layover_remaining_secs` goes negative when a connection is eaten.

pub mod divert;
pub mod geometry;
pub use geometry::{trip_path, trip_segment, LegKind, LegPath};
pub use divert::{cancel_of, cancel_rows, diversion_of, diversion_rows, draft_view, CancelRow, DiversionDraft, DiversionRow, DraftPattern, DraftStop, DraftStopRole, DraftView};

use std::collections::{BTreeMap, BTreeSet};

use chrono::NaiveDate;
use ts_rs::TS;

use transit_network::{materialize, MaterializedTrip, Network, StopTime};
use transit_live::{classify, LiveAlert, LiveDay, LiveStatus, Visit};
use transit_sync::{Diversion, EditRecord, EditReport, TripCancel};

/// One row on a departure board. The trip instance is its (trip, start)
/// identity. The stop is the concrete stop the row leaves from: a bay, on a
/// pooled station board. The destination is the sign's text at this visit.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, TS)]
#[ts(export)]
pub struct BoardDeparture {
    pub trip: String,
    /// The service date, "YYYY-MM-DD". Set only by a board asked at a moment
    /// ([`stop_board_at`], [`grouped_board_at`]), where a row may be the
    /// previous date's after-midnight trip. `departure_secs` counts from this
    /// date's anchor. Absent from a single day's board.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub date: Option<String>,
    pub start_secs: u32,
    pub route: String,
    /// The route's public name ("12").
    pub route_name: String,
    /// The route's long name ("Lincoln"), when it has one.
    pub route_long_name: Option<String>,
    pub pattern: String,
    pub stop: String,
    /// That stop's name ("Fisher's Landing TC: Bay B").
    pub stop_name: String,
    /// That stop's own identifier at its station (GTFS
    /// `platform_code`: "B"), when it has one.
    pub platform: Option<String>,
    pub departure_secs: u32,
    /// The sign's text at this visit ([`transit_network::visit_destination`]).
    pub destination: String,
}

/// One trip in a block's chain for a service date. The chain is the authored
/// runcut. Blocks and layovers are model state that GTFS never had. A diverted
/// trip, or one the live document says ends late, eats into its next layover.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, TS)]
#[ts(export)]
pub struct BlockChainTrip {
    /// The stored trip (a frequency trip's instances share it).
    pub trip: String,
    pub pattern_trips: String,
    pub start_secs: u32,
    pub route: String,
    pub route_name: String,
    pub pattern: String,
    /// Whether this trip runs on the date. A trip that does not run stays in
    /// the authored chain.
    pub runs: bool,
    pub first_stop: String,
    pub last_stop: String,
    /// The trip's end (last arrival), after the date's operator edits. `None`
    /// when the trip does not run.
    pub end_secs: Option<u32>,
    /// The live delay at the last stop (arrival, else departure), against the
    /// published schedule. `None` when no live document predicts this instance.
    pub live_end_delay_secs: Option<i32>,
    /// The authored layover after this trip, if any.
    pub layover_after_secs: Option<u32>,
    /// The layover left after this trip's delay: the authored layover minus how
    /// late the trip ends. Lateness is the live delay at its last stop when
    /// predicted (against the published time, D-18), else what the edits added.
    /// Negative means the next trip starts late. `None` without a layover, or
    /// when the trip does not run.
    pub layover_remaining_secs: Option<i64>,
    /// Health under the caller's tight threshold: overlap (the delay ate the
    /// whole layover), tight (under the threshold), or ok. `None` exactly when
    /// `layover_remaining_secs` is.
    pub layover_health: Option<LayoverHealth>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum LayoverHealth {
    Ok,
    Tight,
    Overlap,
}

/// The default tight-layover threshold: under five minutes left.
pub const TIGHT_LAYOVER_SECS: u32 = 300;

pub fn layover_health(remaining_secs: i64, tight_secs: u32) -> LayoverHealth {
    if remaining_secs < 0 {
        LayoverHealth::Overlap
    } else if remaining_secs < tight_secs as i64 {
        LayoverHealth::Tight
    } else {
        LayoverHealth::Ok
    }
}

// Patching the materialized day with operator edits

/// Apply operator edits to a materialized day. The input is never mutated.
/// A trip matches when it has the route and both anchors in order. The model
/// has no GTFS direction. The pattern is the direction.
///
/// * *Skip*: interior stops listed in `skipped_stops` lose their stop times.
///   Nothing here reports the loss.
/// * *Shift*: every stop time from the last anchor onward moves later by
///   `added_secs`, the detour's running time.
///
/// The day carries no geometry. The detour polyline is drawing, not schedule.
/// A trip's line splices it in where the trip is drawn (`geometry::trip_path`).
///
/// Edits apply in the order given. Each trip can absorb several.
pub fn patch_day(day: &[MaterializedTrip], diversions: &[Diversion]) -> Vec<MaterializedTrip> {
    let mut out = day.to_vec();
    for d in diversions {
        for trip in &mut out {
            if trip.route != d.route_id {
                continue;
            }
            patch_trip(trip, d);
        }
    }
    out
}

fn patch_trip(trip: &mut MaterializedTrip, d: &Diversion) {
    let Some((p0, p1)) = anchor_positions(&trip.stop_times, d) else {
        return;
    };
    trip.stop_times = std::mem::take(&mut trip.stop_times)
        .into_iter()
        .enumerate()
        .filter_map(|(i, st)| {
            let interior = i > p0 && i < p1;
            if interior && d.skipped_stops.iter().any(|s| s == &st.stop) {
                None
            } else {
                Some(st)
            }
        })
        .collect();
    // The detour's time lands on the last anchor and after it. An earlier edit
    // may have moved the anchors, so locate them again.
    if d.added_secs > 0 {
        if let Some((_, p1)) = anchor_positions(&trip.stop_times, d) {
            for st in &mut trip.stop_times[p1..] {
                st.arrival_secs += d.added_secs;
                st.departure_secs += d.added_secs;
            }
        }
    }
}

/// Take cancelled trips out of a day. The input is never mutated.
/// A trip instance that any cancel names ([`TripCancel::cancels`]) does not
/// run. Every other trip is kept whole, in order (D-45).
pub fn cancel_day(day: &[MaterializedTrip], cancels: &[TripCancel]) -> Vec<MaterializedTrip> {
    day.iter().filter(|t| !cancels.iter().any(|c| c.cancels(&t.trip, t.start_secs))).cloned().collect()
}

/// The first occurrence of each anchor in a trip's stop times, in order. `None`
/// means the trip is not affected.
fn anchor_positions(stop_times: &[StopTime], d: &Diversion) -> Option<(usize, usize)> {
    let p0 = stop_times.iter().position(|st| st.stop == d.first_kept_stop)?;
    let p1 = stop_times[p0 + 1..]
        .iter()
        .position(|st| st.stop == d.last_kept_stop)
        .map(|p| p + p0 + 1)?;
    Some((p0, p1))
}

// Validation: the edits against the network

/// Validate one record against the network. Count the trips it would touch,
/// over every schedule whose pattern matches. There is no date and no calendar
/// expansion. The function is pure and total.
pub fn report_against_network(network: &Network, records: &[EditRecord]) -> Vec<EditReport> {
    records
        .iter()
        .map(|record| {
            let verdict = match (&record.diversion, &record.cancel) {
                (Some(d), None) => validate_against_network(network, d).map(|()| affected_trips(network, d)),
                (None, Some(c)) => cancelled_instances(network, c),
                _ => Err("a record carries exactly one edit".into()),
            };
            match verdict {
                Err(error) => EditReport {
                    id: record.id.clone(),
                    valid: false,
                    error: Some(error),
                    affected_trips: 0,
                },
                Ok(affected_trips) => EditReport { id: record.id.clone(), valid: true, error: None, affected_trips },
            }
        })
        .collect()
}

/// How many instances a cancel takes away. Its trip must exist. A named
/// instance must be one the trip runs.
fn cancelled_instances(network: &Network, c: &TripCancel) -> Result<usize, String> {
    let (_, _, trip) = network.trip(&c.trip_id).ok_or_else(|| format!("unknown trip {}", c.trip_id))?;
    let starts = trip.departs.instances();
    match c.start_secs {
        None => Ok(starts.len()),
        Some(s) if starts.contains(&s) => Ok(1),
        Some(s) => Err(format!("trip {} has no instance starting at {s}", c.trip_id)),
    }
}

fn validate_against_network(network: &Network, d: &Diversion) -> Result<(), String> {
    if !network.routes.contains_key(&d.route_id) {
        return Err(format!("unknown route {}", d.route_id));
    }
    for stop in [&d.first_kept_stop, &d.last_kept_stop] {
        if !network.stops.contains_key(stop) {
            return Err(format!("unknown anchor stop {stop}"));
        }
    }
    if d.first_kept_stop == d.last_kept_stop {
        return Err("anchor stops must differ".into());
    }
    for skipped in &d.skipped_stops {
        if !network.stops.contains_key(skipped) {
            return Err(format!("unknown skipped stop {skipped}"));
        }
        if skipped == &d.first_kept_stop || skipped == &d.last_kept_stop {
            return Err("skipped stops must not include an anchor".into());
        }
    }
    if d.start_date > d.end_date {
        return Err("start date is after end date".into());
    }
    if let Some(detour) = &d.detour {
        if detour.len() < 2 {
            return Err("a detour needs at least two points".into());
        }
        if detour.iter().any(|p| !p.lat.is_finite() || !p.lon.is_finite()) {
            return Err("detour contains non-finite coordinates".into());
        }
    }
    Ok(())
}

/// How many trip instances the edit touches: every schedule of the route whose
/// pattern carries both anchors in order.
fn affected_trips(network: &Network, d: &Diversion) -> usize {
    network
        .pattern_trips
        .values()
        .filter(|pattern_trips| {
            network
                .patterns
                .get(&pattern_trips.pattern)
                .is_some_and(|pattern| pattern_matches(pattern, d))
        })
        .map(|pattern_trips| pattern_trips.trips.iter().map(|t| t.departs.instances().len()).sum::<usize>())
        .sum()
}

fn pattern_matches(pattern: &transit_network::Pattern, d: &Diversion) -> bool {
    if pattern.route != d.route_id {
        return false;
    }
    let stops = &pattern.stops;
    let Some(p0) = stops.iter().position(|ps| ps.stop == d.first_kept_stop) else {
        return false;
    };
    stops[p0 + 1..].iter().any(|ps| ps.stop == d.last_kept_stop)
}

// Boards

/// The board at one stop: every visit the patched trips make to that stop from
/// `from_secs` on, soonest first. A stop passed twice appears twice. A trip's
/// final visit is an arrival, so a bus that ends here is not on the board. A
/// loop's first visit to its terminal still departs.
pub fn board(
    network: &Network,
    day: &[MaterializedTrip],
    stop_id: &str,
    from_secs: u32,
    limit: usize,
) -> Vec<BoardDeparture> {
    board_over(network, day, &[stop_id.to_string()], from_secs, limit)
}

/// The station board: the board pooled over the walking group (the stop, its
/// parent, siblings sharing that parent, and the parent's children). A station
/// pin has no departures of its own. Its bays carry the trips.
pub fn station_board(
    network: &Network,
    day: &[MaterializedTrip],
    stop_id: &str,
    from_secs: u32,
    limit: usize,
) -> Vec<BoardDeparture> {
    let group: Vec<String> = transit_network::walking_group(network, stop_id)
        .into_keys()
        .collect();
    board_over(network, day, &group, from_secs, limit)
}

/// One row of the board: the expected departure, the delay against the patched
/// time, the live status, the wait, and the alerts that speak about it.
/// Schedule-only rows say `no_prediction`.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, TS)]
#[ts(export)]
pub struct BoardRow {
    #[serde(flatten)]
    #[ts(flatten)]
    pub departure: BoardDeparture,
    pub expected_secs: Option<u32>,
    pub delay_secs: Option<i32>,
    pub status: LiveStatus,
    /// Time from `from_secs` until it leaves: the expected time when live, else
    /// the patched time.
    pub wait: Wait,
    pub alerts: Vec<String>,
}

/// How long until a departure leaves, as a rider reads it: due under a
/// minute, whole minutes (rounded down) under an hour, then the clock time, in
/// seconds after the service-day anchor.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, TS)]
#[ts(export)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Wait {
    Due,
    Minutes { minutes: u32 },
    Clock { secs: u32 },
}

/// `wait` is the one rule: the only place a departure time becomes "how long".
pub fn wait(at_secs: u32, from_secs: u32) -> Wait {
    wait_after(at_secs, from_secs as i64)
}

/// The wait from an instant on the same clock. The instant may precede the
/// anchor (the hour before a fall-back day's anchor).
fn wait_after(at_secs: u32, from: i64) -> Wait {
    let d = (at_secs as i64 - from).max(0) as u32;
    if d < 60 {
        Wait::Due
    } else if d < 3_600 {
        Wait::Minutes { minutes: d / 60 }
    } else {
        Wait::Clock { secs: at_secs }
    }
}

/// A departure the edits took away from this stop, and why: the diversion that
/// skips the stop, or the cancel that takes the trip out. A dispatcher's cancel
/// shows here, unlike a live-feed cancellation (D-100, D-73). Neither applies
/// when the trip stops reaching the stop for another reason.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, TS)]
#[ts(export)]
pub struct SkippedVisit {
    #[serde(flatten)]
    #[ts(flatten)]
    pub departure: BoardDeparture,
    pub diversion: Option<String>,
    pub cancel: Option<String>,
    pub alerts: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, TS)]
#[ts(export)]
pub struct StopBoard {
    pub rows: Vec<BoardRow>,
    /// Scheduled departures the in-force edits take away here, soonest first.
    /// They show why a stop is emptier than its schedule.
    pub skipped: Vec<SkippedVisit>,
    /// Live alerts that speak about a row, a skipped visit, or the stop.
    pub alerts: Vec<LiveAlert>,
    /// The stop had nothing of its own and is in a walking group. The rows are
    /// the group's, each naming its bay.
    pub pooled: bool,
    pub live: bool,
}

/// The edits behind an effective day: the unpatched day, and the diversions and
/// cancels in force. A board compares against them.
#[derive(Debug, Clone)]
pub struct DayEdits {
    pub base: Vec<MaterializedTrip>,
    pub diversions: Vec<Diversion>,
    pub cancels: Vec<TripCancel>,
}

/// The day for (network, edits, date): the network materialized for the date,
/// patched with the diversions in force, with cancelled trips taken out. With
/// an edit in force, it also holds what the edits took away. With none in
/// force, the trips are the materialized day, so one materialization answers.
/// A pure function of the documents [`day_key`] names. A host may hold it in
/// the kernel's derived registry, so every reader answers the same.
#[derive(Debug, Clone)]
pub struct Day {
    /// The trips that run: the patched day, minus cancelled trips.
    pub trips: Vec<MaterializedTrip>,
    /// The unpatched day and the edits in force. `None` when no edit is in
    /// force, because nothing was taken away.
    pub edits: Option<DayEdits>,
}

pub fn day(network: &Network, date: NaiveDate, records: &[EditRecord]) -> Result<Day, transit_network::NetworkError> {
    let base = materialize(network, date)?;
    let diversions = transit_sync::in_force_on(records, date);
    let cancels = transit_sync::cancels_in_force_on(records, date);
    if diversions.is_empty() && cancels.is_empty() {
        return Ok(Day { trips: base, edits: None });
    }
    let trips = cancel_day(&patch_day(&base, &diversions), &cancels);
    Ok(Day { trips, edits: Some(DayEdits { base, diversions, cancels }) })
}

/// The key a held [`Day`] is found by: the documents and the date it is a
/// function of.
pub fn day_key(network_hash: &str, edits_hash: Option<&str>, date: &str) -> String {
    format!("{network_hash}|{}|{date}", edits_hash.unwrap_or("-"))
}

/// A day request's date, read before any document. A date that does not parse
/// is the caller's error, not a missing document.
pub fn day_date(date: &str) -> Result<NaiveDate, String> {
    NaiveDate::parse_from_str(date, "%Y-%m-%d").map_err(|e| format!("invalid date {date:?}: {e}"))
}

/// [`day`], with the refusal a capability answers. Every capability computes
/// the day here, under [`day_key`], so one assembly holds one copy.
pub fn day_of(network: &Network, records: &[EditRecord], date: NaiveDate) -> Result<Day, String> {
    day(network, date, records).map_err(|e| format!("the network does not materialize: {e}"))
}

/// Live delays never reach back further than this in a search for late buses.
/// A 3-hour delay is a data fault.
const MAX_LOOKBACK_SECS: u32 = 3 * 3600;

/// The board: when service leaves this stop, realtime-first, in the order
/// buses actually leave. A late bus scheduled before `from_secs` stays until it
/// leaves. An early bus that already left is not. A visit the live document
/// cancels or skips is silent (D-73).
/// With `edits`, the departures they took away are `skipped`, each with its
/// diversion or cancel. Live alerts about the board ride along.
/// A stop with nothing of its own (no departures, no skips) in a walking group
/// answers its group's board. A plain stop never pools, so a diverted stop
/// shows its emptiness and the reason.
pub fn stop_board(
    network: &Network,
    day: &[MaterializedTrip],
    stop_id: &str,
    from_secs: u32,
    limit: usize,
    live: Option<&LiveDay>,
    edits: Option<&DayEdits>,
) -> StopBoard {
    let own = [stop_id.to_string()];
    let rows = live_rows(network, day, &own, from_secs, limit, live);
    let skipped = skipped_visits(network, day, &own, from_secs, limit, edits);
    if rows.is_empty() && skipped.is_empty() {
        let group: Vec<String> = transit_network::walking_group(network, stop_id).into_keys().collect();
        if group.len() >= 2 {
            let rows = live_rows(network, day, &group, from_secs, limit, live);
            let skipped = skipped_visits(network, day, &group, from_secs, limit, edits);
            return with_alerts(network, rows, skipped, &group, true, live);
        }
    }
    with_alerts(network, rows, skipped, &own, false, live)
}

/// One group on a stop's display: a route to one destination, its next
/// departures, and the visits the edits took from it.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, TS)]
#[ts(export)]
pub struct DepartureGroup {
    pub route: String,
    pub route_name: String,
    pub route_long_name: Option<String>,
    /// The route's authored colors (hex, no `#`), when it has them.
    pub route_color: Option<String>,
    pub route_text_color: Option<String>,
    pub destination: String,
    /// Next departures, soonest first. Empty once the day's last one has left.
    pub departures: Vec<BoardRow>,
    pub skipped: Vec<SkippedVisit>,
    /// The group's next run after it has stopped for the day. `None` while
    /// departures are left, or when none falls within the week.
    pub next: Option<NextService>,
}

/// How many days past the board's date [`with_next_service`] looks (D-42): a
/// week, then nothing.
pub const NEXT_SERVICE_DAYS: u32 = 7;

/// A stopped group's next departure on a later day (D-42).
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, TS)]
#[ts(export)]
pub struct NextService {
    /// The service date it runs on, "YYYY-MM-DD".
    pub date: String,
    /// Days after the board's date. 1 is tomorrow. The maximum is
    /// [`NEXT_SERVICE_DAYS`].
    pub days_ahead: u32,
    /// The date's weekday, in English ("Monday").
    pub weekday: String,
    /// Seconds after that date's service-day midnight.
    pub departure_secs: u32,
    /// The stop it leaves from (a bay, when the board pools).
    pub stop: String,
}

/// Fill each stopped group's [`DepartureGroup::next`] with the earliest
/// departure of its route to its destination. Take it from the first of the
/// next [`NEXT_SERVICE_DAYS`] dates that has one. `day_on` answers each date's
/// patched day, never the live document. A departure counts only if it leaves
/// at or after `from_secs` (which may run past midnight). Dates are asked in
/// order, and only while a stopped group has no answer.
pub fn with_next_service<D: std::ops::Deref<Target = Day>>(
    network: &Network,
    board: &mut GroupedBoard,
    stop_id: &str,
    date: NaiveDate,
    from_secs: u32,
    mut day_on: impl FnMut(NaiveDate) -> Result<D, String>,
) -> Result<(), String> {
    let stops: Vec<String> = if board.pooled {
        transit_network::walking_group(network, stop_id).into_keys().collect()
    } else {
        vec![stop_id.to_string()]
    };
    let mut pending: Vec<usize> = (0..board.groups.len()).filter(|&i| board.groups[i].departures.is_empty()).collect();
    for k in 1..=NEXT_SERVICE_DAYS {
        if pending.is_empty() {
            break;
        }
        let Some(on) = date.checked_add_days(chrono::Days::new(k as u64)) else { break };
        let day = day_on(on)?;
        let rows = board_over(network, &day.trips, &stops, from_secs.saturating_sub(k * 86_400), usize::MAX);
        pending.retain(|&i| {
            let group = &mut board.groups[i];
            let Some(row) = rows.iter().find(|r| r.route == group.route && r.destination == group.destination) else {
                return true;
            };
            group.next = Some(NextService {
                date: on.format("%Y-%m-%d").to_string(),
                days_ahead: k,
                weekday: on.format("%A").to_string(),
                departure_secs: row.departure_secs,
                stop: row.stop.clone(),
            });
            false
        });
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, TS)]
#[ts(export)]
pub struct GroupedBoard {
    pub groups: Vec<DepartureGroup>,
    /// Live alerts about the stop, or any departure still to leave it.
    pub alerts: Vec<LiveAlert>,
    pub pooled: bool,
    pub live: bool,
}

/// The board by route and destination, for a stop's display: one group per
/// route and destination that serves the stop on the day. A group whose last
/// departure has left stays, empty. Each group holds its next `per_group`
/// departures and the visits the edits took away. The day is the stop's whole
/// day. A stop with nothing of its own all day, in a walking group, pools it,
/// like the board.
/// Groups list in route display order (`sort_order`, lower first; routes
/// without one after, by id), then by destination.
pub fn grouped_board(
    network: &Network,
    day: &[MaterializedTrip],
    stop_id: &str,
    from_secs: u32,
    per_group: usize,
    live: Option<&LiveDay>,
    edits: Option<&DayEdits>,
) -> GroupedBoard {
    let served = |stops: &[String]| -> BTreeSet<(String, String)> {
        let base = edits.map(|e| e.base.as_slice()).unwrap_or_default();
        board_over(network, day, stops, 0, usize::MAX)
            .into_iter()
            .chain(board_over(network, base, stops, 0, usize::MAX))
            .map(|d| (d.route, d.destination))
            .collect()
    };
    let own = vec![stop_id.to_string()];
    let mut stops = own.clone();
    let mut keys = served(&own);
    let mut pooled = false;
    if keys.is_empty() {
        let group: Vec<String> = transit_network::walking_group(network, stop_id).into_keys().collect();
        if group.len() >= 2 {
            keys = served(&group);
            stops = group;
            pooled = true;
        }
    }
    let rows = live_rows(network, day, &stops, from_secs, usize::MAX, live);
    let skipped = skipped_visits(network, day, &stops, from_secs, usize::MAX, edits);
    let board = with_alerts(network, rows, skipped, &stops, pooled, live);

    let mut keys: Vec<(String, String)> = keys.into_iter().collect();
    let order = |route: &str| network.routes.get(route).and_then(|r| r.sort_order);
    keys.sort_by(|a, b| {
        let (sa, sb) = (order(&a.0), order(&b.0));
        (sa.is_none(), sa, &a.0, &a.1).cmp(&(sb.is_none(), sb, &b.0, &b.1))
    });
    let groups = keys
        .into_iter()
        .map(|(route, destination)| {
            let ours = |d: &BoardDeparture| d.route == route && d.destination == destination;
            let r = network.routes.get(&route);
            DepartureGroup {
                route_name: route_name(network, &route),
                route_long_name: r.and_then(|r| r.long_name.clone()),
                route_color: r.and_then(|r| r.color.clone()),
                route_text_color: r.and_then(|r| r.text_color.clone()),
                departures: board.rows.iter().filter(|x| ours(&x.departure)).take(per_group).cloned().collect(),
                skipped: board.skipped.iter().filter(|x| ours(&x.departure)).take(per_group).cloned().collect(),
                next: None,
                route,
                destination,
            }
        })
        .collect();
    GroupedBoard { groups, alerts: board.alerts, pooled: board.pooled, live: board.live }
}

// A board at a moment: its date, and the previous date's late trips

#[derive(Debug, Clone, Copy)]
pub struct ServiceDay<'a> {
    /// "YYYY-MM-DD".
    pub date: &'a str,
    /// The trips that run ([`Day::trips`]).
    pub trips: &'a [MaterializedTrip],
    /// What the edits took away ([`Day::edits`]).
    pub edits: Option<&'a DayEdits>,
}

/// The previous service date at the board's moment: its day, and the moment
/// on its own clock (past 86 400 is after midnight). Its length runs anchor to
/// anchor: 86 400, or 23 h or 25 h across a clock change. The host answers
/// both.
#[derive(Debug, Clone, Copy)]
pub struct Overnight<'a> {
    pub day: ServiceDay<'a>,
    pub from_secs: u32,
    pub length_secs: u32,
}

/// When and where a board is asked: a service date and the moment on its
/// clock. The moment clamps to 0 before the anchor (the hour before a fall-back
/// day's anchor). With `previous`, that date at the same moment is included.
#[derive(Debug, Clone, Copy)]
pub struct BoardAt<'a> {
    pub today: ServiceDay<'a>,
    pub from_secs: u32,
    pub previous: Option<Overnight<'a>>,
}

impl BoardAt<'_> {
    /// The moment on today's clock, signed. A clamped 0 before the anchor means
    /// the previous clock's moment, less that day's length.
    fn today_secs(&self) -> i64 {
        match self.previous {
            Some(p) if self.from_secs == 0 => (p.from_secs as i64 - p.length_secs as i64).min(0),
            _ => self.from_secs as i64,
        }
    }
}

/// How long until a row leaves, on its own clock: one order across both dates.
fn leaves_in(at_secs: u32, from: i64) -> i64 {
    at_secs as i64 - from
}

fn dated(d: &mut BoardDeparture, date: &str) {
    d.date = Some(date.to_string());
}

/// Rows of both dates in one order, by how long until each leaves (the expected
/// time when live). Each wait is counted on its own clock.
fn merge_rows(today: Vec<BoardRow>, today_from: i64, previous: Vec<BoardRow>, previous_from: i64, limit: usize) -> Vec<BoardRow> {
    let mut all: Vec<(i64, BoardRow)> = today
        .into_iter()
        .map(|r| (today_from, r))
        .chain(previous.into_iter().map(|r| (previous_from, r)))
        .map(|(from, mut r)| {
            let at = r.expected_secs.unwrap_or(r.departure.departure_secs);
            r.wait = wait_after(at, from);
            (leaves_in(at, from), r)
        })
        .collect();
    all.sort_by(|(a, x), (b, y)| {
        a.cmp(b)
            .then_with(|| x.departure.trip.cmp(&y.departure.trip))
            .then_with(|| x.departure.date.cmp(&y.departure.date))
    });
    all.into_iter().take(limit).map(|(_, r)| r).collect()
}

fn merge_skipped(
    today: Vec<SkippedVisit>,
    today_from: i64,
    previous: Vec<SkippedVisit>,
    previous_from: i64,
    limit: usize,
) -> Vec<SkippedVisit> {
    let mut all: Vec<(i64, SkippedVisit)> = today
        .into_iter()
        .map(|s| (leaves_in(s.departure.departure_secs, today_from), s))
        .chain(previous.into_iter().map(|s| (leaves_in(s.departure.departure_secs, previous_from), s)))
        .collect();
    all.sort_by(|(a, x), (b, y)| a.cmp(b).then_with(|| x.departure.trip.cmp(&y.departure.trip)));
    all.into_iter().take(limit).map(|(_, s)| s).collect()
}

/// Alerts of both dates' boards: each once, in order of first mention.
fn merge_alerts(today: Vec<LiveAlert>, previous: Vec<LiveAlert>) -> Vec<LiveAlert> {
    let mut out = today;
    for a in previous {
        if !out.iter().any(|b| b.id == a.id) {
            out.push(a);
        }
    }
    out
}

/// The board ([`stop_board`]) at a moment, with the previous date's departures
/// still to leave. Each date reads its own edits and live instances. An
/// instance with no date is today's ([`LiveDay::on`]). Rows carry their date
/// and order by wait.
pub fn stop_board_at(network: &Network, at: &BoardAt, stop_id: &str, limit: usize, live: Option<&LiveDay>) -> StopBoard {
    let current = at.today.date;
    let today_live = live.map(|l| l.on(current, current));
    let mut board = stop_board(network, at.today.trips, stop_id, at.from_secs, limit, today_live.as_ref(), at.today.edits);
    for r in &mut board.rows {
        dated(&mut r.departure, current);
    }
    for s in &mut board.skipped {
        dated(&mut s.departure, current);
    }
    let today_from = at.today_secs();
    let Some(prev) = at.previous else {
        board.rows = merge_rows(board.rows, today_from, Vec::new(), 0, limit);
        return board;
    };
    let prev_live = live.map(|l| l.on(prev.day.date, current));
    let mut earlier = stop_board(network, prev.day.trips, stop_id, prev.from_secs, limit, prev_live.as_ref(), prev.day.edits);
    for r in &mut earlier.rows {
        dated(&mut r.departure, prev.day.date);
    }
    for s in &mut earlier.skipped {
        dated(&mut s.departure, prev.day.date);
    }
    let prev_from = prev.from_secs as i64;
    StopBoard {
        rows: merge_rows(board.rows, today_from, earlier.rows, prev_from, limit),
        skipped: merge_skipped(board.skipped, today_from, earlier.skipped, prev_from, limit),
        alerts: merge_alerts(board.alerts, earlier.alerts),
        pooled: board.pooled,
        live: board.live || earlier.live,
    }
}

/// The grouped board ([`grouped_board`]) at a moment, answered as
/// [`stop_board_at`] answers. Each group's departures of both dates are in one
/// order. A group only the previous date runs is included while it has
/// departures to leave.
pub fn grouped_board_at(network: &Network, at: &BoardAt, stop_id: &str, per_group: usize, live: Option<&LiveDay>) -> GroupedBoard {
    let current = at.today.date;
    let today_live = live.map(|l| l.on(current, current));
    let mut board = grouped_board(network, at.today.trips, stop_id, at.from_secs, per_group, today_live.as_ref(), at.today.edits);
    let today_from = at.today_secs();
    for g in &mut board.groups {
        for r in &mut g.departures {
            dated(&mut r.departure, current);
        }
        for s in &mut g.skipped {
            dated(&mut s.departure, current);
        }
        g.departures = merge_rows(std::mem::take(&mut g.departures), today_from, Vec::new(), 0, per_group);
    }
    let Some(prev) = at.previous else { return board };
    let prev_live = live.map(|l| l.on(prev.day.date, current));
    let earlier = grouped_board(network, prev.day.trips, stop_id, prev.from_secs, per_group, prev_live.as_ref(), prev.day.edits);
    let prev_from = prev.from_secs as i64;
    for mut e in earlier.groups {
        if e.departures.is_empty() && e.skipped.is_empty() {
            continue;
        }
        for r in &mut e.departures {
            dated(&mut r.departure, prev.day.date);
        }
        for s in &mut e.skipped {
            dated(&mut s.departure, prev.day.date);
        }
        match board.groups.iter_mut().find(|g| g.route == e.route && g.destination == e.destination) {
            Some(g) => {
                g.departures = merge_rows(std::mem::take(&mut g.departures), today_from, e.departures, prev_from, per_group);
                g.skipped = merge_skipped(std::mem::take(&mut g.skipped), today_from, e.skipped, prev_from, per_group);
            }
            None => {
                e.departures = merge_rows(Vec::new(), today_from, e.departures, prev_from, per_group);
                board.groups.push(e);
            }
        }
    }
    let order = |route: &str| network.routes.get(route).and_then(|r| r.sort_order);
    board.groups.sort_by(|a, b| {
        let (sa, sb) = (order(&a.route), order(&b.route));
        (sa.is_none(), sa, &a.route, &a.destination).cmp(&(sb.is_none(), sb, &b.route, &b.destination))
    });
    board.alerts = merge_alerts(board.alerts, earlier.alerts);
    board.live |= earlier.live;
    board
}

/// What an alert reaches on a day: each departure (not a trip's last visit)
/// that the board would show it on. A scope selects a visit when every field it
/// names matches (`LiveAlert::touches`), or when it names the stop and no route
/// or trip (`touches_stop`). A trip scope speaks on its date only
/// ([`AlertScope::on`]).
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize, TS)]
#[ts(export)]
pub struct Reach {
    pub departures: u32,
    /// Each stop it reaches, by id, with its departures there.
    pub stops: Vec<ReachedStop>,
    pub routes: Vec<String>,
    /// Each departure it reaches as (stop, trip, start), for a law to compare.
    #[serde(skip)]
    #[ts(skip)]
    pub visits: Vec<(String, String, u32)>,
}

/// One stop an alert reaches.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, TS)]
#[ts(export)]
pub struct ReachedStop {
    pub stop: String,
    pub name: String,
    pub lat: f64,
    pub lon: f64,
    pub departures: u32,
}

pub fn alert_reach(network: &Network, day: &[MaterializedTrip], scopes: &[transit_live::AlertScope], date: &str) -> Reach {
    reach_over(network, departures_of(day), scopes, date)
}

/// [`alert_reach`] over a held day, as the board shows it: the patched day's
/// departures, and the base day's departures that the edits took away (the
/// board's skipped rows).
pub fn alert_reach_day(network: &Network, day: &Day, scopes: &[transit_live::AlertScope], date: &str) -> Reach {
    let mut all = departures_of(&day.trips);
    if let Some(edits) = &day.edits {
        let kept: BTreeSet<(String, u32, String)> = all.iter().map(|d| (d.trip.to_string(), d.start_secs, d.stop.to_string())).collect();
        all.extend(departures_of(&edits.base).into_iter().filter(|d| !kept.contains(&(d.trip.to_string(), d.start_secs, d.stop.to_string()))));
    }
    reach_over(network, all, scopes, date)
}

struct Departure<'a> {
    route: &'a str,
    pattern: &'a str,
    trip: &'a str,
    start_secs: u32,
    stop: &'a str,
}

/// Every departure of a day: each visit but its trip's last.
fn departures_of(day: &[MaterializedTrip]) -> Vec<Departure<'_>> {
    day.iter()
        .flat_map(|t| {
            let departing = t.stop_times.len().saturating_sub(1);
            t.stop_times[..departing].iter().map(move |st| Departure { route: &t.route, pattern: &t.pattern, trip: &t.trip, start_secs: t.start_secs, stop: &st.stop })
        })
        .collect()
}

fn reach_over(network: &Network, departures: Vec<Departure<'_>>, scopes: &[transit_live::AlertScope], date: &str) -> Reach {
    let alert = LiveAlert { scopes: scopes.iter().filter(|s| s.on(date, date)).cloned().collect(), ..Default::default() };
    if alert.scopes.is_empty() {
        return Reach::default();
    }
    let mut by_stop: BTreeMap<String, u32> = BTreeMap::new();
    let mut routes: BTreeSet<String> = BTreeSet::new();
    let mut visits = Vec::new();
    for d in departures {
        let direction = network.patterns.get(d.pattern).and_then(|p| p.direction.as_deref());
        let visit = Visit { route: d.route, direction, stop: d.stop, trip: d.trip, start_secs: d.start_secs };
        if alert.touches(&visit) || alert.touches_stop(d.stop) {
            *by_stop.entry(d.stop.to_string()).or_default() += 1;
            routes.insert(d.route.to_string());
            visits.push((d.stop.to_string(), d.trip.to_string(), d.start_secs));
        }
    }
    let stops: Vec<ReachedStop> = by_stop
        .into_iter()
        .map(|(stop, departures)| {
            let at = network.stops.get(&stop);
            ReachedStop {
                name: stop_name(network, Some(&stop)),
                lat: at.map(|s| s.lat).unwrap_or_default(),
                lon: at.map(|s| s.lon).unwrap_or_default(),
                stop,
                departures,
            }
        })
        .collect();
    Reach { departures: visits.len() as u32, stops, routes: routes.into_iter().collect(), visits }
}

/// Attach live alerts. Each row and skipped visit lists the ids that touch it.
/// The board lists every alert that touches a row, a skipped visit, or the
/// stops, in the document's order.
fn with_alerts(
    network: &Network,
    mut rows: Vec<BoardRow>,
    mut skipped: Vec<SkippedVisit>,
    stops: &[String],
    pooled: bool,
    live: Option<&LiveDay>,
) -> StopBoard {
    let all = live.map(|l| l.alerts.as_slice()).unwrap_or_default();
    let ids = |d: &BoardDeparture| -> Vec<String> {
        let direction = network.patterns.get(&d.pattern).and_then(|p| p.direction.as_deref());
        let visit = Visit { route: &d.route, direction, stop: &d.stop, trip: &d.trip, start_secs: d.start_secs };
        all.iter().filter(|a| a.touches(&visit) || a.touches_stop(&d.stop)).map(|a| a.id.clone()).collect()
    };
    for r in &mut rows {
        r.alerts = ids(&r.departure);
    }
    for s in &mut skipped {
        s.alerts = ids(&s.departure);
    }
    let alerts = all
        .iter()
        .filter(|a| {
            stops.iter().any(|s| a.touches_stop(s))
                || rows.iter().any(|r| r.alerts.contains(&a.id))
                || skipped.iter().any(|s| s.alerts.contains(&a.id))
        })
        .cloned()
        .collect();
    StopBoard { rows, skipped, alerts, pooled, live: live.is_some() }
}

/// The base day's departures from these stops that the patched day no
/// longer makes, from `from_secs` on, soonest first.
fn skipped_visits(
    network: &Network,
    day: &[MaterializedTrip],
    stops: &[String],
    from_secs: u32,
    limit: usize,
    edits: Option<&DayEdits>,
) -> Vec<SkippedVisit> {
    let Some(edits) = edits else { return Vec::new() };
    let kept: BTreeSet<(String, u32, String)> = board_over(network, day, stops, 0, usize::MAX)
        .into_iter()
        .map(|d| (d.trip, d.start_secs, d.stop))
        .collect();
    board_over(network, &edits.base, stops, from_secs, usize::MAX)
        .into_iter()
        .filter(|d| !kept.contains(&(d.trip.clone(), d.start_secs, d.stop.clone())))
        .take(limit)
        .map(|d| {
            let cancel = edits.cancels.iter().find(|c| c.cancels(&d.trip, d.start_secs)).map(|c| c.id.clone());
            SkippedVisit {
                diversion: cancel
                    .is_none()
                    .then(|| edits.diversions.iter().find(|v| v.route_id == d.route && v.skipped_stops.contains(&d.stop)))
                    .flatten()
                    .map(|v| v.id.clone()),
                cancel,
                departure: d,
                alerts: Vec::new(),
            }
        })
        .collect()
}

fn live_rows(
    network: &Network,
    day: &[MaterializedTrip],
    stops: &[String],
    from_secs: u32,
    limit: usize,
    live: Option<&LiveDay>,
) -> Vec<BoardRow> {
    let window = live.map(|l| l.window).unwrap_or_default();
    let lookback = live
        .into_iter()
        .flat_map(|l| l.trips.iter().flat_map(|t| t.stops.iter()))
        .filter_map(|s| Some(s.predicted_departure?.saturating_sub(s.scheduled_departure?)))
        .max()
        .unwrap_or(0)
        .min(MAX_LOOKBACK_SECS);
    let mut rows: Vec<BoardRow> = board_over(network, day, stops, from_secs.saturating_sub(lookback), usize::MAX)
        .into_iter()
        .map(|d| {
            let expected = live
                .and_then(|l| l.visit(&d.trip, d.start_secs, &d.stop, d.departure_secs))
                .and_then(|v| v.predicted_departure);
            let delay = expected.map(|e| e as i32 - d.departure_secs as i32);
            BoardRow {
                status: classify(delay, &window),
                wait: wait(expected.unwrap_or(d.departure_secs), from_secs),
                expected_secs: expected,
                delay_secs: delay,
                departure: d,
                alerts: Vec::new(),
            }
        })
        .filter(|r| r.expected_secs.unwrap_or(r.departure.departure_secs) >= from_secs)
        // A visit the live document cancels or skips leaves silently (D-73).
        .filter(|r| {
            let d = &r.departure;
            live.is_none_or(|l| l.runs(&d.trip, d.start_secs, &d.stop, d.departure_secs))
        })
        .collect();
    rows.sort_by(|a, b| {
        let at = |r: &BoardRow| r.expected_secs.unwrap_or(r.departure.departure_secs);
        at(a)
            .cmp(&at(b))
            .then_with(|| a.departure.trip.cmp(&b.departure.trip))
            .then_with(|| a.departure.start_secs.cmp(&b.departure.start_secs))
    });
    rows.truncate(limit);
    rows
}

fn board_over(
    network: &Network,
    day: &[MaterializedTrip],
    stops: &[String],
    from_secs: u32,
    limit: usize,
) -> Vec<BoardDeparture> {
    let mut rows = Vec::new();
    for trip in day {
        // The last visit ends the trip. It is not a departure.
        let departing = trip.stop_times.len().saturating_sub(1);
        for (visit, st) in trip.stop_times[..departing].iter().enumerate() {
            if !stops.iter().any(|s| s == &st.stop) || st.departure_secs < from_secs {
                continue;
            }
            rows.push(BoardDeparture {
                trip: trip.trip.clone(),
                date: None,
                start_secs: trip.start_secs,
                route: trip.route.clone(),
                route_name: route_name(network, &trip.route),
                route_long_name: network.routes.get(&trip.route).and_then(|r| r.long_name.clone()),
                pattern: trip.pattern.clone(),
                stop: st.stop.clone(),
                stop_name: stop_name(network, Some(&st.stop)),
                platform: network.stops.get(&st.stop).and_then(|s| s.info.platform.clone()),
                departure_secs: st.departure_secs,
                destination: transit_network::visit_destination(network, trip, visit),
            });
        }
    }
    rows.sort_by(|a, b| {
        a.departure_secs
            .cmp(&b.departure_secs)
            .then_with(|| a.trip.cmp(&b.trip))
            .then_with(|| a.start_secs.cmp(&b.start_secs))
    });
    rows.truncate(limit);
    rows
}

fn route_name(network: &Network, route: &str) -> String {
    network.routes.get(route).map(|r| r.label().to_string()).unwrap_or_else(|| route.to_string())
}

fn stop_name(network: &Network, stop: Option<&str>) -> String {
    stop.map(|id| {
        network.stops.get(id).map(|s| s.name.clone()).unwrap_or_else(|| id.to_string())
    })
    .unwrap_or_default()
}

// The authored block chain

/// One block's chain for a service date, in order. Each trip is materialized
/// when it runs. A frequency trip is every instance it runs, back to back. The
/// layovers carry the ripple: the operator edits, and with a live document,
/// each trip's predicted lateness. Unknown blocks answer empty, never an error.
pub fn block_chain(
    network: &Network,
    day: &[MaterializedTrip],
    block_id: &str,
    tight_secs: u32,
    live: Option<&LiveDay>,
) -> Vec<BlockChainTrip> {
    let Some(block) = network.blocks.get(block_id) else {
        return Vec::new();
    };
    let mut rows = Vec::with_capacity(block.trips.len());
    for (index, trip_ref) in block.trips.iter().enumerate() {
        let stored = network.trip(&trip_ref.trip);
        let mut runs: Vec<&MaterializedTrip> = day.iter().filter(|t| t.trip == trip_ref.trip).collect();
        runs.sort_by_key(|t| t.start_secs);
        let layover = block
            .layovers
            .iter()
            .find(|l| l.after_trip == index)
            .map(|l| l.secs);
        if runs.is_empty() {
            rows.push(BlockChainTrip {
                trip: trip_ref.trip.clone(),
                pattern_trips: stored.map(|(key, _, _)| key.to_string()).unwrap_or_default(),
                start_secs: stored.and_then(|(_, _, t)| t.departs.first_start()).unwrap_or(0),
                route: String::new(),
                route_name: String::new(),
                pattern: String::new(),
                runs: false,
                first_stop: String::new(),
                last_stop: String::new(),
                end_secs: None,
                live_end_delay_secs: None,
                layover_after_secs: layover,
                layover_remaining_secs: None,
                layover_health: None,
            });
            continue;
        }
        let last = runs.len() - 1;
        for (k, trip) in runs.into_iter().enumerate() {
            let effective_end = trip.stop_times.last().map(|st| st.arrival_secs.max(st.departure_secs));
            // The authored end. The layover was authored against it.
            let authored_end = stored.and_then(|(_, _, t)| transit_network::trip_arrival_end_secs(t, trip.start_secs));
            let live_delay = match (live, trip.stop_times.last(), authored_end) {
                (Some(live), Some(st), Some(published)) => live
                    .visit(&trip.trip, trip.start_secs, &st.stop, published)
                    .and_then(|v| v.arrival_delay.or(v.departure_delay)),
                _ => None,
            };
            let delay = match (live_delay, effective_end, authored_end) {
                (Some(d), _, _) => d as i64,
                (None, Some(e), Some(a)) => e as i64 - a as i64,
                _ => 0,
            };
            // Only the last instance carries the layover.
            let layover = if k == last { layover } else { None };
            let layover_remaining = layover.map(|secs| secs as i64 - delay);
            rows.push(BlockChainTrip {
                trip: trip_ref.trip.clone(),
                pattern_trips: trip.pattern_trips.clone(),
                start_secs: trip.start_secs,
                route: trip.route.clone(),
                route_name: route_name(network, &trip.route),
                pattern: trip.pattern.clone(),
                runs: true,
                first_stop: trip.stop_times.first().map(|st| st.stop.clone()).unwrap_or_default(),
                last_stop: trip.stop_times.last().map(|st| st.stop.clone()).unwrap_or_default(),
                end_secs: effective_end,
                live_end_delay_secs: live_delay,
                layover_after_secs: layover,
                layover_remaining_secs: layover_remaining,
                layover_health: layover_remaining.map(|r| layover_health(r, tight_secs)),
            });
        }
    }
    rows
}

/// The effective day for a date: materialized, patched with the edits in force,
/// with cancelled trips out.
pub fn effective_day(
    network: &Network,
    date: NaiveDate,
    records: &[EditRecord],
) -> Result<Vec<MaterializedTrip>, transit_network::NetworkError> {
    let day = materialize(network, date)?;
    let diversions = transit_sync::in_force_on(records, date);
    Ok(cancel_day(&patch_day(&day, &diversions), &transit_sync::cancels_in_force_on(records, date)))
}

// The day's fleet and service keys

/// One block out on the service day, and how many of its trips run.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, TS)]
#[ts(export)]
pub struct DayBlock {
    pub block_id: String,
    /// How many of the block's trip instances run that day.
    pub trip_count: u32,
}

/// The day's blocks: every authored block with a trip in the effective day,
/// sorted by block id, with its running trip count. A block whose every trip is
/// off that day is in the garage. A picker must not offer it.
pub fn day_blocks(network: &Network, day: &[MaterializedTrip]) -> Vec<DayBlock> {
    let mut out = Vec::new();
    for (block_id, block) in &network.blocks {
        let ids: BTreeSet<&str> = block.trips.iter().map(|t| t.trip.as_str()).collect();
        let count = day.iter().filter(|d| ids.contains(d.trip.as_str())).count();
        if count > 0 {
            out.push(DayBlock { block_id: block_id.clone(), trip_count: count as u32 });
        }
    }
    out
}

/// The day's fleet in one answer (QAF-3): blocks that run, the peak concurrent
/// vehicles and when, and the platform, revenue, and layover hours. It replaces
/// one `block_trips` call per block plus host-side glue.
///
/// A vehicle is out from its block's first running start to its last running
/// arrival. Spans are half-open, so a bus arriving as another departs is one
/// vehicle. Platform is the fleet's out hours. Revenue is the trips' own
/// runtimes. Layover is the rest.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, TS)]
#[ts(export)]
pub struct FleetSummary {
    pub blocks: u32,
    pub peak_vehicles: u32,
    /// The first time the peak is reached; None on an empty day.
    pub peak_at_secs: Option<u32>,
    /// Every running block's span (last arrival minus first start), in hours.
    pub platform_hours: f64,
    /// The running trips' own runtimes (arrival − start), hours.
    pub revenue_hours: f64,
    /// Platform minus revenue, in hours: recovery, pull gaps, and the authored
    /// layovers the day actually runs.
    pub layover_hours: f64,
    /// Each running block's outline, in block order: span, trips, longest gap.
    pub spans: Vec<BlockSpan>,
}

/// One running block's day in outline. Its longest gap is the wait between
/// one trip's end and the next start, where the block might be cut.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, TS)]
#[ts(export)]
pub struct BlockSpan {
    pub block_id: String,
    pub trips: u32,
    pub first_start_secs: u32,
    pub last_end_secs: u32,
    /// `last_end_secs − first_start_secs`.
    pub span_secs: u32,
    /// The longest gap between consecutive trips (the first of equal ones), and
    /// when it starts (the earlier trip's end). `None` for a block running one
    /// trip.
    pub longest_gap_secs: Option<u32>,
    pub longest_gap_at_secs: Option<u32>,
}

/// The day's fleet: every block with a running trip, folded into the peak and
/// the hour bases. The caller supplies the day, as every day view does.
pub fn fleet_summary(network: &Network, day: &[MaterializedTrip]) -> FleetSummary {
    let mut blocks = 0u32;
    let mut platform_secs = 0u64;
    let mut revenue_secs = 0u64;
    // (time, delta): a block's span is half-open [start, end).
    let mut events: Vec<(u32, i64)> = Vec::new();
    let mut spans = Vec::new();
    for (block_id, block) in &network.blocks {
        let ids: BTreeSet<&str> = block.trips.iter().map(|t| t.trip.as_str()).collect();
        let mut runs: Vec<(u32, u32)> = day
            .iter()
            .filter(|t| ids.contains(t.trip.as_str()))
            .map(|trip| {
                let end = trip
                    .stop_times
                    .last()
                    .map(|st| st.arrival_secs.max(st.departure_secs))
                    .unwrap_or(trip.start_secs);
                (trip.start_secs, end)
            })
            .collect();
        if runs.is_empty() {
            continue;
        }
        runs.sort_unstable();
        blocks += 1;
        revenue_secs += runs.iter().map(|(s, e)| e.saturating_sub(*s) as u64).sum::<u64>();
        let start = runs[0].0;
        let end = runs.iter().map(|r| r.1).max().expect("running block has an end");
        platform_secs += end.saturating_sub(start) as u64;
        events.push((start, 1));
        events.push((end, -1));
        let mut longest: Option<(u32, u32)> = None;
        for pair in runs.windows(2) {
            let gap = pair[1].0.saturating_sub(pair[0].1);
            if longest.is_none_or(|(g, _)| gap > g) {
                longest = Some((gap, pair[0].1));
            }
        }
        spans.push(BlockSpan {
            block_id: block_id.clone(),
            trips: runs.len() as u32,
            first_start_secs: start,
            last_end_secs: end,
            span_secs: end - start,
            longest_gap_secs: longest.map(|l| l.0),
            longest_gap_at_secs: longest.map(|l| l.1),
        });
    }
    // -1 sorts before +1 at one second. An arriving bus does not double-count.
    events.sort();
    let mut current = 0i64;
    let mut peak = 0u32;
    let mut peak_at: Option<u32> = None;
    for (time, delta) in events {
        current += delta;
        if current > peak as i64 {
            peak = current as u32;
            peak_at = Some(time);
        }
    }
    FleetSummary {
        blocks,
        peak_vehicles: peak,
        peak_at_secs: peak_at,
        platform_hours: platform_secs as f64 / 3600.0,
        revenue_hours: revenue_secs as f64 / 3600.0,
        layover_hours: (platform_secs - revenue_secs) as f64 / 3600.0,
        spans,
    }
}

/// The service keys running on a date: the distinct service types of the
/// effective day's trips, sorted. A key whose every trip is skipped by edits,
/// or that the model never represented, is not running, however its calendar
/// reads.
pub fn active_services(network: &Network, day: &[MaterializedTrip]) -> Vec<String> {
    let mut keys: BTreeSet<&str> = BTreeSet::new();
    for trip in day {
        if let Some(pattern_trips) = network.pattern_trips.get(&trip.pattern_trips) {
            keys.insert(pattern_trips.service_type.as_str());
        }
    }
    keys.into_iter().map(String::from).collect()
}

/// The dates one service type runs on in a window: the calendar read backwards
/// ("When does 1-WKDY/31 run?", the inverse of active_services). An unknown
/// type, a type no trips run under, or an inverted window answers empty, never
/// an error. With no records, the rule's dates answer. With records, the
/// effective day decides each date.
pub fn service_dates(
    network: &Network,
    records: &[EditRecord],
    service_type: &str,
    from: NaiveDate,
    to: NaiveDate,
) -> Result<Vec<NaiveDate>, transit_network::NetworkError> {
    // A type no trips run under never runs, however its rule reads. The two-way
    // agreement with active_services depends on it.
    if !network.pattern_trips.values().any(|s| s.service_type == service_type) {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    let mut date = from;
    while date <= to {
        let runs = if records.is_empty() {
            network
                .service_types
                .get(service_type)
                .is_some_and(|t| t.rule.runs_on(date))
        } else {
            let day = effective_day(network, date, records)?;
            active_services(network, &day).iter().any(|s| s == service_type)
        };
        if runs {
            out.push(date);
        }
        date += chrono::Duration::days(1);
    }
    Ok(out)
}

/// One stop of a trip instance's day view, for the map's click-through. The
/// answer carries no geometry: the network document holds the coordinates.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, TS)]
#[ts(export)]
pub struct TripStopView {
    pub stop_id: String,
    pub name: String,
    pub arrival_secs: u32,
    pub departure_secs: u32,
    pub timepoint: bool,
}

/// One trip instance's stops in the effective day, named from the network. The
/// instance is (trip, start_secs). A trip not in the day answers empty, never
/// an error: it does not run, or the edits removed it.
pub fn trip_stops(
    network: &Network,
    day: &[MaterializedTrip],
    trip: &str,
    start_secs: u32,
) -> Vec<TripStopView> {
    let Some(trip) = day
        .iter()
        .find(|t| t.trip == trip && t.start_secs == start_secs)
    else {
        return Vec::new();
    };
    trip.stop_times
        .iter()
        .map(|st| TripStopView {
            stop_id: st.stop.clone(),
            name: stop_name(network, Some(&st.stop)),
            arrival_secs: st.arrival_secs,
            departure_secs: st.departure_secs,
            timepoint: st.timepoint,
        })
        .collect()
}
