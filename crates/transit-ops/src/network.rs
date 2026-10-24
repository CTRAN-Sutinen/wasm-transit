//! The Layer A → Layer B bridge: an authored network document
//! (transit-network) expanded into the derived-feed view
//! (transit-core's `Gtfs`) that every downstream op — departures,
//! planning, blocks, geojson — already consumes.
//!
//! This is the only place where model-owned ids and the derived view
//! meet, and it is a *translation*, not a model: GTFS-the-format
//! stays out of transit-network by design, so this module owns the
//! id conventions of the derived view:
//!
//! * trip identity is the derived pair — `"<service_id>@<start_secs>"`;
//! * the GTFS service key *is* the network service id (so departure
//!   rows and block rows carry the planner's own keys);
//! * the calendar becomes explicit per-date rows (one `added` entry
//!   per running date): the derived equivalent of an exception-only
//!   feed, exact for any authored rule including Added exceptions.
//!
//! Shapes: a pattern's authored `path` becomes a `shapes.txt` entry
//! keyed by the pattern id, and every trip the pattern materializes
//! references it. Patterns without a path keep `shape_id: None` and
//! draw stop-to-stop lines (the base `trip_path` fallback).

use std::collections::{BTreeMap, BTreeSet, HashMap};

use chrono::{Duration, NaiveDate};

use transit_core::{
    Agency, CalendarDate, FeedInfo, Gtfs, Route, ServiceCalendar, ShapePoint, Stop, StopTime, Trip,
};
use transit_network::{
    materialize_service, Anchor, Block, CalendarException, CalendarRule, Coord, DateRange, DayState,
    Mode, Network, Pattern, PatternStop, Schedule, Service, TimingProfile, TripRef, WeekdaySet,
};

use crate::OpError;

/// Expand a validated network document into a derived feed. Invalid
/// documents are rejected: the registry only ever holds feeds built
/// from documents that passed validation (the `network_validate` op
/// reports the full issue list for authoring UIs).
pub fn network_to_gtfs(net: &Network) -> Result<Gtfs, OpError> {
    if let Err(first) = net.validate() {
        let count = transit_network::validation_issues(net).len();
        return Err(OpError::new(format!(
            "invalid network: {first} ({count} issues total; network_validate returns all)"
        )));
    }

    let agencies = vec![Agency {
        agency_id: None,
        agency_name: net.name.clone(),
        agency_url: String::new(),
        agency_timezone: String::new(),
    }];

    let routes: HashMap<String, Route> = net
        .routes
        .iter()
        .map(|(id, r)| {
            (
                id.clone(),
                Route {
                    route_id: id.clone(),
                    agency_id: None,
                    route_short_name: Some(r.name.clone()),
                    route_long_name: None,
                    route_type: gtfs_route_type(r.mode),
                    route_color: r.color.clone(),
                    route_text_color: None,
                },
            )
        })
        .collect();

    let stops: HashMap<String, Stop> = net
        .stops
        .iter()
        .map(|(id, s)| {
            (
                id.clone(),
                Stop {
                    stop_id: id.clone(),
                    stop_code: None,
                    stop_name: s.name.clone(),
                    stop_lat: Some(s.lat),
                    stop_lon: Some(s.lon),
                    location_type: Some(0),
                    parent_station: s.parent.clone(),
                },
            )
        })
        .collect();

    // Runcut: which block runs which derived trip.
    let mut block_of: BTreeMap<(String, u32), String> = BTreeMap::new();
    for (block_id, block) in &net.blocks {
        for trip in &block.trips {
            block_of.insert((trip.service.clone(), trip.start_secs), block_id.clone());
        }
    }

    let mut trips: HashMap<String, Trip> = HashMap::new();
    let mut stop_times: Vec<StopTime> = Vec::new();
    let mut calendar_dates: Vec<CalendarDate> = Vec::new();

    // Authored alignments, in the derived view's shape map: one
    // shapes.txt polyline per pattern, keyed by the pattern id.
    let shapes: HashMap<String, Vec<ShapePoint>> = net
        .patterns
        .iter()
        .filter_map(|(pattern_id, p)| {
            let path = p.path.as_ref()?;
            if path.len() < 2 {
                return None;
            }
            Some((
                pattern_id.clone(),
                path.iter()
                    .enumerate()
                    .map(|(i, c)| ShapePoint {
                        shape_id: pattern_id.clone(),
                        shape_pt_lat: c.lat,
                        shape_pt_lon: c.lon,
                        shape_pt_sequence: (i + 1) as u32,
                        shape_dist_traveled: None,
                    })
                    .collect(),
            ))
        })
        .collect();

    for (service_id, service) in &net.services {
        // Calendar: explicit running dates, exact for any rule.
        if let Some(rule) = net.calendars.get(&service.calendar) {
            for date in running_dates(rule) {
                calendar_dates.push(CalendarDate {
                    service_id: service_id.clone(),
                    date,
                    exception_type: 1,
                });
            }
        }
        for trip in materialize_service(net, service_id).map_err(|e| OpError::new(e.to_string()))? {
            let trip_id = format!("{service_id}@{}", trip.start_secs);
            trips.insert(
                trip_id.clone(),
                Trip {
                    route_id: trip.route.clone(),
                    service_id: service_id.clone(),
                    trip_id: trip_id.clone(),
                    trip_headsign: net
                        .patterns
                        .get(&trip.pattern)
                        .map(|p| p.name.clone()),
                    direction_id: None,
                    block_id: block_of
                        .get(&(service_id.clone(), trip.start_secs))
                        .cloned(),
                    // The pattern's authored alignment, when there is
                    // one; None keeps the stop-to-stop fallback.
                    shape_id: net
                        .patterns
                        .get(&trip.pattern)
                        .and_then(|p| p.path.as_ref())
                        .filter(|path| path.len() >= 2)
                        .map(|_| trip.pattern.clone()),
                },
            );
            for (i, st) in trip.stop_times.iter().enumerate() {
                stop_times.push(StopTime {
                    trip_id: trip_id.clone(),
                    stop_id: st.stop.clone(),
                    stop_sequence: (i + 1) as u32,
                    arrival_time: Some(st.arrival_secs),
                    departure_time: Some(st.departure_secs),
                    stop_headsign: None,
                });
            }
        }
    }

    let services = ServiceCalendar::new(Vec::new(), calendar_dates);
    let feed_info = Some(FeedInfo {
        feed_id: None,
        feed_version: None,
        feed_publisher_name: Some(net.name.clone()),
        feed_publisher_url: None,
        feed_lang: None,
    });

    Ok(Gtfs::from_parts(
        agencies,
        routes,
        stops,
        trips,
        stop_times,
        services,
        shapes,
        feed_info,
    ))
}

