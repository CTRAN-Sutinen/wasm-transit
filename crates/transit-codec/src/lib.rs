//! [`import`] reads a [`Feed`] into an authored network document, and
//! [`export`] writes it back. This crate is the only place GTFS and
//! transit-network meet. cap-network wraps it.
//!
//! * A pattern is one (route, direction, stops, exactness, shape, and what
//!   each stop row says). A row is exact when it has both times and does not
//!   say `timepoint=0`. A row that is not a stop at a time makes a
//!   demand-responsive trip.
//! * The [`Boundary`] holds what the bytes need and the document cannot:
//!   sequence numbering, a frequency trip's base, a generated agency id, the
//!   file
//!   that names a fare network, and services written as dates. An authored
//!   document exports with `Boundary::default()`.
//! * A service written as dates alone imports as the weekly rule that its
//!   dates read as, because otherwise a fork would carry nothing. The dates
//!   stay exact through added and removed exceptions, and the export writes
//!   dates again, after a fork too (D-111; [`read_weekly_rule`]).
//! * The export invents no headsign.

pub mod feed;
pub use feed::{base64_encode, Feed, FeedError, FeedInfo, FeedSummary};
mod extras;
mod values;
pub mod load;

use values::*;

use std::collections::{BTreeMap, BTreeSet};

use chrono::{Duration, NaiveDate};

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use feed::{CalendarDateRow, CalendarRow, FrequencyRow, RouteNetworkRow, RouteRow, ShapeRow, StopTimeRow, TripRow};
use transit_network::demand::{DemandTrip, Visit, VisitPlace};
use transit_network::{
    StopRule, Alignment, AlignmentPoint, Block, CalendarException, CalendarRule, DateRange, DayState, Departs, Direction, HeadwayBand, Layover, Mode,
    Network, Pattern, PatternStop, PatternTrips, ServiceType, StopOffset, TimingProfile, Trip, TripRef, WeekdaySet,
};

transit_version::kind_tag!(
    BoundaryKind = "boundary"
);

impl transit_version::Versioned for Boundary {
    const KIND: &'static str = "boundary";
    const READS: u32 = 1;
    fn schema_version(&self) -> u32 {
        self.schema_version.0
    }
}

