//! The trip laws: a trip is a stored record — its own id, its own
//! departure, its own times — and the pattern's timing profile is
//! the planner's tool for producing times, never the schedule.
//!
//! Written BEFORE the implementation, as the definition of the
//! behavior; every expected answer is recomputed here from the
//! generated input, with no code shared with the implementation.
//!
//! The API these laws require (the contract the model must meet):
//!
//! * `Trip { id: TripId, departs: Departs, times: Vec<StopOffset>, .. }`
//!   (`Default`), stored in `PatternTrips { pattern, service_type,
//!   trips: Vec<Trip> }`.
//! * `Departs::At { start_secs }` — one trip; `Departs::Every { bands,
//!   exact }` — a frequency trip, one instance per band slot, all
//!   sharing the trip's id and times.
//! * `StopOffset { arrival_secs: Option<u32>, departure_secs:
//!   Option<u32> }` — seconds after the instance's start, one per
//!   pattern stop; `None` = not scheduled (approximate). The first
//!   and last stops carry both.
//! * `TripRef { trip: TripId }` — a block's reference, as GTFS has
//!   it: a block runs a trip whole, so a frequency trip in a block is
//!   every one of its instances, back to back.
//! * `MaterializedTrip.trip: TripId`.
//! * `read_network(&str)` reads current AND pre-trip documents
//!   (`departures` + `(pattern_trips, start_secs)` block refs).
//! * `Default` on `Network`, `Stop`, `Route`, `Pattern`,
//!   `PatternStop`, `Trip`, `StopOffset` (new fields keep arriving).
//! * Edits: `RetimeTrip { trip, to_secs }`, `AddTrip { pattern_trips,
//!   id, start_secs }`, `RemoveTrip { trip }`, `AdjustRunningTime {
//!   route, direction, service_type, from_secs, to_secs, delta_secs }`,
//!   and `SetHeadway` as it stands.

use std::collections::{BTreeMap, BTreeSet};

use chrono::NaiveDate;
use proptest::prelude::*;
use transit_network::{
    apply_edits, materialize_service, read_network, validation_issues, Block, CalendarRule, DateRange,
    Departs, HeadwayBand, Layover, MaterializedTrip, Mode, Network, NetworkEdit, Pattern, PatternStop,
    PatternTrips, Route, ServiceType, Stop, StopOffset, TimingProfile, Trip, TripRef, WeekdaySet,
};

// ===================================================================
// Generator: valid by construction.
// ===================================================================

/// One generated trip before ids: when it departs and its times.
#[derive(Clone, Debug)]
struct GenTrip {
    pattern: usize,
    saturday: bool,
    /// `None` = a frequency trip (`every` holds its band).
    start_secs: Option<u32>,
    every: (u32, u32, u32, bool),
    /// Per stop: (leg seconds to reach it, dwell seconds, which
    /// times are scheduled: 0 both, 1 arrival only, 2 departure only,
    /// 3 neither). Timepoint stops — the first and last always among
    /// them — carry both, as GTFS requires of an exact row.
    rows: Vec<(u32, u32, u8)>,
}

#[derive(Clone, Debug)]
struct Spec {
    /// Each pattern: its stop indices (distinct, 2..=5) and per-stop
    /// timepoint flags.
    patterns: Vec<(Vec<usize>, Vec<bool>)>,
    trips: Vec<GenTrip>,
    /// Starts come from a small pool, so duplicates (two vehicles on
    /// one trip) are common.
    with_blocks: bool,
}

fn spec_strategy() -> impl Strategy<Value = Spec> {
    let pattern = (2usize..=5).prop_flat_map(|n| {
        (
            proptest::sample::subsequence((0..6).collect::<Vec<usize>>(), n),
            proptest::collection::vec(any::<bool>(), n),
        )
    });
    (proptest::collection::vec(pattern, 1..=2), any::<bool>()).prop_flat_map(|(patterns, with_blocks)| {
        let shapes: Vec<usize> = patterns.iter().map(|(stops, _)| stops.len()).collect();
        let trip = (0..patterns.len()).prop_flat_map(move |p| {
            let n = shapes[p];
            (
                Just(p),
                any::<bool>(),
                proptest::option::weighted(0.85, (5u32 * 3600..26 * 3600).prop_map(|s| s - s % 600)),
                (6u32 * 3600..10 * 3600, 3600u32..4 * 3600, 600u32..1800, any::<bool>()),
                proptest::collection::vec((60u32..900, 0u32..120, 0u8..4), n),
            )
                .prop_map(|(pattern, saturday, start_secs, (s, len, h, exact), rows)| GenTrip {
                    pattern,
                    saturday,
                    start_secs,
                    every: (s - s % 60, s - s % 60 + len, h - h % 60, exact),
                    rows,
                })
        });
        (Just(patterns), proptest::collection::vec(trip, 0..=8), Just(with_blocks))
            .prop_map(|(patterns, trips, with_blocks)| Spec { patterns, trips, with_blocks })
    })
}

