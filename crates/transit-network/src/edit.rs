//! Authoring: the network document's edit vocabulary.
//!
//! A planner's change is a list of typed edits applied to a document
//! that is never mutated in place — `apply` answers the NEXT document
//! (the caller re-addresses it by hash and wholesale-replaces its
//! copy, the way every host owns documents). The list is atomic: one
//! edit that cannot apply rejects the whole change, naming it.
//!
//! The vocabulary has two levels: entity-level (upsert / remove per
//! collection, plus the plan frame) and GESTURE-level — one edit per
//! thing a planner does (add a stop to a pattern, make a stop a
//! timepoint, add a trip, set a layover), each carrying the rule that
//! keeps the document coherent (timing vectors aligned with
//! timepoints, every trip's times aligned with its pattern's stops,
//! layovers only between trips, minted ids). An authoring UI, a script, and an agent all
//! express a change in the same edits and get the same document. Removal refuses while anything still
//! references the entity — a dangling reference is a broken document,
//! and the model, not each host, is where that rule lives. Validity of
//! the RESULT is not enforced here: a document mid-edit may be
//! invalid, and validation reports it (`validation_issues`).

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use std::collections::BTreeMap;

use chrono::NaiveDate;

use crate::materialize::produce_times;
use crate::timetable_edit;
use crate::model::{
    Block, CalendarException, CalendarRule, DateRange, DayState, Departs, Direction, HeadwayBand, Layover, Mode,
    Network, Pattern, PatternStop, PatternTrips, PlanMeta, Route, ServiceType, Stop, StopOffset, TimingProfile, Trip,
    TripRef, WeekdaySet,
};

/// One change to a network document. Upserts insert or replace the
/// whole entity under its id; removes refuse while referenced.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(tag = "edit", rename_all = "snake_case")]
pub enum NetworkEdit {
    /// The plan frame: identity, name, pick date. Forking a plan into
    /// the next pick, or stamping a fresh import into the chain, is
    /// this edit.
    SetPlan { plan: PlanMeta },
    /// Change the plan frame field-wise: `name`; `effective_from`
    /// ("YYYY-MM-DD", or "" to clear the date); `id` only stamps an
    /// UNVERSIONED plan (an empty id) — a framed pick keeps its
    /// identity.
    UpdatePlan {
        #[serde(default)]
        #[ts(optional)]
        name: Option<String>,
        #[serde(default)]
        #[ts(optional)]
        effective_from: Option<String>,
        #[serde(default)]
        #[ts(optional)]
        id: Option<String>,
    },
    /// The document's display name.
    SetName { name: String },
    UpsertStop { id: String, stop: Stop },
    /// Refused while a pattern visits the stop, a layover names it,
    /// or another stop names it as parent.
    RemoveStop { id: String },
    UpsertRoute { id: String, route: Route },
    /// Refused while a pattern runs on the route.
    RemoveRoute { id: String },
    UpsertPattern { id: String, pattern: Pattern },
    /// Refused while a schedule runs the pattern.
    RemovePattern { id: String },
    UpsertServiceType { id: String, service_type: ServiceType },
    /// Refused while a schedule runs under the type.
    RemoveServiceType { id: String },
    UpsertPatternTrips { id: String, pattern_trips: PatternTrips },
    /// Move a one-start trip to start at `to_secs`. It keeps its id,
    /// its running times, and its blocks (they reference it by id).
    RetimeTrip { trip: String, to_secs: u32 },
    /// A new one-start trip of `pattern_trips` at `start_secs`, timed
    /// by its pattern's profile covering the start. An absent id is
    /// minted (`{pattern_trips}@{start}`, suffixed while taken).
    AddTrip {
        pattern_trips: String,
        #[serde(default)]
        #[ts(optional)]
        id: Option<String>,
        start_secs: u32,
    },
    /// Remove a trip everywhere: its pattern trips (removed when
    /// emptied) and its block (the layovers either side go; a block
    /// it empties goes).
    RemoveTrip { trip: String },
    /// Refused while a block carries one of its trips.
    RemovePatternTrips { id: String },
    UpsertBlock { id: String, block: Block },
    RemoveBlock { id: String },

    // --- gestures: creation (an absent id is minted; `apply` reports it)
    /// A new stop, named "Stop N"; placed at `lat`/`lon`, else at the
    /// centre of the network's stops.
    AddStop {
        #[serde(default)]
        #[ts(optional)]
        id: Option<String>,
        #[serde(default)]
        #[ts(optional)]
        lat: Option<f64>,
        #[serde(default)]
        #[ts(optional)]
        lon: Option<f64>,
    },
    /// A new bus route, named "Route N".
    AddRoute {
        #[serde(default)]
        #[ts(optional)]
        id: Option<String>,
    },
    /// A new pattern on `route` (else the first route): the first two
    /// stops, both timepoints, 500 m apart, one all-day profile at
    /// 0 / 10 minutes — a skeleton to author from.
    AddPattern {
        #[serde(default)]
        #[ts(optional)]
        id: Option<String>,
        #[serde(default)]
        #[ts(optional)]
        route: Option<String>,
    },
    /// A new weekday service type running a year from `from`.
    AddServiceType {
        #[serde(default)]
        #[ts(optional)]
        id: Option<String>,
        from: String,
    },
    /// A new pattern trips: `pattern` / `service_type` (else the first
    /// of each), one trip at 08:00 timed by the pattern's profile.
    AddPatternTrips {
        #[serde(default)]
        #[ts(optional)]
        id: Option<String>,
        #[serde(default)]
        #[ts(optional)]
        pattern: Option<String>,
        #[serde(default)]
        #[ts(optional)]
        service_type: Option<String>,
    },
    /// A new, empty block.
    AddBlock {
        #[serde(default)]
        #[ts(optional)]
        id: Option<String>,
    },

