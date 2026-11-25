//! An rt trip id is a model trip id, and stop updates pair with the schedule
//! by stop_sequence (cap-rt/OPS.md, `trip_progress` and `live`). A frequency
//! trip's instance is the update's start_time: on the grid when the trip runs
//! exact times, anywhere in a band when it does not.

use std::collections::BTreeMap;

use proptest::prelude::*;
use transit_codec::Boundary;
use transit_live::OnTimeWindow;
use transit_network::{
    trip_instance, CalendarRule, DateRange, Departs, HeadwayBand, Mode, Network, Pattern, PatternStop,
    PatternTrips, Route, ServiceType, Stop, StopOffset, Trip, WeekdaySet,
};
use transit_rt::{live_day, trip_progress, RtOverview, StopUpdateView, TripUpdateView, VehicleView};

#[derive(Clone, Debug)]
enum GenDeparts {
    At(u32),
    Every(u32, u32, u32, bool),
}

#[derive(Clone, Debug)]
struct GenTrip {
    stops: Vec<usize>,
    legs: Vec<(u32, u32)>,
    departs: GenDeparts,
    sequences: Option<Vec<u32>>,
}

#[derive(Clone, Debug)]
struct GenUpdate {
    trip: usize,
    /// A frequency trip's update names an instance (0), a time inside a band
    /// off the grid (1), or a time outside every band (2).
    start_kind: u8,
    start_pick: u32,
    /// (stop index, delay, name it by sequence?)
    stops: Vec<(usize, i32, bool)>,
}

#[derive(Clone, Debug)]
struct Spec {
    trips: Vec<GenTrip>,
    updates: Vec<GenUpdate>,
}

fn gen_trip() -> impl Strategy<Value = GenTrip> {
    (2usize..=5).prop_flat_map(|n| {
        (
            proptest::collection::vec(0usize..5, n),
            proptest::collection::vec((60u32..600, 0u32..90), n),
            prop_oneof![
                (6u32 * 3600..20 * 3600).prop_map(|s| GenDeparts::At(s - s % 60)),
                (6u32 * 3600..9 * 3600, 1u32..3, 600u32..1800, any::<bool>())
                    .prop_map(|(s, h, w, exact)| GenDeparts::Every(s - s % 60, h * 3600, w - w % 60, exact)),
            ],
            proptest::option::of(proptest::collection::vec(1u32..20, n)),
        )
            .prop_map(|(mut stops, legs, departs, gaps)| {
                for i in 1..stops.len() {
                    if stops[i] == stops[i - 1] {
                        stops[i] = (stops[i] + 1) % 5;
                    }
                }
                let sequences = gaps.map(|g| {
                    let mut seq = 0;
                    g.iter().map(|d| {
                        seq += d;
                        seq
                    })
                    .collect()
                });
                GenTrip { stops, legs, departs, sequences }
            })
    })
}

fn spec_strategy() -> impl Strategy<Value = Spec> {
    proptest::collection::vec(gen_trip(), 1..=4).prop_flat_map(|trips| {
        let n = trips.len();
        let update = (
            0..n + 1,
            0u8..3,
            any::<u32>(),
            proptest::collection::vec((0usize..5, -120i32..900, any::<bool>()), 0..=3),
        )
            .prop_map(|(trip, start_kind, start_pick, stops)| GenUpdate { trip, start_kind, start_pick, stops });
        (Just(trips), proptest::collection::vec(update, 0..=4)).prop_map(|(trips, updates)| Spec { trips, updates })
    })
}

fn offsets(t: &GenTrip) -> Vec<(u32, u32)> {
    let mut at = 0u32;
    t.legs
        .iter()
        .enumerate()
        .map(|(i, &(leg, dwell))| {
            if i > 0 {
                at += leg;
            }
            let arrival = at;
            at += dwell;
            (arrival, at)
        })
        .collect()
}