fn offsets(rows: &[(u32, u32, u8)], timepoints: &[bool]) -> Vec<StopOffset> {
    let last = rows.len() - 1;
    let mut t = 0u32;
    rows.iter()
        .enumerate()
        .map(|(i, &(leg, dwell, shown))| {
            if i > 0 {
                t += leg;
            }
            let arrival = t;
            let dwell = if i == 0 || i == last { dwell % 2 * 60 } else { dwell };
            t += dwell;
            let shown = if i == 0 || i == last || timepoints[i] { 0 } else { shown };
            StopOffset {
                arrival_secs: (shown == 0 || shown == 1).then_some(arrival),
                departure_secs: (shown == 0 || shown == 2).then_some(t),
            }
        })
        .collect()
}

/// The end of an instance: arrival at the last stop.
fn end_offset(trip: &Trip) -> u32 {
    trip.times.last().and_then(|o| o.arrival_secs).expect("last stop carries an arrival")
}

fn build(spec: &Spec) -> Network {
    let mut net = Network { name: "Generated".into(), ..Default::default() };
    for i in 0..6 {
        net.stops.insert(
            format!("s{i}"),
            Stop { name: format!("Stop {i}"), lat: 45.5 + i as f64 * 0.01, lon: -122.6, ..Default::default() },
        );
    }
    net.routes.insert("r0".into(), Route { mode: Mode::Bus, ..Default::default() });
    for (p, (stops, flags)) in spec.patterns.iter().enumerate() {
        let n = stops.len();
        let timepoints = (0..n).filter(|&i| flags[i] || i == 0 || i == n - 1).count() as u32;
        net.patterns.insert(
            format!("p{p}"),
            Pattern {
                route: "r0".into(),
                name: format!("Pattern {p}"),
                stops: stops
                    .iter()
                    .enumerate()
                    .map(|(i, s)| PatternStop {
                        stop: format!("s{s}"),
                        timepoint: flags[i] || i == 0 || i == n - 1,
                        distance_m: i as f64 * 400.0,
                        ..Default::default()
                    })
                    .collect(),
                // The planner's tool: a plain profile new trips are
                // produced from.
                timing: vec![TimingProfile {
                    start_secs: 0,
                    end_secs: 40 * 3600,
                    at_timepoints: (0..timepoints).map(|k| k * 300).collect(),
                }],
                ..Default::default()
            },
        );
    }
    let range = DateRange {
        start: NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
        end: NaiveDate::from_ymd_opt(2026, 12, 31).unwrap(),
    };
    let weekday = WeekdaySet::weekdays();
    let saturday = WeekdaySet { sat: true, ..WeekdaySet::none() };
    for (id, days) in [("wk", weekday), ("sa", saturday)] {
        net.service_types.insert(
            id.into(),
            ServiceType {
                name: id.into(),
                rule: CalendarRule { days, ranges: vec![range.clone()], exceptions: vec![] },
            },
        );
    }
    for (i, t) in spec.trips.iter().enumerate() {
        let timepoints: Vec<bool> = net.patterns[&format!("p{}", t.pattern)].stops.iter().map(|s| s.timepoint).collect();
        let service = if t.saturday { "sa" } else { "wk" };
        let key = format!("{service}:p{}", t.pattern);
        let departs = match t.start_secs {
            Some(start_secs) => Departs::At { start_secs },
            None => Departs::Every {
                bands: vec![HeadwayBand { start_secs: t.every.0, end_secs: t.every.1, headway_secs: t.every.2 }],
                exact: t.every.3,
            },
        };
        net.pattern_trips
            .entry(key)
            .or_insert_with(|| PatternTrips {
                pattern: format!("p{}", t.pattern),
                service_type: service.into(),
                trips: vec![],
            })
            .trips
            .push(Trip { id: format!("t{i}"), departs, times: offsets(&t.rows, &timepoints), ..Default::default() });
    }
    if spec.with_blocks {
        add_blocks(&mut net);
    }
    net
}

