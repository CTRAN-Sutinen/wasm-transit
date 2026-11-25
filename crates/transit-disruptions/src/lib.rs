//! What is disrupted on a day, and whether an alert explains it (D-213).
//!
//! These answers live here, not in cap-alerts, because an alerts op that read
//! the live document and the notes would break the split rule
//! (crates/AGENTS.md).

#![forbid(unsafe_code)]

use std::collections::{BTreeMap, BTreeSet};

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use transit_alerts::{AlertEffect, AlertEntity, AlertSource, AlertStatus, ServiceAlerts};
use transit_live::{LiveDay, LiveStatus};
use transit_network::Network;
use transit_notes::{NoteRow, NoteTarget, Notes};
use transit_sync::{EditRecord, EditStatus};

pub struct Inputs<'a> {
    pub network: &'a Network,
    pub edits: &'a [EditRecord],
    pub live: Option<&'a LiveDay>,
    pub stops: &'a transit_stops::Stops,
    pub alerts: &'a ServiceAlerts,
    pub notes: &'a Notes,
    pub date: NaiveDate,
    pub now: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum Origin {
    Edits,
    Feed,
    Stops,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
pub struct NamedStop {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub lat: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub lon: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS, schemars::JsonSchema)]
#[ts(export)]
pub struct Disruption {
    pub source: AlertSource,
    pub from: Origin,
    pub effect: AlertEffect,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub route: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub trip: Option<String>,
    /// Seconds after the service-day anchor. Absent when a cancel takes every
    /// instance of the trip.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub start_secs: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub first_stop: Option<NamedStop>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub headsign: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub next_start_secs: Option<u32>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[ts(optional, as = "Option<Vec<NamedStop>>")]
    pub stops: Vec<NamedStop>,
    /// "YYYY-MM-DD". A suspension's end is the first date the stop is back.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub start_date: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub end_date: Option<String>,
    pub explained_by: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub latest_note: Option<NoteRow>,
    /// Its stops, and each stop of a cancelled trip. A stop filter matches
    /// these.
    #[serde(skip)]
    #[ts(skip)]
    #[schemars(skip)]
    touches: BTreeSet<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
pub struct AlertGap {
    pub source: AlertSource,
    pub effect: AlertEffect,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub route: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub trip: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub start_secs: Option<u32>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[ts(optional, as = "Option<Vec<String>>")]
    pub stops: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub start_date: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub end_date: Option<String>,
    pub informed: Vec<AlertEntity>,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS, schemars::JsonSchema)]
#[ts(export)]
pub struct RouteDay {
    pub route: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub long_name: Option<String>,
    pub disruptions: Vec<Disruption>,
    pub late_trips: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub worst_delay_secs: Option<i32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS, schemars::JsonSchema)]
