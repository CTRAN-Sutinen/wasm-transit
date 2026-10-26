//! Diversions: operator-side service edits layered on a parsed feed.
//!
//! GTFS static has no delta format — when a road closes, an agency
//! regenerates `stop_times.txt` and `shapes.txt` for the affected trips.
//! This module plays that same game locally: a [`Diversion`] describes
//! one edit (which route pattern, between which two anchor stops, what
//! to skip, what detour to draw, how much extra running time, which
//! dates), and [`apply_diversions`] produces a patched copy of the
//! feed for a given service date. The base feed is never mutated, so
//! before/after comparison and on/off toggling stay trivial.
//!
//! Semantics of one diversion, per matching trip:
//!
//! * *Matching*: same `route_id`, same `direction_id` when the diversion
//!   names one, and both anchor stops present in that order. Trips of
//!   the route that don't contain the anchors run untouched.
//! * *Skip*: stop times at interior stops listed in `skipped_stops` are
//!   removed and `stop_sequence` renumbered.
//! * *Shift*: every stop time from the last anchor onward moves later
//!   by `added_secs` — the detour's extra running time is absorbed at
//!   the last kept stop's arrival.
//! * *Shape*: when a detour polyline is provided, the trip's shape is
//!   spliced: base shape up to the vertex nearest the first anchor, the
//!   detour, then the base shape from the vertex nearest the last anchor.
//!
//! The date range is honored by the caller passing a service date and
//! [`Diversion::covers`] filtering; the patched feed is per-date, which
//! matches how the rest of the crate queries (departures, plans and
//! blocks all take a date).

use chrono::NaiveDate;
use std::collections::{HashMap, HashSet};

use transit_sync::{DetourPoint, Diversion, EditRecord, EditReport};

use crate::model::ShapePoint;
use crate::{Gtfs, GtfsError, Result};

/// The edit-document types (Diversion, DetourPoint) live in
/// transit-sync — the dispatcher's own document contract, not the
/// GTFS codec. This module is the FEED-side patch path the monolith
/// still serves: validate an edit against a parsed feed, and produce
/// the patched per-date copy. It dies with set_diversions.

/// Validate one record against a feed. Pure: same feed + same
/// record always yields the same report, on any platform.
pub fn report(gtfs: &Gtfs, record: &EditRecord) -> EditReport {
    let diversion = &record.diversion;
    match validate_diversion(gtfs, diversion) {
        Err(e) => EditReport {
            id: record.id.clone(),
            valid: false,
            error: Some(e.to_string()),
            affected_trips: 0,
        },
        Ok(()) => EditReport {
            id: record.id.clone(),
            valid: true,
            error: None,
            affected_trips: affected_trips(gtfs, diversion).len(),
        },
    }
}

/// Validate a whole document: one report per record, input order.
pub fn report_all(gtfs: &Gtfs, records: &[EditRecord]) -> Vec<EditReport> {
    records.iter().map(|r| report(gtfs, r)).collect()
}

/// Reject diversions that could never apply: unknown route or stops,
/// degenerate anchors, anchors in the skip list, inverted dates, or a
/// pointless detour polyline. Called when an edit is added so `apply`
/// can trust its inputs.
pub fn validate_diversion(gtfs: &Gtfs, d: &Diversion) -> Result<()> {
    let invalid = |message: &str| GtfsError::Invalid { file: "diversion", message: message.into() };
    if !gtfs.routes.contains_key(&d.route_id) {
        return Err(invalid(&format!("unknown route {}", d.route_id)));
    }
    if let Some(dir) = d.direction_id {
        if dir > 1 {
            return Err(invalid(&format!("direction_id must be 0 or 1, got {dir}")));
        }
    }
    for stop in std::iter::once(&d.first_kept_stop).chain(std::iter::once(&d.last_kept_stop)) {
        if !gtfs.stops.contains_key(stop) {
            return Err(invalid(&format!("unknown anchor stop {stop}")));
        }
    }
    if d.first_kept_stop == d.last_kept_stop {
        return Err(invalid("anchor stops must differ"));
    }
    for skipped in &d.skipped_stops {
        if !gtfs.stops.contains_key(skipped) {
            return Err(invalid(&format!("unknown skipped stop {skipped}")));
        }
        if skipped == &d.first_kept_stop || skipped == &d.last_kept_stop {
            return Err(invalid("skipped stops must not include an anchor"));
        }
    }
    if d.start_date > d.end_date {
        return Err(invalid("start date is after end date"));
    }
    if let Some(detour) = &d.detour {
        if detour.len() == 1 {
            return Err(invalid("a detour needs at least two points"));
        }
        if detour.iter().any(|p| !p.lat.is_finite() || !p.lon.is_finite()) {
            return Err(invalid("detour contains non-finite coordinates"));
        }
    }
    Ok(())
}

