//! A pick and the stops document (`transit-stops`; D-153, D-157). A
//! stop's life is its own; a pick only reads it: [`read_records`] takes
//! each stop's record as the document has it on the pick's first date,
//! and [`visit_warnings`] tells the planner of a stop the pick visits
//! that is not in service on its dates. [`seed`] is how the first
//! document is made, from the stops a network already records.

use std::collections::{BTreeMap, BTreeSet};

use chrono::NaiveDate;
use transit_stops::{Details, Event, EventKind, Status, StopOn, Stops};

use crate::model::{Block, CalendarRule, DateRange, Layover, TripRef};
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
            site: BTreeMap::new(),
            refs: BTreeMap::new(),
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

/// A stop's record in the network's terms (amenities, the site survey,
/// and references are the stops document's alone).
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

// ---------------------------------------------------------------------
// GTFS out and in with the stops' history (D-150, D-151, D-157)
// ---------------------------------------------------------------------

fn fmt_date(d: NaiveDate) -> String {
    d.format("%Y-%m-%d").to_string()
}

/// A made id's stop: `<id>@<date>` is a version of `<id>` (D-151).
fn made_base(id: &str) -> Option<(&str, NaiveDate)> {
    let (head, tail) = id.rsplit_once('@')?;
    Some((head, parse(tail)?))
}

/// The network over `[from, until)` with each stop's history in GTFS
/// terms (D-151): a stop whose record changes inside the dates is one
/// stop per version — the version in force on the last date under its
/// id, each earlier one under `<id>@<its first date>` (the first one's
/// is `from`) — and each pattern visiting it splits at those dates: a
/// copy of the pattern and its trips per earlier version's dates, its
/// trips under the service type cut to those dates (`<type>@<first
/// date>..<last date>`; `<type>@<first date>` for the open-ended last),
/// the last dates keeping the pattern's and trips' own ids, every copy
/// beside its trip in each block. Before `from` the first version runs,
/// from `until` on the last. A suspension or retirement changes no
/// record, so splits nothing; with no record changing, this is
/// [`read_records`] on `from`.
pub fn export_versions(network: &Network, stops: &Stops, from: NaiveDate, until: Option<NaiveDate>) -> Network {
    let mut out = read_records(network, stops, from);
    // each stop's versions: its first date and its record
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

    // each pattern visiting a versioned stop splits at its stops' dates
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
        // the periods: [from-or-ever, b1), [b1, b2), …, [bk, ever)
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
                // a cut calendar is named by both its ends: two patterns
                // splitting at different dates never share one
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
    // a service type no schedule runs under any more goes
    let used: BTreeSet<String> = out.pattern_trips.values().map(|pt| pt.service_type.clone()).collect();
    out.service_types.retain(|id, _| used.contains(id) || !network.pattern_trips.values().any(|pt| &pt.service_type == id));
    // each block runs a trip's copies beside it (they never share a day)
    for block in out.blocks.values_mut() {
        *block = with_copies(block, &copies);
    }
    out
}

/// A calendar rule cut to `[lo, hi]` (open where none): its ranges
/// intersected, its exceptions outside dropped.
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
                (start <= end).then_some(DateRange { start, end })
            })
            .collect(),
        exceptions: rule.exceptions.iter().filter(|e| inside(e.date)).cloned().collect(),
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
            trips.push(TripRef { trip: made.clone() });
        }
        at.push(here);
    }
    let mut layovers: Vec<Layover> = Vec::new();
    for l in &block.layovers {
        for &i in at.get(l.after_trip).into_iter().flatten() {
            layovers.push(Layover { after_trip: i, ..l.clone() });
        }
    }
    Block { trips, layovers }
}

/// One event an import added, and the stop it is for.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, ts_rs::TS, schemars::JsonSchema)]
#[ts(export)]
pub struct AddedEvent {
    pub stop: String,
    pub event: Event,
}

/// What an import does to the stops document (D-150).
#[derive(Debug, Clone, PartialEq)]
pub struct ImportedStops {
    /// The document with every event the import added.
    pub stops: Stops,
    /// Each event added, in stop order.
    pub added: Vec<AddedEvent>,
    /// The stops the document holds that the feed does not list: they
    /// keep their events (many feeds list only the stops they serve).
    pub unlisted: Vec<String>,
    /// The stops whose difference could not be recorded without breaking
    /// their history (a retired stop the feed still lists with another
    /// record): nothing is added for them, for a person to look at.
    pub unapplied: Vec<String>,
}

/// A feed's stops recorded in the stops document on `on`, the feed's
/// first service date (D-150): each stop the feed lists compared with
/// the document on that date, each difference an event dated that day —
/// `in_service` for a stop the document lacks, or has not yet in service
/// (planned, permitted, built, not there yet), `moved` for another
/// point, `changed` for the details that differ (a detail the feed does
/// not carry is not a difference). A suspension is real time's, so a
/// suspended stop the feed lists stays suspended. Events already recorded
/// all stand. A made id (`<id>@<date>`, D-151) is a version of its stop:
/// the version in force on `on` is compared. The document is never
/// written here: the caller writes the answer through the receive path.
pub fn import_events(stops: &Stops, feed: &Network, on: NaiveDate) -> ImportedStops {
    let mut next = stops.clone();
    let mut added = Vec::new();
    let mut unapplied = Vec::new();
    // each stop the feed lists, at its version in force on `on`: its own
    // id stands for its last version, so an earlier version in force on
    // `on` wins over it (a made id sorts after its stop's own)
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
    let unlisted = stops.stops.keys().filter(|id| !listed.contains_key(id.as_str())).cloned().collect();
    ImportedStops { stops: next, added, unlisted, unapplied }
}

/// The details `feed` sets that differ from `now` (a detail the feed
/// does not carry is no difference).
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