/// Serialization only, never planner intent. `Boundary::default()` exports
/// stop_sequence as 1..=n and bases a frequency trip's rows at its first band.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
pub struct Boundary {
    #[serde(default)]
    #[ts(type = "\"boundary\"")]
    pub kind: BoundaryKind,
    #[serde(default)]
    #[ts(type = "number")]
    pub schema_version: transit_version::Version<1>,
    /// Kept only where the numbers are not 1..=n.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    #[ts(optional, as = "Option<BTreeMap<String, Vec<u32>>>")]
    pub stop_sequences: BTreeMap<String, Vec<u32>>,
    /// The first arrival of a frequency trip's rows. The spec reads those
    /// rows as offsets only.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    #[ts(optional, as = "Option<BTreeMap<String, u32>>")]
    pub frequency_bases: BTreeMap<String, u32>,
    /// Kept only where the numbers are not 1..=n.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    #[ts(optional, as = "Option<BTreeMap<String, Vec<u32>>>")]
    pub shape_sequences: BTreeMap<String, Vec<u32>>,
    /// The id given to the one agency that the feed names without an
    /// agency_id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub anonymous_operator: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    #[ts(optional, as = "Option<bool>")]
    pub fare_networks_on_routes: bool,
    /// The export writes these services back as dates, under the
    /// calendar.txt row they had (zero flags), if any.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    #[ts(optional, as = "Option<BTreeMap<String, DatedCalendar>>")]
    pub dated_calendars: BTreeMap<String, DatedCalendar>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
pub struct DatedCalendar {
    /// start_date and end_date as written. None when the feed had no row.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub row: Option<(String, String)>,
}

/// D-111. A weekday is part of a dated service's weekly rule when the service
/// runs on at least this many of them...
pub const WEEKLY_MIN_DAYS: u32 = 4;
/// ...and on at least this share (numerator, denominator) of that weekday's
/// occurrences between its first and last date.
pub const WEEKLY_MIN_SHARE: (u32, u32) = (3, 4);

/// The rule runs on exactly `dates`: one range over the qualifying
/// weekdays, a removed date for each one it skips, and an added date for each
/// other date. None when no weekday qualifies, so the service stays dated.
pub fn read_weekly_rule(dates: &BTreeSet<NaiveDate>) -> Option<CalendarRule> {
    use chrono::Datelike;
    let (first, last) = (*dates.first()?, *dates.last()?);
    let index = |d: NaiveDate| d.weekday().num_days_from_monday() as usize;
    let (mut occurrences, mut ran) = ([0u32; 7], [0u32; 7]);
    let mut d = first;
    while d <= last {
        occurrences[index(d)] += 1;
        ran[index(d)] += dates.contains(&d) as u32;
        d += Duration::days(1);
    }
    let (num, den) = WEEKLY_MIN_SHARE;
    let on: [bool; 7] = std::array::from_fn(|w| ran[w] >= WEEKLY_MIN_DAYS && den * ran[w] >= num * occurrences[w]);
    if !on.iter().any(|&b| b) {
        return None;
    }
    let flagged: Vec<NaiveDate> = dates.iter().copied().filter(|&d| on[index(d)]).collect();
    let range = DateRange { start: flagged[0], end: *flagged.last().expect("a weekday qualified"), extra: Default::default() };
    let mut exceptions = Vec::new();
    let (mut d, end) = (first.min(range.start), last.max(range.end));
    while d <= end {
        let by_rule = on[index(d)] && range.start <= d && d <= range.end;
        match (by_rule, dates.contains(&d)) {
            (true, false) => exceptions.push(CalendarException { date: d, state: DayState::Removed, extra: Default::default() }),
            (false, true) => exceptions.push(CalendarException { date: d, state: DayState::Added, extra: Default::default() }),
            _ => {}
        }
        d += Duration::days(1);
    }
    let [mon, tue, wed, thu, fri, sat, sun] = on;
    Some(CalendarRule { days: WeekdaySet { mon, tue, wed, thu, fri, sat, sun }, ranges: vec![range], exceptions, extra: Default::default() })
}

#[derive(Debug, Clone, PartialEq, Serialize, TS, schemars::JsonSchema)]
#[ts(export)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ImportNote {
    DroppedTrips {
        single_stop: u32,
        no_departure: u32,
        missing_stop_coords: u32,
    },
    /// A transfers.txt row marks a stop whose walking group fewer than two
    /// routes serve. The flag is reported, not applied.
    UnusableTransferPoint { stop: String },
    UnknownShape { shape: String },
}

#[derive(Debug, Clone, PartialEq, Serialize, TS, schemars::JsonSchema)]
#[ts(export)]
pub struct ImportOutcome {
    pub network: Network,
    pub boundary: Boundary,
    pub notes: Vec<ImportNote>,
}

struct Read<'a> {
    row: &'a TripRow,
    id: &'a str,
    route: &'a str,
    service: &'a str,
    departs: Departs,
    base: u32,
    times: Vec<StopOffset>,
    stops: Vec<String>,
    flags: Vec<bool>,
    distances: Vec<f64>,
    /// Part of the pattern's identity.
    attrs: Vec<StopAttrs>,
}

/// The published distance is held as bits so that it can key a pattern.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
struct StopAttrs {
    headsign: Option<String>,
    pickup: StopRule,
    drop_off: StopRule,
    continuous_pickup: Option<StopRule>,
    continuous_drop_off: Option<StopRule>,
    pickup_booking: Option<String>,
    drop_off_booking: Option<String>,
    published_distance: Option<u64>,
}

