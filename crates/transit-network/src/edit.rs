//! Removal refuses while anything still references the entity, because a
//! dangling reference breaks the document, so every host gets that rule from
//! the model. `apply` does not check that the result is valid. A document in
//! the middle of an edit may be invalid, and `validation_issues` reports it.

use serde::{Deserialize, Serialize};
#[cfg(feature = "bindings")]
use ts_rs::TS;

use std::collections::BTreeMap;

use transit_date::Date;

use crate::materialize::produce_times;
use crate::model::{
    Alignment, AlignmentPoint, Block, CalendarException, CalendarRule, Coord, DateRange, DayState,
    Departs, Direction, HeadwayBand, Layover, Mode, Network, Pattern, PatternStop, PatternTrips,
    PlanMeta, Route, ServiceType, Stop, StopOffset, TimingProfile, Trip, TripRef, WeekdaySet,
};
use crate::timetable_edit;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS, schemars::JsonSchema))]
#[cfg_attr(feature = "bindings", ts(export))]
#[serde(tag = "edit", rename_all = "snake_case")]
pub enum NetworkEdit {
    SetPlan {
        plan: PlanMeta,
        #[serde(default)]
        #[cfg_attr(feature = "bindings", ts(optional))]
        through: Option<String>,
    },
    UpdatePlan {
        #[serde(default)]
        #[cfg_attr(feature = "bindings", ts(optional))]
        name: Option<String>,
        #[serde(default)]
        #[cfg_attr(feature = "bindings", ts(optional))]
        effective_from: Option<String>,
        #[serde(default)]
        #[cfg_attr(feature = "bindings", ts(optional))]
        id: Option<String>,
    },
    SetName {
        name: String,
    },
    UpsertStop {
        id: String,
        stop: Stop,
    },
    RemoveStop {
        id: String,
    },
    UpsertRoute {
        id: String,
        route: Route,
    },
    RemoveRoute {
        id: String,
    },
    UpsertPattern {
        id: String,
        pattern: Pattern,
    },
    RemovePattern {
        id: String,
    },
    UpsertServiceType {
        id: String,
        service_type: ServiceType,
    },
    RemoveServiceType {
        id: String,
    },
    UpsertPatternTrips {
        id: String,
        pattern_trips: PatternTrips,
    },
    RetimeTrip {
        trip: String,
        to_secs: u32,
    },
    AddTrip {
        pattern_trips: String,
        #[serde(default)]
        #[cfg_attr(feature = "bindings", ts(optional))]
        id: Option<String>,
        start_secs: u32,
    },
    RemoveTrip {
        trip: String,
    },
    RemovePatternTrips {
        id: String,
    },
    UpsertBlock {
        id: String,
        block: Block,
    },
    RemoveBlock {
        id: String,
    },

    AddStop {
        #[serde(default)]
        #[cfg_attr(feature = "bindings", ts(optional))]
        id: Option<String>,
        #[serde(default)]
        #[cfg_attr(feature = "bindings", ts(optional))]
        lat: Option<f64>,
        #[serde(default)]
        #[cfg_attr(feature = "bindings", ts(optional))]
        lon: Option<f64>,
    },
    AddRoute {
        #[serde(default)]
        #[cfg_attr(feature = "bindings", ts(optional))]
        id: Option<String>,
    },
    AddPattern {
        #[serde(default)]
        #[cfg_attr(feature = "bindings", ts(optional))]
        id: Option<String>,
        #[serde(default)]
        #[cfg_attr(feature = "bindings", ts(optional))]
        route: Option<String>,
    },
    AddPatternThrough {
        #[serde(default)]
        #[cfg_attr(feature = "bindings", ts(optional))]
        id: Option<String>,
        route: String,
        #[serde(default)]
        #[cfg_attr(feature = "bindings", ts(optional))]
        direction: Option<String>,
        #[serde(default)]
        #[cfg_attr(feature = "bindings", ts(optional))]
        name: Option<String>,
        stops: Vec<String>,
        run_secs: u32,
    },
    AddServiceType {
        #[serde(default)]
        #[cfg_attr(feature = "bindings", ts(optional))]
        id: Option<String>,
        from: String,
    },
    AddPatternTrips {
        #[serde(default)]
        #[cfg_attr(feature = "bindings", ts(optional))]
        id: Option<String>,
        #[serde(default)]
        #[cfg_attr(feature = "bindings", ts(optional))]
        pattern: Option<String>,
        #[serde(default)]
        #[cfg_attr(feature = "bindings", ts(optional))]
        service_type: Option<String>,
    },
    AddBlock {
        #[serde(default)]
        #[cfg_attr(feature = "bindings", ts(optional))]
        id: Option<String>,
    },

