//! transit-dispatch — the materialized day's views, in model
//! vocabulary.
//!
//! The day's questions (when service leaves a stop, what one
//! vehicle's chain of work is, which vehicles and service keys
//! are out) are answered over the NETWORK document — never over a
//! GTFS expansion. The day is [`transit_network::materialize`]
//! applied to one service date; operator edits (their own
//! document, transit-sync) PATCH that materialized day. There is
//! no derived registry and no date-keyed effective cache: a day is
//! computed per call, and it is cheap because it is one day, not
//! a calendar.
//!
//! This is shared domain code, off the wire: several capabilities
//! (the board, the runcut, the calendar, the diversion editor) sit over
//! the same library. Everything here is pure and total. Times are
//! seconds after the service-day anchor; values past 86400 are
//! legitimate after-midnight trips.

pub mod divert;
pub mod geometry;
pub use geometry::{trip_path, trip_segment, LegKind, LegPath};
pub use divert::{diversion_of, diversion_rows, draft_view, DiversionDraft, DiversionRow, DraftPattern, DraftStop, DraftStopRole, DraftView};

use std::collections::BTreeSet;

use chrono::NaiveDate;
use ts_rs::TS;

use transit_network::{materialize, MaterializedTrip, Network, StopTime};
use transit_live::{classify, LiveAlert, LiveDay, LiveStatus, Visit};
use transit_sync::{Diversion, EditRecord, EditReport};

/// One row on a departure board. Model vocabulary: the trip instance
/// is its (trip, start) identity, the stop is the CONCRETE stop the row
/// leaves from (the bay, on a pooled station board), and the
/// destination is what the sign says at this visit.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, TS)]
#[ts(export)]
pub struct BoardDeparture {
    pub trip: String,
    pub start_secs: u32,
    pub route: String,
    /// The route's public name ("12").
    pub route_name: String,
    /// The route's long name ("Lincoln"), when it has one.
    pub route_long_name: Option<String>,
    pub pattern: String,
    /// The concrete stop this row leaves from — on a station board,
    /// WHICH BAY a pooled row leaves from matters to the reader.
    pub stop: String,
    /// That stop's name ("Fisher's Landing TC: Bay B").
    pub stop_name: String,
    /// Seconds after the service-day anchor (past-86400 values are
    /// after-midnight trips, not errors).
    pub departure_secs: u32,
    /// Where this bus is going, as its sign says at this visit
    /// ([`transit_network::visit_destination`]).
    pub destination: String,
}

/// One trip in a block's chain for a service date: the AUTHORED runcut
/// (blocks and layovers are model state GTFS never had), materialized
/// for the day, with the operator-edit ripple made visible — a
/// diverted trip that ends later eats into its next layover.
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
    /// Whether this trip of the block runs on the date. A trip that
    /// does not run is still part of the authored chain (the vehicle's
    /// work is authored, not observed) — the reader sees the hole.
    pub runs: bool,
    pub first_stop: String,
    pub last_stop: String,
    /// The trip's end (last arrival), EFFECTIVE for the date: after
    /// the day's operator edits. `None` when the trip does not run.
    pub end_secs: Option<u32>,
    /// The authored layover following this trip, when the runcut
    /// places one there.
    pub layover_after_secs: Option<u32>,
    /// The layover that REMAINS after this trip absorbed the day's
    /// edits: authored layover minus the trip's delay. Negative means
    /// the next trip starts late — the connection-health ripple.
    /// `None` when there is no layover or the trip does not run.
    pub layover_remaining_secs: Option<i64>,
    /// The connection's health, classified under the caller's tight
    /// threshold: overlap (the delay ate the whole layover — the next
    /// trip starts late), tight (under the threshold), ok. `None`
    /// exactly when `layover_remaining_secs` is.
    pub layover_health: Option<LayoverHealth>,
}

/// A layover's health — the dispatcher's connection read.
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

/// Classify a remaining layover under a tight threshold.
pub fn layover_health(remaining_secs: i64, tight_secs: u32) -> LayoverHealth {
    if remaining_secs < 0 {
        LayoverHealth::Overlap
    } else if remaining_secs < tight_secs as i64 {
        LayoverHealth::Tight
    } else {
        LayoverHealth::Ok
    }
}

