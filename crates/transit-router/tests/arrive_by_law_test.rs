//! Arrive-by laws (QAF-72, "the last bus home").
//!
//! The intent, as the user stated it: an arrive-by query answers the
//! LATEST departure from `from` that reaches `to` by the deadline;
//! with no deadline, the latest departure on the service day that
//! reaches `to` at all. Times past 86400 are after-midnight trips of
//! the same service day, never errors.
//!
//! The laws, each against the independent fixpoint oracle
//! (`prop_support::oracle_arrival`, no code shared with the search):
//!
//! 1. the answer's itinerary is feasible from its own departure and
//!    arrives no later than the deadline;
//! 2. no later departure reaches `to` by the deadline — checked at
//!    the answer + 1 s, which covers every later second because
//!    leaving later never arrives earlier (FIFO: waiting is free);
//! 3. "no answer" means no departure at all (from the start of the
//!    service day) reaches `to` by the deadline;
//! 4. with no deadline, the deadline is the service day's end — the
//!    latest arrival of any trip that day (after-midnight included):
//!    nothing reaches anywhere later than that, so "reaches at all"
//!    and "reaches by the day's end" are the same question;
//! 5. the leg bound holds: with `k` legs, the same two-sided law
//!    against the router's own earliest-arrival search at `k` legs.

use chrono::NaiveDate;
use proptest::collection::vec;
use proptest::prelude::*;
use transit_router::{earliest_arrival, latest_departure, Leg, RouterNetwork};

#[path = "prop_support.rs"]
mod support;
use support::{assert_feasible, build_zip_spaced, model_day, oracle_arrival, TripSpec, INF};

const DATE: NaiveDate = NaiveDate::from_ymd_opt(2026, 10, 23).unwrap(); // Friday
/// Enough legs for any path through feeds this small.
const MAX_LEGS: usize = 12;

prop_compose! {
    fn trip_spec(n_stops: usize)
        (stops in vec(0u32..(n_stops as u32), 2..=5),
         // Past 86400: after-midnight trips are legitimate.
         start in 0u32..100_000,
         step in 100u32..900)
    -> TripSpec {
        TripSpec { stops, start, step }
    }
}

/// Stops ~89 m apart (every near pair walkable) or ~333 m apart (no
/// footpaths: transit only).
fn feed() -> impl Strategy<Value = (usize, f64, Vec<TripSpec>, usize, usize, Option<u32>)> {
    (3usize..=8).prop_flat_map(|n_stops| {
        (
            Just(n_stops),
            prop_oneof![Just(0.0008), Just(0.003)],
            vec(trip_spec(n_stops), 1..=10),
            0usize..n_stops,
            0usize..n_stops,
            proptest::option::of(0u32..104_000),
        )
    })
}