    UpdateStop {
        id: String,
        #[serde(default)]
        #[cfg_attr(feature = "bindings", ts(optional))]
        name: Option<String>,
        #[serde(default)]
        #[cfg_attr(feature = "bindings", ts(optional))]
        lat: Option<f64>,
        #[serde(default)]
        #[cfg_attr(feature = "bindings", ts(optional))]
        lon: Option<f64>,
        #[serde(default)]
        #[cfg_attr(feature = "bindings", ts(optional))]
        transfer: Option<bool>,
    },
    UpdateRoute {
        id: String,
        #[serde(default)]
        #[cfg_attr(feature = "bindings", ts(optional))]
        name: Option<String>,
        #[serde(default)]
        #[cfg_attr(feature = "bindings", ts(optional))]
        mode: Option<Mode>,
    },
    UpdatePattern {
        id: String,
        #[serde(default)]
        #[cfg_attr(feature = "bindings", ts(optional))]
        name: Option<String>,
        #[serde(default)]
        #[cfg_attr(feature = "bindings", ts(optional))]
        route: Option<String>,
        #[serde(default)]
        #[cfg_attr(feature = "bindings", ts(optional))]
        direction: Option<String>,
    },
    DrawAlignment {
        pattern: String,
        points: Vec<Coord>,
        redraw: Redraw,
        #[serde(default)]
        #[cfg_attr(feature = "bindings", ts(optional))]
        id: Option<String>,
    },
    SetDirection {
        route: String,
        id: String,
        name: String,
    },
    RemoveDirection {
        route: String,
        id: String,
    },
    UpdateServiceType {
        id: String,
        name: String,
    },
    SetPatternTripsRefs {
        id: String,
        #[serde(default)]
        #[cfg_attr(feature = "bindings", ts(optional))]
        pattern: Option<String>,
        #[serde(default)]
        #[cfg_attr(feature = "bindings", ts(optional))]
        service_type: Option<String>,
    },

    InsertPatternStop {
        pattern: String,
        stop: String,
        #[serde(default)]
        #[cfg_attr(feature = "bindings", ts(optional))]
        index: Option<usize>,
    },
    RemovePatternStop {
        pattern: String,
        index: usize,
    },
    SetPatternStop {
        pattern: String,
        index: usize,
        #[serde(default)]
        #[cfg_attr(feature = "bindings", ts(optional))]
        stop: Option<String>,
        #[serde(default)]
        #[cfg_attr(feature = "bindings", ts(optional))]
        distance_m: Option<f64>,
    },
    ToggleTimepoint {
        pattern: String,
        index: usize,
    },
    AddTimingProfile {
        pattern: String,
    },
    RemoveTimingProfile {
        pattern: String,
        index: usize,
    },
    SetTimingProfile {
        pattern: String,
        index: usize,
        #[serde(default)]
        #[cfg_attr(feature = "bindings", ts(optional))]
        start_secs: Option<u32>,
        #[serde(default)]
        #[cfg_attr(feature = "bindings", ts(optional))]
        end_secs: Option<u32>,
    },
    SetTimepointTime {
        pattern: String,
        profile: usize,
        timepoint: usize,
        secs: u32,
    },

    ToggleServiceDay {
        id: String,
        day: Day,
    },
    AddServiceRange {
        id: String,
        #[serde(default)]
        #[cfg_attr(feature = "bindings", ts(optional))]
        start: Option<String>,
        #[serde(default)]
        #[cfg_attr(feature = "bindings", ts(optional))]
        end: Option<String>,
    },
    SetServiceRange {
        id: String,
        index: usize,
        #[serde(default)]
        #[cfg_attr(feature = "bindings", ts(optional))]
        start: Option<String>,
        #[serde(default)]
        #[cfg_attr(feature = "bindings", ts(optional))]
        end: Option<String>,
    },
    RemoveServiceRange {
        id: String,
        index: usize,
    },
    SetServiceException {
        id: String,
        date: String,
        state: DayState,
    },
    RemoveServiceException {
        id: String,
        date: String,
    },

    SetBand {
        trip: String,
        index: usize,
        #[serde(default)]
        #[cfg_attr(feature = "bindings", ts(optional))]
        start_secs: Option<u32>,
        #[serde(default)]
        #[cfg_attr(feature = "bindings", ts(optional))]
        end_secs: Option<u32>,
        #[serde(default)]
        #[cfg_attr(feature = "bindings", ts(optional))]
        headway_secs: Option<u32>,
    },
    AddBand {
        trip: String,
    },
    RemoveBand {
        trip: String,
        index: usize,
    },