    // --- gestures: fields
    UpdateStop {
        id: String,
        #[serde(default)]
        #[ts(optional)]
        name: Option<String>,
        #[serde(default)]
        #[ts(optional)]
        lat: Option<f64>,
        #[serde(default)]
        #[ts(optional)]
        lon: Option<f64>,
        #[serde(default)]
        #[ts(optional)]
        transfer: Option<bool>,
    },
    UpdateRoute {
        id: String,
        #[serde(default)]
        #[ts(optional)]
        name: Option<String>,
        #[serde(default)]
        #[ts(optional)]
        mode: Option<Mode>,
    },
    /// A pattern's fields. `direction` names one of its route's
    /// directions ("" leaves it in none); moving the pattern to
    /// another `route` leaves it in none unless `direction` places it
    /// there in the same edit (direction ids are per route).
    UpdatePattern {
        id: String,
        #[serde(default)]
        #[ts(optional)]
        name: Option<String>,
        #[serde(default)]
        #[ts(optional)]
        route: Option<String>,
        #[serde(default)]
        #[ts(optional)]
        direction: Option<String>,
    },
    /// Add a direction to a route, or rename one.
    SetDirection { route: String, id: String, name: String },
    /// Refused while a pattern runs in the direction.
    RemoveDirection { route: String, id: String },
    UpdateServiceType { id: String, name: String },
    SetPatternTripsRefs {
        id: String,
        #[serde(default)]
        #[ts(optional)]
        pattern: Option<String>,
        #[serde(default)]
        #[ts(optional)]
        service_type: Option<String>,
    },

    // --- gestures: a pattern's stops and timing
    /// Visit `stop` before visit `index` (default: the last stop),
    /// halfway along that leg, as a non-timepoint (the endpoints stay
    /// the endpoints; the timing vectors are untouched).
    InsertPatternStop {
        pattern: String,
        stop: String,
        #[serde(default)]
        #[ts(optional)]
        index: Option<usize>,
    },
    /// Drop a MIDDLE visit (endpoints are fixed); a timepoint's entry
    /// leaves every timing profile with it.
    RemovePatternStop { pattern: String, index: usize },
    SetPatternStop {
        pattern: String,
        index: usize,
        #[serde(default)]
        #[ts(optional)]
        stop: Option<String>,
        #[serde(default)]
        #[ts(optional)]
        distance_m: Option<f64>,
    },
    /// Flip a MIDDLE visit's timepoint flag, keeping every timing
    /// profile aligned: turning on inserts the midpoint of its
    /// neighbours' times, turning off drops the entry.
    ToggleTimepoint { pattern: String, index: usize },
    /// A new profile after the last one (6 hours long), copying its
    /// times.
    AddTimingProfile { pattern: String },
    /// Refused for the last remaining profile.
    RemoveTimingProfile { pattern: String, index: usize },
    SetTimingProfile {
        pattern: String,
        index: usize,
        #[serde(default)]
        #[ts(optional)]
        start_secs: Option<u32>,
        #[serde(default)]
        #[ts(optional)]
        end_secs: Option<u32>,
    },
    SetTimepointTime { pattern: String, profile: usize, timepoint: usize, secs: u32 },

    // --- gestures: a service type's calendar
    ToggleServiceDay { id: String, day: Day },
    /// A new date range after the last one (starting where it ends).
    AddServiceRange { id: String },
    SetServiceRange {
        id: String,
        index: usize,
        #[serde(default)]
        #[ts(optional)]
        start: Option<String>,
        #[serde(default)]
        #[ts(optional)]
        end: Option<String>,
    },
    RemoveServiceRange { id: String, index: usize },
    /// Add or change the exception for `date` (kept in date order).
    SetServiceException { id: String, date: String, state: DayState },
    RemoveServiceException { id: String, date: String },

    // --- gestures: a frequency trip's bands
    SetBand {
        trip: String,
        index: usize,
        #[serde(default)]
        #[ts(optional)]
        start_secs: Option<u32>,
        #[serde(default)]
        #[ts(optional)]
        end_secs: Option<u32>,
        #[serde(default)]
        #[ts(optional)]
        headway_secs: Option<u32>,
    },
    /// A new 3-hour, 15-minute band after the last one.
    AddBand { trip: String },
    /// Refused for the last remaining band.
    RemoveBand { trip: String, index: usize },

