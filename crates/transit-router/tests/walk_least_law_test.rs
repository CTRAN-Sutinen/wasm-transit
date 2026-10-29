//! A plan walks the least it can: at equal arrival and the same rides,
//! every walk is the shortest that still connects (D-35, encoded
//! provisionally).
//!
//! The intent, as the user stated it (a C-TRAN plan walked 13 minutes
//! up Broadway to a route 032 trip's first stop when the same bus
//! passed a stop four minutes away, arriving the same): when another
//! stop of the same bus serves the connection with less walking, the
//! plan uses it. The earliest arrival and the rides are the search's
//! answer and stay; only where each ride is boarded and left moves.
//!
//! The laws, each against an oracle that shares nothing with the
//! router — its own Dijkstra over the network's footpaths (walks chain
//! through stops, as the router's do) — on plans the fixpoint oracle
//! already pins for arrival (`prop_test.rs`):
//!
//! 1. **The first boarding walks least**: no stop of the first ride's
//!    trip before where it is left is reached from the origin with a
//!    shorter walk in time to board there.
//! 2. **Every transfer walks least**: no stop where the earlier ride
//!    could be left (after it is boarded) and stop where the later one
//!    could be boarded (before it is left) are joined by a shorter
//!    walk that still makes the connection.
//! 3. **The last alighting walks least**: no stop of the last ride's
//!    trip after it is boarded reaches the destination by a shorter
//!    walk without arriving later.
//! 4. The arrival is the earliest (the fixpoint oracle), and the plan
//!    is feasible from its departure.

#[path = "prop_support.rs"]
mod support;

use std::collections::BinaryHeap;

use chrono::NaiveDate;
use proptest::collection::vec;
use proptest::prelude::*;
use support::{assert_feasible, build_zip_spaced, model_day, oracle_arrival, TripSpec, INF};
use transit_router::{earliest_arrival, Itinerary, Leg, RouterNetwork};

const DATE: NaiveDate = NaiveDate::from_ymd_opt(2026, 10, 23).unwrap(); // Friday
const MAX_LEGS: usize = 12;

prop_compose! {
    fn trip_spec(n_stops: usize)
        (stops in vec(0u32..(n_stops as u32), 2..=6),
         start in 0u32..3_000,
         // Short hops: a bus is still there to be caught a stop or
         // two on, so most rides offer more than one boarding.
         step in 5u32..150)
    -> TripSpec {
        TripSpec { stops, start, step }
    }
}

/// Stops ~89 m apart: every stop within three of another is a walk,
/// so most rides can be boarded and left at more than one stop.
fn case() -> impl Strategy<Value = (usize, Vec<TripSpec>, usize, usize, u32)> {
    (4usize..=9).prop_flat_map(|n| (Just(n), vec(trip_spec(n), 1..=10), 0..n, 0..n, 0u32..3_000))
}

/// The oracle's walking seconds from `sources` to every stop, over the
/// network's footpaths, chains included.
fn walks_from(net: &RouterNetwork, sources: &[(usize, u32)]) -> Vec<u32> {
    let mut dist = vec![INF; net.stops.len()];
    let mut heap = BinaryHeap::new();
    for &(s, c) in sources {
        if c < dist[s] {
            dist[s] = c;
            heap.push(std::cmp::Reverse((c, s)));
        }
    }
    while let Some(std::cmp::Reverse((d, s))) = heap.pop() {
        if d > dist[s] {
            continue;
        }
        for e in &net.stops[s].footpaths {
            let (t, c) = (e.to as usize, d + e.walk_secs);
            if c < dist[t] {
                dist[t] = c;
                heap.push(std::cmp::Reverse((c, t)));
            }
        }
    }
    dist
}

/// A ride of the plan, as the network holds it: its pattern's stops
/// and its trip's times, and the positions boarded and left.
struct Ride<'a> {
    stops: &'a [u32],
    departures: &'a [u32],
    arrivals: &'a [u32],
    board: usize,
    alight: usize,
}

fn ride<'a>(net: &'a RouterNetwork, leg: &Leg) -> Ride<'a> {
    let Leg::Transit { trip, start_secs, board_stop_id, board_secs, alight_stop_id, alight_secs, .. } = leg else {
        unreachable!()
    };
    let (b, a) = (net.stop_index(board_stop_id).unwrap(), net.stop_index(alight_stop_id).unwrap());
    for p in &net.patterns {
        for t in &p.trips {
            if &t.trip != trip || t.start_secs != *start_secs {
                continue;
            }
            let board = (0..p.stop_positions.len())
                .find(|&i| p.stop_positions[i] == b && t.departures[i] == *board_secs)
                .expect("the boarding is a visit of the trip");
            let alight = (board + 1..p.stop_positions.len())
                .find(|&i| p.stop_positions[i] == a && t.arrivals[i] == *alight_secs)
                .expect("the alighting is a later visit of the trip");
            return Ride { stops: &p.stop_positions, departures: &t.departures, arrivals: &t.arrivals, board, alight };
        }
    }
    panic!("trip {trip} at {start_secs} is not in the network");
}