#[ts(export)]
pub struct Today {
    pub routes: Vec<RouteDay>,
    pub unaffected_routes: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
pub struct Filter {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub route: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub stop: Option<String>,
}

fn named(network: &Network, id: &str) -> NamedStop {
    let stop = network.stops.get(id);
    NamedStop { id: id.to_string(), name: stop.map(|s| s.name.clone()), lat: stop.map(|s| s.lat), lon: stop.map(|s| s.lon) }
}

fn ymd(d: NaiveDate) -> String {
    d.format("%Y-%m-%d").to_string()
}

fn note_target(source: &AlertSource) -> NoteTarget {
    match source {
        AlertSource::Edit { id } => NoteTarget::Edit { id: id.clone() },
        AlertSource::Stop { stop } => NoteTarget::Stop { stop: stop.clone() },
        AlertSource::Trip { trip, date, start_secs } => NoteTarget::Trip { trip: trip.clone(), date: date.clone(), start_secs: *start_secs },
    }
}

struct TripFacts {
    route: String,
    direction: Option<String>,
    stops: Vec<String>,
    headsign: Option<String>,
}

fn trip_facts(network: &Network, trip: &str) -> Option<TripFacts> {
    let (_, pt, t) = network.trip(trip)?;
    let pattern = network.patterns.get(&pt.pattern)?;
    Some(TripFacts {
        route: pattern.route.to_string(),
        direction: pattern.direction.as_ref().map(|d| d.to_string()),
        stops: pattern.stops.iter().map(|s| s.stop.to_string()).collect(),
        headsign: t.headsign.clone(),
    })
}

pub fn disruptions(i: &Inputs) -> Vec<Disruption> {
    let date = ymd(i.date);
    let explained: BTreeMap<&AlertSource, Vec<String>> = {
        let mut m: BTreeMap<&AlertSource, Vec<String>> = BTreeMap::new();
        for (id, a) in &i.alerts.alerts {
            if a.status == AlertStatus::Retired || a.ended_by(i.now) {
                continue;
            }
            for s in &a.explains {
                m.entry(s).or_default().push(id.clone());
            }
        }
        m
    };
    let feed_cancels: Vec<&transit_live::LiveTrip> = {
        let mut v: Vec<_> = i
            .live
            .map(|l| l.trips.iter().filter(|t| t.canceled && t.date.as_deref().is_none_or(|d| d == date)).collect())
            .unwrap_or_default();
        v.sort_by(|a, b| (a.start_secs, &a.trip).cmp(&(b.start_secs, &b.trip)));
        v
    };
    let edit_cancels: Vec<&transit_sync::TripCancel> = i
        .edits
        .iter()
        .filter(|r| matches!(r.status, EditStatus::Submitted | EditStatus::Accepted))
        .filter_map(|r| r.cancel.as_ref().filter(|c| c.covers(i.date)))
        .collect();
    let cancelled = |trip: &str, start: u32| {
        edit_cancels.iter().any(|c| c.cancels(trip, start)) || feed_cancels.iter().any(|t| t.trip == trip && t.start_secs == start)
    };
    // The day's trips, read only when a cancel asks for its next trip.
    let mut day: Option<Vec<transit_network::MaterializedTrip>> = None;
    let mut next_after = |facts: &TripFacts, start: u32| -> Option<u32> {
        let trips = day.get_or_insert_with(|| transit_network::materialize(i.network, i.date).unwrap_or_default());
        trips
            .iter()
            .filter(|t| t.route == facts.route && t.start_secs > start)
            .filter(|t| t.stop_times.first().map(|s| s.stop.as_str()) == facts.stops.first().map(String::as_str))
            .filter(|t| i.network.patterns.get(t.pattern.as_str()).map(|p| p.direction.as_ref().map(|d| d.to_string())) == Some(facts.direction.clone()))
            .filter(|t| !cancelled(&t.trip, t.start_secs))
            .map(|t| t.start_secs)
            .min()
    };
    let finish = |mut d: Disruption| {
        d.explained_by = explained.get(&d.source).cloned().unwrap_or_default();
        d.latest_note = i.notes.on(&note_target(&d.source)).into_iter().max_by(|a, b| (a.note.written.at, &a.id).cmp(&(b.note.written.at, &b.id)));
        d
    };
    let blank = |source: AlertSource, from: Origin, effect: AlertEffect| Disruption {
        source,
        from,
        effect,
        route: None,
        trip: None,
        start_secs: None,
        first_stop: None,
        headsign: None,
        next_start_secs: None,
        stops: Vec::new(),
        start_date: None,
        end_date: None,
        explained_by: Vec::new(),
        latest_note: None,
        touches: BTreeSet::new(),
    };

    let mut out = Vec::new();
    for r in i.edits.iter().filter(|r| matches!(r.status, EditStatus::Submitted | EditStatus::Accepted)) {
        let source = AlertSource::Edit { id: r.id.clone() };
        if let Some(dv) = r.diversion.as_ref().filter(|d| d.covers(i.date)) {
            let mut d = blank(source, Origin::Edits, AlertEffect::Detour);
            d.route = Some(dv.route_id.clone());
            d.stops = dv.skipped_stops.iter().map(|s| named(i.network, s)).collect();
            d.touches = dv.skipped_stops.iter().cloned().collect();
            d.start_date = Some(ymd(dv.start_date));
            d.end_date = Some(ymd(dv.end_date));
            out.push(finish(d));
        } else if let Some(c) = r.cancel.as_ref().filter(|c| c.covers(i.date)) {
            let mut d = blank(source, Origin::Edits, AlertEffect::NoService);
            let facts = trip_facts(i.network, &c.trip_id);
            d.trip = Some(c.trip_id.clone());
            d.start_secs = c.start_secs;
            d.start_date = Some(ymd(c.date));
            d.end_date = Some(ymd(c.date));
            if let Some(f) = facts {
                d.route = Some(f.route.clone());
                d.first_stop = f.stops.first().map(|s| named(i.network, s));
                d.headsign = f.headsign.clone();
                d.next_start_secs = c.start_secs.and_then(|s| next_after(&f, s));
                d.touches = f.stops.iter().cloned().collect();
            }
            out.push(finish(d));
        }
    }
    for t in &feed_cancels {
        let source = AlertSource::Trip { trip: t.trip.clone(), date: date.clone(), start_secs: t.start_secs };
        let mut d = blank(source, Origin::Feed, AlertEffect::NoService);
        let facts = trip_facts(i.network, &t.trip);
        d.trip = Some(t.trip.clone());
        d.start_secs = Some(t.start_secs);
        d.start_date = Some(date.clone());
        d.end_date = Some(date.clone());
        d.route = t.route.clone().or_else(|| facts.as_ref().map(|f| f.route.clone()));
        if let Some(f) = facts {
            d.first_stop = f.stops.first().map(|s| named(i.network, s));
            d.headsign = f.headsign.clone();
            d.next_start_secs = next_after(&f, t.start_secs);
            d.touches = f.stops.iter().cloned().collect();
        }
        out.push(finish(d));
    }
    let mut suspended: Vec<transit_stops::StopOn> =
        i.stops.on_date(i.date).into_iter().filter(|s| s.status == transit_stops::Status::Suspended).collect();
    suspended.sort_by(|a, b| a.stop_id.cmp(&b.stop_id));
    for s in suspended {
        let mut d = blank(AlertSource::Stop { stop: s.stop_id.clone() }, Origin::Stops, AlertEffect::NoService);
        d.stops = vec![NamedStop { lat: Some(s.lat), lon: Some(s.lon), ..named(i.network, &s.stop_id) }];
        d.touches = [s.stop_id.clone()].into_iter().collect();
        d.start_date = s.since;
        d.end_date = s.until;
        out.push(finish(d));
    }
    out
}

#[derive(Debug, Clone, PartialEq, Serialize, TS, schemars::JsonSchema)]
#[ts(export)]
pub struct AlertStart {
    pub alert: transit_alerts::ServiceAlert,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub preview_stop: Option<String>,
}

pub fn start_alert(i: &Inputs, source: Option<&AlertSource>) -> AlertStart {
    let gap = source.and_then(|s| gaps(i).into_iter().find(|g| &g.source == s));
    let alert = transit_alerts::ServiceAlert {
        effect: gap.as_ref().map_or(AlertEffect::Unknown, |g| g.effect),
        header: vec![transit_alerts::AlertText::default()],
        informed: gap.as_ref().map(|g| g.informed.clone()).unwrap_or_default(),
        explains: source.cloned().into_iter().collect(),
        communication: vec![transit_alerts::AlertPeriod { start: Some(i.now), end: None }],
        ..Default::default()
    };
    let preview_stop = gap.and_then(|g| {
        g.stops.first().cloned().or_else(|| disruptions(i).into_iter().find(|d| d.source == g.source).and_then(|d| d.first_stop).map(|s| s.id))
    });
    AlertStart { alert, preview_stop }
}

pub fn gaps(i: &Inputs) -> Vec<AlertGap> {
    disruptions(i)
        .into_iter()
        .filter(|d| d.explained_by.is_empty())
        .map(|d| AlertGap {
            informed: informed(&d),
            source: d.source,
            effect: d.effect,
            route: if d.effect == AlertEffect::Detour { d.route } else { None },
            trip: d.trip,
            start_secs: d.start_secs,
            stops: d.stops.into_iter().map(|s| s.id).collect(),
            start_date: d.start_date,
            end_date: d.end_date,
        })
        .collect()
}

fn informed(d: &Disruption) -> Vec<AlertEntity> {
    if d.from == Origin::Stops {
        return d.stops.iter().map(|s| AlertEntity { stop: Some(s.id.clone()), ..Default::default() }).collect();
    }
    if let Some(trip) = &d.trip {
        // The instance's date: the feed's names it; an edit's cancel is
        // the day's.
        let date = match &d.source {
            AlertSource::Trip { date, .. } => Some(date.clone()),
            _ => d.start_secs.and(d.start_date.clone()),
        };
        return vec![AlertEntity { route: d.route.clone(), trip: Some(trip.clone()), date, start_secs: d.start_secs, ..Default::default() }];
    }
    let route = AlertEntity { route: d.route.clone(), ..Default::default() };
    std::iter::once(route)
        .chain(d.stops.iter().map(|s| AlertEntity { route: d.route.clone(), stop: Some(s.id.clone()), ..Default::default() }))
        .collect()
}

fn name_key(name: &str) -> (u8, u64, String) {
    match name.trim().parse::<u64>() {
        Ok(n) => (0, n, String::new()),
        Err(_) => (1, 0, name.to_string()),
    }
}

pub fn today(i: &Inputs, filter: &Filter) -> Today {
    let date = ymd(i.date);
    let mut by_route: BTreeMap<String, Vec<Disruption>> = BTreeMap::new();
    let place: Option<BTreeSet<&str>> = filter.stop.as_deref().map(|asked| {
        std::iter::once(asked)
            .chain(i.network.stops.iter().filter(|(_, s)| s.parent.as_ref().is_some_and(|p| p.as_str() == asked)).map(|(id, _)| id.as_str()))
            .collect()
    });
    for d in disruptions(i) {
        if place.as_ref().is_some_and(|p| !d.touches.iter().any(|t| p.contains(t.as_str()))) {
            continue;
        }
        let routes: Vec<String> = match (&d.route, d.from) {
            (Some(r), _) => vec![r.clone()],
            // A suspended stop belongs to every route that serves it.
            (None, Origin::Stops) => {
                let stop = &d.stops[0].id;
                let served: BTreeSet<String> =
                    i.network.patterns.values().filter(|p| p.stops.iter().any(|s| s.stop.as_str() == stop)).map(|p| p.route.to_string()).collect();
                served.into_iter().collect()
            }
            (None, _) => Vec::new(),
        };
        for r in routes {
            by_route.entry(r).or_default().push(d.clone());
        }
    }
    let mut late: BTreeMap<String, (u32, Option<i32>)> = BTreeMap::new();
    if filter.stop.is_none() {
        for t in i.live.map(|l| l.trips.as_slice()).unwrap_or_default() {
            let on_day = t.date.as_deref().is_none_or(|d| d == date);
            if !on_day || t.canceled || !matches!(t.status, LiveStatus::Late | LiveStatus::VeryLate) {
                continue;
            }
            let Some(route) = t.route.clone().or_else(|| trip_facts(i.network, &t.trip).map(|f| f.route)) else { continue };
            let e = late.entry(route).or_insert((0, None));
            e.0 += 1;
            e.1 = e.1.max(t.worst_delay_secs);
        }
    }
    let listed: BTreeSet<String> = by_route.keys().chain(late.keys()).filter(|r| filter.route.as_ref().is_none_or(|f| f == *r)).cloned().collect();
    let mut routes: Vec<RouteDay> = listed
        .into_iter()
        .map(|id| {
            let route = i.network.routes.get(id.as_str());
            let (late_trips, worst) = late.get(&id).copied().unwrap_or((0, None));
            RouteDay {
                name: route.map(|r| r.name.clone()).unwrap_or_else(|| id.clone()),
                long_name: route.and_then(|r| r.long_name.clone()),
                disruptions: by_route.remove(&id).unwrap_or_default(),
                late_trips,
                worst_delay_secs: worst,
                route: id,
            }
        })
        .collect();
    routes.sort_by(|a, b| {
        let order = |r: &RouteDay| i.network.routes.get(r.route.as_str()).and_then(|x| x.sort_order).unwrap_or(u32::MAX);
        (order(a), name_key(&a.name), &a.route).cmp(&(order(b), name_key(&b.name), &b.route))
    });
    let unaffected_routes = (i.network.routes.len() as u32).saturating_sub(routes.len() as u32);
    Today { routes, unaffected_routes }
}
