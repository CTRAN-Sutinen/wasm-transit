//! transit-codec — the network codec, as a shared library.
//!
//! GTFS ⇄ network: [`import`] reads a whole-spec [`Feed`] into an
//! authored network document (the planner's bootstrap), and
//! [`export`] writes a document back as a feed (the publish path).
//! The cap-network module dispatches through it — the same codec,
//! gated once. GTFS-the-format stays out of transit-network by
//! design; this crate is the only place the two meet.
//!
//! The trip is the unit both ways: a feed's trips ARE the network's
//! trips — the agency's ids, every scheduled time, frequency trips as
//! frequency trips, block membership by trip. Patterns are stop
//! sequences (route, direction, stops, exactness flags, shape); how
//! long a trip takes is the trip's own business.
//!
//! What a document cannot hold and the feed's own bytes need lives
//! in the [`Boundary`] — the import's serialization residue (the
//! feed's stop_sequence numbering, a frequency trip's stop_times
//! base). An authored document exports with `Boundary::default()`.
//!
//! Conventions of the mapping:
//!
//! * a GTFS service_id is a service type id, its calendar row the
//!   rule's weekday flags and range, its calendar_dates rows the
//!   rule's exceptions;
//! * a trip's times are offsets from its first arrival: a one-start
//!   trip starts there, a frequency trip's instances start on its
//!   bands;
//! * a stop row is exact (the pattern's timepoint) when it carries
//!   both times and does not say `timepoint=0`;
//! * a pattern's authored `path` is a `shapes.txt` polyline keyed by
//!   the pattern id;
//! * transfer-point flags come from transfers.txt (a stop named by a
//!   possible transfer, where at least two routes meet) and do not
//!   cross back (GTFS declares transfers pairwise).

pub mod feed;
pub use feed::{Feed, FeedError};

use std::collections::{BTreeMap, BTreeSet};

use chrono::{Duration, NaiveDate};

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use feed::{
    AgencyRow, CalendarDateRow, CalendarRow, FeedInfoRow, FrequencyRow, RouteRow, ShapeRow, StopRow, StopTimeRow,
    TripRow,
};
use transit_network::{
    Block, CalendarException, CalendarRule, Coord, DateRange, DayState, Departs, Direction, HeadwayBand, Layover, Mode,
    Network, Pattern, PatternStop, PatternTrips, ServiceType, StopOffset, TimingProfile, Trip, TripRef, WeekdaySet,
};

// ---------------------------------------------------------------------
// The boundary residue and the import's report.
// ---------------------------------------------------------------------

/// What the feed's bytes carry that the document does not: pure
/// serialization, never planner intent. Emitted with the imported
/// document; `Boundary::default()` for an authored one (stop_sequence
/// 1..=n, a frequency trip's rows based at its first band).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Boundary {
    /// Per trip, the feed's stop_sequence numbers — kept only where
    /// they are not 1..=n.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    #[ts(optional, as = "Option<BTreeMap<String, Vec<u32>>>")]
    pub stop_sequences: BTreeMap<String, Vec<u32>>,
    /// Per frequency trip, the first arrival its stop_times rows
    /// carried (the spec reads those rows as offsets only).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    #[ts(optional, as = "Option<BTreeMap<String, u32>>")]
    pub frequency_bases: BTreeMap<String, u32>,
}

/// One thing the import could not carry into the document. Reported,
/// never silent — the planner needs to see what the bootstrap did to
/// their feed.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ImportNote {
    /// Trips the model cannot represent at all, aggregated by
    /// reason (single-stop shuttles, no first time, serving stops
    /// without coordinates).
    DroppedTrips {
        single_stop: u32,
        no_departure: u32,
        missing_stop_coords: u32,
    },
    /// A transfers.txt row marks a stop whose walking group is
    /// served by fewer than two routes — the feed's intent cannot
    /// compose; the flag is reported, not applied.
    UnusableTransferPoint { stop: String },
}

/// What [`import`] hands back: the document, its boundary residue,
/// and the notes describing what could not cross.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct ImportOutcome {
    pub network: Network,
    pub boundary: Boundary,
    pub notes: Vec<ImportNote>,
}

