//! Pins planning over a live document (D-73). The properties:
//!
//! 1. An empty live document gives the schedule-only plan.
//! 2. A live plan arrives when the schedule-only plan of the predicted world
//!    arrives. The oracle builds a second feed whose trips run at the
//!    predicted times, independently of the router's live join.
//! 3. A transit leg's `board_delay_secs` and `alight_delay_secs` hold the
//!    stated delay, or `None` where the document states none. `secs - delay`
//!    is the scheduled time.
//! 4. A late bus makes a connection that the schedule misses, and misses one
//!    that the schedule makes.
//! 5. A cancelled trip is never ridden, and a skipped stop is never boarded or
//!    alighted at (D-73). The live plan is the schedule plan of the world
//!    without them.

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
/// No walk crosses this spacing, so the plans of property 5 ride.
const SPREAD: f64 = 0.05;

/// One generated trip. `live` is `None` (no prediction) or the first stated
/// position and the delay. A delay stated mid-trip is never negative, because
/// an early bus would run backwards in time past the earlier on-schedule stops.
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

/// One entry per stated visit, keyed by the trip instance (the materialized
/// day's start) and the scheduled departure. JSON builds it so that the other
/// fields keep their defaults.
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

    /// Property 1.
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

    /// Properties 2 and 3.
    #[test]
    fn a_live_plan_is_the_predicted_worlds_plan((n_stops, trips, from, to, depart) in feed()) {
        let timed: Vec<_> = trips.iter().map(|t| (t.stops.clone(), t.scheduled())).collect();
        let (network, day) = model_day(&build_zip_timed(n_stops, 0.0008, &timed), DATE);
        let live = live_doc(&trips, &day);
        let net = RouterNetwork::build(&network, &day, DATE, Some(&live));

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

/// Two buses meet at s1, and the stops are too far apart to walk:
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
    let on_time = connection(|_| vec![]).unwrap();
    assert_eq!(on_time.arrival_secs, 8 * 3600 + 1800);
    // t0 predicted 5 min late arrives 08:15. t1 is gone, so t2 arrives 08:50.
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
    // t0 predicted 4 min late arrives 08:14 and misses t1 at 08:12 on the
    // schedule. But t1 is predicted 5 min late and leaves 08:17, so the
    // connection is made and arrives 08:35.
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
            // s0 is before t0's first stated delay, so it stays on schedule.
            assert_eq!((*board_secs, *board_delay_secs), (8 * 3600, None));
        }
        other => panic!("t0 then t1, got {other:?}"),
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn a_cancelled_trip_is_never_ridden(
        (n_stops, trips, from, to, depart) in feed(),
        cancel in vec(any::<bool>(), 10),
    ) {
        let timed: Vec<_> = trips.iter().map(|t| (t.stops.clone(), t.scheduled())).collect();
        let (network, day) = model_day(&build_zip_timed(n_stops, SPREAD, &timed), DATE);
        let cancelled: Vec<serde_json::Value> = trips
            .iter()
            .enumerate()
            .filter(|(i, _)| cancel[*i])
            .map(|(i, _)| {
                let id = format!("t{i}");
                let start = day.iter().find(|m| m.trip == id).unwrap().start_secs;
                serde_json::json!({ "trip": id, "start_secs": start, "route": "1", "route_name": "R",
                    "worst_delay_secs": null, "status": "no_prediction", "canceled": true, "stops": [] })
            })
            .collect();
        let net = RouterNetwork::build(&network, &day, DATE, Some(&empty_live_with(cancelled)));

        // The oracle world keeps only the trips that run, and all the stops. A
        // stop that no trip serves is still a place to walk from.
        let running: Vec<_> = trips.iter().enumerate().filter(|(i, _)| !cancel[*i]).map(|(_, t)| t).collect();
        let got = plan(&net, from, to, depart);
        for leg in got.iter().flat_map(|i| &i.legs) {
            if let Leg::Transit { trip, .. } = leg {
                prop_assert!(!cancel[trip[1..].parse::<usize>().unwrap()], "rode cancelled {}", trip);
            }
        }
        if !running.is_empty() {
            let timed: Vec<_> = running.iter().map(|t| (t.stops.clone(), t.scheduled())).collect();
            let (onet, oday) = model_day(&build_zip_timed(n_stops, SPREAD, &timed), DATE);
            let oracle = RouterNetwork::build_from_day(&onet, &oday, DATE);
            prop_assert_eq!(got.map(|i| i.arrival_secs), plan(&oracle, from, to, depart).map(|i| i.arrival_secs));
        }
    }
}

#[test]
fn a_cancelled_connection_waits_for_the_next_bus() {
    let it = connection(|day| {
        let start = day.iter().find(|m| m.trip == "t1").unwrap().start_secs;
        vec![serde_json::json!({ "trip": "t1", "start_secs": start, "route": "1", "route_name": "R",
            "worst_delay_secs": null, "status": "no_prediction", "canceled": true, "stops": [] })]
    })
    .unwrap();
    assert_eq!(it.arrival_secs, 8 * 3600 + 3000);
    assert!(it.legs.iter().all(|l| !matches!(l, Leg::Transit { trip, .. } if trip == "t1")));
}

#[test]
fn a_skipped_stop_is_not_boarded() {
    let it = connection(|day| {
        let start = day.iter().find(|m| m.trip == "t1").unwrap().start_secs;
        vec![serde_json::json!({ "trip": "t1", "start_secs": start, "route": "1", "route_name": "R",
            "worst_delay_secs": null, "status": "no_prediction", "stops": [
                { "stop": "s1", "scheduled_departure": 8 * 3600 + 720, "arrival_delay": null,
                  "departure_delay": null, "predicted_departure": 8 * 3600 + 720, "skipped": true }
            ] })]
    })
    .unwrap();
    assert_eq!(it.arrival_secs, 8 * 3600 + 3000);
    assert!(it.legs.iter().all(|l| !matches!(l, Leg::Transit { trip, .. } if trip == "t1")));
}