/// Trips a diversion edits: same route (and direction, when given),
/// containing both anchors in order. Deterministic (sorted by trip
/// id) so patched output is stable.
pub fn affected_trips(gtfs: &Gtfs, d: &Diversion) -> Vec<String> {    let mut ids: Vec<String> = Vec::new();
    for trip in gtfs.trips.values() {
        if trip.route_id != d.route_id {
            continue;
        }
        if let Some(dir) = d.direction_id {
            if trip.direction_id != Some(dir) {
                continue;
            }
        }
        let Some(rows) = gtfs.trip_stop_times(&trip.trip_id) else { continue };
        let first = rows.iter().position(|r| r.stop_id == d.first_kept_stop);
        let last = first.and_then(|f| rows[f + 1..].iter().position(|r| r.stop_id == d.last_kept_stop).map(|p| p + f + 1));
        if last.is_some() {
            ids.push(trip.trip_id.clone());
        }
    }
    ids.sort();
    ids
}

/// Produce a patched copy of the feed for `date`, applying every
/// diversion whose date range covers it (in the order given). The
/// base feed is untouched. Diversions must have passed
/// [`validate_diversion`] against the same feed.
pub fn apply_diversions(base: &Gtfs, date: NaiveDate, diversions: &[Diversion]) -> Result<Gtfs> {
    let mut gtfs = clone_feed(base);
    let active: Vec<&Diversion> = diversions.iter().filter(|d| d.covers(date)).collect();
    if active.is_empty() {
        gtfs.rebuild_indexes();
        return Ok(gtfs);
    }

    // Work per-trip so edits never fight the flat row indexes; the
    // indexes are rebuilt once at the end. Trip row order within a
    // Vec is the file's, so sort by sequence first.
    let mut rows_by_trip: HashMap<String, Vec<crate::StopTime>> = HashMap::new();
    for st in gtfs.stop_times.drain(..) {
        rows_by_trip.entry(st.trip_id.clone()).or_default().push(st);
    }

    for d in active {
        let skipped: HashSet<&str> = d.skipped_stops.iter().map(|s| s.as_str()).collect();
        for trip_id in affected_trips(base, d) {
            let Some(rows) = rows_by_trip.get_mut(&trip_id) else { continue };
            rows.sort_by_key(|r| r.stop_sequence);

            // Anchor positions: first occurrence of each, in order.
            let Some(p0) = rows.iter().position(|r| r.stop_id == d.first_kept_stop) else { continue };
            let Some(p1) = rows[p0 + 1..]
                .iter()
                .position(|r| r.stop_id == d.last_kept_stop)
                .map(|p| p + p0 + 1)
            else {
                continue;
            };

            let mut kept: Vec<crate::StopTime> = Vec::with_capacity(rows.len());
            let mut last_kept_pos = 0;
            for (i, row) in rows.drain(..).enumerate() {
                let interior = i > p0 && i < p1;
                if interior && skipped.contains(row.stop_id.as_str()) {
                    continue;
                }
                if i == p1 {
                    last_kept_pos = kept.len();
                }
                kept.push(row);
            }

            // Renumber (GTFS allows arbitrary sequences; contiguity
            // keeps the "sorted by sequence" invariants obvious).
            for (i, row) in kept.iter_mut().enumerate() {
                row.stop_sequence = i as u32;
            }

            // The detour's running time lands on the last anchor's
            // arrival and everything after it.
            if d.added_secs > 0 {
                for row in &mut kept[last_kept_pos..] {
                    if let Some(t) = row.arrival_time {
                        row.arrival_time = Some(t + d.added_secs);
                    }
                    if let Some(t) = row.departure_time {
                        row.departure_time = Some(t + d.added_secs);
                    }
                }
            }

            if let Some(detour) = &d.detour {
                splice_shape(&mut gtfs, d, &trip_id, detour);
            }
            rows_by_trip.insert(trip_id, kept);
        }
    }

    gtfs.stop_times = rows_by_trip.into_values().flatten().collect();
    gtfs.rebuild_indexes();
    Ok(gtfs)
}