fn build(spec: &Spec) -> (Network, Boundary) {
    let mut net = Network { name: "Generated".into(), ..Default::default() };
    for i in 0..5 {
        net.stops.insert(format!("s{i}"), Stop { name: format!("Stop {i}"), lat: 45.5, lon: -122.6 + i as f64 * 0.01, ..Default::default() });
    }
    net.routes.insert("r".into(), Route { name: "R".into(), mode: Mode::Bus, ..Default::default() });
    net.service_types.insert(
        "wk".into(),
        ServiceType {
            name: "Weekday".into(),
            rule: CalendarRule {
                days: WeekdaySet::weekdays(),
                ranges: vec![DateRange {
                    start: chrono::NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
                    end: chrono::NaiveDate::from_ymd_opt(2026, 12, 31).unwrap(),
                    extra: Default::default(),
                }],
                exceptions: vec![],
                extra: Default::default(),
            },
            extra: Default::default(),
        },
    );
    let mut boundary = Boundary::default();
    for (i, t) in spec.trips.iter().enumerate() {
        let p = format!("p{i}");
        net.patterns.insert(
            p.clone(),
            Pattern {
                route: "r".into(),
                name: p.clone(),
                stops: t
                    .stops
                    .iter()
                    .enumerate()
                    .map(|(k, s)| PatternStop { stop: format!("s{s}"), timepoint: true, distance_m: k as f64 * 300.0, ..Default::default() })
                    .collect(),
                ..Default::default()
            },
        );
        let departs = match t.departs {
            GenDeparts::At(start_secs) => Departs::At { start_secs },
            GenDeparts::Every(s, len, h, exact) => Departs::Every {
                bands: vec![HeadwayBand { start_secs: s, end_secs: s + len, headway_secs: h, extra: Default::default() }],
                exact,
            },
        };
        net.pattern_trips.insert(
            format!("wk:{p}"),
            PatternTrips {
                pattern: p,
                service_type: "wk".into(),
                trips: vec![Trip {
                    id: format!("trip {i}"),
                    departs,
                    times: offsets(t)
                        .into_iter()
                        .map(|(a, d)| StopOffset { arrival_secs: Some(a), departure_secs: Some(d) })
                        .collect(),
                    ..Default::default()
                }],
                extra: Default::default(),
            },
        );
        if let Some(seq) = &t.sequences {
            boundary.stop_sequences.insert(format!("trip {i}"), seq.clone());
        }
    }
    (net, boundary)
}

fn hms(secs: u32) -> String {
    format!("{:02}:{:02}:{:02}", secs / 3600, secs / 60 % 60, secs % 60)
}

fn expected_start(spec: &Spec, u: &GenUpdate) -> Option<u32> {
    let t = spec.trips.get(u.trip)?;
    match t.departs {
        GenDeparts::At(s) => Some(s),
        GenDeparts::Every(s, len, h, exact) => {
            let picked = picked_start(t, u)?;
            let in_band = s <= picked && picked < s + len;
            let on_grid = (picked - s) % h == 0;
            (in_band && (on_grid || !exact)).then_some(picked)
        }
    }
}

fn picked_start(t: &GenTrip, u: &GenUpdate) -> Option<u32> {
    let GenDeparts::Every(s, len, h, _) = t.departs else { return None };
    let slots = len.div_ceil(h);
    Some(match u.start_kind {
        0 => s + (u.start_pick % slots) * h,
        1 => s + (u.start_pick % len) / 60 * 60,
        _ => s + len + (u.start_pick % 3600) / 60 * 60,
    })
}

fn sequence(t: &GenTrip, k: usize) -> u32 {
    t.sequences.as_ref().map_or(k as u32 + 1, |s| s[k])
}