fn stop_attrs(row: &StopTimeRow) -> StopAttrs {
    StopAttrs {
        headsign: text(&row.stop_headsign),
        pickup: stop_rule(&row.pickup_type).unwrap_or_default(),
        drop_off: stop_rule(&row.drop_off_type).unwrap_or_default(),
        continuous_pickup: stop_rule(&row.continuous_pickup),
        continuous_drop_off: stop_rule(&row.continuous_drop_off),
        pickup_booking: text(&row.pickup_booking_rule_id),
        drop_off_booking: text(&row.drop_off_booking_rule_id),
        published_distance: parse_f64(&row.shape_dist_traveled).map(f64::to_bits),
    }
}

fn is_flexible(row: &StopTimeRow) -> bool {
    row.stop_id.is_none()
        || row.location_group_id.is_some()
        || row.location_id.is_some()
        || row.start_pickup_drop_off_window.is_some()
        || row.end_pickup_drop_off_window.is_some()
}

fn visit(row: &StopTimeRow) -> Visit {
    let place = match (&row.stop_id, &row.location_group_id, &row.location_id) {
        (Some(stop), _, _) => VisitPlace::Stop(stop.clone()),
        (None, Some(group), _) => VisitPlace::LocationGroup(group.clone()),
        (None, None, Some(zone)) => VisitPlace::Zone(zone.clone()),
        (None, None, None) => VisitPlace::default(),
    };
    Visit {
        place,
        arrival_secs: time(&row.arrival_time),
        departure_secs: time(&row.departure_time),
        window_start_secs: time(&row.start_pickup_drop_off_window),
        window_end_secs: time(&row.end_pickup_drop_off_window),
        headsign: text(&row.stop_headsign),
        pickup: stop_rule(&row.pickup_type).unwrap_or_default(),
        drop_off: stop_rule(&row.drop_off_type).unwrap_or_default(),
        continuous_pickup: stop_rule(&row.continuous_pickup),
        continuous_drop_off: stop_rule(&row.continuous_drop_off),
        timepoint: opt_bit(&row.timepoint),
        distance: parse_f64(&row.shape_dist_traveled),
        pickup_booking: text(&row.pickup_booking_rule_id),
        drop_off_booking: text(&row.drop_off_booking_rule_id),
        extra: Default::default(),
    }
}

impl Read<'_> {
    fn first_start(&self) -> u32 {
        self.departs.first_start().unwrap_or(self.base)
    }
}