/// The service day's end, recomputed from the generated trips (not
/// from the router): the latest final arrival of any trip.
fn day_end(trips: &[TripSpec]) -> u32 {
    trips.iter().map(|t| t.start + (t.stops.len() as u32 - 1) * t.step).max().unwrap_or(0)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// Laws 1–4: the answer is the latest feasible departure, against
    /// the oracle; absent, nothing reaches by the deadline.
    #[test]
    fn the_answer_is_the_latest_departure_that_arrives_by_the_deadline(
        (n_stops, spacing, trips, from, to, deadline) in feed()
    ) {
        let (network, day) = model_day(&build_zip_spaced(n_stops, spacing, &trips), DATE);
        let net = RouterNetwork::build_from_day(&network, &day, DATE);
        let (from_id, to_id) = (format!("s{from}"), format!("s{to}"));
        let fi = net.stop_index(&from_id).unwrap() as usize;
        let ti = net.stop_index(&to_id).unwrap() as usize;

        let answer = latest_departure(&net, &from_id, &to_id, deadline, MAX_LEGS).expect("valid query");
        let by = deadline.unwrap_or_else(|| day_end(&trips));
        prop_assert_eq!(answer.arrive_by_secs, by, "no deadline means the day's end");

        match answer.itinerary {
            Some(it) => {
                let d = it.depart_secs;
                assert_feasible(&it, d);
                prop_assert!(it.arrival_secs <= by, "arrives {} after the deadline {}", it.arrival_secs, by);
                let oracle_now = oracle_arrival(&net, fi, ti, d);
                prop_assert!(oracle_now != INF && oracle_now <= by, "the oracle cannot reach by {by} from {d}");
                let oracle_later = oracle_arrival(&net, fi, ti, d + 1);
                prop_assert!(
                    oracle_later == INF || oracle_later > by,
                    "leaving at {} still arrives {} <= {} — {} is not the latest", d + 1, oracle_later, by, d
                );
            }
            None => {
                let oracle = oracle_arrival(&net, fi, ti, 0);
                prop_assert!(oracle == INF || oracle > by,
                    "no answer, yet leaving at 0 arrives {oracle} <= {by}");
            }
        }
    }

    /// Law 5: under a leg bound, the answer is two-sided against the
    /// earliest-arrival search at the same bound.
    #[test]
    fn the_leg_bound_holds(
        (n_stops, spacing, trips, from, to, deadline) in feed(),
        legs in 1usize..=3,
    ) {
        let (network, day) = model_day(&build_zip_spaced(n_stops, spacing, &trips), DATE);
        let net = RouterNetwork::build_from_day(&network, &day, DATE);
        let (from_id, to_id) = (format!("s{from}"), format!("s{to}"));
        let answer = latest_departure(&net, &from_id, &to_id, deadline, legs).expect("valid query");
        let by = answer.arrive_by_secs;
        let reaches = |t: u32| {
            earliest_arrival(&net, &from_id, &to_id, t, legs).unwrap().is_some_and(|it| it.arrival_secs <= by)
        };
        match answer.itinerary {
            Some(it) => {
                let transit = it.legs.iter().filter(|l| matches!(l, Leg::Transit { .. })).count();
                prop_assert!(transit <= legs, "{transit} legs > {legs}");
                prop_assert!(reaches(it.depart_secs));
                prop_assert!(!reaches(it.depart_secs + 1), "a later departure reaches within {legs} legs");
            }
            None => prop_assert!(!reaches(0), "no answer, yet leaving at 0 reaches within {legs} legs"),
        }
    }
}

/// The last bus home, after midnight (the QAF-72 shape): the last
/// trip leaves 23:07 and arrives 24:01 — the no-deadline answer is
/// that trip, not "unreachable".
#[test]
fn the_last_bus_home_runs_past_midnight() {
    let trips = vec![
        TripSpec { stops: vec![0, 1], start: 22 * 3600, step: 54 * 60 },
        TripSpec { stops: vec![0, 1], start: 23 * 3600 + 7 * 60, step: 54 * 60 },
        // A far-away trip that ends later still: the day's end, not
        // a reason to leave later from s0.
        TripSpec { stops: vec![5, 6], start: 25 * 3600, step: 300 },
    ];
    let (network, day) = model_day(&build_zip_spaced(7, 0.003, &trips), DATE);
    let net = RouterNetwork::build_from_day(&network, &day, DATE);
    let answer = latest_departure(&net, "s0", "s1", None, 4).unwrap();
    let it = answer.itinerary.expect("the last bus home exists");
    assert_eq!(it.arrival_secs, 24 * 3600 + 60, "arrives 24:01");
    match &it.legs[..] {
        [Leg::Transit { board_secs, .. }] => assert_eq!(*board_secs, 23 * 3600 + 7 * 60, "the 23:07"),
        other => panic!("one ride home, got {other:?}"),
    }
    // A deadline before 24:01 answers the earlier bus.
    let earlier = latest_departure(&net, "s0", "s1", Some(24 * 3600), 4).unwrap();
    assert_eq!(earlier.itinerary.expect("the 22:00 reaches").arrival_secs, 22 * 3600 + 54 * 60);
    // A deadline before every arrival answers nothing.
    assert!(latest_departure(&net, "s0", "s1", Some(22 * 3600), 4).unwrap().itinerary.is_none());
}

/// Arriving where you already are: you may leave at the deadline.
#[test]
fn from_is_to_leaves_at_the_deadline() {
    let trips = vec![TripSpec { stops: vec![0, 1], start: 8 * 3600, step: 600 }];
    let (network, day) = model_day(&build_zip_spaced(3, 0.003, &trips), DATE);
    let net = RouterNetwork::build_from_day(&network, &day, DATE);
    let it = latest_departure(&net, "s2", "s2", Some(9 * 3600), 4).unwrap().itinerary.unwrap();
    assert_eq!((it.depart_secs, it.arrival_secs, it.legs.len()), (9 * 3600, 9 * 3600, 0));
    assert!(latest_departure(&net, "nope", "s2", None, 4).is_err(), "unknown stops are errors");
}