/// Splice a detour into the trip's shape: base polyline up to the
/// vertex nearest the first anchor, the drawn detour, then the base
/// polyline from the vertex nearest the last anchor onward. The result
/// is stored under a fresh shape id so the original stays reusable.
/// No-op when the trip has no shape, the anchors have no coordinates,
/// or they map to reversed positions on the polyline.
fn splice_shape(gtfs: &mut Gtfs, d: &Diversion, trip_id: &str, detour: &[DetourPoint]) {
    if detour.len() < 2 {
        return;
    }
    let coords = |stop_id: &str| {
        gtfs.stops.get(stop_id).and_then(|s| match (s.stop_lat, s.stop_lon) {
            (Some(lat), Some(lon)) => Some((lat, lon)),
            _ => None,
        })
    };
    let (Some(first), Some(last)) = (coords(&d.first_kept_stop), coords(&d.last_kept_stop)) else {
        return;
    };
    let Some(shape_id) = gtfs.trips.get(trip_id).and_then(|t| t.shape_id.clone()) else {
        return;
    };
    let Some(points) = gtfs.shapes.get(&shape_id) else { return };
    if points.len() < 2 {
        return;
    }
    let i0 = crate::nearest_shape_point(points, first);
    let i1 = crate::nearest_shape_point(points, last);
    if i0 > i1 {
        return;
    }

    let new_id = format!("{shape_id}::div::{}", d.id);
    if !gtfs.shapes.contains_key(&new_id) {
        let mut spliced: Vec<ShapePoint> = Vec::with_capacity(points.len() + detour.len());
        let push = |pts: &mut Vec<ShapePoint>, lat: f64, lon: f64| {
            pts.push(ShapePoint {
                shape_id: new_id.clone(),
                shape_pt_lat: lat,
                shape_pt_lon: lon,
                shape_pt_sequence: 0,
                shape_dist_traveled: None,
            });
        };
        for p in &points[..=i0] {
            push(&mut spliced, p.shape_pt_lat, p.shape_pt_lon);
        }
        for p in detour {
            push(&mut spliced, p.lat, p.lon);
        }
        for p in &points[i1..] {
            push(&mut spliced, p.shape_pt_lat, p.shape_pt_lon);
        }
        for (i, p) in spliced.iter_mut().enumerate() {
            p.shape_pt_sequence = i as u32;
        }
        gtfs.shapes.insert(new_id.clone(), spliced);
    }
    if let Some(trip) = gtfs.trips.get_mut(trip_id) {
        trip.shape_id = Some(new_id);
    }
}

/// Deep copy of a feed with empty indexes (the diversion patcher
/// rebuilds them once at the end).
fn clone_feed(base: &Gtfs) -> Gtfs {
    Gtfs {
        agencies: base.agencies.clone(),
        routes: base.routes.clone(),
        stops: base.stops.clone(),
        trips: base.trips.clone(),
        stop_times: base.stop_times.clone(),
        services: base.services.clone(),
        stop_times_by_stop: HashMap::new(),
        stop_times_by_trip: HashMap::new(),
        blocks: HashMap::new(),
        shapes: base.shapes.clone(),
        transfers: base.transfers.clone(),
        feed_info: base.feed_info.clone(),
    }
}
