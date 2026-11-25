//! How a pick reads the stops document. A pick never writes
//! it.

use std::collections::{BTreeMap, BTreeSet};

use chrono::NaiveDate;
use crate::{Details, Event, EventKind, Status, StopOn, Stops};

use transit_network::model::{Block, CalendarRule, DateRange, Layover, TripRef};
use transit_network::places::{StopInfo, StopKind};
use serde::Serialize;
use transit_network::{Network, NetworkEdit, NetworkWarning, Stop};
use transit_plans::{PlanInfo, PublishedPlans};
use ts_rs::TS;

/// The `in_service` event has no date, because no feed records when a stop
/// went into service.
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
            kind: (stop.kind == StopKind::Station).then_some(crate::StopKind::Station),
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
            site: BTreeMap::new(),
            refs: BTreeMap::new(),
        },
        ..Event::default()
    }
}

pub fn read_records(network: &Network, stops: &Stops, on: NaiveDate) -> Network {
    let mut next = network.clone();
    for (id, stop) in next.stops.iter_mut() {
        if let Some(found) = stops.stop_on(id, on) {
            *stop = record(&found);
        }
    }
    next
}

/// A plan takes a stop from these records.
pub fn records(stops: &Stops, ids: &[String], on: NaiveDate) -> BTreeMap<String, Stop> {
    ids.iter().filter_map(|id| stops.stop_on(id, on).map(|found| (id.clone(), record(&found)))).collect()
}

pub fn take_stops(network: &Network, stops: &Stops, ids: &[String], on: NaiveDate) -> Result<Vec<NetworkEdit>, Vec<String>> {
    let mut seen = std::collections::BTreeSet::new();
    let wanted: Vec<&String> = ids.iter().filter(|id| !network.stops.contains_key(*id) && seen.insert(*id)).collect();
    let mut edits = Vec::new();
    let mut missing = Vec::new();
    for id in wanted {
        match stops.stop_on(id, on) {
            Some(found) => edits.push(NetworkEdit::UpsertStop { id: id.clone(), stop: record(&found) }),
            None => missing.push(id.clone()),
        }
    }
    if missing.is_empty() { Ok(edits) } else { Err(missing) }
}

/// Amenities, the site survey, and references stay in the stops document.
fn record(on: &StopOn) -> Stop {
    let d = &on.details;
    Stop {
        name: d.name.clone().unwrap_or_default(),
        lat: on.lat,
        lon: on.lon,
        parent: d.parent.clone(),
        kind: match d.kind {
            Some(crate::StopKind::Station) => StopKind::Station,
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
            extra: Default::default(),
        },
        transfer: d.transfer.unwrap_or(false),
        extra: Default::default(),
    }
}

fn parse(s: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").ok()
}

