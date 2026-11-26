//! Pins plans whose ends are stops or places (D-33). The oracle is
//! `prop_support::oracle_arrival`, which shares no code with the search. The
//! properties:
//!
//! 1. Leave-at: the arrival is the best over every access walk, the stop
//!    oracle, and every egress walk, and over the direct walk. The itinerary
//!    is feasible from place to place. "No plan" means no combination reaches.
//! 2. Arrive-by: the departure is the latest from which that best arrival
//!    still meets the deadline.
//! 3. Properties 1 and 2 hold over the router's own stop search at `k` legs.
//! 4. A stop plans like a place with one zero-second walk to it.
//! 5. A place id that names a stop is refused, and so is a walk that names an
//!    unknown stop.
//! 6. The direct walk wins its ties.

use transit_date::Date;
use proptest::collection::vec;
use proptest::prelude::*;
use transit_router::{
    earliest_arrival, earliest_arrival_between, latest_departure, latest_departure_between, End, Leg, Place,
    PlaceWalk, RouterError, RouterNetwork,
};

#[path = "prop_support.rs"]
mod support;
use support::{assert_feasible, build_zip_spaced, model_day, oracle_arrival, TripSpec, INF};

const DATE: Date = Date::from_ymd_opt(2026, 10, 23).unwrap(); // Friday
const MAX_LEGS: usize = 12;

prop_compose! {
    fn trip_spec(n_stops: usize)
        (stops in vec(0u32..(n_stops as u32), 2..=5),
         start in 0u32..86_000,
         step in 100u32..900)
    -> TripSpec {
        TripSpec { stops, start, step }
    }
}

fn place_walks(n_stops: usize) -> impl Strategy<Value = Vec<(usize, u32)>> {
    vec((0..n_stops, 0u32..600), 0..=3)
}

#[derive(Debug, Clone)]
enum GenEnd {
    Stop(usize),
    Place(Vec<(usize, u32)>),
}

fn end(n_stops: usize) -> impl Strategy<Value = GenEnd> {
    prop_oneof![
        (0..n_stops).prop_map(GenEnd::Stop),
        place_walks(n_stops).prop_map(GenEnd::Place),
    ]
}

type Case = (usize, f64, Vec<TripSpec>, GenEnd, GenEnd, Option<u32>, u32);

fn case() -> impl Strategy<Value = Case> {
    (3usize..=8).prop_flat_map(|n_stops| {
        (
            Just(n_stops),
            // About 89 m apart (every near pair is a transfer) or about 333 m
            // (none).
            prop_oneof![Just(0.0008), Just(0.003)],
            vec(trip_spec(n_stops), 1..=10),
            end(n_stops),
            end(n_stops),
            proptest::option::of(0u32..900),
            0u32..86_000,
        )
    })
}

fn as_end(e: &GenEnd, id: &str) -> End {
    match e {
        GenEnd::Stop(s) => End::Stop(format!("s{s}")),
        GenEnd::Place(walks) => End::Place(Place {
            id: id.into(),
            name: format!("Place {id}"),
            walks: walks.iter().map(|(s, w)| PlaceWalk { stop_id: format!("s{s}"), walk_secs: *w, path: vec![] }).collect(),
        }),
    }
}

fn sides(net: &RouterNetwork, e: &GenEnd) -> Vec<(usize, u32)> {
    let idx = |s: &usize| net.stop_index(&format!("s{s}")).unwrap() as usize;
    match e {
        GenEnd::Stop(s) => vec![(idx(s), 0)],
        GenEnd::Place(walks) => walks.iter().map(|(s, w)| (idx(s), *w)).collect(),
    }
}

fn oracle(net: &RouterNetwork, from: &GenEnd, to: &GenEnd, direct: Option<u32>, depart: u32) -> u32 {
    let mut best = direct.map_or(INF, |w| depart + w);
    for (a, wa) in sides(net, from) {
        for &(e, we) in &sides(net, to) {
            let t = oracle_arrival(net, a, e, depart + wa);
            if t != INF {
                best = best.min(t + we);
            }
        }
    }
    best
}