/// Every iteration is ordered, so the same feed yields the same document on
/// every host.
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

    extras::import_operators(feed, &mut net, &mut boundary);
    extras::import_places(feed, &mut net);
    extras::import_fares(feed, &mut net);
    extras::import_demand_catalog(feed, &mut net);
    let fare_networks = extras::route_fare_networks(feed, &mut boundary);
    let mut route_names: BTreeMap<&str, String> = BTreeMap::new();
    for row in &feed.routes {
        let Some(id) = &row.route_id else { continue };
        let route_type: Option<u16> = int(&row.route_type);
        let mode = route_type.map_or(Mode::Other, mode_from_gtfs_type);
        let route = transit_network::Route {
            name: row.route_short_name.clone().unwrap_or_default(),
            mode,
            color: row.route_color.clone(),
            long_name: text(&row.route_long_name),
            description: text(&row.route_desc),
            url: text(&row.route_url),
            text_color: text(&row.route_text_color),
            sort_order: int(&row.route_sort_order),
            // Empty and 1 both mean no continuous stopping.
            continuous_pickup: stop_rule(&row.continuous_pickup).filter(|r| *r != StopRule::NotAvailable),
            continuous_drop_off: stop_rule(&row.continuous_drop_off).filter(|r| *r != StopRule::NotAvailable),
            operator: text(&row.agency_id),
            fare_network: fare_networks.get(id).cloned(),
            contactless: contactless(&row.cemv_support),
            vehicle_type: route_type.filter(|t| mode == Mode::Other || gtfs_route_type(mode) != *t),
            directions: Default::default(),
            extra: Default::default(),
        };
        let label = route.label();
        route_names.insert(id, if label.is_empty() { id.clone() } else { label.to_string() });
        net.routes.insert(id.clone(), route);
    }

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
        entry.0.push(HeadwayBand { start_secs: start, end_secs: end, headway_secs: headway, extra: Default::default() });
        entry.1 |= flag(&row.exact_times);
    }

    let (mut single_stop, mut no_departure, mut missing_coords) = (0u32, 0u32, 0u32);
    let mut demand_services: BTreeSet<String> = BTreeSet::new();
    let mut trips: Vec<Read> = Vec::new();
    let mut by_id: Vec<&TripRow> = feed.trips.iter().filter(|t| t.trip_id.is_some()).collect();
    by_id.sort_by(|a, b| a.trip_id.cmp(&b.trip_id));
    for row in by_id {
        let id = row.trip_id.as_deref().expect("filtered");
        let rows = rows_of.get(id).map(Vec::as_slice).unwrap_or(&[]);
        let sequences: Vec<u32> = rows.iter().map(|(seq, _)| *seq).collect();
        if rows.iter().any(|(_, r)| is_flexible(r)) {
            if sequences.iter().enumerate().any(|(i, s)| *s != i as u32 + 1) {
                boundary.stop_sequences.insert(id.to_string(), sequences);
            }
            demand_services.insert(row.service_id.clone().unwrap_or_default());
            net.demand.trips.insert(
                id.to_string(),
                DemandTrip {
                    route: row.route_id.clone().unwrap_or_default(),
                    service_type: row.service_id.clone().unwrap_or_default(),
                    direction: text(&row.direction_id),
                    alignment: text(&row.shape_id),
                    block: text(&row.block_id),
                    headsign: text(&row.trip_headsign),
                    short_name: text(&row.trip_short_name),
                    wheelchair: yes_no(&row.wheelchair_accessible),
                    bikes: yes_no(&row.bikes_allowed),
                    cars: yes_no(&row.cars_allowed),
                    visits: rows.iter().map(|(_, r)| visit(r)).collect(),
                    extra: Default::default(),
                },
            );
            continue;
        }
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
            attrs: rows.iter().map(|(_, r)| stop_attrs(r)).collect(),
        });
    }
    trips.sort_by(|a, b| a.first_start().cmp(&b.first_start()).then_with(|| a.id.cmp(b.id)));

    type Key = (String, String, Vec<String>, Vec<bool>, Option<String>, Vec<StopAttrs>);
    let mut clusters: BTreeMap<Key, Vec<usize>> = BTreeMap::new();
    for (i, t) in trips.iter().enumerate() {
        let direction = t.row.direction_id.clone().unwrap_or_else(|| "n".into());
        clusters
            .entry((t.route.to_string(), direction, t.stops.clone(), t.flags.clone(), t.row.shape_id.clone(), t.attrs.clone()))
            .or_default()
            .push(i);
    }
    let day_end = trips.iter().map(|t| t.first_start()).max().map_or(30 * 3600, |m| (m + 1).max(30 * 3600));
    let mut pattern_of: Vec<String> = vec![String::new(); trips.len()];
    let mut index: BTreeMap<(String, String), usize> = BTreeMap::new();
    let stop_name = |id: &str| net.stops.get(id).map_or(id.to_string(), |s| s.name.clone());
    let mut shapes: BTreeMap<&str, Vec<(u32, AlignmentPoint)>> = BTreeMap::new();
    for row in &feed.shapes {
        let (Some(id), Some(lat), Some(lon)) = (row.shape_id.as_deref(), parse_f64(&row.shape_pt_lat), parse_f64(&row.shape_pt_lon))
        else {
            continue;
        };
        let seq = row.shape_pt_sequence.as_deref().and_then(|s| s.trim().parse().ok()).unwrap_or(0);
        shapes.entry(id).or_default().push((seq, AlignmentPoint { lat, lon, distance: parse_f64(&row.shape_dist_traveled) }));
    }
    for (id, mut points) in shapes {
        points.sort_by_key(|(seq, _)| *seq);
        let sequences: Vec<u32> = points.iter().map(|(seq, _)| *seq).collect();
        if !sequences.iter().enumerate().all(|(i, &s)| s == i as u32 + 1) {
            boundary.shape_sequences.insert(id.to_string(), sequences);
        }
        net.alignments.insert(id.to_string(), Alignment { points: points.into_iter().map(|(_, p)| p).collect(), extra: Default::default() });
    }
    let mut unknown_shapes: BTreeSet<String> = BTreeSet::new();
    let mut patterns: Vec<(String, Pattern)> = Vec::new();
    for ((route, direction, stops, flags, shape, attrs), members) in &clusters {
        let n = index.entry((route.clone(), direction.clone())).or_default();
        let pattern_id = format!("pat-{route}-{direction}-{n}");
        *n += 1;
        let first = &trips[members[0]];
        // One all-day profile from the earliest member's timepoint arrivals,
        // when both ends are exact.
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
                    extra: Default::default(),
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
                alignment: shape.as_ref().and_then(|s| {
                    net.alignments.contains_key(s).then(|| s.clone()).or_else(|| {
                        unknown_shapes.insert(s.clone());
                        None
                    })
                }),
                stops: stops
                    .iter()
                    .zip(flags)
                    .zip(&first.distances)
                    .zip(attrs)
                    .map(|(((stop, timepoint), distance_m), a)| PatternStop {
                        stop: stop.clone(),
                        timepoint: *timepoint,
                        distance_m: *distance_m,
                        distance_published: a.published_distance.is_some(),
                        headsign: a.headsign.clone(),
                        pickup: a.pickup,
                        drop_off: a.drop_off,
                        continuous_pickup: a.continuous_pickup,
                        continuous_drop_off: a.continuous_drop_off,
                        pickup_booking: a.pickup_booking.clone(),
                        drop_off_booking: a.drop_off_booking.clone(),
                        extra: Default::default(),
                    })
                    .collect(),
                timing,
                extra: Default::default(),
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
            .or_insert_with(|| PatternTrips { pattern: pattern_of[i].clone(), service_type: t.service.to_string(), trips: vec![], extra: Default::default() })
            .trips
            .push(Trip {
                id: t.id.to_string(),
                departs: t.departs.clone(),
                times: t.times.clone(),
                headsign: text(&t.row.trip_headsign),
                short_name: text(&t.row.trip_short_name),
                wheelchair: yes_no(&t.row.wheelchair_accessible),
                bikes: yes_no(&t.row.bikes_allowed),
                cars: yes_no(&t.row.cars_allowed),
                extra: Default::default(),
            });
    }

    // A direction is named by its trips' most common headsign, else "To" its
    // most common terminal. A tie goes to the lexicographically first.
    let mut headsigns: BTreeMap<(String, String), BTreeMap<String, usize>> = BTreeMap::new();
    let mut terminals: BTreeMap<(String, String), BTreeMap<String, usize>> = BTreeMap::new();
    for ((route, direction, stops, ..), members) in &clusters {
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
            r.directions.insert(direction.clone(), Direction { name, extra: Default::default() });
        }
    }

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
            rule.ranges.push(DateRange { start, end, extra: Default::default() });
        }
    }
    for row in &feed.calendar_dates {
        let (Some(id), Some(date)) = (&row.service_id, row.date.as_deref().and_then(parse_date)) else { continue };
        let state = match row.exception_type.as_deref().map(str::trim) {
            Some("1") => DayState::Added,
            Some("2") => DayState::Removed,
            _ => continue,
        };
        rules.entry(id.clone()).or_insert_with(empty_rule).exceptions.push(CalendarException { date, state, extra: Default::default() });
    }
    for service in trips.iter().map(|t| t.service.to_string()).chain(demand_services) {
        rules.entry(service).or_insert_with(empty_rule);
    }
    for (id, mut rule) in rules {
        rule.exceptions.sort_by_key(|e| e.date);
        // The boundary keeps how the service was written.
        let dated = rule.days == WeekdaySet::none()
            && !rule.exceptions.is_empty()
            && rule.exceptions.iter().all(|e| e.state == DayState::Added);
        if dated {
            let dates: BTreeSet<NaiveDate> = rule.exceptions.iter().map(|e| e.date).collect();
            if let Some(weekly) = read_weekly_rule(&dates) {
                let row = feed
                    .calendar
                    .iter()
                    .find(|r| r.service_id.as_deref() == Some(id.as_str()))
                    .map(|r| (r.start_date.clone().unwrap_or_default(), r.end_date.clone().unwrap_or_default()));
                boundary.dated_calendars.insert(id.clone(), DatedCalendar { row });
                rule = weekly;
            }
        }
        net.service_types.insert(id.clone(), ServiceType { name: id, rule, extra: Default::default() });
    }

    // GTFS has no layover. The observed gap between consecutive one-start
    // trips of one service type becomes the layover.
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
                layovers.push(Layover { after_trip: k, secs: to - end, at: a.stops.last().cloned(), extra: Default::default() });
            }
        }
        net.blocks.insert(
            id.to_string(),
            Block { trips: members.iter().map(|&i| TripRef { trip: trips[i].id.to_string(), extra: Default::default() }).collect(), layovers, extra: Default::default() },
        );
    }

    // transfers.txt type 3 ("not possible") never marks a transfer point. A
    // flag applies only where at least two routes meet across the stop's
    // walking group.
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
    notes.extend(unknown_shapes.into_iter().map(|shape| ImportNote::UnknownShape { shape }));
    ImportOutcome { network: net, boundary, notes }
}

