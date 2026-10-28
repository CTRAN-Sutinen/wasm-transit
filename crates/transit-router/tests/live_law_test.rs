//! Live planning laws (QAF-73: plans use live data).
//!
//! The intent, as the user stated it: with a live document, the
//! router plans over the day with each trip instance's PREDICTED
//! times where the live document states them (the stop entries of
//! `LiveDay::trips`, keyed by trip instance, stop and scheduled
//! departure: predicted = schedule + delay, arrival delay likewise;
//! stops absent before the first stated delay stay on schedule), and
//! the schedule elsewhere. Legs say which times are predicted.
//!
//! The laws:
//!
//! 1. an empty live document changes nothing — the plan equals the
//!    schedule-only plan;
//! 2. a live plan IS the schedule plan of the predicted world: the
//!    oracle builds a second feed whose trips run at the predicted
//!    times outright (independently of the router's live join) and
//!    plans it schedule-only — the arrivals agree;
//! 3. a transit leg's times are the predicted ones, and say so:
//!    `board_delay_secs` / `alight_delay_secs` are the stated delay
//!    where the document states one and `None` (schedule) where it
//!    does not, and `secs - delay` is the scheduled time;
//! 4. scenarios: a predicted-late bus makes a connection the schedule
//!    says is missed, and a predicted-late bus misses a connection
//!    the schedule says is made.

use chrono::NaiveDate;
use proptest::collection::vec;
use proptest::prelude::*;
use transit_live::LiveDay;
use transit_router::{earliest_arrival, Leg, RouterNetwork};

#[path = "prop_support.rs"]
mod support;
use support::{build_zip_timed, model_day};

const DATE: NaiveDate = NaiveDate::from_ymd_opt(2026, 10, 23).unwrap(); // Friday
const MAX_LEGS: usize = 12;

/// One generated trip: its stops, scheduled start and step, and its
/// live state — `None` (no prediction) or (first stated position,
/// delay). A delay stated mid-trip is never negative here: an early
/// bus that has not yet reached the earlier, on-schedule stops would
/// run backwards in time — not a valid world.
#[derive(Debug, Clone)]
struct LiveTripSpec {
    stops: Vec<u32>,
    start: u32,
    step: u32,
    live: Option<(usize, i32)>,
}

impl LiveTripSpec {
    fn scheduled(&self) -> Vec<u32> {
        (0..self.stops.len() as u32).map(|i| self.start + i * self.step).collect()
    }
    /// The delay stated at visit `i`, if any.
    fn delay_at(&self, i: usize) -> Option<i32> {
        self.live.and_then(|(first, d)| (i >= first).then_some(d))
    }
    fn predicted(&self) -> Vec<u32> {
        self.scheduled()
            .iter()
            .enumerate()
            .map(|(i, &t)| (t as i64 + self.delay_at(i).unwrap_or(0) as i64) as u32)
            .collect()
    }
}

prop_compose! {
    fn live_trip(n_stops: usize)
        (stops in vec(0u32..(n_stops as u32), 2..=5),
         start in 1_000u32..86_000,
         step in 100u32..900,
         live in proptest::option::of((0usize..5, -600i32..1_200)))
    -> LiveTripSpec {
        let live = live.map(|(first, d)| {
            let first = first.min(stops.len() - 1);
            (first, if first > 0 { d.abs() } else { d })
        });
        LiveTripSpec { stops, start, step, live }
    }
}

fn feed() -> impl Strategy<Value = (usize, Vec<LiveTripSpec>, usize, usize, u32)> {
    (3usize..=8).prop_flat_map(|n_stops| {
        (Just(n_stops), vec(live_trip(n_stops), 1..=10), 0usize..n_stops, 0usize..n_stops, 0u32..86_000)
    })
}