fn network(n_stops: usize, spacing: f64, trips: &[TripSpec]) -> RouterNetwork {
    let (network, day) = model_day(&build_zip_spaced(n_stops, spacing, trips), DATE);
    RouterNetwork::build_from_day(&network, &day, DATE)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// Property 1.
    #[test]
    fn a_trip_from_a_place_walks_rides_and_walks(
        (n_stops, spacing, trips, from, to, direct, depart) in case()
    ) {
        let net = network(n_stops, spacing, &trips);
        let (f, t) = (as_end(&from, "here"), as_end(&to, "there"));
        let plan = earliest_arrival_between(&net, &f, &t, direct, depart, MAX_LEGS).expect("valid query");
        let want = oracle(&net, &from, &to, direct, depart);
        match plan {
            None => prop_assert_eq!(want, INF, "no plan, yet the oracle arrives"),
            Some(it) => {
                prop_assert_eq!(it.arrival_secs, want);
                assert_feasible(&it, depart);
                let id = |e: &End| match e { End::Stop(s) => s.clone(), End::Place(p) => p.id.clone() };
                prop_assert_eq!(&it.from_stop_id, &id(&f));
                prop_assert_eq!(&it.to_stop_id, &id(&t));
            }
        }
    }

    /// Property 2.
    #[test]
    fn arrive_by_from_a_place_leaves_as_late_as_it_can(
        (n_stops, spacing, trips, from, to, direct, deadline) in case()
    ) {
        let net = network(n_stops, spacing, &trips);
        let (f, t) = (as_end(&from, "here"), as_end(&to, "there"));
        let answer = latest_departure_between(&net, &f, &t, direct, Some(deadline), MAX_LEGS).expect("valid query");
        prop_assert_eq!(answer.arrive_by_secs, deadline);
        match answer.itinerary {
            Some(it) => {
                let d = it.depart_secs;
                assert_feasible(&it, d);
                prop_assert!(it.arrival_secs <= deadline);
                prop_assert!(oracle(&net, &from, &to, direct, d) <= deadline);
                let later = oracle(&net, &from, &to, direct, d + 1);
                prop_assert!(later == INF || later > deadline, "leaving at {} still arrives {}", d + 1, later);
            }
            None => {
                let at_zero = oracle(&net, &from, &to, direct, 0);
                prop_assert!(at_zero == INF || at_zero > deadline, "no answer, yet leaving at 0 arrives {at_zero}");
            }
        }
    }

    /// Property 3.
    #[test]
    fn the_leg_bound_holds_from_a_place(
        (n_stops, spacing, trips, from, to, direct, depart) in case(),
        legs in 1usize..=3,
    ) {
        let net = network(n_stops, spacing, &trips);
        let (f, t) = (as_end(&from, "here"), as_end(&to, "there"));
        let mut want = direct.map_or(INF, |w| depart + w);
        for (a, wa) in sides(&net, &from) {
            for &(e, we) in &sides(&net, &to) {
                let (a_id, e_id) = (&net.stops[a].stop_id, &net.stops[e].stop_id);
                if let Some(it) = earliest_arrival(&net, a_id, e_id, depart + wa, legs).unwrap() {
                    want = want.min(it.arrival_secs + we);
                }
            }
        }
        let got = earliest_arrival_between(&net, &f, &t, direct, depart, legs).unwrap();
        prop_assert_eq!(got.as_ref().map_or(INF, |it| it.arrival_secs), want);
        if let Some(it) = got {
            let transit = it.legs.iter().filter(|l| matches!(l, Leg::Transit { .. })).count();
            prop_assert!(transit <= legs, "{transit} legs > {legs}");
        }
    }

    /// Property 4, for leave-at and arrive-by.
    #[test]
    fn a_stop_is_a_place_with_one_zero_second_walk(
        (n_stops, spacing, trips, _from, _to, _direct, depart) in case(),
        from in 0usize..3, to in 0usize..3,
    ) {
        let net = network(n_stops, spacing, &trips);
        let (fs, ts) = (format!("s{from}"), format!("s{to}"));
        let place = |id: &str, s: &str| End::Place(Place {
            id: id.into(), name: id.into(), walks: vec![PlaceWalk { stop_id: s.into(), walk_secs: 0, path: vec![] }],
        });
        let stops = earliest_arrival(&net, &fs, &ts, depart, MAX_LEGS).unwrap().map(|it| it.arrival_secs);
        let places = earliest_arrival_between(&net, &place("here", &fs), &place("there", &ts), None, depart, MAX_LEGS)
            .unwrap().map(|it| it.arrival_secs);
        prop_assert_eq!(stops, places);
        let stops = latest_departure(&net, &fs, &ts, Some(depart), MAX_LEGS).unwrap();
        let places = latest_departure_between(&net, &place("here", &fs), &place("there", &ts), None, Some(depart), MAX_LEGS).unwrap();
        prop_assert_eq!(stops.itinerary.map(|it| it.depart_secs), places.itinerary.map(|it| it.depart_secs));
    }
}