fn overview(spec: &Spec) -> RtOverview {
    let trips: Vec<TripUpdateView> = spec
        .updates
        .iter()
        .map(|u| {
            let t = spec.trips.get(u.trip);
            TripUpdateView { start_date: None,
                trip_id: Some(match t {
                    Some(_) => format!("trip {}", u.trip),
                    None => "nobody's trip".into(),
                }),
                route_id: None,
                start_time: t.and_then(|t| picked_start(t, u)).map(hms),
                canceled: false,
                stop_updates: t
                    .map(|t| {
                        let mut named: Vec<(usize, i32, bool)> =
                            u.stops.iter().filter(|(k, ..)| *k < t.stops.len()).cloned().collect();
                        named.sort_by_key(|(k, ..)| *k);
                        named.dedup_by_key(|(k, ..)| *k);
                        named
                            .into_iter()
                            .map(|(k, delay, by_sequence)| {
                                // A stop the trip visits twice can only be
                                // named by its sequence.
                                let unique = t.stops.iter().filter(|s| **s == t.stops[k]).count() == 1;
                                let by_sequence = by_sequence || !unique;
                                StopUpdateView {
                                    stop_id: (!by_sequence).then(|| format!("s{}", t.stops[k])),
                                    stop_sequence: by_sequence.then(|| sequence(t, k)),
                                    arrival_delay: Some(delay),
                                    departure_delay: Some(delay),
                                    skipped: false,
                                }
                            })
                            .collect()
                    })
                    .unwrap_or_default(),
            }
        })
        .collect();
    RtOverview {
        gtfs_realtime_version: None,
        timestamp: None,
        entities: trips.len(),
        trip_updates: trips.len(),
        vehicles: 0,
        alerts: 0,
        trips,
        vehicle_positions: vec![],
        service_alerts: vec![],
    }
}

