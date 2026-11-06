//! How long a planned trip takes, and from when (D-40).
//!
//! The intent, as the user decided it (D-40, 2026-11-05): door to
//! door. An itinerary carries `leave_secs` — when the rider leaves the
//! origin: the first ride's boarding minus the walk before it — and
//! `duration_secs`, the arrival minus that leave. "Leave in N min" is
//! the board's one wait rule (D-24, `transit_dispatch::wait`) from the
//! moment the plan was asked to leave (`depart_secs`) to `leave_secs`.
//! "That matches what other trip planning services do today."
//!
//! The laws, over generated feeds and places (walk legs before the
//! first ride), leave-at and arrive-by alike:
//!
//! 1. **Leave is the walk back from the first boarding**: with a ride,
//!    `leave_secs` = the first transit leg's `board_secs` minus every
//!    walk before it; with no ride (a walk-only plan, or a stop to
//!    itself) the rider leaves when asked, `leave_secs = depart_secs`.
//! 2. **Duration is arrival minus leave**, door to door.
//! 3. **A leave-at plan never leaves before it was asked**:
//!    `depart_secs <= leave_secs <= arrival_secs`.
//! 4. **"Leave in" is the board's wait rule**:
//!    `leave == transit_dispatch::wait(leave_secs, depart_secs)`.

use chrono::NaiveDate;
use proptest::collection::vec;
use proptest::prelude::*;
use transit_router::{earliest_arrival_between, latest_departure_between, End, Itinerary, Leg, Place, PlaceWalk, RouterNetwork};

#[path = "prop_support.rs"]
mod support;
use support::{build_zip_spaced, model_day, TripSpec};

const DATE: NaiveDate = NaiveDate::from_ymd_opt(2026, 10, 23).unwrap(); // Friday
const MAX_LEGS: usize = 8;

prop_compose! {
    fn trip_spec(n_stops: usize)
        (stops in vec(0u32..(n_stops as u32), 2..=5),
         start in 0u32..86_000,
         step in 100u32..900)
    -> TripSpec {
        TripSpec { stops, start, step }
    }
}

fn end(n_stops: usize, id: &'static str) -> impl Strategy<Value = End> {
    prop_oneof![
        (0..n_stops).prop_map(|s| End::Stop(format!("s{s}"))),
        vec((0..n_stops, 0u32..600), 1..=3).prop_map(move |walks| End::Place(Place {
            id: id.into(),
            name: id.into(),
            walks: walks.into_iter().map(|(s, secs)| PlaceWalk { stop_id: format!("s{s}"), walk_secs: secs, path: Vec::new() }).collect(),
        })),
    ]
}

/// Laws 1, 2 and 4 — what every itinerary says about itself.
fn check_door_to_door(it: &Itinerary) {
    let first_ride = it.legs.iter().position(|l| matches!(l, Leg::Transit { .. }));
    let expected_leave = match first_ride {
        Some(i) => {
            let Leg::Transit { board_secs, .. } = &it.legs[i] else { unreachable!() };
            let walked: u32 = it.legs[..i]
                .iter()
                .map(|l| match l {
                    Leg::Walk { walk_secs, .. } => *walk_secs,
                    Leg::Transit { .. } => 0,
                })
                .sum();
            board_secs - walked
        }
        None => it.depart_secs,
    };
    assert_eq!(it.leave_secs, expected_leave, "leave is the first boarding minus the walk before it: {it:?}");
    assert_eq!(it.duration_secs, it.arrival_secs - it.leave_secs, "duration is arrival minus leave: {it:?}");
    assert_eq!(it.leave, transit_dispatch::wait(it.leave_secs, it.depart_secs), "leave in is the board's wait rule");
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 96, ..ProptestConfig::default() })]

    #[test]
    fn a_leave_at_plan_is_door_to_door(
        n_stops in 3usize..7,
        trips in (3usize..7).prop_flat_map(|n| vec(trip_spec(n), 1..6)),
        (from, to) in (3usize..7).prop_flat_map(|n| (end(n, "home"), end(n, "work"))),
        direct in proptest::option::of(0u32..3_000),
        depart in 0u32..86_000,
    ) {
        let n = trips.iter().flat_map(|t| t.stops.iter()).map(|&s| s as usize + 1).max().unwrap_or(1).max(n_stops);
        let zip = build_zip_spaced(n, 0.003, &trips);
        let (network, day) = model_day(&zip, DATE);
        let net = RouterNetwork::build_from_day(&network, &day, DATE);
        let known = |e: &End| match e {
            End::Stop(id) => net.stops.iter().any(|s| &s.stop_id == id),
            End::Place(p) => p.walks.iter().all(|w| net.stops.iter().any(|s| s.stop_id == w.stop_id)),
        };
        prop_assume!(known(&from) && known(&to));
        let direct = match (&from, &to) { (End::Place(_), _) | (_, End::Place(_)) => direct, _ => None };
        if let Some(it) = earliest_arrival_between(&net, &from, &to, direct, depart, MAX_LEGS).unwrap() {
            check_door_to_door(&it);
            prop_assert!(it.depart_secs <= it.leave_secs, "never leaves before asked: {it:?}");
            prop_assert!(it.leave_secs <= it.arrival_secs);
        }
    }

    #[test]
    fn an_arrive_by_plan_is_door_to_door(
        n_stops in 3usize..7,
        trips in (3usize..7).prop_flat_map(|n| vec(trip_spec(n), 1..6)),
        (from, to) in (3usize..7).prop_flat_map(|n| (end(n, "home"), end(n, "work"))),
        by in proptest::option::of(0u32..90_000),
    ) {
        let n = trips.iter().flat_map(|t| t.stops.iter()).map(|&s| s as usize + 1).max().unwrap_or(1).max(n_stops);
        let zip = build_zip_spaced(n, 0.003, &trips);
        let (network, day) = model_day(&zip, DATE);
        let net = RouterNetwork::build_from_day(&network, &day, DATE);
        let known = |e: &End| match e {
            End::Stop(id) => net.stops.iter().any(|s| &s.stop_id == id),
            End::Place(p) => p.walks.iter().all(|w| net.stops.iter().any(|s| s.stop_id == w.stop_id)),
        };
        prop_assume!(known(&from) && known(&to));
        if let Some(it) = latest_departure_between(&net, &from, &to, None, by, MAX_LEGS).unwrap().itinerary {
            check_door_to_door(&it);
        }
    }
}

/// The worked case: a 300 s walk to a bus boarding at 08:10 that
/// arrives 08:30, asked at 08:00 — leave 08:05, 25 minutes door to
/// door, leave in 5 min.
#[test]
fn a_walk_then_a_ride_leaves_at_the_walk_before_the_boarding() {
    let zip = build_zip_spaced(2, 0.003, &[TripSpec { stops: vec![0, 1], start: 29_400, step: 1_200 }]);
    let (network, day) = model_day(&zip, DATE);
    let net = RouterNetwork::build_from_day(&network, &day, DATE);
    let home = End::Place(Place {
        id: "home".into(),
        name: "Home".into(),
        walks: vec![PlaceWalk { stop_id: "s0".into(), walk_secs: 300, path: Vec::new() }],
    });
    let it = earliest_arrival_between(&net, &home, &End::Stop("s1".into()), None, 28_800, 2).unwrap().unwrap();
    assert_eq!(it.leave_secs, 29_100);
    assert_eq!(it.duration_secs, 1_500);
    assert_eq!(it.leave, transit_dispatch::Wait::Minutes { minutes: 5 });
}