/// The walking seconds before each ride and after the last.
fn walks(it: &Itinerary) -> Vec<u32> {
    let mut out = vec![0];
    for leg in &it.legs {
        match leg {
            Leg::Walk { walk_secs, .. } => *out.last_mut().unwrap() += walk_secs,
            Leg::Transit { .. } => out.push(0),
        }
    }
    out
}

/// Laws 1–3 over one plan; Err names the shorter walk the plan missed.
fn walks_least(net: &RouterNetwork, it: &Itinerary, from: usize, to: usize) -> Result<(), String> {
    let legs: Vec<&Leg> = it.legs.iter().filter(|l| matches!(l, Leg::Transit { .. })).collect();
    if legs.is_empty() {
        return Ok(());
    }
    let rides: Vec<Ride> = legs.iter().map(|l| ride(net, l)).collect();
    let used = walks(it);
    let name = |s: u32| net.stops[s as usize].stop_id.clone();

    // Law 1: the first boarding.
    let d = walks_from(net, &[(from, 0)]);
    let r = &rides[0];
    for p in 0..r.alight {
        let w = d[r.stops[p] as usize];
        if w < used[0] && it.depart_secs + w <= r.departures[p] {
            return Err(format!("board at {} walking {w} s, not {} s", name(r.stops[p]), used[0]));
        }
    }
    // Law 2: every transfer.
    for i in 0..rides.len() - 1 {
        let (a, b) = (&rides[i], &rides[i + 1]);
        for q in a.board + 1..a.stops.len() {
            let d = walks_from(net, &[(a.stops[q] as usize, 0)]);
            for p in 0..b.alight {
                let w = d[b.stops[p] as usize];
                if w < used[i + 1] && a.arrivals[q] + w <= b.departures[p] {
                    return Err(format!(
                        "transfer {} → {} walking {w} s, not {} s",
                        name(a.stops[q]), name(b.stops[p]), used[i + 1]
                    ));
                }
            }
        }
    }
    // Law 3: the last alighting.
    let r = rides.last().unwrap();
    for q in r.board + 1..r.stops.len() {
        let w = walks_from(net, &[(r.stops[q] as usize, 0)])[to];
        if w < *used.last().unwrap() && r.arrivals[q] + w <= it.arrival_secs {
            return Err(format!("leave at {} walking {w} s, not {} s", name(r.stops[q]), used.last().unwrap()));
        }
    }
    Ok(())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(1024))]

    #[test]
    fn every_walk_is_the_least_that_connects((n, trips, from, to, depart) in case()) {
        let (network, day) = model_day(&build_zip_spaced(n, 0.0008, &trips), DATE);
        let net = RouterNetwork::build_from_day(&network, &day, DATE);
        let (f, t) = (format!("s{from}"), format!("s{to}"));
        let (fi, ti) = (net.stop_index(&f).unwrap() as usize, net.stop_index(&t).unwrap() as usize);
        let Some(it) = earliest_arrival(&net, &f, &t, depart, MAX_LEGS).unwrap() else {
            prop_assert_eq!(oracle_arrival(&net, fi, ti, depart), INF);
            return Ok(());
        };
        prop_assert_eq!(it.arrival_secs, oracle_arrival(&net, fi, ti, depart), "law 4: the earliest arrival");
        assert_feasible(&it, depart);
        if let Err(e) = walks_least(&net, &it, fi, ti) {
            prop_assert!(false, "{e} — {:?}", it.legs);
        }
    }
}

/// The Broadway case: a bus that starts three stops up the street and
/// comes back past the stop next door. The rider boards next door —
/// the same bus, the same arrival, a walk of one stop, not three.
#[test]
fn the_rider_boards_the_same_bus_at_the_nearer_stop() {
    // s0 … s9 in a line, ~89 m apart (69 s on foot). The bus starts at
    // s3, passes s1, and ends at s9.
    let trips = [TripSpec { stops: vec![3, 1, 9], start: 1_250, step: 10 }];
    let (network, day) = model_day(&build_zip_spaced(10, 0.0008, &trips), DATE);
    let net = RouterNetwork::build_from_day(&network, &day, DATE);
    let it = earliest_arrival(&net, "s0", "s9", 900, 4).unwrap().expect("walk, ride");
    assert_eq!(it.arrival_secs, 1_270);
    match &it.legs[..] {
        [Leg::Walk { to_stop_id, .. }, Leg::Transit { board_stop_id, .. }] => {
            assert_eq!((to_stop_id.as_str(), board_stop_id.as_str()), ("s1", "s1"), "{:?}", it.legs)
        }
        legs => panic!("{legs:?}"),
    }
}