/// One row of the trip-id map: a model trip as rt feeds address it —
/// its pattern trips and first start — and the agency trip ids that
/// name it (its own id: the import keeps the agency's).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct TripIdEntry {
    pub pattern_trips: String,
    pub start_secs: u32,
    pub agency_trip_ids: Vec<String>,
}

/// The join rt data is keyed by: every trip of a network, by id. A
/// frequency trip answers its first instance (rt start_time is not
/// read yet).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct TripIdMap {
    pub entries: Vec<TripIdEntry>,
}

/// The network's trip-id map, in trip order.
pub fn trip_map(net: &Network) -> TripIdMap {
    TripIdMap {
        entries: net
            .trips()
            .map(|(key, _, t)| TripIdEntry {
                pattern_trips: key.to_string(),
                start_secs: t.departs.first_start().unwrap_or(0),
                agency_trip_ids: vec![t.id.clone()],
            })
            .collect(),
    }
}

// ---------------------------------------------------------------------
// Text ⇄ values.
// ---------------------------------------------------------------------

/// GTFS time ("H:MM:SS", hours past 24 legal) → seconds.
fn parse_time(text: &str) -> Option<u32> {
    let mut parts = text.trim().split(':');
    let (h, m, s) = (parts.next()?, parts.next()?, parts.next()?);
    if parts.next().is_some() {
        return None;
    }
    Some(h.parse::<u32>().ok()? * 3600 + m.parse::<u32>().ok()? * 60 + s.parse::<u32>().ok()?)
}

/// Seconds → GTFS time, "HH:MM:SS".
fn format_time(secs: u32) -> String {
    format!("{:02}:{:02}:{:02}", secs / 3600, secs / 60 % 60, secs % 60)
}

fn parse_date(text: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(text.trim(), "%Y%m%d").ok()
}

fn format_date(date: NaiveDate) -> String {
    date.format("%Y%m%d").to_string()
}

fn parse_f64(text: &Option<String>) -> Option<f64> {
    text.as_deref().and_then(|t| t.trim().parse::<f64>().ok()).filter(|v| v.is_finite())
}

fn flag(text: &Option<String>) -> bool {
    text.as_deref().map(str::trim) == Some("1")
}

// ---------------------------------------------------------------------
// Import.
// ---------------------------------------------------------------------

/// One feed trip, read: its rows' offsets and the pattern it runs.
struct Read<'a> {
    row: &'a TripRow,
    id: &'a str,
    route: &'a str,
    service: &'a str,
    departs: Departs,
    /// The first arrival: what the offsets are measured from.
    base: u32,
    times: Vec<StopOffset>,
    stops: Vec<String>,
    flags: Vec<bool>,
    distances: Vec<f64>,
}

impl Read<'_> {
    fn first_start(&self) -> u32 {
        self.departs.first_start().unwrap_or(self.base)
    }
}

