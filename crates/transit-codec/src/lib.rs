//! transit-codec — the network codec, as a shared library.
//!
//! The Layer A → Layer B bridge: an authored network document
//! (transit-network) expanded into the derived-feed view
//! (transit-core's `Gtfs`) that every downstream op — departures,
//! planning, blocks, geojson — already consumes, and the reverse
//! inference (GTFS → authored network) that is the planner's
//! bootstrap. The second crate extracted out of transit-ops for the
//! capability bus (after transit-analysis): transit-ops and the
//! cap-network module share this code — the same codec, gated once,
//! dispatched from two hosts.
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
//!   feed, exact for any authored rule including Added exceptions;
//! * transfer-point flags are authored model state with no single
//!   stop in the derived view to hang them on (GTFS declares
//!   transfers pairwise): they do not cross into the derived feed —
//!   the planner's document round-trips them.
//!
//! Shapes: a pattern's authored `path` becomes a `shapes.txt` entry
//! keyed by the pattern id, and every trip the pattern materializes
//! references it. Patterns without a path keep `shape_id: None` and
//! draw stop-to-stop lines (the base `trip_path` fallback).

use std::collections::{BTreeMap, BTreeSet, HashMap};

use chrono::{Duration, NaiveDate};

use ts_rs::TS;
use serde::Serialize;

use transit_core::{
    Agency, CalendarDate, FeedInfo, Gtfs, Route, ServiceCalendar, ShapePoint, Stop, StopTime, Trip,
};
use transit_network::{
    materialize_service, Anchor, Block, CalendarException, CalendarRule, Coord, DateRange, DayState,
    Mode, Network, Pattern, PatternSchedule, PatternStop, Schedule, ServiceType, TimingProfile, TripRef, WeekdaySet,
};