// ---------------------------------------------------------------------
// Patching the materialized day with operator edits
// ---------------------------------------------------------------------

/// Apply operator edits to a materialized day, producing a NEW day —
/// the input is never mutated. Semantics per matching trip (matching
/// is the model's: same route, both anchors present in order — the
/// model has no GTFS direction; the pattern IS the direction):
///
/// * *Skip*: interior stops listed in `skipped_stops` lose their
///   stop times (the stop loses service, reported nowhere silently).
/// * *Shift*: every stop time from the last anchor onward moves
///   later by `added_secs` — the detour's running time.
///
/// The detour polyline is DRAWING, not schedule: the host renders it
/// on the map directly from the document. Shapes are a GTFS
/// serialization concern and do not exist here.
///
/// Edits apply in the order given; each trip can absorb several.
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
    // Skip interior stops between the anchors.
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
    // The detour's running time lands on the last anchor's arrival
    // and everything after it. Anchors may have moved if an earlier
    // edit removed stops; re-locate them.
    if d.added_secs > 0 {
        if let Some((_, p1)) = anchor_positions(&trip.stop_times, d) {
            for st in &mut trip.stop_times[p1..] {
                st.arrival_secs += d.added_secs;
                st.departure_secs += d.added_secs;
            }
        }
    }
}

/// Anchor positions in a trip's stop times: first occurrence of each
/// anchor, in order. `None` = this trip is not affected.
fn anchor_positions(stop_times: &[StopTime], d: &Diversion) -> Option<(usize, usize)> {
    let p0 = stop_times.iter().position(|st| st.stop == d.first_kept_stop)?;
    let p1 = stop_times[p0 + 1..]
        .iter()
        .position(|st| st.stop == d.last_kept_stop)
        .map(|p| p + p0 + 1)?;
    Some((p0, p1))
}

// ---------------------------------------------------------------------
// Validation: the edit document against the NETWORK
// ---------------------------------------------------------------------

/// Validate one record against an authored network document and
/// count the trips it would touch (over every schedule whose pattern
/// matches — no date, no calendar expansion). Pure and total.
pub fn report_against_network(network: &Network, records: &[EditRecord]) -> Vec<EditReport> {
    records
        .iter()
        .map(|record| {
            let d = &record.diversion;
            match validate_against_network(network, d) {
                Err(error) => EditReport {
                    id: record.id.clone(),
                    valid: false,
                    error: Some(error),
                    affected_trips: 0,
                },
                Ok(()) => EditReport {
                    id: record.id.clone(),
                    valid: true,
                    error: None,
                    affected_trips: affected_trips(network, d),
                },
            }
        })
        .collect()
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

/// How many trip instances the edit touches: every schedule of the
/// route whose pattern carries both anchors in order, expanded.
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

// ---------------------------------------------------------------------
// Boards
// ---------------------------------------------------------------------

/// The board at one stop: every visit the day's (patched) trips make
/// to that stop, from `from_secs` on, soonest first. A stop a trip
/// passes twice appears twice — both visits are service. A trip's
/// FINAL visit is an arrival, not a departure: a bus ending its trip
/// here does not leave, so it is not on the board (a loop's first
/// visit to its terminal still departs).
pub fn board(
    network: &Network,
    day: &[MaterializedTrip],
    stop_id: &str,
    from_secs: u32,
    limit: usize,
) -> Vec<BoardDeparture> {
    board_over(network, day, &[stop_id.to_string()], from_secs, limit)
}

/// The STATION board: the same board pooled over the stop's WALKING
/// GROUP (the model's own transfer-group concept — the stop, its
/// parent, siblings sharing that parent, the parent's children). A
/// station pin answers no departures of its own; its bays carry the
/// trips, and each row names its concrete stop.
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

/// One row of THE board: the departure, and what live data says
/// about it — the expected departure, the delay against the board's
/// own (patched) time, its status under the live document's window,
/// and the ids of the board's alerts that speak about it.
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
    pub alerts: Vec<String>,
}

