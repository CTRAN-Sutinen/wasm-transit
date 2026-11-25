//! The draft is form state that the host holds. [`diversion_of`] turns a
//! ready draft into a record, by one rule for every host.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use transit_network::{Coord, Network};
use transit_sync::{DetourPoint, Diversion, EditRecord, EditStatus, TripCancel};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct DiversionDraft {
    pub route_id: String,
    /// If unset, or not the route's, the route's longest pattern applies.
    #[serde(default)]
    #[ts(optional)]
    pub pattern: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    pub first_kept_stop: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    pub last_kept_stop: Option<String>,
    /// Every interior stop that is not listed here is skipped, so new anchors
    /// never leave stale skips behind.
    #[serde(default)]
    pub served_stops: Vec<String>,
    /// One slice per waypoint step, so undo removes a whole step.
    #[serde(default)]
    pub slices: Vec<Vec<DetourPoint>>,
    /// Minutes.
    #[serde(default)]
    pub added_mins: f64,
    /// Dates are "YYYY-MM-DD".
    pub start_date: String,
    pub end_date: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct DraftPattern {
    pub pattern: String,
    pub name: String,
    pub stop_count: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum DraftStopRole {
    FirstKept,
    LastKept,
    Interior,
    Outside,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct DraftStop {
    pub index: u32,
    pub stop_id: String,
    pub name: String,
    pub role: DraftStopRole,
    pub skipped: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct DraftView {
    pub patterns: Vec<DraftPattern>,
    pub pattern: Option<String>,
    pub stops: Vec<DraftStop>,
    pub skipped_stops: Vec<String>,
    pub alignment: Vec<Coord>,
    /// Consecutive duplicates are dropped, because routing can return the
    /// previous endpoint.
    pub detour: Vec<DetourPoint>,
    pub detour_length_m: f64,
    pub problems: Vec<String>,
}

pub fn draft_view(network: &Network, draft: &DiversionDraft) -> DraftView {
    let mut problems = Vec::new();
    let patterns: Vec<DraftPattern> = network
        .patterns
        .iter()
        .filter(|(_, p)| p.route == draft.route_id)
        .map(|(id, p)| DraftPattern { pattern: id.clone(), name: p.name.clone(), stop_count: p.stops.len() as u32 })
        .collect();
    let pattern = draft
        .pattern
        .as_ref()
        .filter(|id| patterns.iter().any(|p| &p.pattern == *id))
        .cloned()
        .or_else(|| {
            patterns
                .iter()
                .fold(None::<&DraftPattern>, |best, p| match best {
                    Some(b) if b.stop_count >= p.stop_count => Some(b),
                    _ => Some(p),
                })
                .map(|p| p.pattern.clone())
        });
    if draft.route_id.is_empty() {
        problems.push("pick a route".into());
    } else if !network.routes.contains_key(&draft.route_id) {
        problems.push(format!("unknown route {}", draft.route_id));
    } else if pattern.is_none() {
        problems.push("the route has no patterns".into());
    }

    let visits: Vec<&str> = pattern
        .as_ref()
        .and_then(|id| network.patterns.get(id))
        .map(|p| p.stops.iter().map(|ps| ps.stop.as_str()).collect())
        .unwrap_or_default();
    let first = draft.first_kept_stop.as_deref().filter(|s| !s.is_empty());
    let last = draft.last_kept_stop.as_deref().filter(|s| !s.is_empty());
    let p0 = first.and_then(|f| visits.iter().position(|s| *s == f));
    let p1 = p0.zip(last).and_then(|(p0, l)| visits[p0 + 1..].iter().position(|s| *s == l).map(|i| i + p0 + 1));
    if pattern.is_some() {
        match (first, last) {
            (None, _) => problems.push("pick the first stop the diversion keeps".into()),
            (Some(f), _) if p0.is_none() => problems.push(format!("{} is not on this pattern", stop_name(network, f))),
            (Some(_), None) => problems.push("pick the last stop the diversion keeps".into()),
            (Some(_), Some(l)) if p1.is_none() => {
                problems.push(format!("{} is not after the first kept stop on this pattern", stop_name(network, l)))
            }
            _ => {}
        }
    }

    let stops: Vec<DraftStop> = visits
        .iter()
        .enumerate()
        .map(|(i, stop)| {
            let role = if Some(i) == p0 {
                DraftStopRole::FirstKept
            } else if Some(i) == p1 {
                DraftStopRole::LastKept
            } else if p0.zip(p1).is_some_and(|(a, b)| a < i && i < b) {
                DraftStopRole::Interior
            } else {
                DraftStopRole::Outside
            };
            let skipped = role == DraftStopRole::Interior && !draft.served_stops.iter().any(|s| s == stop);
            DraftStop { index: i as u32, stop_id: stop.to_string(), name: stop_name(network, stop), role, skipped }
        })
        .collect();
    let mut skipped_stops: Vec<String> = Vec::new();
    for s in stops.iter().filter(|s| s.skipped) {
        // An anchor visited again inside the span is still an anchor.
        // The record must never skip it.
        if !skipped_stops.contains(&s.stop_id) && Some(s.stop_id.as_str()) != first && Some(s.stop_id.as_str()) != last {
            skipped_stops.push(s.stop_id.clone());
        }
    }

    let alignment = pattern.as_ref().map(|id| transit_network::pattern_alignment(network, id)).unwrap_or_default();
    let mut detour: Vec<DetourPoint> = Vec::new();
    for p in draft.slices.iter().flatten() {
        if detour.last().is_some_and(|q| (q.lat - p.lat).abs() < 1e-12 && (q.lon - p.lon).abs() < 1e-12) {
            continue;
        }
        detour.push(*p);
    }
    let detour_length_m = path_length_m(&detour);

    if !(draft.added_mins.is_finite() && draft.added_mins >= 0.0) {
        problems.push("added time must be zero or more minutes".into());
    }
    match (parse_date(&draft.start_date), parse_date(&draft.end_date)) {
        (Some(a), Some(b)) if a > b => problems.push("the start date is after the end date".into()),
        (Some(_), Some(_)) => {}
        _ => problems.push("both dates must be YYYY-MM-DD".into()),
    }

    DraftView { patterns, pattern, stops, skipped_stops, alignment, detour, detour_length_m, problems }
}

/// `id` is the domain id that the host creates. The record id belongs to the
/// sync envelope.
pub fn diversion_of(network: &Network, draft: &DiversionDraft, id: &str) -> Result<Diversion, String> {
    let view = draft_view(network, draft);
    if !view.problems.is_empty() {
        return Err(view.problems.join("; "));
    }
    Ok(Diversion {
        id: id.to_string(),
        route_id: draft.route_id.clone(),
        // The pattern decides the anchors. The model never matches on GTFS's
        // `direction_id`.
        direction_id: None,
        first_kept_stop: draft.first_kept_stop.clone().unwrap_or_default(),
        last_kept_stop: draft.last_kept_stop.clone().unwrap_or_default(),
        skipped_stops: view.skipped_stops,
        detour: (view.detour.len() >= 2).then_some(view.detour),
        added_secs: (draft.added_mins * 60.0).round() as u32,
        start_date: parse_date(&draft.start_date).expect("checked by the view"),
        end_date: parse_date(&draft.end_date).expect("checked by the view"),
        extra: Default::default(),
    })
}

/// A cancel of a trip that does not run on the date takes nothing away, so it
/// is refused (D-45). `id` is the domain id that the host creates.
pub fn cancel_of(
    network: &Network,
    id: &str,
    trip_id: &str,
    start_secs: Option<u32>,
    date: &str,
) -> Result<TripCancel, String> {
    let on = parse_date(date).ok_or_else(|| format!("invalid date {date:?} (YYYY-MM-DD)"))?;
    let (_, pattern_trips, trip) = network.trip(trip_id).ok_or_else(|| format!("unknown trip {trip_id}"))?;
    if let Some(s) = start_secs {
        if !trip.departs.instances().contains(&s) {
            return Err(format!("trip {trip_id} has no instance starting at {s}"));
        }
    }
    let runs = network.service_types.get(&pattern_trips.service_type).is_some_and(|t| t.rule.runs_on(on));
    if !runs {
        return Err(format!("trip {trip_id} does not run on {date}"));
    }
    Ok(TripCancel { id: id.to_string(), trip_id: trip_id.to_string(), start_secs, date: on, extra: Default::default() })
}

/// An id that the network lacks shows as itself.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct DiversionRow {
    pub record_id: String,
    pub status: EditStatus,
    pub in_force: bool,
    pub route_name: String,
    pub first_kept_name: String,
    pub last_kept_name: String,
    pub skipped_names: Vec<String>,
    pub added_secs: u32,
    pub detour_drawn: bool,
    pub start_date: String,
    pub end_date: String,
}

pub fn diversion_rows(network: &Network, records: &[EditRecord]) -> Vec<DiversionRow> {
    records
        .iter()
        .filter_map(|r| Some((r, r.diversion.as_ref()?)))
        .map(|(r, d)| {
            DiversionRow {
                record_id: r.id.clone(),
                status: r.status,
                in_force: r.status != EditStatus::Retired,
                route_name: network.routes.get(&d.route_id).map(|x| x.label().to_string()).unwrap_or_else(|| d.route_id.clone()),
                first_kept_name: stop_name(network, &d.first_kept_stop),
                last_kept_name: stop_name(network, &d.last_kept_stop),
                skipped_names: d.skipped_stops.iter().map(|s| stop_name(network, s)).collect(),
                added_secs: d.added_secs,
                detour_drawn: d.detour.as_ref().is_some_and(|p| !p.is_empty()),
                start_date: d.start_date.format("%Y-%m-%d").to_string(),
                end_date: d.end_date.format("%Y-%m-%d").to_string(),
            }
        })
        .collect()
}

/// An id that the network lacks shows as itself.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct CancelRow {
    pub record_id: String,
    pub status: EditStatus,
    pub in_force: bool,
    pub trip_id: String,
    pub start_secs: Option<u32>,
    pub route_name: String,
    pub from_name: String,
    pub to_name: String,
    pub date: String,
}

pub fn cancel_rows(network: &Network, records: &[EditRecord]) -> Vec<CancelRow> {
    records
        .iter()
        .filter_map(|r| Some((r, r.cancel.as_ref()?)))
        .map(|(r, c)| {
            let found = network.trip(&c.trip_id);
            let pattern = found.and_then(|(_, pt, _)| network.patterns.get(&pt.pattern));
            let end = |last: bool| {
                pattern
                    .and_then(|p| if last { p.stops.last() } else { p.stops.first() })
                    .map(|ps| stop_name(network, &ps.stop))
                    .unwrap_or_default()
            };
            let only_start = found.map(|(_, _, t)| t.departs.instances()).filter(|s| s.len() == 1).map(|s| s[0]);
            CancelRow {
                record_id: r.id.clone(),
                status: r.status,
                in_force: r.status != EditStatus::Retired,
                trip_id: c.trip_id.clone(),
                start_secs: c.start_secs.or(only_start),
                route_name: pattern
                    .map(|p| network.routes.get(&p.route).map(|x| x.label().to_string()).unwrap_or_else(|| p.route.clone()))
                    .unwrap_or_else(|| c.trip_id.clone()),
                from_name: end(false),
                to_name: end(true),
                date: c.date.format("%Y-%m-%d").to_string(),
            }
        })
        .collect()
}

fn stop_name(network: &Network, stop: &str) -> String {
    network.stops.get(stop).map(|s| s.name.clone()).unwrap_or_else(|| stop.to_string())
}

fn parse_date(raw: &str) -> Option<chrono::NaiveDate> {
    chrono::NaiveDate::parse_from_str(raw, "%Y-%m-%d").ok()
}

/// Path length in meters.
fn path_length_m(points: &[DetourPoint]) -> f64 {
    points.windows(2).map(|w| transit_geo::distance_m(w[0].lat, w[0].lon, w[1].lat, w[1].lon)).sum()
}
