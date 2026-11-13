//! A pick's stops are a copy, refreshed from the stops document
//! (`transit-stops`; D-156, `docs/units/stop-lifecycle.md` item 1).
//!
//! The network document keeps its shape, stops included. The stops
//! document beside the picks is the record of every stop; [`refresh`]
//! rewrites a pick's stops from it for the pick's window, so the copy
//! never wins. A stop that is open, as recorded, every day of the
//! window carries no events, so a network whose stops do not change
//! keeps its bytes and hash; a stop that changes inside the window —
//! closed, moved, renamed, opened, retired — carries the events that
//! apply on its dates, and [`stop_on`] answers it on a date. [`seed`]
//! starts a stops document from a network's stops.

use std::collections::BTreeMap;

use chrono::NaiveDate;
use transit_stops::{Details, Event, EventKind, Status, StopOn, Stops};

use crate::places::{StopInfo, StopKind};
use crate::{Network, Stop};

/// A stops document from a network's stops: each stop one `opened`
/// event with no date (no feed records a stop's opening), carrying its
/// point and every detail. Refreshing the network from it gives the
/// network back.
pub fn seed(network: &Network) -> Stops {
    let stops = network.stops.iter().map(|(id, stop)| (id.clone(), vec![opening(stop)])).collect();
    Stops { stops, ..Stops::default() }
}

fn opening(stop: &Stop) -> Event {
    let info = &stop.info;
    Event {
        event: EventKind::Opened,
        lat: Some(stop.lat),
        lon: Some(stop.lon),
        details: Details {
            name: Some(stop.name.clone()).filter(|n| !n.is_empty()),
            kind: (stop.kind == StopKind::Station).then_some(transit_stops::StopKind::Station),
            parent: stop.parent.clone(),
            code: info.code.clone(),
            spoken_name: info.spoken_name.clone(),
            description: info.description.clone(),
            url: info.url.clone(),
            fare_zone: info.fare_zone.clone(),
            wheelchair: info.wheelchair,
            transfer: stop.transfer.then_some(true),
            platform: info.platform.clone(),
            level: info.level.clone(),
            timezone: info.timezone.clone(),
            street_access: info.street_access,
            amenities: BTreeMap::new(),
        },
        ..Event::default()
    }
}

/// The network's record of a stop as the stops document has it on a
/// date (amenities are the stops document's alone).
fn record(on: &StopOn) -> Stop {
    let d = &on.details;
    Stop {
        name: d.name.clone().unwrap_or_default(),
        lat: on.lat,
        lon: on.lon,
        parent: d.parent.clone(),
        kind: match d.kind {
            Some(transit_stops::StopKind::Station) => StopKind::Station,
            _ => StopKind::Stop,
        },
        info: StopInfo {
            code: d.code.clone(),
            spoken_name: d.spoken_name.clone(),
            description: d.description.clone(),
            url: d.url.clone(),
            fare_zone: d.fare_zone.clone(),
            wheelchair: d.wheelchair,
            platform: d.platform.clone(),
            level: d.level.clone(),
            timezone: d.timezone.clone(),
            street_access: d.street_access,
        },
        transfer: d.transfer.unwrap_or(false),
        events: Vec::new(),
    }
}

fn parse(s: &Option<String>) -> Option<NaiveDate> {
    s.as_deref().and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok())
}

/// Whether an event applies on some date of `[from, until)`.
fn touches(e: &Event, from: NaiveDate, until: Option<NaiveDate>) -> bool {
    let starts_before_end = match (parse(&e.on), until) {
        (Some(on), Some(until)) => on < until,
        _ => true,
    };
    let ends_after_start = parse(&e.until).is_none_or(|u| u > from);
    starts_before_end && ends_after_start
}

/// The dates inside the window where the stop can change: the window's
/// first, and each event's start and end that fall inside it.
fn boundaries(events: &[Event], from: NaiveDate, until: Option<NaiveDate>) -> Vec<NaiveDate> {
    let inside = |d: NaiveDate| d > from && until.is_none_or(|u| d < u);
    let mut dates = vec![from];
    dates.extend(events.iter().flat_map(|e| [parse(&e.on), parse(&e.until)]).flatten().filter(|d| inside(*d)));
    dates.sort();
    dates.dedup();
    dates
}

/// A pick's stops refreshed from the stops document over its window —
/// `[from, until)`, open-ended without `until` — for the pick's plan.
///
/// Each stop the document holds is the pick's as the document has it:
/// its record as the window opens (or as it first exists inside it),
/// and, unless it is open as recorded every day of the window, the
/// events that apply inside it. It is in the pick when it exists on a
/// date of the window other than retired, or when the pick already has
/// it (a pattern still visiting it). Another plan's proposals stay out;
/// `plan`'s own are in, as proposed. A stop the document does not hold
/// is left as the pick had it.
pub fn refresh(network: &Network, stops: &Stops, from: NaiveDate, until: Option<NaiveDate>, plan: Option<&str>) -> Network {
    let mut next = network.clone();
    for (id, events) in &stops.stops {
        let proposal = events.first().filter(|e| e.event == EventKind::Proposed).and_then(|e| e.plan.as_deref());
        if proposal.is_some_and(|p| Some(p) != plan) {
            continue;
        }
        let states: Vec<Option<StopOn>> = boundaries(events, from, until).into_iter().map(|d| stops.stop_on(id, d)).collect();
        let present = states.iter().flatten().any(|s| s.status != Status::Retired);
        if !present && !network.stops.contains_key(id) {
            continue;
        }
        let first = states.iter().flatten().next().cloned().or_else(|| {
            // it exists on no date of the window: as it is when it
            // first does, after the window
            events.iter().filter_map(|e| parse(&e.on)).find_map(|d| stops.stop_on(id, d))
        });
        let Some(first) = first else { continue };
        let mut stop = record(&first);
        // open, as recorded, and naming no plan (which a network's
        // record cannot hold) on every date of the window
        let constant =
            states.iter().all(|s| s.as_ref().is_some_and(|s| s.status == Status::Open && s.plan.is_none() && record(s) == stop));
        if !constant {
            stop.events = events.iter().filter(|e| touches(e, from, until)).cloned().collect();
            if stop.events.is_empty() {
                // not there on any date of the window: its first event,
                // which applies after it, says so
                stop.events = events.first().cloned().into_iter().collect();
            }
        }
        next.stops.insert(id.clone(), stop);
    }
    next
}

/// A network's stop on a date: its record, open, when it carries no
/// events; else its events applied (null before it exists).
pub fn stop_on(network: &Network, id: &str, date: NaiveDate) -> Option<StopOn> {
    let stop = network.stops.get(id)?;
    if stop.events.is_empty() {
        return Some(StopOn {
            stop_id: id.to_string(),
            status: Status::Open,
            lat: stop.lat,
            lon: stop.lon,
            plan: None,
            details: opening(stop).details,
        });
    }
    let held = Stops { stops: BTreeMap::from([(id.to_string(), stop.events.clone())]), ..Stops::default() };
    held.stop_on(id, date)
}