/// A departure the day's edits took away from this stop: the visit as
/// scheduled, and the diversion that skips it (none when the trip no
/// longer reaches the stop for another reason).
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, TS)]
#[ts(export)]
pub struct SkippedVisit {
    #[serde(flatten)]
    #[ts(flatten)]
    pub departure: BoardDeparture,
    pub diversion: Option<String>,
    pub alerts: Vec<String>,
}

/// The board at a stop, answered whole.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, TS)]
#[ts(export)]
pub struct StopBoard {
    pub rows: Vec<BoardRow>,
    /// Scheduled departures the in-force edits skip here, soonest
    /// first — why a detoured-away stop is empty.
    pub skipped: Vec<SkippedVisit>,
    /// The live document's alerts about this board: any that speak
    /// about a row, a skipped visit, or the stop itself.
    pub alerts: Vec<LiveAlert>,
    /// The stop had nothing of its own and is part of a walking
    /// group: the rows are the group's, each naming its bay.
    pub pooled: bool,
    /// A live document fed this answer.
    pub live: bool,
}

/// The edits behind an effective day: the unpatched day and the
/// diversions in force — what a board compares against to say what
/// was taken away.
#[derive(Debug, Clone)]
pub struct DayEdits {
    pub base: Vec<MaterializedTrip>,
    pub diversions: Vec<Diversion>,
}

/// The day's edits for a date — `None` when no edit is in force (the
/// effective day IS the materialized day; nothing was taken away).
pub fn day_edits(
    network: &Network,
    date: NaiveDate,
    records: &[EditRecord],
) -> Result<Option<DayEdits>, transit_network::NetworkError> {
    let diversions = transit_sync::in_force_on(records, date);
    if diversions.is_empty() {
        return Ok(None);
    }
    Ok(Some(DayEdits { base: materialize(network, date)?, diversions }))
}

/// Live delays never reach back further than this when looking for
/// late buses still to come (a 3-hour delay is a data fault).
const MAX_LOOKBACK_SECS: u32 = 3 * 3600;

/// THE board — when does service leave this stop — realtime-first.
/// Every departure of the day's (patched) trips from the stop, ordered
/// by when it will actually leave: a late bus scheduled before
/// `from_secs` that has not left yet is on the board; an early one
/// that already left is not. With the day's `edits`, the departures
/// they took away from the stop are listed as `skipped`, each with
/// the diversion that skips it; with a live document, its alerts
/// about the board ride along. A stop with nothing of its own —
/// nothing leaving and nothing skipped — that belongs to a walking
/// group (a station pin) answers its group's board instead; a plain
/// stop never pools, so a diverted-away stop shows its emptiness and
/// the reason.
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
            return with_alerts(rows, skipped, &group, true, live);
        }
    }
    with_alerts(rows, skipped, &own, false, live)
}