/// Read a feed into a network document. Deterministic: every
/// iteration is ordered, so the same feed yields the same document on
/// every host. What the model cannot hold yet is reported in the
/// notes (stops without coordinates, and the trips serving them;
/// single-stop trips; trips with no first time).
pub fn import(feed: &Feed) -> ImportOutcome {
    let mut notes: Vec<ImportNote> = Vec::new();
    let mut boundary = Boundary::default();
    let mut net = Network {
        name: feed
            .agency
            .first()
            .and_then(|a| a.agency_name.clone())
            .unwrap_or_else(|| "Imported network".into()),
        ..Default::default()
    };

    for row in &feed.stops {
        let Some(id) = &row.stop_id else { continue };
        // Missing coordinates: the model cannot represent a place at
        // (0, 0), and silently mapping one there poisons every map.
        let (Some(lat), Some(lon)) = (parse_f64(&row.stop_lat), parse_f64(&row.stop_lon)) else { continue };
        net.stops.insert(
            id.clone(),
            transit_network::Stop {
                name: row.stop_name.clone().unwrap_or_default(),
                lat,
                lon,
                parent: row.parent_station.clone(),
                transfer: false,
            },
        );
    }
    let mut route_names: BTreeMap<&str, String> = BTreeMap::new();
    for row in &feed.routes {
        let Some(id) = &row.route_id else { continue };
        let name = row.route_short_name.clone().or_else(|| row.route_long_name.clone()).unwrap_or_else(|| id.clone());
        route_names.insert(id, name.clone());
        net.routes.insert(
            id.clone(),
            transit_network::Route {
                name,
                mode: row.route_type.as_deref().and_then(|t| t.trim().parse().ok()).map_or(Mode::Other, mode_from_gtfs_type),
                color: row.route_color.clone(),
                directions: Default::default(),
            },
        );
    }

    // Rows by trip, in stop_sequence order.
    let mut rows_of: BTreeMap<&str, Vec<(u32, &StopTimeRow)>> = BTreeMap::new();
    for row in &feed.stop_times {
        let Some(trip) = row.trip_id.as_deref() else { continue };
        let seq = row.stop_sequence.as_deref().and_then(|s| s.trim().parse().ok()).unwrap_or(0);
        rows_of.entry(trip).or_default().push((seq, row));
    }
    for rows in rows_of.values_mut() {
        rows.sort_by_key(|(seq, _)| *seq);
    }
    let mut bands_of: BTreeMap<&str, (Vec<HeadwayBand>, bool)> = BTreeMap::new();
    for row in &feed.frequencies {
        let Some(trip) = row.trip_id.as_deref() else { continue };
        let (Some(start), Some(end), Some(headway)) = (
            row.start_time.as_deref().and_then(parse_time),
            row.end_time.as_deref().and_then(parse_time),
            row.headway_secs.as_deref().and_then(|h| h.trim().parse().ok()),
        ) else {
            continue;
        };
        let entry = bands_of.entry(trip).or_default();
        entry.0.push(HeadwayBand { start_secs: start, end_secs: end, headway_secs: headway });
        entry.1 |= flag(&row.exact_times);
    }

    let (mut single_stop, mut no_departure, mut missing_coords) = (0u32, 0u32, 0u32);
    let mut trips: Vec<Read> = Vec::new();
    let mut by_id: Vec<&TripRow> = feed.trips.iter().filter(|t| t.trip_id.is_some()).collect();
    by_id.sort_by(|a, b| a.trip_id.cmp(&b.trip_id));
    for row in by_id {
        let id = row.trip_id.as_deref().expect("filtered");
        let rows = rows_of.get(id).map(Vec::as_slice).unwrap_or(&[]);
        if rows.len() < 2 {
            single_stop += 1;
            continue;
        }
        let stops: Vec<String> = rows.iter().map(|(_, r)| r.stop_id.clone().unwrap_or_default()).collect();
        if stops.iter().any(|s| !net.stops.contains_key(s)) {
            missing_coords += 1;
            continue;
        }
        let times: Vec<(Option<u32>, Option<u32>)> = rows
            .iter()
            .map(|(_, r)| {
                (r.arrival_time.as_deref().and_then(parse_time), r.departure_time.as_deref().and_then(parse_time))
            })
            .collect();
        let Some(base) = times[0].0.or(times[0].1) else {
            no_departure += 1;
            continue;
        };
        let offsets: Vec<StopOffset> = times
            .iter()
            .map(|(a, d)| StopOffset {
                arrival_secs: a.map(|t| t.saturating_sub(base)),
                departure_secs: d.map(|t| t.saturating_sub(base)),
            })
            .collect();
        let flags: Vec<bool> = rows
            .iter()
            .zip(&times)
            .map(|((_, r), (a, d))| a.is_some() && d.is_some() && r.timepoint.as_deref().map(str::trim) != Some("0"))
            .collect();
        let departs = match bands_of.get(id) {
            Some((bands, exact)) => {
                let mut bands = bands.clone();
                bands.sort_by_key(|b| (b.start_secs, b.end_secs));
                boundary.frequency_bases.insert(id.to_string(), base);
                Departs::Every { bands, exact: *exact }
            }
            None => Departs::At { start_secs: base },
        };
        let sequences: Vec<u32> = rows.iter().map(|(seq, _)| *seq).collect();
        if sequences.iter().enumerate().any(|(i, s)| *s != i as u32 + 1) {
            boundary.stop_sequences.insert(id.to_string(), sequences);
        }
        trips.push(Read {
            row,
            id,
            route: row.route_id.as_deref().unwrap_or(""),
            service: row.service_id.as_deref().unwrap_or(""),
            departs,
            base,
            times: offsets,
            stops,
            flags,
            distances: resolve_distances(&rows.iter().map(|(_, r)| parse_f64(&r.shape_dist_traveled)).collect::<Vec<_>>()),
        });
    }
    // Earliest first, then id: the order a pattern's trips are stored in.
    trips.sort_by(|a, b| a.first_start().cmp(&b.first_start()).then_with(|| a.id.cmp(b.id)));

    // Patterns: one per (route, direction, stops, exactness, shape).
    type Key = (String, String, Vec<String>, Vec<bool>, Option<String>);
    let mut clusters: BTreeMap<Key, Vec<usize>> = BTreeMap::new();
    for (i, t) in trips.iter().enumerate() {
        let direction = t.row.direction_id.clone().unwrap_or_else(|| "n".into());
        clusters
            .entry((t.route.to_string(), direction, t.stops.clone(), t.flags.clone(), t.row.shape_id.clone()))
            .or_default()
            .push(i);
    }
    let day_end = trips.iter().map(|t| t.first_start()).max().map_or(30 * 3600, |m| (m + 1).max(30 * 3600));
    let mut pattern_of: Vec<String> = vec![String::new(); trips.len()];
    let mut index: BTreeMap<(String, String), usize> = BTreeMap::new();
    let stop_name = |id: &str| net.stops.get(id).map_or(id.to_string(), |s| s.name.clone());
    let mut shapes: BTreeMap<&str, Vec<(u32, Coord)>> = BTreeMap::new();
    for row in &feed.shapes {
        let (Some(id), Some(lat), Some(lon)) = (row.shape_id.as_deref(), parse_f64(&row.shape_pt_lat), parse_f64(&row.shape_pt_lon))
        else {
            continue;
        };
        let seq = row.shape_pt_sequence.as_deref().and_then(|s| s.trim().parse().ok()).unwrap_or(0);
        shapes.entry(id).or_default().push((seq, Coord { lat, lon }));
    }
    for points in shapes.values_mut() {
        points.sort_by_key(|(seq, _)| *seq);
    }
    let mut patterns: Vec<(String, Pattern)> = Vec::new();
    for ((route, direction, stops, flags, shape), members) in &clusters {
        let n = index.entry((route.clone(), direction.clone())).or_default();
        let pattern_id = format!("pat-{route}-{direction}-{n}");
        *n += 1;
        let first = &trips[members[0]];
        // The production tool: one all-day profile from the earliest
        // member's timepoint arrivals, when its ends are exact.
        let timing = (flags.first() == Some(&true) && flags.last() == Some(&true))
            .then(|| TimingProfile {
                start_secs: 0,
                end_secs: day_end,
                at_timepoints: first
                    .times
                    .iter()
                    .zip(flags)
                    .filter(|(_, f)| **f)
                    .map(|(o, _)| o.arrival_secs.unwrap_or(0))
                    .collect(),
            })
            .into_iter()
            .collect();
        let route_name = route_names.get(route.as_str()).cloned().unwrap_or_else(|| route.clone());
        patterns.push((
            pattern_id.clone(),
            Pattern {
                route: route.clone(),
                direction: (direction != "n").then(|| direction.clone()),
                name: format!("{route_name} → {}", stop_name(stops.last().map_or("", String::as_str))),
                path: shape
                    .as_deref()
                    .and_then(|s| shapes.get(s))
                    .filter(|points| points.len() >= 2)
                    .map(|points| points.iter().map(|(_, c)| *c).collect()),
                stops: stops
                    .iter()
                    .zip(flags)
                    .zip(&first.distances)
                    .map(|((stop, timepoint), distance_m)| PatternStop {
                        stop: stop.clone(),
                        timepoint: *timepoint,
                        distance_m: *distance_m,
                    })
                    .collect(),
                timing,
            },
        ));
        for &i in members {
            pattern_of[i] = pattern_id.clone();
        }
    }
    net.patterns.extend(patterns);
    for (i, t) in trips.iter().enumerate() {
        let key = format!("{}:{}", t.service, pattern_of[i]);
        net.pattern_trips
            .entry(key)
            .or_insert_with(|| PatternTrips { pattern: pattern_of[i].clone(), service_type: t.service.to_string(), trips: vec![] })
            .trips
            .push(Trip { id: t.id.to_string(), departs: t.departs.clone(), times: t.times.clone() });
    }

    // Directions, each named by its trips' most common headsign (else
    // "To" its most common terminal). Ties: the lexicographically
    // first — deterministic.
    let mut headsigns: BTreeMap<(String, String), BTreeMap<String, usize>> = BTreeMap::new();
    let mut terminals: BTreeMap<(String, String), BTreeMap<String, usize>> = BTreeMap::new();
    for ((route, direction, stops, _, _), members) in &clusters {
        if direction == "n" {
            continue;
        }
        let key = (route.clone(), direction.clone());
        let terminal = stop_name(stops.last().map_or("", String::as_str));
        *terminals.entry(key.clone()).or_default().entry(terminal).or_default() += members.len();
        let signs = headsigns.entry(key).or_default();
        for &i in members {
            if let Some(h) = trips[i].row.trip_headsign.as_deref() {
                *signs.entry(h.to_string()).or_default() += 1;
            }
        }
    }
    let most = |counts: &BTreeMap<String, usize>| {
        counts.iter().max_by(|a, b| a.1.cmp(b.1).then(b.0.cmp(a.0))).map(|(k, _)| k.clone())
    };
    for (key @ (route, direction), ends) in &terminals {
        let name = headsigns
            .get(key)
            .and_then(&most)
            .unwrap_or_else(|| format!("To {}", most(ends).unwrap_or_default()));
        if let Some(r) = net.routes.get_mut(route) {
            r.directions.insert(direction.clone(), Direction { name });
        }
    }

    // Service types: every service the feed's calendars name, and any
    // a kept trip runs under.
    let mut rules: BTreeMap<String, CalendarRule> = BTreeMap::new();
    for row in &feed.calendar {
        let Some(id) = &row.service_id else { continue };
        let rule = rules.entry(id.clone()).or_insert_with(empty_rule);
        rule.days = WeekdaySet {
            mon: flag(&row.monday),
            tue: flag(&row.tuesday),
            wed: flag(&row.wednesday),
            thu: flag(&row.thursday),
            fri: flag(&row.friday),
            sat: flag(&row.saturday),
            sun: flag(&row.sunday),
        };
        if let (Some(start), Some(end)) =
            (row.start_date.as_deref().and_then(parse_date), row.end_date.as_deref().and_then(parse_date))
        {
            rule.ranges.push(DateRange { start, end });
        }
    }
    for row in &feed.calendar_dates {
        let (Some(id), Some(date)) = (&row.service_id, row.date.as_deref().and_then(parse_date)) else { continue };
        let state = match row.exception_type.as_deref().map(str::trim) {
            Some("1") => DayState::Added,
            Some("2") => DayState::Removed,
            _ => continue,
        };
        rules.entry(id.clone()).or_insert_with(empty_rule).exceptions.push(CalendarException { date, state });
    }
    for t in &trips {
        rules.entry(t.service.to_string()).or_insert_with(empty_rule);
    }
    for (id, mut rule) in rules {
        rule.exceptions.sort_by_key(|e| e.date);
        net.service_types.insert(id.clone(), ServiceType { name: id, rule });
    }

    // Blocks: every trip under its block id, in the order they run
    // (first start, then id). The observed gap between consecutive
    // one-start trips of one service type becomes the layover (GTFS
    // has none; the planner re-authors from there).
    let mut blocks: BTreeMap<&str, Vec<usize>> = BTreeMap::new();
    for (i, t) in trips.iter().enumerate() {
        if let Some(b) = t.row.block_id.as_deref() {
            blocks.entry(b).or_default().push(i);
        }
    }
    for (id, members) in blocks {
        let mut layovers = Vec::new();
        for (k, pair) in members.windows(2).enumerate() {
            let (a, b) = (&trips[pair[0]], &trips[pair[1]]);
            let (Departs::At { start_secs: from }, Departs::At { start_secs: to }) = (&a.departs, &b.departs) else {
                continue;
            };
            let end = from + a.times.last().and_then(|o| o.arrival_secs).unwrap_or(0);
            if a.service == b.service && end <= *to {
                layovers.push(Layover { after_trip: k, secs: to - end, at: a.stops.last().cloned() });
            }
        }
        net.blocks.insert(
            id.to_string(),
            Block { trips: members.iter().map(|&i| TripRef { trip: trips[i].id.to_string() }).collect(), layovers },
        );
    }

    // Transfer points: transfers.txt declares them (pairs; type 3 is
    // "not possible" and never marks). A flag only composes where at
    // least two routes meet across the stop's walking group —
    // otherwise it is reported, not applied.
    {
        let index = transit_network::stop_route_index(&net);
        let marked: BTreeSet<&str> = feed
            .transfers
            .iter()
            .filter(|t| t.transfer_type.as_deref().map(str::trim) != Some("3"))
            .flat_map(|t| [t.from_stop_id.as_deref(), t.to_stop_id.as_deref()])
            .flatten()
            .collect();
        for stop in marked {
            if !net.stops.contains_key(stop) {
                continue;
            }
            if transit_network::routes_meeting_at(&net, &index, stop).len() >= 2 {
                net.stops.get_mut(stop).expect("checked above").transfer = true;
            } else {
                notes.push(ImportNote::UnusableTransferPoint { stop: stop.to_string() });
            }
        }
    }

    if single_stop + no_departure + missing_coords > 0 {
        notes.push(ImportNote::DroppedTrips {
            single_stop,
            no_departure,
            missing_stop_coords: missing_coords,
        });
    }
    ImportOutcome { network: net, boundary, notes }
}