/// Every date a rule runs on: walked day by day across its ranges
/// (so per-date exceptions and weekday flags apply), plus any `Added`
/// exception outside the ranges.
fn running_dates(rule: &CalendarRule) -> BTreeSet<NaiveDate> {
    let mut dates = BTreeSet::new();
    for range in &rule.ranges {
        let mut date = range.start;
        while date <= range.end {
            if rule.runs_on(date) {
                dates.insert(date);
            }
            date += Duration::days(1);
        }
    }
    for exc in &rule.exceptions {
        if exc.state == DayState::Added {
            dates.insert(exc.date);
        }
    }
    dates
}

/// Derived-view route type. The model's `Mode::Other` has no GTFS
/// code; bus is the least-wrong default for display purposes.
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

// ---------------------------------------------------------------------
// The other direction: infer an authored network document from a
// parsed GTFS feed (the bootstrap path — a planner starts from the
// network the agency already runs).
// ---------------------------------------------------------------------

/// Infer a [`Network`] from a parsed feed. Deliberately lossy, in
/// documented ways:
///
/// * trips cluster into patterns by (route, direction, stop
///   sequence); the cluster's earliest trip provides the pattern's
///   single timing profile, so run-time variation between trips of
///   one pattern is not preserved;
/// * every stop becomes a timepoint — exact times, no interpolation;
/// * stop distances are fabricated (index × 500 m): GTFS has none;
/// * single-stop trips and trips without a first departure are
///   dropped (the model requires two stops and a start);
/// * blocks keep their trip chains but carry no layovers (GTFS has
///   no layover data — the planner authors those);
/// * the pattern's alignment comes from the member trips' richest
///   `shapes.txt` polyline (ties: lowest trip id) — variation between
///   trips of one pattern is not preserved;
/// * stops without coordinates (malformed per the GTFS spec) drop
///   out, along with every trip that serves them: the model cannot
///   represent a place at (0, 0) and silently mapping one there
///   poisons every map view.
///
/// Deterministic: every iteration is sorted, so the same feed yields
/// the same document on every host.
pub fn gtfs_to_network(gtfs: &Gtfs) -> Network {
    let mut net = Network {
        name: gtfs
            .agencies
            .first()
            .map(|a| a.agency_name.clone())
            .unwrap_or_else(|| "Imported network".into()),
        anchor: Anchor::NoonMinusTwelve,
        stops: Default::default(),
        routes: Default::default(),
        patterns: Default::default(),
        calendars: Default::default(),
        services: Default::default(),
        blocks: Default::default(),
    };

    for (id, s) in &gtfs.stops {
        // Missing coordinates are a spec violation; importing a stop
        // at (0, 0) would place it in the Gulf of Guinea on every map.
        let (Some(lat), Some(lon)) = (s.stop_lat, s.stop_lon) else {
            continue;
        };
        net.stops.insert(
            id.clone(),
            transit_network::Stop {
                name: s.stop_name.clone(),
                lat,
                lon,
                parent: s.parent_station.clone(),
            },
        );
    }
    for (id, r) in &gtfs.routes {
        net.routes.insert(
            id.clone(),
            transit_network::Route {
                name: r.display_name(),
                mode: mode_from_gtfs_type(r.route_type),
                color: r.route_color.clone(),
            },
        );
    }

    // Kept trips, in trip-id order (determinism: gtfs.trips is a
    // HashMap, its order must not leak).
    struct Kept<'a> {
        trip_id: &'a str,
        route_id: String,
        service_id: String,
        block_id: Option<String>,
        start_secs: u32,
        times: Vec<u32>,     // cumulative from start, one per stop
        seq: Vec<String>,    // stop ids
        pattern_id: String,   // filled in during clustering
    }
    let mut kept: Vec<Kept> = Vec::new();
    for trip_id in {
        let mut ids: Vec<&String> = gtfs.trips.keys().collect();
        ids.sort();
        ids
    } {
        let trip = &gtfs.trips[trip_id];
        let Some(sts) = gtfs.trip_stop_times(trip_id) else { continue };
        if sts.len() < 2 {
            continue; // the model requires two stops; single-stop
        } // trips (e.g. one-stop shuttles) cannot be represented
        if sts.iter().any(|st| !net.stops.contains_key(&st.stop_id)) {
            continue; // serves a stop the import dropped (no coords)
        }
        let Some(start) = sts[0].departure_time.or(sts[0].arrival_time) else {
            continue;
        };
        let mut times = Vec::with_capacity(sts.len());
        let mut prev = 0u32;
        for st in &sts {
            let t = st
                .arrival_time
                .or(st.departure_time)
                .map(|t| t.saturating_sub(start))
                .unwrap_or(prev)
                .max(prev); // clamp: GTFS rows can be non-monotonic
            prev = t;
            times.push(t);
        }
        kept.push(Kept {
            trip_id,
            route_id: trip.route_id.clone(),
            service_id: trip.service_id.clone(),
            block_id: trip.block_id.clone(),
            start_secs: start,
            times,
            seq: sts.iter().map(|st| st.stop_id.clone()).collect(),
            pattern_id: String::new(),
        });
    }

    // One timing profile covers every kept start (computed up front).
    let day_end = kept
        .iter()
        .map(|t| t.start_secs)
        .max()
        .map(|m| (m + 1).max(30 * 3600))
        .unwrap_or(30 * 3600);

    // Cluster by (route, direction, stop sequence). BTreeMap order is
    // the pattern id order: deterministic everywhere.
    let mut clusters: BTreeMap<(String, String, Vec<String>), Vec<usize>> = BTreeMap::new();
    for (i, t) in kept.iter().enumerate() {
        let dir = gtfs
            .trips
            .get(t.trip_id)
            .and_then(|tr| tr.direction_id)
            .map(|d| d.to_string())
            .unwrap_or_else(|| "n".into());
        clusters
            .entry((t.route_id.clone(), dir, t.seq.clone()))
            .or_default()
            .push(i);
    }

    let mut pattern_seq: BTreeMap<(String, String), usize> = BTreeMap::new();
    for ((route_id, dir, seq), members) in &clusters {
        let index = pattern_seq.entry((route_id.clone(), dir.clone())).or_default();
        let pattern_id = format!("pat-{route_id}-{dir}-{index}");
        *index += 1;

        // The earliest trip's times become the profile (min by start,
        // ties by trip id — deterministic).
        let earliest = members
            .iter()
            .map(|&i| &kept[i])
            .min_by_key(|t| (t.start_secs, t.trip_id))
            .expect("clusters are non-empty");
        let last_stop_name = seq
            .last()
            .and_then(|s| gtfs.stops.get(s))
            .map(|s| s.stop_name.as_str())
            .unwrap_or(seq.last().map(String::as_str).unwrap_or(""));
        let route_name = gtfs
            .routes
            .get(route_id)
            .map(|r| r.display_name())
            .unwrap_or_else(|| route_id.clone());

        // The alignment: the member trips' richest shapes.txt
        // polyline. Members are in trip-id order, so ties keep the
        // lowest id — deterministic.
        let mut path: Option<Vec<Coord>> = None;
        let mut best = 0usize;
        for &i in members {
            let Some(shape_id) = gtfs
                .trips
                .get(kept[i].trip_id)
                .and_then(|t| t.shape_id.as_deref())
            else {
                continue;
            };
            let Some(points) = gtfs.shapes.get(shape_id) else { continue };
            if points.len() > best {
                best = points.len();
                path = Some(
                    points
                        .iter()
                        .map(|p| Coord { lat: p.shape_pt_lat, lon: p.shape_pt_lon })
                        .collect(),
                );
            }
        }

        net.patterns.insert(
            pattern_id.clone(),
            Pattern {
                route: route_id.clone(),
                name: format!("{route_name} → {last_stop_name}"),
                path,
                stops: seq
                    .iter()
                    .enumerate()
                    .map(|(i, s)| PatternStop {
                        stop: s.clone(),
                        timepoint: true,
                        distance_m: i as f64 * 500.0,
                    })
                    .collect(),
                timing: vec![TimingProfile {
                    start_secs: 0,
                    end_secs: day_end,
                    at_timepoints: earliest.times.clone(),
                }],
            },
        );

        for &i in members {
            kept[i].pattern_id = pattern_id.clone();
        }

        // One service per (GTFS service key, pattern): exact starts.
        let mut by_service: BTreeMap<&str, Vec<u32>> = BTreeMap::new();
        for &i in members {
            by_service
                .entry(kept[i].service_id.as_str())
                .or_default()
                .push(kept[i].start_secs);
        }
        for (sid, starts) in by_service {
            net.services.insert(
                format!("{sid}:{pattern_id}"),
                Service {
                    pattern: pattern_id.clone(),
                    calendar: sid.to_string(),
                    schedule: Schedule::ExactTimes { starts },
                },
            );
        }
    }

    // Calendars for every referenced service key, in id order. Block
    // trips reference the same service keys, so nothing is missed.
    let mut sids: BTreeSet<String> = net.services.values().map(|s| s.calendar.clone()).collect();
    for sid in &sids {
        let mut days = WeekdaySet::none();
        let mut ranges = Vec::new();
        if let Some(c) = gtfs.services.regular.get(sid) {
            days = WeekdaySet {
                mon: c.monday,
                tue: c.tuesday,
                wed: c.wednesday,
                thu: c.thursday,
                fri: c.friday,
                sat: c.saturday,
                sun: c.sunday,
            };
            ranges.push(DateRange { start: c.start_date, end: c.end_date });
        }
        // Exceptions: HashMap order must not leak — sort by date.
        let mut exceptions: Vec<CalendarException> = gtfs
            .services
            .exceptions
            .iter()
            .filter(|((s, _), _)| s == sid)
            .map(|((_, date), &kind)| CalendarException {
                date: *date,
                state: if kind == 1 { DayState::Added } else { DayState::Removed },
            })
            .collect();
        exceptions.sort_by_key(|e| e.date);
        net.calendars.insert(
            sid.clone(),
            CalendarRule { days, ranges, exceptions },
        );
    }

    // Runcut: trip chains per block_id, chronological, no layovers.
    let mut blocks: BTreeMap<&str, Vec<&Kept>> = BTreeMap::new();
    for t in &kept {
        if let Some(b) = &t.block_id {
            blocks.entry(b.as_str()).or_default().push(t);
        }
    }
    for (bid, trips) in blocks {
        let mut refs: Vec<TripRef> = trips
            .iter()
            .map(|t| TripRef {
                service: format!("{}:{}", t.service_id, t.pattern_id),
                start_secs: t.start_secs,
            })
            .collect();
        refs.sort_by(|a, b| a.start_secs.cmp(&b.start_secs).then_with(|| a.service.cmp(&b.service)));
        net.blocks.insert(bid.to_string(), Block { trips: refs, layovers: Vec::new() });
    }

    net
}

/// Reverse of the derived-view route table: GTFS codes → model modes.
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