/// Attach the live document's alerts: on each row and skipped visit
/// the ids that touch it; on the board every alert touching either,
/// or the stops themselves, in the document's order.
fn with_alerts(
    mut rows: Vec<BoardRow>,
    mut skipped: Vec<SkippedVisit>,
    stops: &[String],
    pooled: bool,
    live: Option<&LiveDay>,
) -> StopBoard {
    let all = live.map(|l| l.alerts.as_slice()).unwrap_or_default();
    let ids = |d: &BoardDeparture| -> Vec<String> {
        let visit = Visit { route: &d.route, stop: &d.stop, trip: &d.trip, start_secs: d.start_secs };
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
        .map(|d| SkippedVisit {
            diversion: edits
                .diversions
                .iter()
                .find(|v| v.route_id == d.route && v.skipped_stops.contains(&d.stop))
                .map(|v| v.id.clone()),
            departure: d,
            alerts: Vec::new(),
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
                expected_secs: expected,
                delay_secs: delay,
                departure: d,
                alerts: Vec::new(),
            }
        })
        .filter(|r| r.expected_secs.unwrap_or(r.departure.departure_secs) >= from_secs)
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
        // The last visit terminates the trip — not a departure.
        let departing = trip.stop_times.len().saturating_sub(1);
        for (visit, st) in trip.stop_times[..departing].iter().enumerate() {
            if !stops.iter().any(|s| s == &st.stop) || st.departure_secs < from_secs {
                continue;
            }
            rows.push(BoardDeparture {
                trip: trip.trip.clone(),
                start_secs: trip.start_secs,
                route: trip.route.clone(),
                route_name: route_name(network, &trip.route),
                route_long_name: network.routes.get(&trip.route).and_then(|r| r.long_name.clone()),
                pattern: trip.pattern.clone(),
                stop: st.stop.clone(),
                stop_name: stop_name(network, Some(&st.stop)),
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

// ---------------------------------------------------------------------
// The authored block chain
// ---------------------------------------------------------------------

/// One block's chain for a service date: the AUTHORED trips in
/// order (the runcut is model state), each materialized for the day
/// when it runs — a frequency trip as every instance it runs, back to
/// back — with the operator-edit ripple on the layovers. Unknown
/// blocks answer empty, never error.
pub fn block_chain(
    network: &Network,
    day: &[MaterializedTrip],
    block_id: &str,
    tight_secs: u32,
) -> Vec<BlockChainTrip> {
    let Some(block) = network.blocks.get(block_id) else {
        return Vec::new();
    };
    let mut rows = Vec::with_capacity(block.trips.len());
    for (index, trip_ref) in block.trips.iter().enumerate() {
        let stored = network.trip(&trip_ref.trip);
        // Runs = its service type runs on the date.
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
                layover_after_secs: layover,
                layover_remaining_secs: None,
                layover_health: None,
            });
            continue;
        }
        let last = runs.len() - 1;
        for (k, trip) in runs.into_iter().enumerate() {
            let effective_end = trip.stop_times.last().map(|st| st.arrival_secs.max(st.departure_secs));
            // The authored end from the trip itself — what the layover
            // was authored against.
            let authored_end = stored.and_then(|(_, _, t)| transit_network::trip_arrival_end_secs(t, trip.start_secs));
            let delay = match (effective_end, authored_end) {
                (Some(e), Some(a)) => e as i64 - a as i64,
                _ => 0,
            };
            // The authored layover follows the trip's last instance.
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
                layover_after_secs: layover,
                layover_remaining_secs: layover_remaining,
                layover_health: layover_remaining.map(|r| layover_health(r, tight_secs)),
            });
        }
    }
    rows
}

/// Convenience: the effective day for a date — materialized, then
/// patched with the edits in force on that date. What every dispatch
/// view starts from.
pub fn effective_day(
    network: &Network,
    date: NaiveDate,
    records: &[EditRecord],
) -> Result<Vec<MaterializedTrip>, transit_network::NetworkError> {
    let day = materialize(network, date)?;
    let diversions = transit_sync::in_force_on(records, date);
    Ok(patch_day(&day, &diversions))
}

// ---------------------------------------------------------------------
// The day's fleet and service keys
// ---------------------------------------------------------------------

/// One block out on the service day: the vehicle's id and how many
/// of its authored trips run.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, TS)]
#[ts(export)]
pub struct DayBlock {
    pub block_id: String,
    /// How many of the block's trip instances run on the date.
    pub trip_count: u32,
}

/// The day's blocks: every authored block with at least one trip in
/// the (effective) day, with that day's running trip counts, sorted
/// by block id. A block whose every trip is off that day is a
/// vehicle still in the garage — a picker of today's blocks must not
/// offer it.
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

/// The day's fleet in one answer (QAF-3): how many blocks run, the
/// peak concurrent vehicles and when, and the platform / revenue /
/// layover hours — the numbers that took one `block_trips` call per
/// block plus host-side concurrency glue.
///
/// A vehicle is out from its block's first running start to its
/// last running arrival (the effective day's times, edits applied
/// by the caller); half-open spans, so a bus arriving as another
/// departs is one vehicle, not two. Platform = the fleet's out
/// hours; revenue = the trips' own runtimes; layover = the rest.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, TS)]
#[ts(export)]
pub struct FleetSummary {
    /// Blocks with at least one trip in the (effective) day.
    pub blocks: u32,
    /// The most blocks out at once.
    pub peak_vehicles: u32,
    /// The first time the peak is reached; None on an empty day.
    pub peak_at_secs: Option<u32>,
    /// Every running block's span (last arrival − first start),
    /// hours — the vehicle hours the fleet is out.
    pub platform_hours: f64,
    /// The running trips' own runtimes (arrival − start), hours.
    pub revenue_hours: f64,
    /// Platform − revenue, hours: recovery, pull gaps, and the
    /// authored layovers the day actually runs.
    pub layover_hours: f64,
}