/// b0: the weekday one-start trips, chained greedily in start order
/// where the vehicle can make it. b1: a weekday frequency trip whole —
/// only when one vehicle can run its instances back to back. b2: a weekday and a Saturday trip at the same
/// minute — the two never share a day, so they cannot conflict.
fn add_blocks(net: &mut Network) {
    let mut weekday: Vec<(u32, u32, String)> = Vec::new();
    let mut saturday: Vec<(u32, String)> = Vec::new();
    let mut every: Option<String> = None;
    for pt in net.pattern_trips.values() {
        for trip in &pt.trips {
            match (&trip.departs, pt.service_type.as_str()) {
                (Departs::At { start_secs }, "wk") => weekday.push((*start_secs, start_secs + end_offset(trip), trip.id.clone())),
                (Departs::At { start_secs }, "sa") => saturday.push((*start_secs, trip.id.clone())),
                (Departs::Every { bands, .. }, "wk") if every.is_none() && bands[0].headway_secs >= end_offset(trip) => {
                    every = Some(trip.id.clone())
                }
                _ => {}
            }
        }
    }
    weekday.sort();
    let mut chain: Vec<(u32, u32, String)> = Vec::new();
    let mut layovers = Vec::new();
    for t in weekday.iter().cloned() {
        if let Some(last) = chain.last() {
            if t.0 < last.1 {
                continue;
            }
            layovers.push(Layover { after_trip: chain.len() - 1, secs: (t.0 - last.1).min(300), at: None });
        }
        chain.push(t);
    }
    let chained: BTreeSet<String> = if chain.len() >= 2 {
        let trips = chain.iter().map(|(_, _, id)| TripRef { trip: id.clone() }).collect();
        net.blocks.insert("b0".into(), Block { trips, layovers });
        chain.iter().map(|(_, _, id)| id.clone()).collect()
    } else {
        BTreeSet::new()
    };
    if let Some(id) = every {
        net.blocks.insert("b1".into(), Block { trips: vec![TripRef { trip: id }], layovers: vec![] });
    }
    // A trip is one vehicle's: b2 takes a weekday trip b0 does not run.
    let free = weekday.iter().find(|(_, _, id)| !chained.contains(id));
    if let (Some((_, _, wk)), Some((_, sa))) = (free, saturday.first()) {
        net.blocks.insert(
            "b2".into(),
            Block {
                trips: vec![
                    TripRef { trip: wk.clone() },
                    TripRef { trip: sa.clone() },
                ],
                layovers: vec![],
            },
        );
    }
}

fn net_strategy() -> impl Strategy<Value = Network> {
    spec_strategy().prop_map(|spec| build(&spec))
}

// ===================================================================
// Oracles.
// ===================================================================

/// Instance starts: a count, then an index — not a walk.
fn oracle_instances(departs: &Departs) -> Vec<u32> {
    match departs {
        Departs::At { start_secs } => vec![*start_secs],
        Departs::Every { bands, .. } => bands
            .iter()
            .flat_map(|b| {
                let n = if b.headway_secs == 0 {
                    0
                } else {
                    b.end_secs.saturating_sub(b.start_secs).div_ceil(b.headway_secs)
                };
                (0..n).map(move |k| b.start_secs + k * b.headway_secs)
            })
            .collect(),
    }
}

fn all_trips(net: &Network) -> Vec<(&str, &PatternTrips, &Trip)> {
    net.pattern_trips
        .iter()
        .flat_map(|(key, pt)| pt.trips.iter().map(move |t| (key.as_str(), pt, t)))
        .collect()
}

fn trip<'a>(net: &'a Network, id: &str) -> Option<(&'a str, &'a PatternTrips, &'a Trip)> {
    all_trips(net).into_iter().find(|(_, _, t)| t.id == id)
}

/// The law for one materialized instance: scheduled times are exact,
/// a one-sided row repeats its side, an unscheduled row sits between
/// its neighbors, and the whole trip runs forward.
fn check_instance(pattern: &Pattern, trip: &Trip, start: u32, got: &MaterializedTrip) -> Result<(), TestCaseError> {
    prop_assert_eq!(&got.trip, &trip.id);
    prop_assert_eq!(got.start_secs, start);
    prop_assert_eq!(got.stop_times.len(), pattern.stops.len(), "one stop time per pattern stop");
    for (i, (st, ps)) in got.stop_times.iter().zip(&pattern.stops).enumerate() {
        prop_assert_eq!(&st.stop, &ps.stop);
        prop_assert_eq!(st.timepoint, ps.timepoint);
        let o = &trip.times[i];
        match (o.arrival_secs, o.departure_secs) {
            (Some(a), Some(d)) => {
                prop_assert_eq!(st.arrival_secs, start + a, "trip {} stop {}: its own arrival", trip.id, i);
                prop_assert_eq!(st.departure_secs, start + d, "trip {} stop {}: its own departure", trip.id, i);
            }
            (Some(a), None) => {
                prop_assert_eq!((st.arrival_secs, st.departure_secs), (start + a, start + a));
            }
            (None, Some(d)) => {
                prop_assert_eq!((st.arrival_secs, st.departure_secs), (start + d, start + d));
            }
            (None, None) => {
                prop_assert_eq!(st.arrival_secs, st.departure_secs, "an unscheduled stop has one time");
            }
        }
    }
    for pair in got.stop_times.windows(2) {
        prop_assert!(pair[0].arrival_secs <= pair[0].departure_secs);
        prop_assert!(pair[0].departure_secs <= pair[1].arrival_secs, "the trip runs forward");
    }
    Ok(())
}