    SetHeadway {
        route: String,
        #[serde(default)]
        #[cfg_attr(feature = "bindings", ts(optional))]
        direction: Option<String>,
        service_type: String,
        from_secs: u32,
        to_secs: u32,
        headway_secs: u32,
        #[serde(default)]
        #[cfg_attr(feature = "bindings", ts(optional))]
        pattern: Option<String>,
    },
    SetSpan {
        route: String,
        #[serde(default)]
        #[cfg_attr(feature = "bindings", ts(optional))]
        direction: Option<String>,
        service_type: String,
        #[serde(default)]
        #[cfg_attr(feature = "bindings", ts(optional))]
        first_secs: Option<u32>,
        #[serde(default)]
        #[cfg_attr(feature = "bindings", ts(optional))]
        last_secs: Option<u32>,
        #[serde(default)]
        #[cfg_attr(feature = "bindings", ts(optional))]
        headway_secs: Option<u32>,
    },
    AdjustRunningTime {
        route: String,
        #[serde(default)]
        #[cfg_attr(feature = "bindings", ts(optional))]
        direction: Option<String>,
        service_type: String,
        from_secs: u32,
        to_secs: u32,
        delta_secs: i32,
    },
    InsertDirectionStop {
        route: String,
        #[serde(default)]
        #[cfg_attr(feature = "bindings", ts(optional))]
        direction: Option<String>,
        stop: String,
        after: String,
    },
    RemoveDirectionStop {
        route: String,
        #[serde(default)]
        #[cfg_attr(feature = "bindings", ts(optional))]
        direction: Option<String>,
        stop: String,
    },

    AddBlockTrip {
        block: String,
        trip: TripRef,
    },
    RemoveBlockTrip {
        block: String,
        index: usize,
    },
    SetLayover {
        block: String,
        after_trip: usize,
        #[serde(default)]
        secs: Option<u32>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS, schemars::JsonSchema))]
#[cfg_attr(feature = "bindings", ts(export))]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS, schemars::JsonSchema))]
#[cfg_attr(feature = "bindings", ts(export))]
#[serde(rename_all = "snake_case")]
pub enum Redraw {
    One,
    All,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "bindings", derive(TS, schemars::JsonSchema))]
#[cfg_attr(feature = "bindings", ts(export))]
pub struct TouchedEntity {
    pub kind: String,
    pub id: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Applied {
    pub network: Network,
    pub created: Vec<CreatedEntity>,
    pub touched: Vec<TouchedEntity>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "bindings", derive(TS, schemars::JsonSchema))]
#[cfg_attr(feature = "bindings", ts(export))]
pub struct CreatedEntity {
    pub kind: String,
    pub id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[cfg_attr(feature = "bindings", derive(TS, schemars::JsonSchema))]
#[cfg_attr(feature = "bindings", ts(export))]
pub struct EditError {
    pub index: usize,
    pub reason: String,
}

impl std::fmt::Display for EditError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "edit {}: {}", self.index, self.reason)
    }
}

pub fn apply_edits(network: &Network, edits: &[NetworkEdit]) -> Result<Network, EditError> {
    apply_edits_reporting(network, edits).map(|applied| applied.network)
}

pub fn apply_edits_reporting(
    network: &Network,
    edits: &[NetworkEdit],
) -> Result<Applied, EditError> {
    let mut next = network.clone();
    let mut created = Vec::new();
    let mut touched = Vec::new();
    for (index, edit) in edits.iter().enumerate() {
        let entity = match edit {
            NetworkEdit::DrawAlignment {
                pattern,
                points,
                redraw,
                id,
            } => draw_alignment(&mut next, pattern, points, *redraw, id, &mut touched),
            _ => apply_one(&mut next, edit),
        }
        .map_err(|reason| EditError { index, reason })?;
        if let Some(entity) = entity {
            created.push(entity);
        }
    }
    Ok(Applied {
        network: next,
        created,
        touched,
    })
}

fn draw_alignment(
    net: &mut Network,
    pattern: &str,
    points: &[Coord],
    redraw: Redraw,
    id: &Option<String>,
    touched: &mut Vec<TouchedEntity>,
) -> Result<Option<CreatedEntity>, String> {
    let current = net
        .patterns
        .get(pattern)
        .ok_or_else(|| format!("unknown pattern {pattern}"))?
        .alignment
        .clone();
    if points.len() < 2 {
        return Err(format!(
            "a line is two points or more; {} given",
            points.len()
        ));
    }
    if let Some(p) = points.iter().find(|p| {
        !(p.lat.is_finite() && p.lon.is_finite() && p.lat.abs() <= 90.0 && p.lon.abs() <= 180.0)
    }) {
        return Err(format!("point ({}, {}) is not on the globe", p.lat, p.lon));
    }
    let line = Alignment {
        points: points
            .iter()
            .map(|p| AlignmentPoint {
                lat: p.lat,
                lon: p.lon,
                distance: None,
            })
            .collect(),
        extra: Default::default(),
    };
    let (redrawn, created) = match (redraw, current) {
        (Redraw::All, Some(shared)) => {
            if id.is_some() {
                return Err(format!(
                    "redraw all keeps the alignment's id ({shared}); an id names a new line"
                ));
            }
            net.alignments.insert(shared.clone(), line);
            let on: Vec<String> = net
                .patterns
                .iter()
                .filter(|(_, p)| p.alignment.as_ref() == Some(&shared))
                .map(|(id, _)| id.clone())
                .collect();
            (on, None)
        }
        _ => {
            let new_id = mint(&net.alignments, id, "al", "alignment")?;
            net.alignments.insert(new_id.clone(), line);
            net.patterns.get_mut(pattern).expect("checked").alignment = Some(new_id.clone());
            (vec![pattern.to_string()], created("alignment", &new_id))
        }
    };
    for id in redrawn {
        for stop in &mut net
            .patterns
            .get_mut(&id)
            .expect("listed from the patterns")
            .stops
        {
            stop.distance_published = false;
        }
        touched.push(TouchedEntity {
            kind: "pattern".into(),
            id,
        });
    }
    Ok(created)
}