    // --- gestures: the timetable (route + direction + service type;
    // `direction` null or omitted: the route's patterns in none). See
    // `timetable_edit` for the rules every one of them keeps.
    /// Frequency by period: a one-start trip every `headway_secs` from
    /// `from_secs` (the first departure) until before `to_secs`.
    /// Trips in the window move to the nearest slots in order, keeping
    /// their ids, times and blocks; open slots are new trips
    /// (`pattern`, else the one the trip before runs); surplus trips
    /// go. Frequency trips are left as they are.
    SetHeadway {
        route: String,
        #[serde(default)]
        #[ts(optional)]
        direction: Option<String>,
        service_type: String,
        from_secs: u32,
        to_secs: u32,
        headway_secs: u32,
        #[serde(default)]
        #[ts(optional)]
        pattern: Option<String>,
    },
    /// The span: trips before `first_secs` or after `last_secs` go; a
    /// bound past the first / last trip is reached at `headway_secs`
    /// (else the edge's median gap), on the edge trip's pattern.
    SetSpan {
        route: String,
        #[serde(default)]
        #[ts(optional)]
        direction: Option<String>,
        service_type: String,
        #[serde(default)]
        #[ts(optional)]
        first_secs: Option<u32>,
        #[serde(default)]
        #[ts(optional)]
        last_secs: Option<u32>,
        #[serde(default)]
        #[ts(optional)]
        headway_secs: Option<u32>,
    },
    /// Running time by period: the one-start trips of the direction
    /// under `service_type` departing in [`from_secs`, `to_secs`) run
    /// `delta_secs` longer (negative: shorter), in proportion along
    /// each trip. Nothing else changes — not another service type, not
    /// the profiles.
    AdjustRunningTime {
        route: String,
        #[serde(default)]
        #[ts(optional)]
        direction: Option<String>,
        service_type: String,
        from_secs: u32,
        to_secs: u32,
        delta_secs: i32,
    },
    /// Visit `stop` right after `after` on every pattern of the
    /// direction that visits `after` before its end.
    InsertDirectionStop {
        route: String,
        #[serde(default)]
        #[ts(optional)]
        direction: Option<String>,
        stop: String,
        after: String,
    },
    /// Stop visiting `stop` on every pattern of the direction;
    /// refused when it ends one.
    RemoveDirectionStop {
        route: String,
        #[serde(default)]
        #[ts(optional)]
        direction: Option<String>,
        stop: String,
    },

    // --- gestures: the runcut
    /// Add a trip to a block, keeping the block in order of first
    /// start; layovers past the last trip are dropped.
    AddBlockTrip { block: String, trip: TripRef },
    RemoveBlockTrip { block: String, index: usize },
    /// Set (or, with `secs` null, clear) the layover after trip
    /// `after_trip` — only between two trips of the block.
    SetLayover {
        block: String,
        after_trip: usize,
        #[serde(default)]
        secs: Option<u32>,
    },
}

/// A day of the week, as a service type's rule names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum Day {
    Mon,
    Tue,
    Wed,
    Thu,
    Fri,
    Sat,
    Sun,
}

/// An entity a change created, with the id it was given.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct CreatedEntity {
    /// stop | route | pattern | service_type | pattern_trips | block | trip
    pub kind: String,
    pub id: String,
}

/// Why a change did not apply: the edit's position in the list and
/// what it collided with. The input document is untouched.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct EditError {
    pub index: usize,
    pub reason: String,
}

impl std::fmt::Display for EditError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "edit {}: {}", self.index, self.reason)
    }
}

/// Apply `edits` in order to a copy of `network`; the input is never
/// touched. All or nothing: the first edit that cannot apply rejects
/// the change.
pub fn apply_edits(network: &Network, edits: &[NetworkEdit]) -> Result<Network, EditError> {
    apply_edits_reporting(network, edits).map(|(next, _)| next)
}

/// [`apply_edits`], also naming what the change created (minted ids
/// included) — what an authoring host opens next.
pub fn apply_edits_reporting(
    network: &Network,
    edits: &[NetworkEdit],
) -> Result<(Network, Vec<CreatedEntity>), EditError> {
    let mut next = network.clone();
    let mut created = Vec::new();
    for (index, edit) in edits.iter().enumerate() {
        if let Some(entity) = apply_one(&mut next, edit).map_err(|reason| EditError { index, reason })? {
            created.push(entity);
        }
    }
    Ok((next, created))
}

/// The first free `{prefix}{n}`, n from 1 — or the caller's id, when
/// free.
fn mint<V>(map: &BTreeMap<String, V>, wanted: &Option<String>, prefix: &str, kind: &str) -> Result<String, String> {
    if let Some(id) = wanted {
        if map.contains_key(id) {
            return Err(format!("{kind} {id} already exists"));
        }
        return Ok(id.clone());
    }
    Ok((1..).map(|n| format!("{prefix}{n}")).find(|id| !map.contains_key(id)).expect("ids are unbounded"))
}

fn created(kind: &str, id: &str) -> Option<CreatedEntity> {
    Some(CreatedEntity { kind: kind.into(), id: id.into() })
}

fn date(text: &str) -> Result<NaiveDate, String> {
    NaiveDate::parse_from_str(text, "%Y-%m-%d").map_err(|e| format!("invalid date {text:?}: {e}"))
}

fn get_mut<'a, V>(map: &'a mut BTreeMap<String, V>, id: &str, kind: &str) -> Result<&'a mut V, String> {
    map.get_mut(id).ok_or_else(|| format!("unknown {kind} {id}"))
}

fn at_mut<'a, T>(items: &'a mut [T], index: usize, what: &str) -> Result<&'a mut T, String> {
    let len = items.len();
    items.get_mut(index).ok_or_else(|| format!("no {what} {index} (there are {len})"))
}