fn empty_rule() -> CalendarRule {
    CalendarRule { days: WeekdaySet::none(), ranges: vec![], exceptions: vec![], extra: Default::default() }
}

/// Refuses an invalid document, so the registry only holds feeds built from
/// validated documents.
pub fn export(net: &Network, boundary: &Boundary) -> Result<Feed, String> {
    if let Err(first) = net.validate() {
        let count = transit_network::validation_issues(net).len();
        return Err(format!("invalid network: {first} ({count} issues total; the validate op returns all)"));
    }
    let s = |v: &str| Some(v.to_string());
    let mut feed = Feed::default();
    extras::export_operators(net, boundary, &mut feed);
    extras::export_places(net, &mut feed);
    extras::export_fares(net, &mut feed);
    extras::export_demand_catalog(net, &mut feed);
    for (id, r) in &net.routes {
        let network = r.fare_network.clone();
        feed.routes.push(RouteRow {
            route_id: s(id),
            agency_id: r.operator.clone(),
            route_short_name: (!r.name.is_empty()).then(|| r.name.clone()),
            route_long_name: r.long_name.clone(),
            route_desc: r.description.clone(),
            route_type: Some(published_route_type(r).to_string()),
            route_url: r.url.clone(),
            route_color: r.color.clone(),
            route_text_color: r.text_color.clone(),
            route_sort_order: int_cell(r.sort_order),
            continuous_pickup: r.continuous_pickup.map(stop_rule_code),
            continuous_drop_off: r.continuous_drop_off.map(stop_rule_code),
            network_id: if boundary.fare_networks_on_routes { network.clone() } else { None },
            cemv_support: r.contactless.map(contactless_code),
        });
        if let (Some(network), false) = (network, boundary.fare_networks_on_routes) {
            feed.route_networks.push(RouteNetworkRow { network_id: Some(network), route_id: s(id) });
        }
    }

    let direction_of = |pattern: &Pattern| {
        let id = pattern.direction.as_ref()?;
        let route = net.routes.get(&pattern.route)?;
        Some((gtfs_direction(route, id)?, &route.directions[id]))
    };
    let block_of: BTreeMap<&str, &str> =
        net.blocks.iter().flat_map(|(b, block)| block.trips.iter().map(move |r| (r.trip.as_str(), b.as_str()))).collect();

    let mut rows: Vec<(&str, TripRow, Vec<StopTimeRow>, Vec<FrequencyRow>)> = Vec::new();
    for pt in net.pattern_trips.values() {
        let pattern = &net.patterns[&pt.pattern];
        let direction = direction_of(pattern);
        let shape = pattern.alignment.clone();
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
                    // An estimated distance is never published.
                    shape_dist_traveled: ps.distance_published.then(|| ps.distance_m.to_string()),
                    stop_headsign: ps.headsign.clone(),
                    pickup_type: boarding_cell(ps.pickup),
                    drop_off_type: boarding_cell(ps.drop_off),
                    continuous_pickup: ps.continuous_pickup.map(stop_rule_code),
                    continuous_drop_off: ps.continuous_drop_off.map(stop_rule_code),
                    pickup_booking_rule_id: ps.pickup_booking.clone(),
                    drop_off_booking_rule_id: ps.drop_off_booking.clone(),
                    ..Default::default()
                })
                .collect();
            rows.push((
                trip.id.as_str(),
                TripRow {
                    route_id: s(&pattern.route),
                    service_id: s(&pt.service_type),
                    trip_id: s(&trip.id),
                    trip_headsign: trip.headsign.clone(),
                    trip_short_name: trip.short_name.clone(),
                    direction_id: direction.map(|(code, _)| code.to_string()),
                    block_id: block_of.get(trip.id.as_str()).map(|b| b.to_string()),
                    shape_id: shape.clone(),
                    wheelchair_accessible: yes_no_cell(trip.wheelchair),
                    bikes_allowed: yes_no_cell(trip.bikes),
                    cars_allowed: yes_no_cell(trip.cars),
                },
                stop_times,
                frequencies,
            ));
        }
    }
    for (id, t) in &net.demand.trips {
        let sequences = boundary
            .stop_sequences
            .get(id)
            .filter(|seq| seq.len() == t.visits.len())
            .cloned()
            .unwrap_or_else(|| (1..=t.visits.len() as u32).collect());
        let stop_times = t
            .visits
            .iter()
            .zip(&sequences)
            .map(|(v, seq)| {
                let (stop, group, zone) = match &v.place {
                    VisitPlace::Stop(id) => (Some(id.clone()), None, None),
                    VisitPlace::LocationGroup(id) => (None, Some(id.clone()), None),
                    VisitPlace::Zone(id) => (None, None, Some(id.clone())),
                };
                StopTimeRow {
                    trip_id: s(id),
                    arrival_time: time_cell(v.arrival_secs),
                    departure_time: time_cell(v.departure_secs),
                    stop_id: stop,
                    location_group_id: group,
                    location_id: zone,
                    stop_sequence: Some(seq.to_string()),
                    stop_headsign: v.headsign.clone(),
                    start_pickup_drop_off_window: time_cell(v.window_start_secs),
                    end_pickup_drop_off_window: time_cell(v.window_end_secs),
                    pickup_type: boarding_cell(v.pickup),
                    drop_off_type: boarding_cell(v.drop_off),
                    continuous_pickup: v.continuous_pickup.map(stop_rule_code),
                    continuous_drop_off: v.continuous_drop_off.map(stop_rule_code),
                    shape_dist_traveled: float_cell(v.distance),
                    timepoint: opt_bit_cell(v.timepoint),
                    pickup_booking_rule_id: v.pickup_booking.clone(),
                    drop_off_booking_rule_id: v.drop_off_booking.clone(),
                }
            })
            .collect();
        rows.push((
            id.as_str(),
            TripRow {
                route_id: s(&t.route),
                service_id: s(&t.service_type),
                trip_id: s(id),
                trip_headsign: t.headsign.clone(),
                trip_short_name: t.short_name.clone(),
                direction_id: t.direction.clone(),
                block_id: t.block.clone(),
                shape_id: t.alignment.clone(),
                wheelchair_accessible: yes_no_cell(t.wheelchair),
                bikes_allowed: yes_no_cell(t.bikes),
                cars_allowed: yes_no_cell(t.cars),
            },
            stop_times,
            vec![],
        ));
    }
    rows.sort_by(|a, b| a.0.cmp(b.0));
    for (_, trip, stop_times, frequencies) in rows {
        feed.trips.push(trip);
        feed.stop_times.extend(stop_times);
        feed.frequencies.extend(frequencies);
    }

    for (id, alignment) in &net.alignments {
        let sequences = boundary.shape_sequences.get(id).filter(|seqs| seqs.len() == alignment.points.len());
        for (i, p) in alignment.points.iter().enumerate() {
            feed.shapes.push(ShapeRow {
                shape_id: s(id),
                shape_pt_lat: Some(p.lat.to_string()),
                shape_pt_lon: Some(p.lon.to_string()),
                shape_pt_sequence: Some(sequences.map_or(i as u32 + 1, |seqs| seqs[i]).to_string()),
                shape_dist_traveled: p.distance.map(|d| d.to_string()),
            });
        }
    }

    for (id, st) in &net.service_types {
        let rule = &st.rule;
        let bit = |b: bool| Some(if b { "1" } else { "0" }.to_string());
        if let Some(dated) = boundary.dated_calendars.get(id) {
            if let Some((start, end)) = &dated.row {
                let zero = || bit(false);
                feed.calendar.push(CalendarRow {
                    service_id: s(id),
                    monday: zero(),
                    tuesday: zero(),
                    wednesday: zero(),
                    thursday: zero(),
                    friday: zero(),
                    saturday: zero(),
                    sunday: zero(),
                    start_date: Some(start.clone()),
                    end_date: Some(end.clone()),
                });
            }
            let named = rule.ranges.iter().flat_map(|r| [r.start, r.end]).chain(rule.exceptions.iter().map(|e| e.date));
            if let (Some(from), Some(to)) = (named.clone().min(), named.max()) {
                let mut date = from;
                while date <= to {
                    if rule.runs_on(date) {
                        feed.calendar_dates.push(CalendarDateRow {
                            service_id: s(id),
                            date: Some(format_date(date)),
                            exception_type: s("1"),
                        });
                    }
                    date += Duration::days(1);
                }
            }
            continue;
        }
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

/// The `direction_id` code is binary. Directions "0" and "1" keep their ids.
/// Any other id takes a free code in id order. A route's third direction, or
/// an id the route does not have, has no code.
pub fn gtfs_direction(route: &transit_network::Route, id: &str) -> Option<u8> {
    if !route.directions.contains_key(id) {
        return None;
    }
    let literal = |k: &str| match k {
        "0" => Some(0u8),
        "1" => Some(1u8),
        _ => None,
    };
    literal(id).or_else(|| {
        let mut free = [0u8, 1].into_iter().filter(|c| !route.directions.keys().any(|k| literal(k) == Some(*c)));
        let rank = route.directions.keys().filter(|k| literal(k).is_none()).position(|k| k == id)?;
        free.nth(rank)
    })
}

/// The inverse of [`gtfs_direction`]. A GTFS-rt alert's `direction_id`
/// resolves through it.
pub fn model_direction(route: &transit_network::Route, code: u32) -> Option<&transit_network::DirectionId> {
    route.directions.keys().find(|k| gtfs_direction(route, k).map(u32::from) == Some(code))
}

/// A GTFS-rt alert's `route_type` selector names this code.
pub fn published_route_type(route: &transit_network::Route) -> u16 {
    route.vehicle_type.unwrap_or_else(|| gtfs_route_type(route.mode))
}

/// `Mode::Other` has no GTFS code, so it publishes as bus.
fn gtfs_route_type(mode: Mode) -> u16 {
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

/// Missing rows interpolate between their bracketing anchors and extrapolate
/// by 500 m steps outside them. A trip without an anchor gets index × 500 m.
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
    // The model requires non-decreasing distances.
    let mut prev = 0.0f64;
    for d in out.iter_mut() {
        if *d < prev {
            *d = prev;
        }
        prev = *d;
    }
    out
}

fn mode_from_gtfs_type(t: u16) -> Mode {
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