/// The live document for the generated trips, written the way the
/// rt boundary writes it: one entry per stated visit, keyed by the
/// trip instance (the materialized day's start) and the scheduled
/// departure. Built from JSON so the document's other fields keep
/// their defaults.
fn live_doc(trips: &[LiveTripSpec], day: &[transit_network::MaterializedTrip]) -> LiveDay {
    let entries: Vec<serde_json::Value> = trips
        .iter()
        .enumerate()
        .filter(|(_, t)| t.live.is_some())
        .map(|(i, t)| {
            let id = format!("t{i}");
            let start = day.iter().find(|m| m.trip == id).expect("trip materialized").start_secs;
            let sched = t.scheduled();
            let stops: Vec<serde_json::Value> = (0..t.stops.len())
                .filter_map(|v| {
                    let d = t.delay_at(v)?;
                    Some(serde_json::json!({
                        "stop": format!("s{}", t.stops[v]),
                        "scheduled_departure": sched[v],
                        "arrival_delay": d,
                        "departure_delay": d,
                        "predicted_departure": (sched[v] as i64 + d as i64) as u32,
                    }))
                })
                .collect();
            serde_json::json!({
                "trip": id, "start_secs": start, "route": "1", "route_name": "R",
                "worst_delay_secs": t.live.map(|(_, d)| d), "status": "late", "stops": stops,
            })
        })
        .collect();
    empty_live_with(entries)
}

fn empty_live_with(trips: Vec<serde_json::Value>) -> LiveDay {
    serde_json::from_value(serde_json::json!({
        "window": { "early_secs": 0, "late_secs": 60, "very_late_secs": 300 },
        "trips": trips,
        "vehicles": [],
        "unmatched_trips": 0,
    }))
    .expect("a valid live document")
}

fn plan(net: &RouterNetwork, from: usize, to: usize, depart: u32) -> Option<transit_router::Itinerary> {
    earliest_arrival(net, &format!("s{from}"), &format!("s{to}"), depart, MAX_LEGS).expect("valid query")
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// Law 1: an empty live document is the schedule.
    #[test]
    fn an_empty_live_document_is_the_schedule((n_stops, trips, from, to, depart) in feed()) {
        let timed: Vec<_> = trips.iter().map(|t| (t.stops.clone(), t.scheduled())).collect();
        let (network, day) = model_day(&build_zip_timed(n_stops, 0.0008, &timed), DATE);
        let schedule = RouterNetwork::build_from_day(&network, &day, DATE);
        let live = RouterNetwork::build(&network, &day, DATE, Some(&empty_live_with(vec![])));
        let (a, b) = (plan(&schedule, from, to, depart), plan(&live, from, to, depart));
        prop_assert_eq!(
            serde_json::to_value(&a).unwrap(),
            serde_json::to_value(&b).unwrap(),
            "an empty live document changed the plan"
        );
    }

    /// Laws 2–3: the live plan is the predicted world's schedule
    /// plan, and its legs carry the predictions they ride.
    #[test]
    fn a_live_plan_is_the_predicted_worlds_plan((n_stops, trips, from, to, depart) in feed()) {
        let timed: Vec<_> = trips.iter().map(|t| (t.stops.clone(), t.scheduled())).collect();
        let (network, day) = model_day(&build_zip_timed(n_stops, 0.0008, &timed), DATE);
        let live = live_doc(&trips, &day);
        let net = RouterNetwork::build(&network, &day, DATE, Some(&live));

        // The oracle world: every trip runs at its predicted times.
        let predicted: Vec<_> = trips.iter().map(|t| (t.stops.clone(), t.predicted())).collect();
        let (pnetwork, pday) = model_day(&build_zip_timed(n_stops, 0.0008, &predicted), DATE);
        let oracle = RouterNetwork::build_from_day(&pnetwork, &pday, DATE);

        let got = plan(&net, from, to, depart);
        let want = plan(&oracle, from, to, depart);
        prop_assert_eq!(got.as_ref().map(|i| i.arrival_secs), want.as_ref().map(|i| i.arrival_secs));

        for leg in got.iter().flat_map(|i| &i.legs) {
            let Leg::Transit { trip, board_stop_id, board_secs, board_delay_secs, alight_stop_id, alight_secs, alight_delay_secs, .. } = leg else { continue };
            let spec = &trips[trip[1..].parse::<usize>().unwrap()];
            let (sched, pred) = (spec.scheduled(), spec.predicted());
            // Some visit of the trip at the board stop is the one
            // ridden: its predicted time, its stated delay (None when
            // none is stated), and secs - delay is its schedule.
            let visit = |stop: &str, secs: u32, delay: Option<i32>| {
                (0..spec.stops.len()).any(|v| {
                    format!("s{}", spec.stops[v]) == stop
                        && pred[v] == secs
                        && spec.delay_at(v) == delay
                        && (secs as i64 - delay.unwrap_or(0) as i64) as u32 == sched[v]
                })
            };
            prop_assert!(visit(board_stop_id, *board_secs, *board_delay_secs),
                "boarding {trip} at {board_stop_id} {board_secs} delay {board_delay_secs:?} is not a predicted visit");
            prop_assert!(visit(alight_stop_id, *alight_secs, *alight_delay_secs),
                "alighting {trip} at {alight_stop_id} {alight_secs} delay {alight_delay_secs:?} is not a predicted visit");
        }
    }
}