/// A middle visit of a pattern — endpoints are fixed by invariant.
fn middle(pattern: &Pattern, id: &str, index: usize) -> Result<(), String> {
    if index == 0 || index + 1 >= pattern.stops.len() {
        return Err(format!("visit {index} of pattern {id} is an endpoint (or absent) — endpoints stay"));
    }
    Ok(())
}

/// The position of visit `index` among the pattern's timepoints.
fn timepoint_position(pattern: &Pattern, index: usize) -> usize {
    pattern.stops[..index].iter().filter(|s| s.timepoint).count()
}

fn trip_mut<'a>(net: &'a mut Network, id: &str) -> Result<&'a mut Trip, String> {
    net.pattern_trips
        .values_mut()
        .flat_map(|pt| pt.trips.iter_mut())
        .find(|t| t.id == id)
        .ok_or_else(|| format!("unknown trip {id}"))
}

fn bands<'a>(net: &'a mut Network, trip: &str) -> Result<&'a mut Vec<HeadwayBand>, String> {
    match &mut trip_mut(net, trip)?.departs {
        Departs::Every { bands, .. } => Ok(bands),
        Departs::At { .. } => Err(format!("trip {trip} runs once, not at a frequency")),
    }
}

/// A new one-start trip of `pattern_trips` at `start`, timed by its
/// pattern's profile; answers its id.
fn add_trip(net: &mut Network, pattern_trips: &str, id: &Option<String>, start: u32) -> Result<String, String> {
    let pt = net.pattern_trips.get(pattern_trips).ok_or_else(|| format!("unknown pattern trips {pattern_trips}"))?;
    let pattern = net.patterns.get(&pt.pattern).ok_or_else(|| format!("unknown pattern {}", pt.pattern))?;
    let times = produce_times(pattern, start).ok_or_else(|| {
        format!(
            "pattern {} has no timing profile to time a trip at {:02}:{:02} — extend its timing first",
            pt.pattern,
            start / 3600,
            start % 3600 / 60
        )
    })?;
    let id = match id {
        Some(id) if net.trip(id).is_some() => return Err(format!("trip {id} already exists")),
        Some(id) => id.clone(),
        None => timetable_edit::fresh_trip_id(net, &Default::default(), &format!("{pattern_trips}@{start}")),
    };
    net.pattern_trips
        .get_mut(pattern_trips)
        .expect("checked")
        .trips
        .push(Trip { id: id.clone(), departs: Departs::At { start_secs: start }, times, ..Default::default() });
    Ok(id)
}

/// Layovers live only between two trips of the block.
pub(crate) fn prune_layovers(block: &mut Block) {
    let trips = block.trips.len();
    block.layovers.retain(|l| l.after_trip + 1 < trips);
}

fn apply_one(net: &mut Network, edit: &NetworkEdit) -> Result<Option<CreatedEntity>, String> {
    if let Some(entity) = apply_gesture(net, edit)? {
        return Ok(entity);
    }
    match edit {
        NetworkEdit::SetPlan { plan } => net.plan = plan.clone(),
        NetworkEdit::SetName { name } => net.name = name.clone(),
        NetworkEdit::UpsertStop { id, stop } => {
            net.stops.insert(id.clone(), stop.clone());
        }
        NetworkEdit::RemoveStop { id } => {
            exists(net.stops.contains_key(id), "stop", id)?;
            if let Some((p, _)) = net.patterns.iter().find(|(_, p)| p.stops.iter().any(|s| &s.stop == id)) {
                return Err(format!("stop {id} is visited by pattern {p}"));
            }
            if let Some((s, _)) = net.stops.iter().find(|(_, s)| s.parent.as_ref() == Some(id)) {
                return Err(format!("stop {id} is the parent of stop {s}"));
            }
            if let Some((b, _)) = net
                .blocks
                .iter()
                .find(|(_, b)| b.layovers.iter().any(|l| l.at.as_ref() == Some(id)))
            {
                return Err(format!("stop {id} is a layover point of block {b}"));
            }
            net.stops.remove(id);
        }
        NetworkEdit::UpsertRoute { id, route } => {
            net.routes.insert(id.clone(), route.clone());
        }
        NetworkEdit::RemoveRoute { id } => {
            exists(net.routes.contains_key(id), "route", id)?;
            if let Some((p, _)) = net.patterns.iter().find(|(_, p)| &p.route == id) {
                return Err(format!("route {id} carries pattern {p}"));
            }
            net.routes.remove(id);
        }
        NetworkEdit::UpsertPattern { id, pattern } => {
            net.patterns.insert(id.clone(), pattern.clone());
        }
        NetworkEdit::RemovePattern { id } => {
            exists(net.patterns.contains_key(id), "pattern", id)?;
            if let Some((s, _)) = net.pattern_trips.iter().find(|(_, s)| &s.pattern == id) {
                return Err(format!("pattern {id} is run by pattern trips {s}"));
            }
            net.patterns.remove(id);
        }
        NetworkEdit::UpsertServiceType { id, service_type } => {
            net.service_types.insert(id.clone(), service_type.clone());
        }
        NetworkEdit::RemoveServiceType { id } => {
            exists(net.service_types.contains_key(id), "service type", id)?;
            if let Some((s, _)) = net.pattern_trips.iter().find(|(_, s)| &s.service_type == id) {
                return Err(format!("service type {id} governs pattern trips {s}"));
            }
            net.service_types.remove(id);
        }
        NetworkEdit::UpsertPatternTrips { id, pattern_trips } => {
            net.pattern_trips.insert(id.clone(), pattern_trips.clone());
        }
        NetworkEdit::RetimeTrip { trip, to_secs } => {
            let t = trip_mut(net, trip)?;
            if !matches!(t.departs, Departs::At { .. }) {
                return Err(format!("trip {trip} runs at a frequency — retime its bands instead"));
            }
            t.departs = Departs::At { start_secs: *to_secs };
        }
        NetworkEdit::RemoveTrip { trip } => {
            exists(net.trip(trip).is_some(), "trip", trip)?;
            timetable_edit::remove_trips(net, &[trip.clone()].into_iter().collect());
        }
        NetworkEdit::RemovePatternTrips { id } => {
            let pt = net.pattern_trips.get(id).ok_or_else(|| format!("unknown pattern trips {id}"))?;
            let ids: std::collections::BTreeSet<&str> = pt.trips.iter().map(|t| t.id.as_str()).collect();
            if let Some((b, _)) = net.blocks.iter().find(|(_, b)| b.trips.iter().any(|t| ids.contains(t.trip.as_str()))) {
                return Err(format!("pattern trips {id} have trips in block {b}"));
            }
            net.pattern_trips.remove(id);
        }
        NetworkEdit::UpsertBlock { id, block } => {
            net.blocks.insert(id.clone(), block.clone());
        }
        NetworkEdit::RemoveBlock { id } => {
            exists(net.blocks.contains_key(id), "block", id)?;
            net.blocks.remove(id);
        }
        _ => unreachable!("gestures apply in apply_gesture"),
    }
    Ok(None)
}

