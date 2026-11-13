//! A pick and the stops document (`transit-stops`; D-153, D-157). A
//! stop's life is its own; a pick only reads it: [`read_records`] takes
//! each stop's record as the document has it on the pick's first date,
//! and [`visit_warnings`] tells the planner of a stop the pick visits
//! that is not in service on its dates. [`seed`] is how the first
//! document is made, from the stops a network already records.

use std::collections::{BTreeMap, BTreeSet};

use chrono::NaiveDate;
use transit_stops::{Details, Event, EventKind, Status, StopOn, Stops};

use crate::places::{StopInfo, StopKind};
use crate::{Network, NetworkWarning, Stop};

/// A stops document from a network's stops: each stop one `in_service`
/// event with no date (no feed records when a stop went into service),
/// carrying its point and every detail.
pub fn seed(network: &Network) -> Stops {
    let stops = network.stops.iter().map(|(id, stop)| (id.clone(), vec![opening(stop)])).collect();
    Stops { stops, ..Stops::default() }
}

fn opening(stop: &Stop) -> Event {
    let info = &stop.info;
    Event {
        event: EventKind::InService,
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

/// The pick with each stop's record — point, name, code, station, and
/// details — as the stops document has it on `on`, the pick's first
/// date. A stop the document does not hold, or that is not there yet
/// that day, keeps the pick's own record; nothing else changes, and the
/// document is never written.
pub fn read_records(network: &Network, stops: &Stops, on: NaiveDate) -> Network {
    let mut next = network.clone();
    for (id, stop) in next.stops.iter_mut() {
        if let Some(found) = stops.stop_on(id, on) {
            *stop = record(&found);
        }
    }
    next
}

/// A stop's record in the network's terms (amenities are the stops
/// document's alone).
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
    }
}

fn parse(s: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").ok()
}

/// What the planner is told of the stops the pick's patterns visit over
/// its period `[from, until)` (open-ended without `until`), in stop
/// order: each span a stop is not in service — planned, permitted,
/// built, retired, not there yet — and each span it is suspended when
/// the suspension covers the whole period or has no end. A warning,
/// never a refusal; a stop the document does not hold says nothing.
pub fn visit_warnings(network: &Network, stops: &Stops, from: NaiveDate, until: Option<NaiveDate>) -> Vec<NetworkWarning> {
    let visited: BTreeSet<&str> = network.patterns.values().flat_map(|p| p.stops.iter().map(|s| s.stop.as_str())).collect();
    let fmt = |d: NaiveDate| d.format("%Y-%m-%d").to_string();
    let mut out = Vec::new();
    for id in visited {
        let Some(events) = stops.stops.get(id) else { continue };
        // the days the stop can change inside the period
        let mut days: BTreeSet<NaiveDate> = BTreeSet::from([from]);
        for e in events {
            for d in [e.on.as_deref(), e.until.as_deref()].into_iter().flatten().filter_map(parse) {
                if d > from && until.is_none_or(|u| d < u) {
                    days.insert(d);
                }
            }
        }
        let days: Vec<NaiveDate> = days.into_iter().collect();
        let mut open: Option<(Option<Status>, NaiveDate)> = None;
        let mut close = |open: &mut Option<(Option<Status>, NaiveDate)>, end: Option<NaiveDate>| {
            if let Some((status, start)) = open.take() {
                out.push(NetworkWarning::StopNotInService { stop: id.to_string(), status, from: fmt(start), until: end.map(fmt) });
            }
        };
        for (i, &d) in days.iter().enumerate() {
            let on = stops.stop_on(id, d);
            let warns = match &on {
                None => true,
                Some(s) => match s.status {
                    Status::InService => false,
                    Status::Suspended => {
                        let back = s.until.as_deref().and_then(parse);
                        let since = s.since.as_deref().and_then(parse);
                        back.is_none() || (since.is_none_or(|x| x <= from) && until.is_some_and(|u| back.is_some_and(|b| b >= u)))
                    }
                    _ => true,
                },
            };
            let status = on.map(|s| s.status);
            match (&open, warns) {
                (Some((now, _)), true) if *now == status => {}
                (_, true) => {
                    close(&mut open, Some(d));
                    open = Some((status, d));
                }
                (_, false) => close(&mut open, Some(d)),
            }
            if i + 1 == days.len() {
                close(&mut open, until);
            }
        }
    }
    out
}