fn empty_rule() -> CalendarRule {
    CalendarRule { days: WeekdaySet::none(), ranges: vec![], exceptions: vec![] }
}

// ---------------------------------------------------------------------
// Export.
// ---------------------------------------------------------------------

/// Write a validated network document as a feed. Invalid documents
/// are refused: the registry only ever holds feeds built from
/// documents that passed validation (cap-network's `validate` op
/// reports the full issue list for authoring UIs).
pub fn export(net: &Network, boundary: &Boundary) -> Result<Feed, String> {
    if let Err(first) = net.validate() {
        let count = transit_network::validation_issues(net).len();
        return Err(format!("invalid network: {first} ({count} issues total; the validate op returns all)"));
    }
    let s = |v: &str| Some(v.to_string());
    let mut feed = Feed::default();
    feed.agency.push(AgencyRow { agency_name: s(&net.name), ..Default::default() });
    feed.feed_info.push(FeedInfoRow { feed_publisher_name: s(&net.name), ..Default::default() });

    let parents: BTreeSet<&str> = net.stops.values().filter_map(|st| st.parent.as_deref()).collect();
    for (id, st) in &net.stops {
        feed.stops.push(StopRow {
            stop_id: s(id),
            stop_name: s(&st.name),
            stop_lat: Some(st.lat.to_string()),
            stop_lon: Some(st.lon.to_string()),
            location_type: parents.contains(id.as_str()).then(|| "1".to_string()),
            parent_station: st.parent.clone(),
            ..Default::default()
        });
    }
    for (id, r) in &net.routes {
        feed.routes.push(RouteRow {
            route_id: s(id),
            route_short_name: s(&r.name),
            route_type: Some(gtfs_route_type(r.mode).to_string()),
            route_color: r.color.clone(),
            ..Default::default()
        });
    }

    // GTFS's direction_id is binary. Directions "0" and "1" (what an
    // import names them) export as themselves; any other id takes a
    // free one in id order, and a route's third direction exports
    // without one.
    let direction_of = |pattern: &Pattern| {
        let id = pattern.direction.as_ref()?;
        let route = net.routes.get(&pattern.route)?;
        let literal = |k: &str| match k {
            "0" => Some(0u8),
            "1" => Some(1u8),
            _ => None,
        };
        let code = literal(id).or_else(|| {
            let mut free = [0u8, 1].into_iter().filter(|c| !route.directions.keys().any(|k| literal(k) == Some(*c)));
            let rank = route.directions.keys().filter(|k| literal(k).is_none()).position(|k| k == id)?;
            free.nth(rank)
        })?;
        Some((code, &route.directions[id]))
    };
    let block_of: BTreeMap<&str, &str> =
        net.blocks.iter().flat_map(|(b, block)| block.trips.iter().map(move |r| (r.trip.as_str(), b.as_str()))).collect();

    let mut rows: Vec<(&str, TripRow, Vec<StopTimeRow>, Vec<FrequencyRow>)> = Vec::new();
    for pt in net.pattern_trips.values() {
        let pattern = &net.patterns[&pt.pattern];
        let direction = direction_of(pattern);
        let shape = pattern.path.as_ref().filter(|p| p.len() >= 2).map(|_| pt.pattern.clone());
        for trip in &pt.trips {
            let (base, frequencies) = match &trip.departs {
                Departs::At { start_secs } => (*start_secs, vec![]),
                Departs::Every { bands, exact } => (
                    boundary
                        .frequency_bases
                        .get(&trip.id)
                        .copied()
                        .or_else(|| trip.departs.first_start())
                        .unwrap_or(0),
                    bands
                        .iter()
                        .map(|b| FrequencyRow {
                            trip_id: s(&trip.id),
                            start_time: Some(format_time(b.start_secs)),
                            end_time: Some(format_time(b.end_secs)),
                            headway_secs: Some(b.headway_secs.to_string()),
                            exact_times: exact.then(|| "1".to_string()),
                        })
                        .collect(),
                ),
            };
            let sequences = boundary
                .stop_sequences
                .get(&trip.id)
                .filter(|seq| seq.len() == pattern.stops.len())
                .cloned()
                .unwrap_or_else(|| (1..=pattern.stops.len() as u32).collect());
            let stop_times = pattern
                .stops
                .iter()
                .zip(&trip.times)
                .zip(&sequences)
                .map(|((ps, o), seq)| StopTimeRow {
                    trip_id: s(&trip.id),
                    stop_id: s(&ps.stop),
                    stop_sequence: Some(seq.to_string()),
                    arrival_time: o.arrival_secs.map(|t| format_time(base + t)),
                    departure_time: o.departure_secs.map(|t| format_time(base + t)),
                    timepoint: Some(if ps.timepoint { "1" } else { "0" }.to_string()),
                    shape_dist_traveled: Some(ps.distance_m.to_string()),
                    ..Default::default()
                })
                .collect();
            rows.push((
                trip.id.as_str(),
                TripRow {
                    route_id: s(&pattern.route),
                    service_id: s(&pt.service_type),
                    trip_id: s(&trip.id),
                    // A directed pattern's trips are signed with the
                    // direction's name (what a rider reads, and what
                    // the import names the direction from).
                    trip_headsign: Some(direction.map_or(pattern.name.clone(), |(_, d)| d.name.clone())),
                    direction_id: direction.map(|(code, _)| code.to_string()),
                    block_id: block_of.get(trip.id.as_str()).map(|b| b.to_string()),
                    shape_id: shape.clone(),
                    ..Default::default()
                },
                stop_times,
                frequencies,
            ));
        }
    }
    rows.sort_by(|a, b| a.0.cmp(b.0));
    for (_, trip, stop_times, frequencies) in rows {
        feed.trips.push(trip);
        feed.stop_times.extend(stop_times);
        feed.frequencies.extend(frequencies);
    }

    for (id, p) in &net.patterns {
        let Some(path) = p.path.as_ref().filter(|p| p.len() >= 2) else { continue };
        for (i, c) in path.iter().enumerate() {
            feed.shapes.push(ShapeRow {
                shape_id: s(id),
                shape_pt_lat: Some(c.lat.to_string()),
                shape_pt_lon: Some(c.lon.to_string()),
                shape_pt_sequence: Some((i + 1).to_string()),
                ..Default::default()
            });
        }
    }

    // The calendar as authored: the weekday flags and the first range
    // as the calendar row, every exception as a calendar_dates row,
    // and a further range's running dates as added dates.
    for (id, st) in &net.service_types {
        let rule = &st.rule;
        let bit = |b: bool| Some(if b { "1" } else { "0" }.to_string());
        if let Some(first) = rule.ranges.first() {
            feed.calendar.push(CalendarRow {
                service_id: s(id),
                monday: bit(rule.days.mon),
                tuesday: bit(rule.days.tue),
                wednesday: bit(rule.days.wed),
                thursday: bit(rule.days.thu),
                friday: bit(rule.days.fri),
                saturday: bit(rule.days.sat),
                sunday: bit(rule.days.sun),
                start_date: Some(format_date(first.start)),
                end_date: Some(format_date(first.end)),
            });
        }
        let mut dates: BTreeMap<NaiveDate, &str> = BTreeMap::new();
        for range in rule.ranges.iter().skip(1) {
            let mut date = range.start;
            while date <= range.end {
                let in_first = rule.ranges[0].start <= date && date <= rule.ranges[0].end;
                if !in_first && rule.days.runs_on(chrono::Datelike::weekday(&date)) {
                    dates.insert(date, "1");
                }
                date += Duration::days(1);
            }
        }
        for e in &rule.exceptions {
            dates.insert(e.date, if e.state == DayState::Added { "1" } else { "2" });
        }
        for (date, kind) in dates {
            feed.calendar_dates.push(CalendarDateRow {
                service_id: s(id),
                date: Some(format_date(date)),
                exception_type: s(kind),
            });
        }
    }
    Ok(feed)
}