/// A fork carries each weekly calendar that still runs in its last week to the period's end.
fn carry_calendars(net: &mut Network, through: Date) {
    // A range that starts after `through`, or a date past it, is outside the
    // calendar of the fork. It must not make a type look closed or
    // still running.
    let in_time = |r: &DateRange| r.start <= through;
    let weekly =
        |rule: &CalendarRule| rule.days != WeekdaySet::none() && rule.ranges.iter().any(in_time);
    let Some(calendar_end) = net
        .service_types
        .values()
        .flat_map(|t| {
            let weekly_end = weekly(&t.rule)
                .then(|| {
                    t.rule
                        .ranges
                        .iter()
                        .filter(|r| in_time(r))
                        .map(|r| r.end.min(through))
                        .max()
                })
                .flatten();
            let added = t
                .rule
                .exceptions
                .iter()
                .filter(|e| e.state == DayState::Added && e.date <= through)
                .map(|e| e.date);
            weekly_end.into_iter().chain(added)
        })
        .max()
    else {
        return;
    };
    let final_week = calendar_end - transit_date::Days::new(6);
    for service_type in net.service_types.values_mut() {
        let rule = &mut service_type.rule;
        if !weekly(rule) {
            continue;
        }
        if let Some(last) = rule
            .ranges
            .iter_mut()
            .filter(|r| r.start <= through)
            .max_by_key(|r| r.end)
        {
            if last.end >= final_week && last.end < through {
                last.end = through;
            }
        }
    }
}

/// The calendar of a fork is its period.
fn clip_calendars(net: &mut Network, from: Option<Date>, through: Option<Date>) {
    let inside = |d: Date| from.is_none_or(|f| d >= f) && through.is_none_or(|t| d <= t);
    for service_type in net.service_types.values_mut() {
        let rule = &mut service_type.rule;
        for r in &mut rule.ranges {
            if let Some(f) = from {
                r.start = r.start.max(f);
            }
            if let Some(t) = through {
                r.end = r.end.min(t);
            }
        }
        rule.ranges.retain(|r| r.start <= r.end);
        rule.exceptions.retain(|e| inside(e.date));
    }
}

fn mint<V>(
    map: &BTreeMap<String, V>,
    wanted: &Option<String>,
    prefix: &str,
    kind: &str,
) -> Result<String, String> {
    if let Some(id) = wanted {
        if map.contains_key(id) {
            return Err(format!("{kind} {id} already exists"));
        }
        return Ok(id.clone());
    }
    Ok((1..)
        .map(|n| format!("{prefix}{n}"))
        .find(|id| !map.contains_key(id))
        .expect("ids are unbounded"))
}

fn created(kind: &str, id: &str) -> Option<CreatedEntity> {
    Some(CreatedEntity {
        kind: kind.into(),
        id: id.into(),
    })
}

fn get_mut<'a, V>(
    map: &'a mut BTreeMap<String, V>,
    id: &str,
    kind: &str,
) -> Result<&'a mut V, String> {
    map.get_mut(id)
        .ok_or_else(|| format!("unknown {kind} {id}"))
}

fn at_mut<'a, T>(items: &'a mut [T], index: usize, what: &str) -> Result<&'a mut T, String> {
    let len = items.len();
    items
        .get_mut(index)
        .ok_or_else(|| format!("no {what} {index} (there are {len})"))
}

fn middle(pattern: &Pattern, id: &str, index: usize) -> Result<(), String> {
    if index == 0 || index + 1 >= pattern.stops.len() {
        return Err(format!(
            "visit {index} of pattern {id} is an endpoint (or absent) — endpoints stay"
        ));
    }
    Ok(())
}

fn timepoint_position(pattern: &Pattern, index: usize) -> usize {
    pattern.stops[..index]
        .iter()
        .filter(|s| s.timepoint)
        .count()
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

fn add_trip(
    net: &mut Network,
    pattern_trips: &str,
    id: &Option<String>,
    start: u32,
) -> Result<String, String> {
    let pt = net
        .pattern_trips
        .get(pattern_trips)
        .ok_or_else(|| format!("unknown pattern trips {pattern_trips}"))?;
    let pattern = net
        .patterns
        .get(&pt.pattern)
        .ok_or_else(|| format!("unknown pattern {}", pt.pattern))?;
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
        None => timetable_edit::fresh_trip_id(
            net,
            &Default::default(),
            &format!("{pattern_trips}@{start}"),
        ),
    };
    net.pattern_trips
        .get_mut(pattern_trips)
        .expect("checked")
        .trips
        .push(Trip {
            id: id.clone(),
            departs: Departs::At { start_secs: start },
            times,
            ..Default::default()
        });
    Ok(id)
}