// ===================================================================
// The laws.
// ===================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// Every generated network validates — including duplicate starts,
    /// frequency trips, unscheduled stops, a block running a frequency
    /// trip whole, and a block whose trips never share a day.
    #[test]
    fn generated_networks_validate(net in net_strategy()) {
        let issues = validation_issues(&net);
        prop_assert!(issues.is_empty(), "{:?}", issues);
    }

    /// The document round-trips through JSON, and `read_network` reads
    /// the current shape as itself.
    #[test]
    fn documents_round_trip(net in net_strategy()) {
        let json = serde_json::to_string(&net).unwrap();
        prop_assert_eq!(&serde_json::from_str::<Network>(&json).unwrap(), &net);
        prop_assert_eq!(&read_network(&json).expect("current documents read"), &net);
    }

    /// Materialization IS the stored trips: one instance per start
    /// (two trips at one minute are two trips), each carrying its
    /// trip's id and its own times.
    #[test]
    fn materialize_answers_the_stored_trips(net in net_strategy()) {
        for (key, pt) in &net.pattern_trips {
            let pattern = &net.patterns[&pt.pattern];
            let got = materialize_service(&net, key).expect("valid documents materialize");
            let mut expected: Vec<(String, u32)> = pt
                .trips
                .iter()
                .flat_map(|t| oracle_instances(&t.departs).into_iter().map(move |s| (t.id.clone(), s)))
                .collect();
            let mut answered: Vec<(String, u32)> = got.iter().map(|m| (m.trip.clone(), m.start_secs)).collect();
            expected.sort();
            answered.sort();
            prop_assert_eq!(&answered, &expected, "{}: the instances", key);
            for m in &got {
                let t = pt.trips.iter().find(|t| t.id == m.trip).expect("instance of a stored trip");
                check_instance(pattern, t, m.start_secs, m)?;
            }
        }
    }

    /// A pattern's timing profile is a production tool: rewriting it
    /// changes no stored trip's materialized times.
    #[test]
    fn the_profile_does_not_govern_stored_trips(net in net_strategy(), scale in 2u32..5) {
        let mut retimed = net.clone();
        for pattern in retimed.patterns.values_mut() {
            for profile in &mut pattern.timing {
                for t in &mut profile.at_timepoints {
                    *t *= scale;
                }
            }
        }
        for key in net.pattern_trips.keys() {
            prop_assert_eq!(
                materialize_service(&net, key).unwrap(),
                materialize_service(&retimed, key).unwrap()
            );
        }
    }

    /// Each trip-level invariant, broken alone, fails validation.
    #[test]
    fn single_trip_mutations_break_validation(net in net_strategy(), pick in any::<prop::sample::Index>()) {
        let trips = all_trips(&net);
        prop_assume!(!trips.is_empty());
        let (key, _, chosen) = trips[pick.index(trips.len())];
        let (key, id) = (key.to_string(), chosen.id.clone());
        let mutate = |f: &dyn Fn(&mut Network)| { let mut n = net.clone(); f(&mut n); n };
        fn trip_mut<'a>(n: &'a mut Network, key: &str, id: &str) -> &'a mut Trip {
            n.pattern_trips.get_mut(key).unwrap().trips.iter_mut().find(|t| t.id == id).unwrap()
        }

        let broken = [
            // Two trips with one id.
            mutate(&|n| {
                let copy = trip_mut(n, &key, &id).clone();
                n.pattern_trips.get_mut(&key).unwrap().trips.push(copy);
            }),
            // Times for a stop the pattern does not have.
            mutate(&|n| trip_mut(n, &key, &id).times.push(StopOffset::default())),
            // The first stop unscheduled.
            mutate(&|n| trip_mut(n, &key, &id).times[0] = StopOffset::default()),
            // A timepoint stop with one time: GTFS requires both on an
            // exact row (the last stop is always a timepoint).
            mutate(&|n| {
                let t = trip_mut(n, &key, &id);
                let last = t.times.len() - 1;
                t.times[last].departure_secs = None;
            }),
            // Time running backwards.
            mutate(&|n| {
                let last = trip_mut(n, &key, &id).times.len() - 1;
                trip_mut(n, &key, &id).times[last] = StopOffset { arrival_secs: Some(0), departure_secs: Some(0) };
                trip_mut(n, &key, &id).times[0] = StopOffset { arrival_secs: Some(60), departure_secs: Some(60) };
            }),
            // A block naming a trip that does not exist.
            mutate(&|n| {
                n.blocks.insert(
                    "ghost".into(),
                    Block { trips: vec![TripRef { trip: "no-such-trip".into() }], layovers: vec![] },
                );
            }),
            // One vehicle cannot run it: a one-start trip in two blocks,
            // or a frequency trip in a block whose instances overlap
            // (a one-second headway).
            mutate(&|n| {
                if let Departs::Every { bands, .. } = &mut trip_mut(n, &key, &id).departs {
                    bands[0].headway_secs = 1;
                } else {
                    n.blocks.insert("twice-a".into(), Block { trips: vec![TripRef { trip: id.clone() }], layovers: vec![] });
                }
                n.blocks.insert("twice-b".into(), Block { trips: vec![TripRef { trip: id.clone() }], layovers: vec![] });
            }),
        ];
        for (i, b) in broken.iter().enumerate() {
            prop_assert!(!validation_issues(b).is_empty(), "mutation {} passed validation", i);
        }
    }

    /// Retiming moves a trip, it does not replace it: same id, the new
    /// start, the same running times, the same block — and nothing
    /// else changes.
    #[test]
    fn retime_keeps_the_trip(net in net_strategy(), pick in any::<prop::sample::Index>(), by in 1u32..20) {
        let starts: Vec<(String, u32)> = all_trips(&net)
            .into_iter()
            .filter_map(|(_, _, t)| match t.departs { Departs::At { start_secs } => Some((t.id.clone(), start_secs)), _ => None })
            .collect();
        prop_assume!(!starts.is_empty());
        let (id, start) = starts[pick.index(starts.len())].clone();
        let to_secs = start + by * 60;
        let next = apply_edits(&net, &[NetworkEdit::RetimeTrip { trip: id.clone(), to_secs }]).expect("retime applies");
        let (_, _, moved) = trip(&next, &id).expect("the trip keeps its id");
        let (_, _, before) = trip(&net, &id).unwrap();
        prop_assert_eq!(&moved.departs, &Departs::At { start_secs: to_secs });
        prop_assert_eq!(&moved.times, &before.times, "running times travel with the trip");
        for (key, _, t) in all_trips(&net) {
            if t.id != id {
                prop_assert_eq!(Some(t), trip(&next, &t.id).map(|x| x.2), "{}: untouched", key);
            }
        }
        prop_assert_eq!(&next.blocks, &net.blocks, "block membership is by id");
    }

    /// A running-time change applies to the trips it names — one
    /// route, one direction, ONE service type, starts in the window —
    /// and only to them: each ends `delta` later, starts when it did,
    /// keeps its id, and still runs forward. Every other trip,
    /// including the same pattern on another service type, is
    /// untouched — planners time weekends separately.
    #[test]
    fn running_time_changes_only_the_named_trips(
        net in net_strategy(),
        from in (5u32 * 3600..20 * 3600).prop_map(|s| s - s % 600),
        len in 1u32..6,
        delta in 1u32..6,
        saturday in any::<bool>(),
    ) {
        let service = if saturday { "sa" } else { "wk" };
        let (to, delta) = (from + len * 3600, delta * 60);
        let next = apply_edits(&net, &[NetworkEdit::AdjustRunningTime {
            route: "r0".into(),
            direction: None,
            service_type: service.into(),
            from_secs: from,
            to_secs: to,
            delta_secs: delta as i32,
        }]).expect("the change applies");
        for (key, pt, t) in all_trips(&net) {
            let (_, _, after) = trip(&next, &t.id).expect("ids survive");
            let named = pt.service_type == service
                && matches!(t.departs, Departs::At { start_secs } if from <= start_secs && start_secs < to);
            if named {
                prop_assert_eq!(&after.departs, &t.departs, "{}: starts when it did", t.id);
                prop_assert_eq!(after.times.len(), t.times.len());
                prop_assert_eq!(&after.times[0], &t.times[0]);
                prop_assert_eq!(end_offset(after), end_offset(t) + delta, "{}: ends delta later", t.id);
                let scheduled: Vec<u32> = after.times.iter()
                    .flat_map(|o| o.arrival_secs.into_iter().chain(o.departure_secs))
                    .collect();
                prop_assert!(scheduled.windows(2).all(|w| w[0] <= w[1]), "{}: runs forward", t.id);
            } else if matches!(t.departs, Departs::At { .. }) || pt.service_type != service {
                prop_assert_eq!(after, t, "{} ({}): not named, untouched", t.id, key);
            }
        }
    }

    /// A headway change over a window leaves one trip per slot. A trip
    /// that stays keeps its id and its own running times (it may move
    /// onto the slot); a new trip gets an id no trip had; nothing
    /// outside the window, the route, or the service type changes.
    #[test]
    fn headway_keeps_the_trips_it_keeps(
        net in net_strategy(),
        from in (6u32 * 3600..18 * 3600).prop_map(|s| s - s % 600),
        len in 1u32..4,
        headway in prop::sample::select(vec![600u32, 900, 1200, 1800]),
    ) {
        let to = from + len * 3600;
        let next = apply_edits(&net, &[NetworkEdit::SetHeadway {
            route: "r0".into(),
            direction: None,
            service_type: "wk".into(),
            from_secs: from,
            to_secs: to,
            headway_secs: headway,
            pattern: None,
        }]).expect("the change applies");

        let in_window = |pt: &PatternTrips, t: &Trip| pt.service_type == "wk"
            && matches!(t.departs, Departs::At { start_secs } if from <= start_secs && start_secs < to);
        let before: BTreeMap<String, Trip> = all_trips(&net).into_iter().map(|(_, _, t)| (t.id.clone(), t.clone())).collect();
        let window_before: BTreeSet<String> = all_trips(&net).into_iter()
            .filter(|(_, pt, t)| in_window(pt, t)).map(|(_, _, t)| t.id.clone()).collect();

        let mut ids = BTreeSet::new();
        let mut slots = Vec::new();
        for (key, pt, t) in all_trips(&next) {
            prop_assert!(ids.insert(t.id.clone()), "{}: id {} appears twice", key, t.id);
            if in_window(pt, t) {
                if let Departs::At { start_secs } = t.departs { slots.push(start_secs); }
                match before.get(&t.id) {
                    Some(old) => {
                        prop_assert!(window_before.contains(&t.id), "{}: an id from outside the window", t.id);
                        prop_assert_eq!(&t.times, &old.times, "{}: a kept trip keeps its times", t.id);
                    }
                    None => {
                        let pattern = &next.patterns[&pt.pattern];
                        prop_assert_eq!(t.times.len(), pattern.stops.len(), "{}: a new trip is timed at every stop", t.id);
                    }
                }
            } else {
                prop_assert_eq!(Some(t), before.get(&t.id), "{}: outside the window, untouched", t.id);
            }
        }
        slots.sort();
        let grid: Vec<u32> = (0..(to - from).div_ceil(headway)).map(|k| from + k * headway).collect();
        prop_assert_eq!(slots, grid, "one trip per slot");
        for block in next.blocks.values() {
            for r in &block.trips {
                prop_assert!(ids.contains(&r.trip), "block names a removed trip {}", r.trip);
            }
        }
    }

    /// QAF-64: a trip's id is its identity, so the timetable against a
    /// baseline reads a moved trip as RETIMED — kept, with its
    /// baseline start — never as removed and added. A one-start trip
    /// both documents hold reads unchanged at its old start and
    /// retimed at a new one; a trip only the proposal holds is added;
    /// a one-start trip only the baseline holds is removed. A
    /// frequency trip's instances have no identity beyond (trip,
    /// start): each is kept, added, or removed by that pair.
    #[test]
    fn a_retimed_trip_reads_retimed_against_its_baseline(
        net in net_strategy(),
        pick in any::<prop::sample::Index>(),
        by in 1u32..20,
        from in (6u32 * 3600..18 * 3600).prop_map(|s| s - s % 600),
        headway in prop::sample::select(vec![600u32, 900, 1800]),
    ) {
        let starts: Vec<(String, u32)> = all_trips(&net)
            .into_iter()
            .filter_map(|(_, _, t)| match t.departs { Departs::At { start_secs } => Some((t.id.clone(), start_secs)), _ => None })
            .collect();
        let mut edits = vec![NetworkEdit::SetHeadway {
            route: "r0".into(),
            direction: None,
            service_type: "wk".into(),
            from_secs: from,
            to_secs: from + 2 * 3600,
            headway_secs: headway,
            pattern: None,
        }];
        if !starts.is_empty() {
            let (id, start) = starts[pick.index(starts.len())].clone();
            edits.insert(0, NetworkEdit::RetimeTrip { trip: id, to_secs: start + by * 60 });
        }
        let next = apply_edits(&net, &edits).expect("the edits apply");
        let Some(t) = transit_network::timetable_against(&next, &net, "r0", None, "wk") else {
            return Ok(());
        };

        // The oracle: the stored trips under r0 / wk, by id.
        let one_start = |n: &Network| -> BTreeMap<String, u32> {
            all_trips(n).into_iter()
                .filter(|(_, pt, _)| pt.service_type == "wk")
                .filter_map(|(_, _, t)| match t.departs { Departs::At { start_secs } => Some((t.id.clone(), start_secs)), _ => None })
                .collect()
        };
        let instances = |n: &Network| -> BTreeSet<(String, u32)> {
            all_trips(n).into_iter()
                .filter(|(_, pt, _)| pt.service_type == "wk")
                .flat_map(|(_, _, t)| oracle_instances(&t.departs).into_iter().map(|s| (t.id.clone(), s)))
                .collect()
        };
        let (was, now) = (one_start(&net), one_start(&next));
        let (was_all, now_all) = (instances(&net), instances(&next));
        for row in &t.trips {
            match (now.get(&row.trip), was.get(&row.trip)) {
                (Some(_), Some(&old)) => {
                    prop_assert!(!row.added, "{}: kept, not added", row.trip);
                    let moved = (old != row.start_secs).then_some(old);
                    prop_assert_eq!(row.retimed_from_secs, moved, "{}: its baseline start", row.trip);
                }
                _ => {
                    prop_assert_eq!(row.added, !was_all.contains(&(row.trip.clone(), row.start_secs)), "{}@{}", row.trip, row.start_secs);
                    prop_assert_eq!(row.retimed_from_secs, None, "{}: not a kept one-start trip", row.trip);
                }
            }
        }
        let expected_removed: BTreeSet<(String, u32)> = was_all
            .iter()
            .filter(|(id, s)| !now_all.contains(&(id.clone(), *s)))
            .filter(|(id, _)| !(was.contains_key(id) && now.contains_key(id)))
            .cloned()
            .collect();
        let got_removed: BTreeSet<(String, u32)> = t.removed.iter().map(|r| (r.trip.clone(), r.start_secs)).collect();
        prop_assert_eq!(got_removed, expected_removed, "removed: the baseline's instances it no longer runs");
    }

    /// Adding a trip mints a fresh id, times it at every stop, and
    /// keeps the document valid.
    #[test]
    fn an_added_trip_is_a_new_trip(net in net_strategy(), pick in any::<prop::sample::Index>(), start in 5u32 * 3600..22 * 3600) {
        let keys: Vec<String> = net.pattern_trips.keys().cloned().collect();
        prop_assume!(!keys.is_empty());
        let key = keys[pick.index(keys.len())].clone();
        let next = apply_edits(&net, &[NetworkEdit::AddTrip { pattern_trips: key.clone(), id: None, start_secs: start }])
            .expect("the trip is added");
        let old: BTreeSet<String> = all_trips(&net).into_iter().map(|(_, _, t)| t.id.clone()).collect();
        let added: Vec<&Trip> = next.pattern_trips[&key].trips.iter().filter(|t| !old.contains(&t.id)).collect();
        prop_assert_eq!(added.len(), 1);
        prop_assert_eq!(&added[0].departs, &Departs::At { start_secs: start });
        prop_assert_eq!(added[0].times.len(), next.patterns[&next.pattern_trips[&key].pattern].stops.len());
        let issues = validation_issues(&next);
        prop_assert!(issues.is_empty(), "{:?}", issues);
    }

    /// Removing a trip removes it everywhere: no trip, no block
    /// reference, layovers still between trips, no empty block.
    #[test]
    fn a_removed_trip_is_gone(net in net_strategy(), pick in any::<prop::sample::Index>()) {
        let trips = all_trips(&net);
        prop_assume!(!trips.is_empty());
        let id = trips[pick.index(trips.len())].2.id.clone();
        let next = apply_edits(&net, &[NetworkEdit::RemoveTrip { trip: id.clone() }]).expect("the trip is removed");
        prop_assert!(trip(&next, &id).is_none());
        for (b, block) in &next.blocks {
            prop_assert!(!block.trips.is_empty(), "{}: an empty block", b);
            prop_assert!(block.trips.iter().all(|r| r.trip != id), "{}: still names {}", b, id);
            prop_assert!(block.layovers.iter().all(|l| l.after_trip + 1 < block.trips.len()), "{}: a layover after the last trip", b);
        }
        let issues = validation_issues(&next);
        prop_assert!(issues.is_empty(), "{:?}", issues);
    }
}