/// The day's fleet: every block with a running trip, folded into
/// the peak and the hour bases. The day is the caller's — patched
/// or plain materialize, the same seam every day view takes.
pub fn fleet_summary(network: &Network, day: &[MaterializedTrip]) -> FleetSummary {
    let mut blocks = 0u32;
    let mut platform_secs = 0u64;
    let mut revenue_secs = 0u64;
    // (time, delta): a block's span is half-open [start, end).
    let mut events: Vec<(u32, i64)> = Vec::new();
    for block in network.blocks.values() {
        let mut first_start: Option<u32> = None;
        let mut last_end: Option<u32> = None;
        let mut running = 0u32;
        let ids: BTreeSet<&str> = block.trips.iter().map(|t| t.trip.as_str()).collect();
        for trip in day.iter().filter(|t| ids.contains(t.trip.as_str())) {
            running += 1;
            let end = trip
                .stop_times
                .last()
                .map(|st| st.arrival_secs.max(st.departure_secs))
                .unwrap_or(trip.start_secs);
            first_start = Some(first_start.map_or(trip.start_secs, |f| f.min(trip.start_secs)));
            last_end = Some(last_end.map_or(end, |l| l.max(end)));
            revenue_secs += end.saturating_sub(trip.start_secs) as u64;
        }
        if running == 0 {
            continue;
        }
        blocks += 1;
        let (start, end) = (first_start.expect("running block has a start"), last_end.expect("running block has an end"));
        platform_secs += end.saturating_sub(start) as u64;
        events.push((start, 1));
        events.push((end, -1));
    }
    // -1 sorts before +1 at the same second: a bus arriving as
    // another departs does not double-count.
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
    }
}

/// The service keys running on a date: the distinct service types the
/// (effective) day's trips run under, sorted — the day's first
/// first question, answered from the day itself. A key whose every
/// trip is skipped by edits (or was never representable in the
/// model) is not running, however its calendar reads.
pub fn active_services(network: &Network, day: &[MaterializedTrip]) -> Vec<String> {
    let mut keys: BTreeSet<&str> = BTreeSet::new();
    for trip in day {
        if let Some(pattern_trips) = network.pattern_trips.get(&trip.pattern_trips) {
            keys.insert(pattern_trips.service_type.as_str());
        }
    }
    keys.into_iter().map(String::from).collect()
}

/// The dates one service type runs on within a window — the calendar
/// read backwards ("when does 1-WKDY/31 run?" is the same question
/// active_services answers in the other direction). An unknown type,
/// or one no pattern trips run under, answers empty (an id lookup,
/// never an error); so does an inverted window. Schedule-only (no
/// records) the answer is the RULE's own dates; with records in
/// force, the effective day decides each date — the same honesty as
/// active_services: a type whose every trip is skipped by edits is
/// not running, however its calendar reads.
pub fn service_dates(
    network: &Network,
    records: &[EditRecord],
    service_type: &str,
    from: NaiveDate,
    to: NaiveDate,
) -> Result<Vec<NaiveDate>, transit_network::NetworkError> {
    // A type no trips run under never runs, however its rule reads —
    // the two-way agreement with active_services depends on it.
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

/// One stop of one trip instance's day view — the trip detail facet
/// (what the map's click-through reads; the answer carries no
/// geometry, the network document holds the coordinates).
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, TS)]
#[ts(export)]
pub struct TripStopView {
    pub stop_id: String,
    pub name: String,
    /// Seconds after the service-day anchor (past-86400 values are
    /// after-midnight trips, not errors).
    pub arrival_secs: u32,
    pub departure_secs: u32,
    pub timepoint: bool,
}

/// One trip instance's stops in the (effective) day, named from the
/// network. The instance's identity is the model's: (trip,
/// start_secs). A trip not in the day (it does not run, or the edits
/// removed it from the question) answers empty — never an error.
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