fn expected_delays(t: &GenTrip, u: &GenUpdate) -> Vec<Option<i32>> {
    let mut stated: BTreeMap<usize, i32> = BTreeMap::new();
    for (k, delay, _) in &u.stops {
        if *k < t.stops.len() {
            stated.entry(*k).or_insert(*delay);
        }
    }
    let mut carried = None;
    (0..t.stops.len())
        .map(|k| {
            if let Some(d) = stated.get(&k) {
                carried = Some(*d);
            }
            carried
        })
        .collect()
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn a_trip_instance_is_its_start_plus_its_times(spec in spec_strategy(), pick in any::<u32>()) {
        let (net, _) = build(&spec);
        for (i, t) in spec.trips.iter().enumerate() {
            let id = format!("trip {i}");
            let u = GenUpdate { trip: i, start_kind: (pick % 3) as u8, start_pick: pick, stops: vec![] };
            let start = match t.departs {
                GenDeparts::At(s) => s,
                _ => picked_start(t, &u).unwrap(),
            };
            let got = trip_instance(&net, &id, start);
            prop_assert_eq!(got.is_some(), expected_start(&spec, &u) == Some(start), "{} at {}", id, start);
            if let Some(m) = got {
                prop_assert_eq!(m.start_secs, start);
                let want: Vec<(u32, u32)> = offsets(t).iter().map(|(a, d)| (start + a, start + d)).collect();
                let have: Vec<(u32, u32)> = m.stop_times.iter().map(|s| (s.arrival_secs, s.departure_secs)).collect();
                prop_assert_eq!(have, want);
            }
        }
    }

    /// A frequency trip with no update names no instance and answers none.
    #[test]
    fn progress_pairs_by_the_publishers_sequence(spec in spec_strategy()) {
        let (net, boundary) = build(&spec);
        let rt = overview(&spec);
        for (i, t) in spec.trips.iter().enumerate() {
            if spec.updates.iter().any(|u| u.trip == i) {
                continue;
            }
            let p = trip_progress(&net, &boundary, &rt, &format!("trip {i}"));
            match t.departs {
                GenDeparts::At(start) => {
                    let p = p.ok_or_else(|| TestCaseError::fail("a one-start trip answers its schedule"))?;
                    prop_assert_eq!(p.start_secs, start);
                    prop_assert!(p.stops.iter().all(|s| s.departure_delay.is_none() && s.arrival_delay.is_none()));
                }
                GenDeparts::Every(..) => prop_assert!(p.is_none()),
            }
        }
        for (n, u) in spec.updates.iter().enumerate() {
            let Some(t) = spec.trips.get(u.trip) else { continue };
            // One update per trip in this snapshot: the first wins.
            if spec.updates[..n].iter().any(|v| v.trip == u.trip) {
                continue;
            }
            let p = trip_progress(&net, &boundary, &rt, &format!("trip {}", u.trip));
            let Some(start) = expected_start(&spec, u) else {
                prop_assert!(p.is_none(), "an update naming no instance resolves to none");
                continue;
            };
            let p = p.ok_or_else(|| TestCaseError::fail("a resolvable update has progress"))?;
            prop_assert_eq!(p.start_secs, start);
            let delays: Vec<Option<i32>> = p.stops.iter().map(|s| s.departure_delay).collect();
            prop_assert_eq!(&delays, &expected_delays(t, u));
            for (k, s) in p.stops.iter().enumerate() {
                let scheduled = start + offsets(t)[k].1;
                prop_assert_eq!(s.stop_sequence, sequence(t, k));
                prop_assert_eq!(s.scheduled_departure, Some(scheduled));
                let predicted = delays[k].map_or(scheduled, |d| (scheduled as i64 + d as i64).max(0) as u32);
                prop_assert_eq!(s.predicted_departure, Some(predicted));
            }
        }
    }

    #[test]
    fn the_live_document_speaks_in_trip_instances(spec in spec_strategy()) {
        let (net, boundary) = build(&spec);
        let rt = overview(&spec);
        let day = live_day(&net, &boundary, Some(&rt), None, None, OnTimeWindow::default());
        let resolved: Vec<(String, u32)> = spec
            .updates
            .iter()
            .filter_map(|u| expected_start(&spec, u).map(|s| (format!("trip {}", u.trip), s)))
            .collect();
        let got: Vec<(String, u32)> = day.trips.iter().map(|t| (t.trip.clone(), t.start_secs)).collect();
        prop_assert_eq!(&got, &resolved);
        prop_assert_eq!(day.unmatched_trips as usize, spec.updates.len() - resolved.len());
        for (trip, start) in &resolved {
            let i: usize = trip[5..].parse().unwrap();
            let t = &spec.trips[i];
            let first = format!("s{}", t.stops[0]);
            let entry = day.trips.iter().find(|x| &x.trip == trip && x.start_secs == *start).unwrap();
            if entry.stops.iter().any(|s| s.stop == first) {
                prop_assert!(day.visit(trip, *start, &first, start + offsets(t)[0].1).is_some());
            }
        }
    }

    #[test]
    fn a_vehicle_rides_its_trip_instance(spec in spec_strategy()) {
        let (net, boundary) = build(&spec);
        let mut rt = overview(&spec);
        rt.vehicle_positions = rt
            .trips
            .iter()
            .enumerate()
            .map(|(n, u)| VehicleView { start_date: None,
                entity_id: format!("v{n}"),
                trip_id: u.trip_id.clone(),
                start_time: u.start_time.clone(),
                route_id: None,
                lat: 45.5,
                lon: -122.6,
                bearing: None,
                stop_id: None,
                vehicle_id: Some(format!("bus {n}")),
                label: None,
            })
            .collect();
        let day = live_day(&net, &boundary, None, Some(&rt), None, OnTimeWindow::default());
        for (v, u) in day.vehicles.iter().zip(&spec.updates) {
            let want = expected_start(&spec, u).map(|s| (format!("trip {}", u.trip), s));
            prop_assert_eq!(v.trip.clone().zip(v.start_secs), want);
        }
    }
}