// ===================================================================
// Migration: documents written before trips were stored.
// ===================================================================

#[derive(Clone, Debug)]
struct OldSpec {
    stops: usize,
    flags: Vec<bool>,
    /// Two profiles split at `split`, each with its own run times.
    split: u32,
    legs: (Vec<u32>, Vec<u32>),
    starts: BTreeSet<u32>,
    band: (u32, u32, u32),
    block: bool,
}

fn old_spec() -> impl Strategy<Value = OldSpec> {
    (2usize..=5).prop_flat_map(|n| {
        (
            Just(n),
            proptest::collection::vec(any::<bool>(), n),
            (8u32 * 3600..16 * 3600).prop_map(|s| s - s % 600),
            (proptest::collection::vec(60u32..600, n), proptest::collection::vec(60u32..600, n)),
            proptest::collection::btree_set((5u32 * 3600..22 * 3600).prop_map(|s| s - s % 300), 1..6),
            (6u32 * 3600..8 * 3600, 1u32..3, 900u32..1800),
            any::<bool>(),
        )
            .prop_map(|(stops, flags, split, legs, starts, (bs, hours, h), block)| OldSpec {
                stops,
                flags,
                split,
                legs,
                starts,
                band: (bs - bs % 60, bs - bs % 60 + hours * 3600, h - h % 60),
                block,
            })
    })
}