/// Feed route type. The model's `Mode::Other` has no GTFS code; bus
/// is the least-wrong default for display purposes.
fn gtfs_route_type(mode: Mode) -> u8 {
    match mode {
        Mode::Tram => 0,
        Mode::Subway => 1,
        Mode::Rail => 2,
        Mode::Bus => 3,
        Mode::Ferry => 4,
        Mode::CableCar => 5,
        Mode::Gondola => 6,
        Mode::Funicular => 7,
        Mode::Trolleybus => 11,
        Mode::Monorail => 12,
        Mode::Other => 3,
    }
}

/// Meters along a trip, resolved from the feed's own
/// `shape_dist_traveled` where it publishes any: known anchors are
/// kept (clamped non-decreasing — the model requires it), missing
/// rows interpolate between their bracketing anchors and
/// extrapolate by 500 m steps outside them. A trip without a
/// single anchor fabricates index × 500 m (GTFS has none to keep).
/// Non-finite values count as absent (they cannot key a cluster).
fn resolve_distances(raw: &[Option<f64>]) -> Vec<f64> {
    let raw: Vec<Option<f64>> = raw
        .iter()
        .map(|d| if d.is_some_and(|v| v.is_finite()) { *d } else { None })
        .collect();
    let n = raw.len();
    if raw.iter().all(|d| d.is_none()) {
        return (0..n).map(|i| i as f64 * 500.0).collect();
    }
    let anchors: Vec<(usize, f64)> = raw
        .iter()
        .enumerate()
        .filter_map(|(i, d)| d.map(|d| (i, d)))
        .collect();
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let d = match raw[i] {
            Some(d) => d,
            None => {
                let before = anchors.iter().rev().find(|(k, _)| *k < i);
                let after = anchors.iter().find(|(k, _)| *k > i);
                match (before, after) {
                    (Some((bi, bd)), Some((ai, ad))) => {
                        let fraction = (i - bi) as f64 / (ai - bi) as f64;
                        bd + (ad - bd) * fraction
                    }
                    (Some((_, bd)), None) => bd + ((i - anchors.last().expect("some anchor").0) as f64) * 500.0,
                    (None, Some((ai, ad))) => (ad - (ai - i) as f64 * 500.0).max(0.0),
                    (None, None) => unreachable!("at least one anchor exists"),
                }
            }
        };
        out.push(d);
    }
    // The model requires non-decreasing distances; the spec's rows
    // can wobble.
    let mut prev = 0.0f64;
    for d in out.iter_mut() {
        if *d < prev {
            *d = prev;
        }
        prev = *d;
    }
    out
}

/// GTFS route types → model modes.
fn mode_from_gtfs_type(t: u8) -> Mode {
    match t {
        0 => Mode::Tram,
        1 => Mode::Subway,
        2 => Mode::Rail,
        3 => Mode::Bus,
        4 => Mode::Ferry,
        5 => Mode::CableCar,
        6 => Mode::Gondola,
        7 => Mode::Funicular,
        11 => Mode::Trolleybus,
        12 => Mode::Monorail,
        _ => Mode::Other,
    }
}