/// The gesture-level edits. `Ok(None)` means "not a gesture" (the
/// entity-level match applies it); `Ok(Some(created))` means applied.
fn apply_gesture(net: &mut Network, edit: &NetworkEdit) -> Result<Option<Option<CreatedEntity>>, String> {
    let created = match edit {
        NetworkEdit::UpdatePlan { name, effective_from, id } => {
            if let Some(name) = name {
                net.plan.name = name.clone();
            }
            match effective_from.as_deref() {
                Some("") => net.plan.effective_from = None,
                Some(day) => net.plan.effective_from = Some(date(day)?),
                None => {}
            }
            if let (Some(id), true) = (id, net.plan.id.is_empty()) {
                net.plan.id = id.clone();
            }
            None
        }
        NetworkEdit::AddStop { id, lat, lon } => {
            let id = mint(&net.stops, id, "s", "stop")?;
            let n = net.stops.len().max(1) as f64;
            let (clat, clon) = net.stops.values().fold((0.0, 0.0), |(a, b), s| (a + s.lat / n, b + s.lon / n));
            let name = format!("Stop {}", net.stops.len() + 1);
            net.stops.insert(
                id.clone(),
                Stop { name, lat: lat.unwrap_or(clat), lon: lon.unwrap_or(clon), parent: None, transfer: false, ..Default::default() },
            );
            created("stop", &id)
        }
        NetworkEdit::AddRoute { id } => {
            let id = mint(&net.routes, id, "r", "route")?;
            let name = format!("Route {}", net.routes.len() + 1);
            net.routes.insert(id.clone(), Route { name, mode: Mode::Bus, color: None, directions: Default::default(), ..Default::default() });
            created("route", &id)
        }
        NetworkEdit::AddPattern { id, route } => {
            let route = match route {
                Some(r) => {
                    exists(net.routes.contains_key(r), "route", r)?;
                    r.clone()
                }
                None => net.routes.keys().next().cloned().ok_or("add a route first")?,
            };
            let stops: Vec<String> = net.stops.keys().take(2).cloned().collect();
            let (first, second) = match stops.as_slice() {
                [] => return Err("add stops first".into()),
                [only] => (only.clone(), only.clone()),
                [a, b, ..] => (a.clone(), b.clone()),
            };
            let id = mint(&net.patterns, id, "pat", "pattern")?;
            let name = format!("Pattern {}", net.patterns.len() + 1);
            net.patterns.insert(
                id.clone(),
                Pattern {
                    direction: None,
                    route,
                    name,
                    alignment: None,
                    stops: vec![
                        PatternStop { stop: first, timepoint: true, distance_m: 0.0, ..Default::default() },
                        PatternStop { stop: second, timepoint: true, distance_m: 500.0, ..Default::default() },
                    ],
                    timing: vec![TimingProfile { start_secs: 0, end_secs: 30 * 3600, at_timepoints: vec![0, 600] }],
                },
            );
            created("pattern", &id)
        }
        NetworkEdit::AddServiceType { id, from } => {
            let start = date(from)?;
            let id = mint(&net.service_types, id, "cal", "service type")?;
            net.service_types.insert(
                id.clone(),
                ServiceType {
                    name: format!("Service type {id}"),
                    rule: CalendarRule {
                        days: WeekdaySet::weekdays(),
                        ranges: vec![DateRange { start, end: start + chrono::Days::new(364) }],
                        exceptions: vec![],
                    },
                },
            );
            created("service_type", &id)
        }
        NetworkEdit::AddPatternTrips { id, pattern, service_type } => {
            let pattern = match pattern {
                Some(p) => p.clone(),
                None => net.patterns.keys().next().cloned().ok_or("add a pattern first")?,
            };
            let service_type = match service_type {
                Some(t) => t.clone(),
                None => net.service_types.keys().next().cloned().ok_or("add a service type first")?,
            };
            exists(net.patterns.contains_key(&pattern), "pattern", &pattern)?;
            exists(net.service_types.contains_key(&service_type), "service type", &service_type)?;
            let id = mint(&net.pattern_trips, id, "svc", "pattern trips")?;
            net.pattern_trips.insert(id.clone(), PatternTrips { pattern, service_type, trips: vec![] });
            if let Err(e) = add_trip(net, &id, &None, 8 * 3600) {
                net.pattern_trips.remove(&id);
                return Err(e);
            }
            created("pattern_trips", &id)
        }
        NetworkEdit::AddBlock { id } => {
            let id = mint(&net.blocks, id, "b", "block")?;
            net.blocks.insert(id.clone(), Block { trips: vec![], layovers: vec![] });
            created("block", &id)
        }
        NetworkEdit::UpdateStop { id, name, lat, lon, transfer } => {
            let stop = get_mut(&mut net.stops, id, "stop")?;
            if let Some(name) = name {
                stop.name = name.clone();
            }
            if let Some(lat) = lat.filter(|v| v.is_finite()) {
                stop.lat = lat;
            }
            if let Some(lon) = lon.filter(|v| v.is_finite()) {
                stop.lon = lon;
            }
            if let Some(transfer) = transfer {
                stop.transfer = *transfer;
            }
            None
        }
        NetworkEdit::UpdateRoute { id, name, mode } => {
            let route = get_mut(&mut net.routes, id, "route")?;
            if let Some(name) = name {
                route.name = name.clone();
            }
            if let Some(mode) = mode {
                route.mode = *mode;
            }
            None
        }
        NetworkEdit::UpdatePattern { id, name, route, direction } => {
            if let Some(r) = route {
                exists(net.routes.contains_key(r), "route", r)?;
            }
            let route_id = match (route, net.patterns.get(id)) {
                (Some(r), _) => r.clone(),
                (None, Some(p)) => p.route.clone(),
                (None, None) => return Err(format!("unknown pattern {id}")),
            };
            if let Some(d) = direction.as_deref().filter(|d| !d.is_empty()) {
                let has = net.routes.get(&route_id).is_some_and(|r| r.directions.contains_key(d));
                if !has {
                    return Err(format!("route {route_id} has no direction {d}"));
                }
            }
            let pattern = get_mut(&mut net.patterns, id, "pattern")?;
            if let Some(name) = name {
                pattern.name = name.clone();
            }
            if let Some(route) = route {
                if &pattern.route != route {
                    pattern.direction = None;
                }
                pattern.route = route.clone();
            }
            if let Some(d) = direction {
                pattern.direction = (!d.is_empty()).then(|| d.clone());
            }
            None
        }
        NetworkEdit::SetDirection { route, id, name } => {
            if id.is_empty() {
                return Err("a direction needs an id".into());
            }
            get_mut(&mut net.routes, route, "route")?
                .directions
                .insert(id.clone(), Direction { name: name.clone() });
            None
        }
        NetworkEdit::RemoveDirection { route, id } => {
            exists(net.routes.contains_key(route), "route", route)?;
            exists(net.routes[route].directions.contains_key(id), "direction", id)?;
            if let Some((p, _)) = net
                .patterns
                .iter()
                .find(|(_, p)| &p.route == route && p.direction.as_ref() == Some(id))
            {
                return Err(format!("direction {id} of route {route} carries pattern {p}"));
            }
            get_mut(&mut net.routes, route, "route")?.directions.remove(id);
            None
        }
        NetworkEdit::UpdateServiceType { id, name } => {
            get_mut(&mut net.service_types, id, "service type")?.name = name.clone();
            None
        }
        NetworkEdit::SetPatternTripsRefs { id, pattern, service_type } => {
            if let Some(p) = pattern {
                exists(net.patterns.contains_key(p), "pattern", p)?;
            }
            if let Some(t) = service_type {
                exists(net.service_types.contains_key(t), "service type", t)?;
            }
            let pattern_trips = get_mut(&mut net.pattern_trips, id, "pattern trips")?;
            if let Some(p) = pattern {
                pattern_trips.pattern = p.clone();
            }
            if let Some(t) = service_type {
                pattern_trips.service_type = t.clone();
            }
            None
        }
        NetworkEdit::InsertPatternStop { pattern, stop, index } => {
            exists(net.stops.contains_key(stop), "stop", stop)?;
            let p = get_mut(&mut net.patterns, pattern, "pattern")?;
            let n = p.stops.len();
            if n < 2 {
                return Err(format!("pattern {pattern} has no last leg to insert into"));
            }
            let at = index.unwrap_or(n - 1);
            if at == 0 || at >= n {
                return Err(format!("visit {at} of pattern {pattern} is not a middle position — endpoints stay"));
            }
            let distance_m = (p.stops[at - 1].distance_m + p.stops[at].distance_m) / 2.0;
            p.stops.insert(at, PatternStop { stop: stop.clone(), timepoint: false, distance_m, ..Default::default() });
            timetable_edit::insert_trip_stop(net, pattern, at);
            None
        }
        NetworkEdit::SetHeadway { route, direction, service_type, from_secs, to_secs, headway_secs, pattern } => {
            timetable_edit::set_headway(
                net,
                route,
                direction.as_deref(),
                service_type,
                *from_secs,
                *to_secs,
                *headway_secs,
                pattern.as_deref(),
            )?
        }
        NetworkEdit::SetSpan { route, direction, service_type, first_secs, last_secs, headway_secs } => {
            timetable_edit::set_span(net, route, direction.as_deref(), service_type, *first_secs, *last_secs, *headway_secs)?
        }
        NetworkEdit::AdjustRunningTime { route, direction, service_type, from_secs, to_secs, delta_secs } => {
            timetable_edit::adjust_running_time(
                net,
                route,
                direction.as_deref(),
                service_type,
                *from_secs,
                *to_secs,
                *delta_secs,
            )?
        }
        NetworkEdit::InsertDirectionStop { route, direction, stop, after } => {
            timetable_edit::insert_direction_stop(net, route, direction.as_deref(), stop, after)?
        }
        NetworkEdit::RemoveDirectionStop { route, direction, stop } => {
            timetable_edit::remove_direction_stop(net, route, direction.as_deref(), stop)?
        }
        NetworkEdit::RemovePatternStop { pattern, index } => {
            let p = get_mut(&mut net.patterns, pattern, "pattern")?;
            middle(p, pattern, *index)?;
            let position = timepoint_position(p, *index);
            if p.stops.remove(*index).timepoint {
                for profile in &mut p.timing {
                    if position < profile.at_timepoints.len() {
                        profile.at_timepoints.remove(position);
                    }
                }
            }
            timetable_edit::remove_trip_stop(net, pattern, *index);
            None
        }
        NetworkEdit::SetPatternStop { pattern, index, stop, distance_m } => {
            if let Some(s) = stop {
                exists(net.stops.contains_key(s), "stop", s)?;
            }
            let p = get_mut(&mut net.patterns, pattern, "pattern")?;
            let row = at_mut(&mut p.stops, *index, "visit")?;
            if let Some(s) = stop {
                row.stop = s.clone();
            }
            if let Some(d) = distance_m.filter(|d| d.is_finite()) {
                row.distance_m = d;
            }
            None
        }
        NetworkEdit::ToggleTimepoint { pattern, index } => {
            let before = net.patterns.get(pattern).cloned();
            let p = get_mut(&mut net.patterns, pattern, "pattern")?;
            middle(p, pattern, *index)?;
            let position = timepoint_position(p, *index);
            let turning_off = p.stops[*index].timepoint;
            p.stops[*index].timepoint = !turning_off;
            for profile in &mut p.timing {
                let times = &mut profile.at_timepoints;
                if turning_off {
                    if position < times.len() {
                        times.remove(position);
                    }
                } else {
                    let prev = if position > 0 { times.get(position - 1).copied().unwrap_or(0) } else { 0 };
                    let next = times.get(position).copied().unwrap_or(prev);
                    times.insert(position.min(times.len()), (prev + next).div_ceil(2));
                }
            }
            // A timepoint is exact on every trip: one that left the stop
            // unscheduled is scheduled where it materialized.
            if !turning_off {
                let before = before.expect("found above");
                for trip in timetable_edit::pattern_trips_mut(net, pattern) {
                    let Ok(at) = crate::materialize::offsets(&before, trip) else { continue };
                    let (a, d) = at[*index];
                    let o: &mut StopOffset = &mut trip.times[*index];
                    o.arrival_secs.get_or_insert(a);
                    o.departure_secs.get_or_insert(d);
                }
            }
            None
        }
        NetworkEdit::AddTimingProfile { pattern } => {
            let p = get_mut(&mut net.patterns, pattern, "pattern")?;
            let (start, times) = match p.timing.last() {
                Some(last) => (last.end_secs, last.at_timepoints.clone()),
                None => (0, vec![0; p.stops.iter().filter(|s| s.timepoint).count()]),
            };
            p.timing.push(TimingProfile { start_secs: start, end_secs: start + 6 * 3600, at_timepoints: times });
            None
        }
        NetworkEdit::RemoveTimingProfile { pattern, index } => {
            let p = get_mut(&mut net.patterns, pattern, "pattern")?;
            if p.timing.len() <= 1 {
                return Err(format!("pattern {pattern} keeps at least one timing profile"));
            }
            at_mut(&mut p.timing, *index, "timing profile")?;
            p.timing.remove(*index);
            None
        }
        NetworkEdit::SetTimingProfile { pattern, index, start_secs, end_secs } => {
            let profile = at_mut(&mut get_mut(&mut net.patterns, pattern, "pattern")?.timing, *index, "timing profile")?;
            if let Some(s) = start_secs {
                profile.start_secs = *s;
            }
            if let Some(e) = end_secs {
                profile.end_secs = *e;
            }
            None
        }
        NetworkEdit::SetTimepointTime { pattern, profile, timepoint, secs } => {
            let profile = at_mut(&mut get_mut(&mut net.patterns, pattern, "pattern")?.timing, *profile, "timing profile")?;
            *at_mut(&mut profile.at_timepoints, *timepoint, "timepoint")? = *secs;
            None
        }
        NetworkEdit::ToggleServiceDay { id, day } => {
            let days = &mut get_mut(&mut net.service_types, id, "service type")?.rule.days;
            let flag = match day {
                Day::Mon => &mut days.mon,
                Day::Tue => &mut days.tue,
                Day::Wed => &mut days.wed,
                Day::Thu => &mut days.thu,
                Day::Fri => &mut days.fri,
                Day::Sat => &mut days.sat,
                Day::Sun => &mut days.sun,
            };
            *flag = !*flag;
            None
        }
        NetworkEdit::AddServiceRange { id } => {
            let ranges = &mut get_mut(&mut net.service_types, id, "service type")?.rule.ranges;
            let last = ranges.last().ok_or_else(|| format!("service type {id} has no range to continue — set one"))?.end;
            ranges.push(DateRange { start: last, end: last });
            None
        }
        NetworkEdit::SetServiceRange { id, index, start, end } => {
            let start = start.as_deref().map(date).transpose()?;
            let end = end.as_deref().map(date).transpose()?;
            let range = at_mut(&mut get_mut(&mut net.service_types, id, "service type")?.rule.ranges, *index, "range")?;
            if let Some(s) = start {
                range.start = s;
            }
            if let Some(e) = end {
                range.end = e;
            }
            None
        }
        NetworkEdit::RemoveServiceRange { id, index } => {
            let ranges = &mut get_mut(&mut net.service_types, id, "service type")?.rule.ranges;
            at_mut(ranges, *index, "range")?;
            ranges.remove(*index);
            None
        }
        NetworkEdit::SetServiceException { id, date: day, state } => {
            let day = date(day)?;
            let exceptions = &mut get_mut(&mut net.service_types, id, "service type")?.rule.exceptions;
            match exceptions.iter_mut().find(|e| e.date == day) {
                Some(e) => e.state = *state,
                None => exceptions.push(CalendarException { date: day, state: *state }),
            }
            exceptions.sort_by_key(|e| e.date);
            None
        }
        NetworkEdit::RemoveServiceException { id, date: day } => {
            let day = date(day)?;
            get_mut(&mut net.service_types, id, "service type")?.rule.exceptions.retain(|e| e.date != day);
            None
        }
        NetworkEdit::AddTrip { pattern_trips, id, start_secs } => {
            let id = add_trip(net, pattern_trips, id, *start_secs)?;
            created("trip", &id)
        }
        NetworkEdit::SetBand { trip, index, start_secs, end_secs, headway_secs } => {
            let band = at_mut(bands(net, trip)?, *index, "band")?;
            if let Some(s) = start_secs {
                band.start_secs = *s;
            }
            if let Some(e) = end_secs {
                band.end_secs = *e;
            }
            if let Some(h) = headway_secs {
                band.headway_secs = *h;
            }
            None
        }
        NetworkEdit::AddBand { trip } => {
            let bands = bands(net, trip)?;
            let start = bands.last().map(|b| b.end_secs).unwrap_or(6 * 3600);
            bands.push(HeadwayBand { start_secs: start, end_secs: start + 3 * 3600, headway_secs: 900 });
            None
        }
        NetworkEdit::RemoveBand { trip, index } => {
            let bands = bands(net, trip)?;
            if bands.len() <= 1 {
                return Err(format!("trip {trip} keeps at least one band"));
            }
            at_mut(bands, *index, "band")?;
            bands.remove(*index);
            None
        }
        NetworkEdit::AddBlockTrip { block, trip } => {
            exists(net.trip(&trip.trip).is_some(), "trip", &trip.trip)?;
            let first: std::collections::BTreeMap<String, u32> = net
                .trips()
                .map(|(_, _, t)| (t.id.clone(), t.departs.first_start().unwrap_or(0)))
                .collect();
            let b = get_mut(&mut net.blocks, block, "block")?;
            let before = b.trips.clone();
            b.trips.push(trip.clone());
            b.trips.sort_by(|a, c| {
                first.get(&a.trip).cmp(&first.get(&c.trip)).then_with(|| a.trip.cmp(&c.trip))
            });
            timetable_edit::reindex_layovers(b, &before);
            None
        }
        NetworkEdit::RemoveBlockTrip { block, index } => {
            let b = get_mut(&mut net.blocks, block, "block")?;
            at_mut(&mut b.trips, *index, "trip")?;
            let before = b.trips.clone();
            b.trips.remove(*index);
            timetable_edit::reindex_layovers(b, &before);
            None
        }
        NetworkEdit::SetLayover { block, after_trip, secs } => {
            let b = get_mut(&mut net.blocks, block, "block")?;
            if after_trip + 1 >= b.trips.len() {
                return Err(format!("block {block} has no trip after trip {after_trip} — a layover sits between two"));
            }
            match (secs, b.layovers.iter_mut().find(|l| l.after_trip == *after_trip)) {
                (None, _) => b.layovers.retain(|l| l.after_trip != *after_trip),
                (Some(s), Some(existing)) => existing.secs = *s,
                (Some(s), None) => b.layovers.push(Layover { after_trip: *after_trip, secs: *s, at: None }),
            }
            None
        }
        _ => return Ok(None),
    };
    Ok(Some(created))
}

fn exists(present: bool, kind: &str, id: &str) -> Result<(), String> {
    if present {
        Ok(())
    } else {
        Err(format!("unknown {kind} {id}"))
    }
}