/// A document in the pre-trip shape: `departures` per pattern trips,
/// blocks referencing `(pattern_trips, start_secs)`.
fn old_document(s: &OldSpec) -> (serde_json::Value, Vec<Vec<u32>>) {
    let flags: Vec<bool> = (0..s.stops).map(|i| s.flags[i] || i == 0 || i == s.stops - 1).collect();
    let tps = flags.iter().filter(|f| **f).count();
    let profile = |legs: &[u32]| -> Vec<u32> {
        let mut t = 0;
        (0..tps).map(|k| { if k > 0 { t += legs[k]; } t }).collect()
    };
    let profiles = vec![profile(&s.legs.0), profile(&s.legs.1)];
    let stops: serde_json::Map<String, serde_json::Value> = (0..s.stops)
        .map(|i| (format!("s{i}"), serde_json::json!({"name": format!("Stop {i}"), "lat": 45.5, "lon": -122.6 + i as f64 * 0.01})))
        .collect();
    let mut blocks = serde_json::json!({});
    if s.block {
        let refs: Vec<serde_json::Value> = s.starts.iter().take(1)
            .map(|st| serde_json::json!({"pattern_trips": "wk:p0", "start_secs": st}))
            .collect();
        blocks = serde_json::json!({"b0": {"trips": refs, "layovers": []}});
    }
    let doc = serde_json::json!({
        "name": "Old",
        "stops": stops,
        "routes": {"r0": {"name": "R", "mode": "bus"}},
        "patterns": {"p0": {
            "route": "r0",
            "name": "P",
            "stops": (0..s.stops).map(|i| serde_json::json!({"stop": format!("s{i}"), "timepoint": flags[i], "distance_m": i as f64 * 400.0})).collect::<Vec<_>>(),
            "timing": [
                {"start_secs": 0, "end_secs": s.split, "at_timepoints": profiles[0]},
                {"start_secs": s.split, "end_secs": 40 * 3600, "at_timepoints": profiles[1]},
            ],
        }},
        "service_types": {"wk": {"name": "Weekday", "rule": {
            "days": {"mon": true, "tue": true, "wed": true, "thu": true, "fri": true},
            "ranges": [{"start": "2026-01-01", "end": "2026-12-31"}],
            "exceptions": [],
        }}},
        "pattern_trips": {
            "wk:p0": {"pattern": "p0", "service_type": "wk", "departures": {"kind": "exact_times", "starts": s.starts}},
            "wk:h0": {"pattern": "p0", "service_type": "wk", "departures": {"kind": "headway_bands", "bands": [
                {"start_secs": s.band.0, "end_secs": s.band.1, "headway_secs": s.band.2},
            ]}},
        },
        "blocks": blocks,
    });
    (doc, profiles)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    /// A pre-trip document reads as stored trips: every departure it
    /// derived — exact starts and headway slots alike — becomes a
    /// one-start trip with the id the old export minted
    /// (`"{pattern_trips}@{start}"`), timed by the profile that covered
    /// its start; block references become trip references. The
    /// migrated document validates, and it runs the same service.
    #[test]
    fn pre_trip_documents_migrate(s in old_spec()) {
        let (doc, profiles) = old_document(&s);
        let net = read_network(&doc.to_string()).expect("pre-trip documents read");
        let issues = validation_issues(&net);
        prop_assert!(issues.is_empty(), "{:?}", issues);

        let flags: Vec<bool> = net.patterns["p0"].stops.iter().map(|p| p.timepoint).collect();
        let band: Vec<u32> = (0..(s.band.1 - s.band.0).div_ceil(s.band.2)).map(|k| s.band.0 + k * s.band.2).collect();
        for (key, starts) in [("wk:p0", s.starts.iter().copied().collect::<Vec<_>>()), ("wk:h0", band)] {
            let pt = &net.pattern_trips[key];
            prop_assert_eq!(pt.trips.len(), starts.len(), "{}: one trip per old departure", key);
            for start in starts {
                let id = format!("{key}@{start}");
                let t = pt.trips.iter().find(|t| t.id == id).ok_or_else(|| TestCaseError::fail(format!("missing {id}")))?;
                prop_assert_eq!(&t.departs, &Departs::At { start_secs: start });
                let profile = if start < s.split { &profiles[0] } else { &profiles[1] };
                let mut k = 0;
                for (i, o) in t.times.iter().enumerate() {
                    if flags[i] {
                        prop_assert_eq!(o.arrival_secs, Some(profile[k]), "{} stop {}: the profile's time", id, i);
                        k += 1;
                    }
                }
                prop_assert_eq!(t.times[0].departure_secs, Some(0), "{}: departs its first stop at its start", id);
            }
        }
        if s.block {
            let first = *s.starts.iter().next().unwrap();
            prop_assert_eq!(
                &net.blocks["b0"].trips,
                &vec![TripRef { trip: format!("wk:p0@{first}") }]
            );
        }
    }
}