pub fn visit_warnings(network: &Network, stops: &Stops, from: NaiveDate, until: Option<NaiveDate>) -> Vec<NetworkWarning> {
    let visited: BTreeSet<&str> = network.patterns.values().flat_map(|p| p.stops.iter().map(|s| s.stop.as_str())).collect();
    let fmt = |d: NaiveDate| d.format("%Y-%m-%d").to_string();
    let mut out = Vec::new();
    for id in visited {
        let Some(events) = stops.stops.get(id) else { continue };
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
                out.push(NetworkWarning::StopNotInService { stop: id.to_string(), status: status.map(|s| s.as_str().to_string()), from: fmt(start), until: end.map(fmt) });
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

fn fmt_date(d: NaiveDate) -> String {
    d.format("%Y-%m-%d").to_string()
}

/// An id `<id>@<date>` is a version of `<id>`.
fn made_base(id: &str) -> Option<(&str, NaiveDate)> {
    let (head, tail) = id.rsplit_once('@')?;
    Some((head, parse(tail)?))
}

pub fn export_versions(network: &Network, stops: &Stops, from: NaiveDate, until: Option<NaiveDate>) -> Network {
    let mut out = read_records(network, stops, from);
    let mut versions: BTreeMap<String, Vec<(NaiveDate, Stop)>> = BTreeMap::new();
    for (id, own) in &network.stops {
        let Some(events) = stops.stops.get(id) else { continue };
        let mut days: BTreeSet<NaiveDate> = BTreeSet::from([from]);
        for e in events {
            for d in [e.on.as_deref(), e.until.as_deref()].into_iter().flatten().filter_map(parse) {
                if d > from && until.is_none_or(|u| d < u) {
                    days.insert(d);
                }
            }
        }
        let mut v: Vec<(NaiveDate, Stop)> = Vec::new();
        for d in days {
            let rec = stops.stop_on(id, d).map(|s| record(&s)).unwrap_or_else(|| own.clone());
            if v.last().is_none_or(|(_, last)| *last != rec) {
                v.push((d, rec));
            }
        }
        if v.len() > 1 {
            versions.insert(id.clone(), v);
        }
    }
    if versions.is_empty() {
        return out;
    }
    let version_id = |id: &str, on: NaiveDate| -> String {
        match versions.get(id) {
            None => id.to_string(),
            Some(v) => {
                let i = v.iter().rposition(|(start, _)| *start <= on).unwrap_or(0);
                if i + 1 == v.len() { id.to_string() } else { format!("{id}@{}", fmt_date(v[i].0)) }
            }
        }
    };
    for (id, v) in &versions {
        for (i, (start, rec)) in v.iter().enumerate() {
            let key = if i + 1 == v.len() { id.clone() } else { format!("{id}@{}", fmt_date(*start)) };
            out.stops.insert(key, rec.clone());
        }
    }

    let mut copies: BTreeMap<String, Vec<String>> = BTreeMap::new(); // trip → its earlier copies
    for (pid, pattern) in &network.patterns {
        let starts: BTreeSet<NaiveDate> = pattern
            .stops
            .iter()
            .filter_map(|ps| versions.get(&ps.stop))
            .flat_map(|v| v.iter().skip(1).map(|(d, _)| *d))
            .collect();
        if starts.is_empty() {
            continue;
        }
        let mut bounds: Vec<NaiveDate> = vec![from];
        bounds.extend(starts.iter().copied());
        let last = bounds.len() - 1;
        for (j, &start) in bounds.iter().enumerate() {
            let label = fmt_date(start);
            let lo = (j > 0).then_some(start);
            let hi = bounds.get(j + 1).and_then(|next| next.pred_opt());
            let mut copy = pattern.clone();
            for ps in &mut copy.stops {
                ps.stop = version_id(&ps.stop, start);
            }
            let new_pid = if j == last { pid.clone() } else { format!("{pid}@{label}") };
            out.patterns.insert(new_pid.clone(), copy);
            for (ptid, pt) in network.pattern_trips.iter().filter(|(_, pt)| &pt.pattern == pid) {
                // A cut calendar is named by both its ends. Two patterns that
                // split at different dates never share one.
                let st = match hi {
                    Some(hi) => format!("{}@{label}..{}", pt.service_type, fmt_date(hi)),
                    None => format!("{}@{label}", pt.service_type),
                };
                if let Some(base) = network.service_types.get(&pt.service_type) {
                    let mut cut = base.clone();
                    cut.rule = clip(&base.rule, lo, hi);
                    out.service_types.entry(st.clone()).or_insert(cut);
                }
                let mut next = pt.clone();
                next.pattern = new_pid.clone();
                next.service_type = st;
                if j != last {
                    for t in &mut next.trips {
                        let made = format!("{}@{label}", t.id);
                        copies.entry(t.id.clone()).or_default().push(made.clone());
                        t.id = made;
                    }
                }
                let new_ptid = if j == last { ptid.clone() } else { format!("{ptid}@{label}") };
                out.pattern_trips.insert(new_ptid, next);
            }
        }
    }
    let used: BTreeSet<String> = out.pattern_trips.values().map(|pt| pt.service_type.clone()).collect();
    out.service_types.retain(|id, _| used.contains(id) || !network.pattern_trips.values().any(|pt| &pt.service_type == id));
    // Each block runs a trip's copies beside it. The copies never share a day.
    for block in out.blocks.values_mut() {
        *block = with_copies(block, &copies);
    }
    out
}

fn clip(rule: &CalendarRule, lo: Option<NaiveDate>, hi: Option<NaiveDate>) -> CalendarRule {
    let inside = |d: NaiveDate| lo.is_none_or(|l| d >= l) && hi.is_none_or(|h| d <= h);
    CalendarRule {
        days: rule.days.clone(),
        ranges: rule
            .ranges
            .iter()
            .filter_map(|r| {
                let start = lo.map_or(r.start, |l| r.start.max(l));
                let end = hi.map_or(r.end, |h| r.end.min(h));
                (start <= end).then_some(DateRange { start, end, extra: Default::default() })
            })
            .collect(),
        exceptions: rule.exceptions.iter().filter(|e| inside(e.date)).cloned().collect(),
        extra: Default::default(),
    }
}

fn with_copies(block: &Block, copies: &BTreeMap<String, Vec<String>>) -> Block {
    let mut trips = Vec::new();
    let mut at: Vec<Vec<usize>> = Vec::new(); // old index → new indices
    for t in &block.trips {
        let mut here = vec![trips.len()];
        trips.push(t.clone());
        for made in copies.get(&t.trip).into_iter().flatten() {
            here.push(trips.len());
            trips.push(TripRef { trip: made.clone(), extra: Default::default() });
        }
        at.push(here);
    }
    let mut layovers: Vec<Layover> = Vec::new();
    for l in &block.layovers {
        for &i in at.get(l.after_trip).into_iter().flatten() {
            layovers.push(Layover { after_trip: i, ..l.clone() });
        }
    }
    Block { trips, layovers, extra: Default::default() }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, ts_rs::TS, schemars::JsonSchema)]
#[ts(export)]
pub struct AddedEvent {
    pub stop: String,
    pub event: Event,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ImportedStops {
    pub stops: Stops,
    pub added: Vec<AddedEvent>,
    /// A stop planned, permitted, built, retired, or not there yet is
    /// expected missing from a feed, so it is not named.
    pub unlisted: Vec<String>,
    /// A person looks at these. Nothing is added for them.
    pub unapplied: Vec<String>,
}

/// `on` is the feed's first service date, the only date a feed states.
/// This function never writes the document.
pub fn import_events(stops: &Stops, feed: &Network, on: NaiveDate) -> ImportedStops {
    import(stops, feed, on, None)
}

pub fn import_feed(stops: &Stops, feed: &Network, on: NaiveDate, feed_id: &str) -> ImportedStops {
    import(stops, feed, on, Some(feed_id))
}

pub fn feed_events(stops: &Stops, feed_id: &str) -> Vec<AddedEvent> {
    stops
        .stops
        .iter()
        .flat_map(|(id, events)| {
            events.iter().filter(|e| e.from_feed.as_deref() == Some(feed_id)).map(move |e| AddedEvent { stop: id.clone(), event: e.clone() })
        })
        .collect()
}

fn import(stops: &Stops, feed: &Network, on: NaiveDate, feed_id: Option<&str>) -> ImportedStops {
    let mut next = stops.clone();
    let mut added = Vec::new();
    let mut unapplied = Vec::new();
    // A stop's own id stands for its last version, so an earlier version in
    // force on `on` wins over it.
    let mut listed: BTreeMap<&str, (NaiveDate, &Stop)> = BTreeMap::new();
    for (id, stop) in &feed.stops {
        match made_base(id).filter(|(base, _)| feed.stops.contains_key(*base)) {
            Some((base, start)) => {
                if start <= on && listed.get(base).is_none_or(|(d, _)| *d < start) {
                    listed.insert(base, (start, stop));
                }
            }
            None => {
                listed.entry(id.as_str()).or_insert((NaiveDate::MIN, stop));
            }
        }
    }
    for (id, (_, stop)) in &listed {
        let mut events: Vec<Event> = Vec::new();
        let now = stops.stop_on(id, on);
        let feed_open = opening(stop);
        // The document refuses any event after `retired`, so a feed that
        // lists a retired stop's id reuses it, even with the same point.
        if now.as_ref().is_some_and(|s| s.status == Status::Retired) {
            unapplied.push(id.to_string());
            continue;
        }
        match now.as_ref().map(|s| s.status) {
            None => events.push(Event { on: Some(fmt_date(on)), ..feed_open.clone() }),
            Some(Status::Planned | Status::Permitted | Status::Built) => {
                events.push(Event { on: Some(fmt_date(on)), event: EventKind::InService, ..Event::default() })
            }
            _ => {}
        }
        if let Some(s) = &now {
            if (s.lat, s.lon) != (stop.lat, stop.lon) {
                events.push(Event { on: Some(fmt_date(on)), event: EventKind::Moved, lat: Some(stop.lat), lon: Some(stop.lon), ..Event::default() });
            }
            let changed = differing(&s.details, &feed_open.details);
            if !changed.is_empty() {
                events.push(Event { on: Some(fmt_date(on)), event: EventKind::Changed, details: changed, ..Event::default() });
            }
        }
        if events.is_empty() {
            continue;
        }
        for e in &mut events {
            e.from_feed = feed_id.map(str::to_string);
        }
        let mut trial = next.clone();
        for e in &events {
            trial = trial.add_event(id, e.clone());
        }
        if trial.clone().checked().is_ok() {
            next = trial;
            added.extend(events.into_iter().map(|event| AddedEvent { stop: id.to_string(), event }));
        } else {
            unapplied.push(id.to_string());
        }
    }
    let unlisted = stops
        .stops
        .keys()
        .filter(|id| !listed.contains_key(id.as_str()))
        .filter(|id| matches!(stops.stop_on(id, on).map(|s| s.status), Some(Status::InService | Status::Suspended)))
        .cloned()
        .collect();
    ImportedStops { stops: next, added, unlisted, unapplied }
}

fn differing(now: &Details, feed: &Details) -> Details {
    fn pick<T: Clone + PartialEq>(now: &Option<T>, feed: &Option<T>) -> Option<T> {
        feed.as_ref().filter(|f| now.as_ref() != Some(*f)).cloned()
    }
    Details {
        name: pick(&now.name, &feed.name),
        kind: pick(&now.kind, &feed.kind),
        parent: pick(&now.parent, &feed.parent),
        code: pick(&now.code, &feed.code),
        spoken_name: pick(&now.spoken_name, &feed.spoken_name),
        description: pick(&now.description, &feed.description),
        url: pick(&now.url, &feed.url),
        fare_zone: pick(&now.fare_zone, &feed.fare_zone),
        wheelchair: pick(&now.wheelchair, &feed.wheelchair),
        transfer: pick(&now.transfer, &feed.transfer),
        platform: pick(&now.platform, &feed.platform),
        level: pick(&now.level, &feed.level),
        timezone: pick(&now.timezone, &feed.timezone),
        street_access: pick(&now.street_access, &feed.street_access),
        amenities: BTreeMap::new(),
        site: BTreeMap::new(),
        refs: BTreeMap::new(),
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS, schemars::JsonSchema)]
#[ts(export, export_to = "cap_stops/")]
pub struct PickPeriod {
    pub from: String,
    /// The date of the next pick that is not retired.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub until: Option<String>,
}

/// See [`transit_plans::period_in`]. A standalone import is no pick, so it
/// has no period.
pub fn pick_period(network: &Network, published: Option<&PublishedPlans>) -> Option<PickPeriod> {
    let plan = &network.plan;
    let from = plan.effective_from?;
    if plan.id.is_empty() {
        return None;
    }
    let fmt = |d: NaiveDate| d.format("%Y-%m-%d").to_string();
    let info = PlanInfo { id: plan.id.clone(), name: plan.name.clone(), effective_from: Some(fmt(from)) };
    Some(PickPeriod { from: fmt(from), until: transit_plans::period_in(published, &info) })
}

#[derive(Debug, Clone, PartialEq)]
pub struct PickReads {
    pub period: PickPeriod,
    pub network: Network,
    pub warnings: Vec<NetworkWarning>,
}

pub fn read_pick(network: &Network, stops: &Stops, published: Option<&PublishedPlans>) -> Option<PickReads> {
    let period = pick_period(network, published)?;
    let (from, until) = period_dates(&period)?;
    Some(PickReads {
        network: read_records(network, stops, from),
        warnings: visit_warnings(network, stops, from, until),
        period,
    })
}

pub fn export_pick(network: &Network, stops: &Stops, published: Option<&PublishedPlans>) -> Option<Network> {
    let period = pick_period(network, published)?;
    let (from, until) = period_dates(&period)?;
    Some(export_versions(network, stops, from, until))
}

fn period_dates(period: &PickPeriod) -> Option<(NaiveDate, Option<NaiveDate>)> {
    let parse = |s: &str| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok();
    Some((parse(&period.from)?, period.until.as_deref().and_then(parse)))
}