/// Two buses that meet at s1, stops far apart (no walking):
/// t0 s0 08:00 → s1 08:10, t1 s1 08:12 → s2 08:30, t2 s1 08:20 → s2 08:50.
fn connection(live: impl Fn(&[transit_network::MaterializedTrip]) -> Vec<serde_json::Value>) -> Option<transit_router::Itinerary> {
    let timed = vec![
        (vec![0, 1], vec![8 * 3600, 8 * 3600 + 600]),
        (vec![1, 2], vec![8 * 3600 + 720, 8 * 3600 + 1800]),
        (vec![1, 2], vec![8 * 3600 + 1200, 8 * 3600 + 3000]),
    ];
    let (network, day) = model_day(&build_zip_timed(3, 0.003, &timed), DATE);
    let doc = empty_live_with(live(&day));
    let net = RouterNetwork::build(&network, &day, DATE, Some(&doc));
    earliest_arrival(&net, "s0", "s2", 7 * 3600, 4).unwrap()
}

fn delayed(trip: &str, start: u32, stops: &[(&str, u32)], d: i32) -> serde_json::Value {
    let stops: Vec<_> = stops
        .iter()
        .map(|(s, sched)| serde_json::json!({
            "stop": s, "scheduled_departure": sched, "arrival_delay": d,
            "departure_delay": d, "predicted_departure": (*sched as i64 + d as i64) as u32,
        }))
        .collect();
    serde_json::json!({ "trip": trip, "start_secs": start, "route": "1", "route_name": "R",
        "worst_delay_secs": d, "status": "late", "stops": stops })
}

#[test]
fn a_late_first_bus_misses_the_scheduled_connection() {
    // On schedule: t0 arrives 08:10, t1 leaves 08:12 → 08:30.
    let on_time = connection(|_| vec![]).unwrap();
    assert_eq!(on_time.arrival_secs, 8 * 3600 + 1800);
    // t0 predicted 5 min late arrives 08:15: t1 is gone, t2 → 08:50.
    let late = connection(|_| vec![delayed("t0", 8 * 3600, &[("s0", 8 * 3600), ("s1", 8 * 3600 + 600)], 300)]).unwrap();
    assert_eq!(late.arrival_secs, 8 * 3600 + 3000, "the predicted-late t0 misses t1");
    match &late.legs[0] {
        Leg::Transit { trip, board_secs, board_delay_secs, alight_secs, alight_delay_secs, .. } => {
            assert_eq!(trip, "t0");
            assert_eq!((*board_secs, *board_delay_secs), (8 * 3600 + 300, Some(300)));
            assert_eq!((*alight_secs, *alight_delay_secs), (8 * 3600 + 900, Some(300)));
        }
        other => panic!("rides t0 first, got {other:?}"),
    }
}

#[test]
fn a_late_second_bus_makes_a_connection_the_schedule_misses() {
    // t0 predicted 4 min late arrives 08:14 — missing t1 at 08:12 on
    // the schedule — but t1 is predicted 5 min late, leaving 08:17:
    // the connection is made, arriving 08:35.
    let it = connection(|_| {
        vec![
            delayed("t0", 8 * 3600, &[("s1", 8 * 3600 + 600)], 240),
            delayed("t1", 8 * 3600 + 720, &[("s1", 8 * 3600 + 720), ("s2", 8 * 3600 + 1800)], 300),
        ]
    })
    .unwrap();
    assert_eq!(it.arrival_secs, 8 * 3600 + 2100, "the late t1 waits for the late t0");
    match &it.legs[..] {
        [Leg::Transit { trip: a, board_secs, board_delay_secs, .. }, Leg::Transit { trip: b, .. }] => {
            assert_eq!((a.as_str(), b.as_str()), ("t0", "t1"));
            // s0 is before t0's first stated delay: on schedule.
            assert_eq!((*board_secs, *board_delay_secs), (8 * 3600, None));
        }
        other => panic!("t0 then t1, got {other:?}"),
    }
}