/// Expand a validated network document into a derived feed. Invalid
/// documents are rejected: the registry only ever holds feeds built
/// from documents that passed validation (cap-network's `validate`
/// op reports the full issue list for authoring UIs).
pub fn network_to_gtfs(net: &Network) -> Result<Gtfs, String> {
    if let Err(first) = net.validate() {
        let count = transit_network::validation_issues(net).len();
        return Err(format!(
            "invalid network: {first} ({count} issues total; the validate op returns all)"
        ));
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
            block_of.insert((trip.schedule.clone(), trip.start_secs), block_id.clone());
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

    for (schedule_id, schedule) in &net.schedules {
        // Service type → explicit running dates, exact for any rule.
        if let Some(service_type) = net.service_types.get(&schedule.service_type) {
            for date in running_dates(&service_type.rule) {
                calendar_dates.push(CalendarDate {
                    service_id: schedule_id.clone(),
                    date,
                    exception_type: 1,
                });
            }
        }
        for trip in materialize_service(net, schedule_id).map_err(|e| e.to_string())? {
            let trip_id = format!("{schedule_id}@{}", trip.start_secs);
            trips.insert(
                trip_id.clone(),
                Trip {
                    route_id: trip.route.clone(),
                    service_id: schedule_id.clone(),
                    trip_id: trip_id.clone(),
                    trip_headsign: net
                        .patterns
                        .get(&trip.pattern)
                        .map(|p| p.name.clone()),
                    direction_id: None,
                    block_id: block_of
                        .get(&(schedule_id.clone(), trip.start_secs))
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
        // transfers: the derived view carries none — transfer-point
        // flags are authored state (see the module docs).
        Vec::new(),
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

/// One resolution the import applied to a shape the model cannot
/// represent as-is. Resolutions are **reported, never silent** — the
/// planner needs to see what the bootstrap did to their feed.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ImportNote {
    /// The same derived trip was claimed by two blocks (two buses
    /// running the same trip); the lowest block id won.
    DuplicateBlockTrip {
        schedule: String,
        start_secs: u32,
        kept_block: String,
        dropped_block: String,
    },
    /// Two trips of one block depart at the same second — one
    /// vehicle cannot; the first by sort order stayed.
    SameSecondBlockTrips {
        block: String,
        start_secs: u32,
        dropped_schedule: String,
    },
    /// A trip of this block departs before the previous trip ends —
    /// the source data is operationally wrong; the impossible
    /// continuation was dropped.
    InfeasibleBlockChain {
        block: String,
        arrival_secs: u32,
        start_secs: u32,
        dropped_schedule: String,
    },
    /// Trips the model cannot represent at all, aggregated by
    /// reason (single-stop shuttles, no first-departure time,
    /// serving stops without coordinates).
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

/// What `gtfs_to_network` hands back: the document plus the notes
/// describing every resolution it applied.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct ImportOutcome {
    pub network: Network,
    pub notes: Vec<ImportNote>,
}

/// Infer a [`Network`] from a parsed feed. Deliberately lossy, in
/// documented ways (every loss that touches the planner's data is
/// reported in [`ImportOutcome::notes`], never silent):
///
/// * trips cluster into patterns by (route, direction, stop
///   sequence); the cluster's shortest-running trip provides the
///   pattern's single timing profile, so run-time variation between
///   trips of one pattern is not preserved (shortest, not earliest,
///   so materialized arrivals never overshoot any real trip's and
///   block chains stay feasible);
/// * every stop becomes a timepoint — exact times, no interpolation;
/// * stop distances are fabricated (index × 500 m): GTFS has none;
/// * single-stop trips and trips without a first departure are
///   dropped (the model requires two stops and a start);
/// * blocks keep their trip chains but carry no layovers (GTFS has
///   no layover data — the planner authors those); derived trips
///   claimed by two blocks and same-second trips of one block are
///   spec-legal but unrepresentable (the model's block chains are
///   strictly increasing) — the import keeps the first in
///   deterministic order and drops the rest;
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
pub fn gtfs_to_network(gtfs: &Gtfs) -> ImportOutcome {
    let mut notes: Vec<ImportNote> = Vec::new();
    let mut dropped_single_stop = 0u32;
    let mut dropped_no_departure = 0u32;
    let mut dropped_missing_stop_coords = 0u32;
    let mut net = Network {
        name: gtfs
            .agencies
            .first()
            .map(|a| a.agency_name.clone())
            .unwrap_or_else(|| "Imported network".into()),
        // A GTFS import is unversioned until the planner frames it
        // as a pick: `plan` defaults to no identity, no date.
        plan: Default::default(),
        anchor: Anchor::NoonMinusTwelve,
        stops: Default::default(),
        routes: Default::default(),
        patterns: Default::default(),
        service_types: Default::default(),
        schedules: Default::default(),
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
                transfer: false,
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
            dropped_single_stop += 1;
            continue; // the model requires two stops; single-stop
        } // trips (e.g. one-stop shuttles) cannot be represented
        if sts.iter().any(|st| !net.stops.contains_key(&st.stop_id)) {
            dropped_missing_stop_coords += 1;
            continue; // serves a stop the import dropped (no coords)
        }
        let Some(start) = sts[0].departure_time.or(sts[0].arrival_time) else {
            dropped_no_departure += 1;
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

        // The cluster's shortest-running trip provides the profile:
        // its last timepoint is the earliest end-of-trip arrival any
        // member can make, so block connections that hold in the
        // source feed still hold after the lossy collapse to one
        // runtime per pattern (a representative with a longer runtime
        // would push modeled arrivals past the next trip's start).
        // Ties: earliest start, then trip id — deterministic.
        let repr = members
            .iter()
            .map(|&i| &kept[i])
            .min_by_key(|t| (t.times.last().copied().unwrap_or(0), t.start_secs, t.trip_id))
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
                    at_timepoints: repr.times.clone(),
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
        for (sid, mut starts) in by_service {
            // The model requires exact-times starts sorted and
            // unique (trip identity is (service, start)); members
            // arrive in trip-id order, and real feeds even carry
            // duplicate starts.
            starts.sort_unstable();
            starts.dedup();
            net.schedules.insert(
                format!("{sid}:{pattern_id}"),
                PatternSchedule {
                    pattern: pattern_id.clone(),
                    service_type: sid.to_string(),
                    schedule: Schedule::ExactTimes { starts },
                },
            );
        }
    }

    // Calendars for every referenced service key, in id order. Block
    // trips reference the same service keys, so nothing is missed.
    let sids: BTreeSet<String> = net.schedules.values().map(|s| s.service_type.clone()).collect();
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
        net.service_types.insert(
            sid.clone(),
            ServiceType { name: sid.clone(), rule: CalendarRule { days, ranges, exceptions } },
        );
    }

    // Runcut: trip chains per block_id, chronological, no layovers.
    // Three shapes a spec-valid feed can still carry cannot be
    // represented as-is — the model's trip identity is (service,
    // start), its block chains are strictly increasing, and its
    // feasibility check is a hard invariant — so the import resolves
    // them, dropping the offender in deterministic order:
    //   * the same derived trip claimed by two blocks (two buses
    //     running the same trip) — the lowest block id wins;
    //   * two trips of one block at the same second (one vehicle
    //     cannot);
    //   * a trip departing before the previous trip of its block
    //     ends (the source data is operationally wrong; only a
    //     GENUINE source error can fire — modeled arrivals use the
    //     pattern's shortest runtime, so they never overshoot a
    //     real arrival).
    // Without this the law "import is valid by construction" fails
    // on legal feeds (the import proptest catches it).
    let mut blocks: BTreeMap<&str, Vec<&Kept>> = BTreeMap::new();
    for t in &kept {
        if let Some(b) = &t.block_id {
            blocks.entry(b.as_str()).or_default().push(t);
        }
    }
    let mut claimed: BTreeMap<(String, u32), String> = BTreeMap::new();
    for (bid, trips) in blocks {
        let mut refs: Vec<TripRef> = trips
            .iter()
            .map(|t| TripRef {
                schedule: format!("{}:{}", t.service_id, t.pattern_id),
                start_secs: t.start_secs,
            })
            .collect();
        refs.sort_by(|a, b| a.start_secs.cmp(&b.start_secs).then_with(|| a.schedule.cmp(&b.schedule)));
        let mut chain: Vec<TripRef> = Vec::with_capacity(refs.len());
        for r in refs {
            if chain.last().is_some_and(|last| last.start_secs == r.start_secs) {
                notes.push(ImportNote::SameSecondBlockTrips {
                    block: bid.to_string(),
                    start_secs: r.start_secs,
                    dropped_schedule: r.schedule.clone(),
                });
                continue; // one vehicle cannot run two trips at once
            }
            if let Some(kept_block) = claimed_clash(&claimed, &r) {
                notes.push(ImportNote::DuplicateBlockTrip {
                    schedule: r.schedule.clone(),
                    start_secs: r.start_secs,
                    kept_block: kept_block,
                    dropped_block: bid.to_string(),
                });
                continue; // this derived trip already belongs to a block
            }
            if let Some(last) = chain.last() {
                if let Ok(arrival) =
                    transit_network::trip_arrival_end_secs(&net, &last.schedule, last.start_secs)
                {
                    if arrival > r.start_secs {
                        notes.push(ImportNote::InfeasibleBlockChain {
                            block: bid.to_string(),
                            arrival_secs: arrival,
                            start_secs: r.start_secs,
                            dropped_schedule: r.schedule.clone(),
                        });
                        continue; // departs before the vehicle arrives: impossible
                    }
                }
            }
            claimed.insert((r.schedule.clone(), r.start_secs), bid.to_string());
            chain.push(r);
        }
        // A block whose every trip was a duplicate is not inserted:
        // the model has no empty blocks.
        if chain.is_empty() {
            continue;
        }
        net.blocks.insert(bid.to_string(), Block { trips: chain, layovers: Vec::new() });
    }

    // Transfer points: transfers.txt declares them (pairs; transfer
    // type 3 is "not possible" and never marks). The model keeps
    // the intent as a stop flag, but a flag only composes when at
    // least two routes meet across the stop's walking group —
    // otherwise it is reported, not applied, so the
    // valid-by-construction law holds (validation would reject it).
    {
        let index = transit_network::stop_route_index(&net);
        let transfer_stop_ids: BTreeSet<&str> = gtfs
            .transfers
            .iter()
            .filter(|t| t.transfer_type != Some(3))
            .flat_map(|t| [t.from_stop_id.as_str(), t.to_stop_id.as_str()])
            .collect();
        for stop_id in transfer_stop_ids {
            if !net.stops.contains_key(stop_id) {
                continue; // row references a stop the import dropped
            }
            let composes = transit_network::routes_meeting_at(&net, &index, stop_id).len() >= 2;
            if composes {
                net.stops.get_mut(stop_id).expect("checked above").transfer = true;
            } else {
                notes.push(ImportNote::UnusableTransferPoint { stop: stop_id.to_string() });
            }
        }
    }

    if dropped_single_stop + dropped_no_departure + dropped_missing_stop_coords > 0 {
        notes.push(ImportNote::DroppedTrips {
            single_stop: dropped_single_stop,
            no_departure: dropped_no_departure,
            missing_stop_coords: dropped_missing_stop_coords,
        });
    }
    ImportOutcome { network: net, notes }
}

/// The block that already claimed this derived trip, if any.
fn claimed_clash(
    claimed: &BTreeMap<(String, u32), String>,
    r: &TripRef,
) -> Option<String> {
    claimed
        .get(&(r.schedule.clone(), r.start_secs))
        .cloned()
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