/// Layovers exist only between two trips of the block.
pub(crate) fn prune_layovers(block: &mut Block) {
    let trips = block.trips.len();
    block.layovers.retain(|l| l.after_trip + 1 < trips);
}

fn apply_one(net: &mut Network, edit: &NetworkEdit) -> Result<Option<CreatedEntity>, String> {
    if let Some(entity) = apply_gesture(net, edit)? {
        return Ok(entity);
    }
    match edit {
        NetworkEdit::SetPlan { plan, through } => {
            if let Some(through) = through {
                let through = Date::read(through)?;
                if let Some(from) = plan.effective_from.filter(|from| through < *from) {
                    return Err(format!(
                        "through {through} is before the pick's effective_from {from}"
                    ));
                }
                carry_calendars(net, through);
            }
            let through = through.as_deref().map(Date::read).transpose()?;
            clip_calendars(net, plan.effective_from, through);
            net.plan = plan.clone();
        }
        NetworkEdit::SetName { name } => net.name = name.clone(),
        NetworkEdit::UpsertStop { id, stop } => {
            net.stops.insert(id.clone(), stop.clone());
        }
        NetworkEdit::RemoveStop { id } => {
            exists(net.stops.contains_key(id), "stop", id)?;
            if let Some((p, _)) = net
                .patterns
                .iter()
                .find(|(_, p)| p.stops.iter().any(|s| &s.stop == id))
            {
                return Err(format!("stop {id} is visited by pattern {p}"));
            }
            if let Some((s, _)) = net
                .stops
                .iter()
                .find(|(_, s)| s.parent.as_ref() == Some(id))
            {
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
            if let Some((s, _)) = net
                .pattern_trips
                .iter()
                .find(|(_, s)| &s.service_type == id)
            {
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
                return Err(format!(
                    "trip {trip} runs at a frequency — retime its bands instead"
                ));
            }
            t.departs = Departs::At {
                start_secs: *to_secs,
            };
        }
        NetworkEdit::RemoveTrip { trip } => {
            exists(net.trip(trip).is_some(), "trip", trip)?;
            timetable_edit::remove_trips(net, &[trip.clone()].into_iter().collect());
        }
        NetworkEdit::RemovePatternTrips { id } => {
            let pt = net
                .pattern_trips
                .get(id)
                .ok_or_else(|| format!("unknown pattern trips {id}"))?;
            let ids: std::collections::BTreeSet<&str> =
                pt.trips.iter().map(|t| t.id.as_str()).collect();
            if let Some((b, _)) = net
                .blocks
                .iter()
                .find(|(_, b)| b.trips.iter().any(|t| ids.contains(t.trip.as_str())))
            {
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
        _ => {
            unreachable!("gestures apply in apply_gesture; draw_alignment in apply_edits_reporting")
        }
    }
    Ok(None)
}

/// The gesture-level edits. `Ok(None)` means the edit is not a gesture, and
/// the entity-level match applies it. `Ok(Some(created))` means applied.
fn apply_gesture(
    net: &mut Network,
    edit: &NetworkEdit,
) -> Result<Option<Option<CreatedEntity>>, String> {
    let created = match edit {
        NetworkEdit::UpdatePlan {
            name,
            effective_from,
            id,
        } => {
            if let Some(name) = name {
                net.plan.name = name.clone();
            }
            match effective_from.as_deref() {
                Some("") => net.plan.effective_from = None,
                Some(day) => net.plan.effective_from = Some(Date::read(day)?),
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
            let (clat, clon) = net
                .stops
                .values()
                .fold((0.0, 0.0), |(a, b), s| (a + s.lat / n, b + s.lon / n));
            let name = format!("Stop {}", net.stops.len() + 1);
            net.stops.insert(
                id.clone(),
                Stop {
                    name,
                    lat: lat.unwrap_or(clat),
                    lon: lon.unwrap_or(clon),
                    parent: None,
                    transfer: false,
                    ..Default::default()
                },
            );
            created("stop", &id)
        }
        NetworkEdit::AddRoute { id } => {
            let id = mint(&net.routes, id, "r", "route")?;
            let name = format!("Route {}", net.routes.len() + 1);
            net.routes.insert(
                id.clone(),
                Route {
                    name,
                    mode: Mode::Bus,
                    color: None,
                    directions: Default::default(),
                    ..Default::default()
                },
            );
            created("route", &id)
        }
        NetworkEdit::AddPattern { id, route } => {
            let route = match route {
                Some(r) => {
                    exists(net.routes.contains_key(r), "route", r)?;
                    r.clone()
                }
                None => net
                    .routes
                    .keys()
                    .next()
                    .cloned()
                    .ok_or("add a route first")?,
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
                        PatternStop {
                            stop: first,
                            timepoint: true,
                            distance_m: 0.0,
                            ..Default::default()
                        },
                        PatternStop {
                            stop: second,
                            timepoint: true,
                            distance_m: 500.0,
                            ..Default::default()
                        },
                    ],
                    timing: vec![TimingProfile {
                        start_secs: 0,
                        end_secs: 30 * 3600,
                        at_timepoints: vec![0, 600],
                        extra: Default::default(),
                    }],
                    extra: Default::default(),
                },
            );
            created("pattern", &id)
        }
        NetworkEdit::AddPatternThrough {
            id,
            route,
            direction,
            name,
            stops,
            run_secs,
        } => {
            let r = net
                .routes
                .get(route)
                .ok_or_else(|| format!("unknown route {route}"))?;
            if let Some(d) = direction {
                if !r.directions.contains_key(d) {
                    return Err(format!("route {route} has no direction {d}"));
                }
            }
            if stops.len() < 2 {
                return Err("a pattern runs through at least two stops".into());
            }
            for s in stops {
                exists(net.stops.contains_key(s), "stop", s)?;
            }
            if let Some(w) = stops.windows(2).find(|w| w[0] == w[1]) {
                return Err(format!("stop {} is visited twice in a row", w[0]));
            }
            if *run_secs == 0 {
                return Err("a pattern needs a run time longer than zero".into());
            }
            let label = if r.name.is_empty() {
                r.long_name.clone().unwrap_or_else(|| route.clone())
            } else {
                r.name.clone()
            };
            let id = mint(&net.patterns, id, "pat", "pattern")?;
            let mut along = 0.0;
            let visits: Vec<PatternStop> = stops
                .iter()
                .enumerate()
                .map(|(i, s)| {
                    if i > 0 {
                        let (a, b) = (&net.stops[&stops[i - 1]], &net.stops[s]);
                        along += transit_geo::distance_m(a.lat, a.lon, b.lat, b.lon);
                    }
                    PatternStop {
                        stop: s.clone(),
                        timepoint: i == 0 || i == stops.len() - 1,
                        distance_m: along,
                        ..Default::default()
                    }
                })
                .collect();
            let last = &net.stops[stops.last().expect("two stops")].name;
            net.patterns.insert(
                id.clone(),
                Pattern {
                    route: route.clone(),
                    direction: direction.clone(),
                    name: name
                        .clone()
                        .filter(|n| !n.trim().is_empty())
                        .unwrap_or_else(|| format!("{label} → {last}")),
                    alignment: None,
                    stops: visits,
                    timing: vec![TimingProfile {
                        start_secs: 0,
                        end_secs: 30 * 3600,
                        at_timepoints: vec![0, *run_secs],
                        extra: Default::default(),
                    }],
                    extra: Default::default(),
                },
            );
            created("pattern", &id)
        }
        NetworkEdit::AddServiceType { id, from } => {
            let start = Date::read(from)?;
            let id = mint(&net.service_types, id, "cal", "service type")?;
            net.service_types.insert(
                id.clone(),
                ServiceType {
                    name: format!("Service type {id}"),
                    rule: CalendarRule {
                        days: WeekdaySet::weekdays(),
                        ranges: vec![DateRange {
                            start,
                            end: start + transit_date::Days::new(364),
                            extra: Default::default(),
                        }],
                        exceptions: vec![],
                        extra: Default::default(),
                    },
                    extra: Default::default(),
                },
            );
            created("service_type", &id)
        }
        NetworkEdit::AddPatternTrips {
            id,
            pattern,
            service_type,
        } => {
            let pattern = match pattern {
                Some(p) => p.clone(),
                None => net
                    .patterns
                    .keys()
                    .next()
                    .cloned()
                    .ok_or("add a pattern first")?,
            };
            let service_type = match service_type {
                Some(t) => t.clone(),
                None => net
                    .service_types
                    .keys()
                    .next()
                    .cloned()
                    .ok_or("add a service type first")?,
            };
            exists(net.patterns.contains_key(&pattern), "pattern", &pattern)?;
            exists(
                net.service_types.contains_key(&service_type),
                "service type",
                &service_type,
            )?;
            let id = mint(&net.pattern_trips, id, "svc", "pattern trips")?;
            net.pattern_trips.insert(
                id.clone(),
                PatternTrips {
                    pattern,
                    service_type,
                    trips: vec![],
                    extra: Default::default(),
                },
            );
            if let Err(e) = add_trip(net, &id, &None, 8 * 3600) {
                net.pattern_trips.remove(&id);
                return Err(e);
            }
            created("pattern_trips", &id)
        }
        NetworkEdit::AddBlock { id } => {
            let id = mint(&net.blocks, id, "b", "block")?;
            net.blocks.insert(
                id.clone(),
                Block {
                    trips: vec![],
                    layovers: vec![],
                    extra: Default::default(),
                },
            );
            created("block", &id)
        }
        NetworkEdit::UpdateStop {
            id,
            name,
            lat,
            lon,
            transfer,
        } => {
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
        NetworkEdit::UpdatePattern {
            id,
            name,
            route,
            direction,
        } => {
            if let Some(r) = route {
                exists(net.routes.contains_key(r), "route", r)?;
            }
            let route_id = match (route, net.patterns.get(id)) {
                (Some(r), _) => r.clone(),
                (None, Some(p)) => p.route.clone(),
                (None, None) => return Err(format!("unknown pattern {id}")),
            };
            if let Some(d) = direction.as_deref().filter(|d| !d.is_empty()) {
                let has = net
                    .routes
                    .get(&route_id)
                    .is_some_and(|r| r.directions.contains_key(d));
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
            get_mut(&mut net.routes, route, "route")?.directions.insert(
                id.clone(),
                Direction {
                    name: name.clone(),
                    extra: Default::default(),
                },
            );
            None
        }
        NetworkEdit::RemoveDirection { route, id } => {
            exists(net.routes.contains_key(route), "route", route)?;
            exists(
                net.routes[route].directions.contains_key(id),
                "direction",
                id,
            )?;
            if let Some((p, _)) = net
                .patterns
                .iter()
                .find(|(_, p)| &p.route == route && p.direction.as_ref() == Some(id))
            {
                return Err(format!(
                    "direction {id} of route {route} carries pattern {p}"
                ));
            }
            get_mut(&mut net.routes, route, "route")?
                .directions
                .remove(id);
            None
        }
        NetworkEdit::UpdateServiceType { id, name } => {
            get_mut(&mut net.service_types, id, "service type")?.name = name.clone();
            None
        }
        NetworkEdit::SetPatternTripsRefs {
            id,
            pattern,
            service_type,
        } => {
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
        NetworkEdit::InsertPatternStop {
            pattern,
            stop,
            index,
        } => {
            exists(net.stops.contains_key(stop), "stop", stop)?;
            let p = get_mut(&mut net.patterns, pattern, "pattern")?;
            let n = p.stops.len();
            if n < 2 {
                return Err(format!("pattern {pattern} has no last leg to insert into"));
            }
            let at = index.unwrap_or(n - 1);
            if at == 0 || at >= n {
                return Err(format!(
                    "visit {at} of pattern {pattern} is not a middle position — endpoints stay"
                ));
            }
            let distance_m = (p.stops[at - 1].distance_m + p.stops[at].distance_m) / 2.0;
            p.stops.insert(
                at,
                PatternStop {
                    stop: stop.clone(),
                    timepoint: false,
                    distance_m,
                    ..Default::default()
                },
            );
            timetable_edit::insert_trip_stop(net, pattern, at);
            None
        }
        NetworkEdit::SetHeadway {
            route,
            direction,
            service_type,
            from_secs,
            to_secs,
            headway_secs,
            pattern,
        } => timetable_edit::set_headway(
            net,
            route,
            direction.as_deref(),
            service_type,
            *from_secs,
            *to_secs,
            *headway_secs,
            pattern.as_deref(),
        )?,
        NetworkEdit::SetSpan {
            route,
            direction,
            service_type,
            first_secs,
            last_secs,
            headway_secs,
        } => timetable_edit::set_span(
            net,
            route,
            direction.as_deref(),
            service_type,
            *first_secs,
            *last_secs,
            *headway_secs,
        )?,
        NetworkEdit::AdjustRunningTime {
            route,
            direction,
            service_type,
            from_secs,
            to_secs,
            delta_secs,
        } => timetable_edit::adjust_running_time(
            net,
            route,
            direction.as_deref(),
            service_type,
            *from_secs,
            *to_secs,
            *delta_secs,
        )?,
        NetworkEdit::InsertDirectionStop {
            route,
            direction,
            stop,
            after,
        } => timetable_edit::insert_direction_stop(net, route, direction.as_deref(), stop, after)?,
        NetworkEdit::RemoveDirectionStop {
            route,
            direction,
            stop,
        } => timetable_edit::remove_direction_stop(net, route, direction.as_deref(), stop)?,
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
        NetworkEdit::SetPatternStop {
            pattern,
            index,
            stop,
            distance_m,
        } => {
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
                    let prev = if position > 0 {
                        times.get(position - 1).copied().unwrap_or(0)
                    } else {
                        0
                    };
                    let next = times.get(position).copied().unwrap_or(prev);
                    times.insert(position.min(times.len()), (prev + next).div_ceil(2));
                }
            }
            // A timepoint is exact on every trip. A trip that left the stop
            // unscheduled gets the time where it materialized.
            if !turning_off {
                let before = before.expect("found above");
                for trip in timetable_edit::pattern_trips_mut(net, pattern) {
                    let Ok(at) = crate::materialize::offsets(&before, trip) else {
                        continue;
                    };
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
            p.timing.push(TimingProfile {
                start_secs: start,
                end_secs: start + 6 * 3600,
                at_timepoints: times,
                extra: Default::default(),
            });
            None
        }
        NetworkEdit::RemoveTimingProfile { pattern, index } => {
            let p = get_mut(&mut net.patterns, pattern, "pattern")?;
            if p.timing.len() <= 1 {
                return Err(format!(
                    "pattern {pattern} keeps at least one timing profile"
                ));
            }
            at_mut(&mut p.timing, *index, "timing profile")?;
            p.timing.remove(*index);
            None
        }
        NetworkEdit::SetTimingProfile {
            pattern,
            index,
            start_secs,
            end_secs,
        } => {
            let profile = at_mut(
                &mut get_mut(&mut net.patterns, pattern, "pattern")?.timing,
                *index,
                "timing profile",
            )?;
            if let Some(s) = start_secs {
                profile.start_secs = *s;
            }
            if let Some(e) = end_secs {
                profile.end_secs = *e;
            }
            None
        }
        NetworkEdit::SetTimepointTime {
            pattern,
            profile,
            timepoint,
            secs,
        } => {
            let profile = at_mut(
                &mut get_mut(&mut net.patterns, pattern, "pattern")?.timing,
                *profile,
                "timing profile",
            )?;
            *at_mut(&mut profile.at_timepoints, *timepoint, "timepoint")? = *secs;
            None
        }
        NetworkEdit::ToggleServiceDay { id, day } => {
            let days = &mut get_mut(&mut net.service_types, id, "service type")?
                .rule
                .days;
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
        NetworkEdit::AddServiceRange { id, start, end } => {
            let start = start.as_deref().map(Date::read).transpose()?;
            let end = end.as_deref().map(Date::read).transpose()?;
            let ranges = &mut get_mut(&mut net.service_types, id, "service type")?
                .rule
                .ranges;
            let start = match start {
                Some(s) => s,
                None => ranges.last().ok_or_else(|| format!("service type {id} has no range to continue — give the new range a start"))?.end,
            };
            let end = end.unwrap_or(start);
            if end < start {
                return Err(format!("the range ends {end}, before it starts {start}"));
            }
            ranges.push(DateRange {
                start,
                end,
                extra: Default::default(),
            });
            None
        }
        NetworkEdit::SetServiceRange {
            id,
            index,
            start,
            end,
        } => {
            let start = start.as_deref().map(Date::read).transpose()?;
            let end = end.as_deref().map(Date::read).transpose()?;
            let range = at_mut(
                &mut get_mut(&mut net.service_types, id, "service type")?
                    .rule
                    .ranges,
                *index,
                "range",
            )?;
            if let Some(s) = start {
                range.start = s;
            }
            if let Some(e) = end {
                range.end = e;
            }
            None
        }
        NetworkEdit::RemoveServiceRange { id, index } => {
            let ranges = &mut get_mut(&mut net.service_types, id, "service type")?
                .rule
                .ranges;
            at_mut(ranges, *index, "range")?;
            ranges.remove(*index);
            None
        }
        NetworkEdit::SetServiceException {
            id,
            date: day,
            state,
        } => {
            let day = Date::read(day)?;
            let exceptions = &mut get_mut(&mut net.service_types, id, "service type")?
                .rule
                .exceptions;
            match exceptions.iter_mut().find(|e| e.date == day) {
                Some(e) => e.state = *state,
                None => exceptions.push(CalendarException {
                    date: day,
                    state: *state,
                    extra: Default::default(),
                }),
            }
            exceptions.sort_by_key(|e| e.date);
            None
        }
        NetworkEdit::RemoveServiceException { id, date: day } => {
            let day = Date::read(day)?;
            get_mut(&mut net.service_types, id, "service type")?
                .rule
                .exceptions
                .retain(|e| e.date != day);
            None
        }
        NetworkEdit::AddTrip {
            pattern_trips,
            id,
            start_secs,
        } => {
            let id = add_trip(net, pattern_trips, id, *start_secs)?;
            created("trip", &id)
        }
        NetworkEdit::SetBand {
            trip,
            index,
            start_secs,
            end_secs,
            headway_secs,
        } => {
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
            bands.push(HeadwayBand {
                start_secs: start,
                end_secs: start + 3 * 3600,
                headway_secs: 900,
                extra: Default::default(),
            });
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
                first
                    .get(&a.trip)
                    .cmp(&first.get(&c.trip))
                    .then_with(|| a.trip.cmp(&c.trip))
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
        NetworkEdit::SetLayover {
            block,
            after_trip,
            secs,
        } => {
            let b = get_mut(&mut net.blocks, block, "block")?;
            if after_trip + 1 >= b.trips.len() {
                return Err(format!("block {block} has no trip after trip {after_trip} — a layover sits between two"));
            }
            match (
                secs,
                b.layovers.iter_mut().find(|l| l.after_trip == *after_trip),
            ) {
                (None, _) => b.layovers.retain(|l| l.after_trip != *after_trip),
                (Some(s), Some(existing)) => existing.secs = *s,
                (Some(s), None) => b.layovers.push(Layover {
                    after_trip: *after_trip,
                    secs: *s,
                    at: None,
                    extra: Default::default(),
                }),
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