fn line() -> RouterNetwork {
    network(3, 0.003, &[TripSpec { stops: vec![0, 2], start: 28_800, step: 600 }])
}

fn place(id: &str, walks: &[(&str, u32)]) -> End {
    End::Place(Place {
        id: id.into(),
        name: format!("Place {id}"),
        walks: walks.iter().map(|(s, w)| PlaceWalk { stop_id: (*s).into(), walk_secs: *w, path: vec![] }).collect(),
    })
}

/// Property 5. A place id that names a stop would give an itinerary two
/// things under one name.
#[test]
fn a_place_is_not_a_stop_and_walks_only_to_stops() {
    let net = line();
    let stop = End::Stop("s2".into());
    let r = earliest_arrival_between(&net, &place("s0", &[("s0", 60)]), &stop, None, 28_000, 3);
    assert!(matches!(&r, Err(RouterError::PlaceIsStop(id)) if id == "s0"), "{r:?}");
    let r = earliest_arrival_between(&net, &place("here", &[("nowhere", 60)]), &stop, None, 28_000, 3);
    assert!(matches!(&r, Err(RouterError::UnknownStop(id)) if id == "nowhere"), "{r:?}");
}

#[test]
fn the_walks_are_the_first_and_last_legs() {
    let net = line();
    let it = earliest_arrival_between(
        &net, &place("here", &[("s0", 240)]), &place("there", &[("s2", 90)]), None, 28_000, 3,
    ).unwrap().expect("walk, ride, walk");
    assert_eq!(it.arrival_secs, 28_800 + 600 + 90);
    match &it.legs[..] {
        [Leg::Walk { from_stop_id: a, from_stop_name, to_stop_id: b, walk_secs: 240, .. },
         Leg::Transit { board_stop_id, alight_stop_id, .. },
         Leg::Walk { from_stop_id: c, to_stop_id: d, to_stop_name, walk_secs: 90, .. }] => {
            assert_eq!((a.as_str(), b.as_str(), c.as_str(), d.as_str()), ("here", "s0", "s2", "there"));
            assert_eq!((from_stop_name.as_str(), to_stop_name.as_str()), ("Place here", "Place there"));
            assert_eq!((board_stop_id.as_str(), alight_stop_id.as_str()), ("s0", "s2"));
        }
        legs => panic!("{legs:?}"),
    }
}

/// Property 6. When walking arrives as early as the bus, the rider walks in
/// one leg from place to place. One second later on foot, the rider rides.
#[test]
fn the_direct_walk_wins_its_ties() {
    let net = line();
    let (here, there) = (place("here", &[("s0", 0)]), place("there", &[("s2", 0)]));
    let it = earliest_arrival_between(&net, &here, &there, Some(1_400), 28_000, 3).unwrap().unwrap();
    assert_eq!(it.arrival_secs, 29_400);
    assert!(
        matches!(&it.legs[..], [Leg::Walk { from_stop_id, to_stop_id, walk_secs: 1_400, via_stop_ids, .. }]
            if from_stop_id == "here" && to_stop_id == "there" && via_stop_ids.is_empty()),
        "{:?}", it.legs
    );
    let it = earliest_arrival_between(&net, &here, &there, Some(1_401), 28_000, 3).unwrap().unwrap();
    assert!(it.legs.iter().any(|l| matches!(l, Leg::Transit { .. })), "{:?}", it.legs);
}

#[test]
fn a_place_with_no_walks_is_unreachable() {
    let net = line();
    let lost = place("lost", &[]);
    assert!(earliest_arrival_between(&net, &lost, &End::Stop("s2".into()), None, 28_000, 3).unwrap().is_none());
    assert!(earliest_arrival_between(&net, &End::Stop("s0".into()), &lost, None, 28_000, 3).unwrap().is_none());
    let by = latest_departure_between(&net, &End::Stop("s0".into()), &lost, None, None, 3).unwrap();
    assert!(by.itinerary.is_none());
}
