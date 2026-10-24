//! Property-based tests for RAPTOR.
//!
//! The oracle is a deliberately dumb implementation: Bellman-Ford-style
//! relaxation over every (trip, board position, alight position) pair
//! and every footpath, iterated to a fixpoint. It shares no code with
//! the round-based search — if RAPTOR ever claims an arrival earlier
//! than the oracle, or misses a connection the oracle finds, the test
//! fails and proptest shrinks the feed to the smallest counterexample.

use chrono::NaiveDate;
use proptest::collection::vec;
use proptest::prelude::*;
use transit_core::Gtfs;
use transit_router::{earliest_arrival, Itinerary, Leg, RouterNetwork};

#[path = "prop_support.rs"]
mod support;
use support::{build_zip, TripSpec};

const INF: u32 = u32::MAX;
const DATE: NaiveDate = NaiveDate::from_ymd_opt(2026, 10, 23).unwrap(); // Friday
/// Enough legs for any path through feeds this small.
const MAX_LEGS: usize = 12;

// ---------------------------------------------------------------------
// generator
// ---------------------------------------------------------------------

prop_compose! {
    fn trip_spec(n_stops: usize)
        (stops in vec(0u32..(n_stops as u32), 2..=5),
         start in 0u32..86_000,
         step in 100u32..900)
    -> TripSpec {
        TripSpec { stops, start, step }
    }
}

fn feed()
-> impl Strategy<Value = (usize, Vec<TripSpec>, usize, usize, u32)> {
    (3usize..=8).prop_flat_map(|n_stops| {
        (
            Just(n_stops),
            vec(trip_spec(n_stops), 1..=10),
            0usize..n_stops,
            0usize..n_stops,
            0u32..86_000,
        )
    })
}

// ---------------------------------------------------------------------
// oracle
// ---------------------------------------------------------------------

fn oracle_earliest_arrival(net: &RouterNetwork, from: usize, to: usize, depart: u32) -> u32 {
    let n = net.stops.len();
    let mut arr = vec![INF; n];
    arr[from] = depart;
    for _ in 0..128 {
        let mut changed = false;
        for pattern in &net.patterns {
            for trip in &pattern.trips {
                for i in 0..pattern.stop_positions.len() {
                    let si = pattern.stop_positions[i] as usize;
                    if arr[si] == INF || arr[si] > trip.departures[i] {
                        continue; // can't board this trip at position i
                    }
                    for j in i + 1..pattern.stop_positions.len() {
                        let sj = pattern.stop_positions[j] as usize;
                        if trip.arrivals[j] < arr[sj] {
                            arr[sj] = trip.arrivals[j];
                            changed = true;
                        }
                    }
                }
            }
        }
        for (s, node) in net.stops.iter().enumerate() {
            if arr[s] == INF {
                continue;
            }
            for edge in &node.footpaths {
                let candidate = arr[s] + edge.walk_secs;
                let t = edge.to as usize;
                if candidate < arr[t] {
                    arr[t] = candidate;
                    changed = true;
                }
            }
        }
        if !changed {
            break;
        }
    }
    arr[to]
}

// ---------------------------------------------------------------------
// properties
// ---------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    /// RAPTOR's arrival must exactly match the fixpoint oracle, and the
    /// reconstructed itinerary must be internally consistent and end at
    /// that arrival time.
    #[test]
    fn matches_oracle_and_reconstructs(
        (n_stops, trips, from, to, depart) in feed()
    ) {
        let gtfs = Gtfs::from_zip_bytes(&build_zip(n_stops, &trips)).expect("generated feed parses");
        let net = RouterNetwork::build(&gtfs, DATE);

        // Resolve the generated stop numbers the same way RAPTOR does:
        // by id, not by index (network stop order is a HashMap artifact).
        let from_id = format!("s{from}");
        let to_id = format!("s{to}");
        let from_idx = net.stop_index(&from_id).expect("from exists") as usize;
        let to_idx = net.stop_index(&to_id).expect("to exists") as usize;
        let expected = oracle_earliest_arrival(&net, from_idx, to_idx, depart);

        let plan = earliest_arrival(&net, &from_id, &to_id, depart, MAX_LEGS)
            .expect("valid query");

        match (plan, expected == INF) {
            (None, true) => {} // both agree: unreachable
            (None, false) => panic!(
                "RAPTOR found no plan; oracle arrives at {expected}. \
                 n_stops={n_stops} from={from} to={to} depart={depart} trips={trips:?}"
            ),
            (Some(_), true) => panic!("RAPTOR claims to reach the unreachable"),
            (Some(it), false) => {
                prop_assert_eq!(it.arrival_secs, expected);
                assert_itinerary_consistent(&it, depart);
            }
        }
    }

    /// Determinism: identical queries give identical answers.
    #[test]
    fn deterministic((n_stops, trips, from, to, depart) in feed()) {
        let gtfs = Gtfs::from_zip_bytes(&build_zip(n_stops, &trips)).expect("parse");
        let net = RouterNetwork::build(&gtfs, DATE);
        let a = earliest_arrival(&net, &format!("s{from}"), &format!("s{to}"), depart, MAX_LEGS).unwrap();
        let b = earliest_arrival(&net, &format!("s{from}"), &format!("s{to}"), depart, MAX_LEGS).unwrap();
        prop_assert_eq!(a.map(|i| i.arrival_secs), b.map(|i| i.arrival_secs));
    }

    /// More legs never hurt: the arrival with MAX_LEGS is at least as
    /// early as with fewer.
    #[test]
    fn more_legs_never_hurt((n_stops, trips, from, to, depart) in feed()) {
        let gtfs = Gtfs::from_zip_bytes(&build_zip(n_stops, &trips)).expect("parse");
        let net = RouterNetwork::build(&gtfs, DATE);
        let few = earliest_arrival(&net, &format!("s{from}"), &format!("s{to}"), depart, 1)
            .unwrap().map(|i| i.arrival_secs);
        let many = earliest_arrival(&net, &format!("s{from}"), &format!("s{to}"), depart, MAX_LEGS)
            .unwrap().map(|i| i.arrival_secs);
        if let (Some(few), Some(many)) = (few, many) {
            prop_assert!(many <= few, "more legs arrived later: {many} > {few}");
        }
    }
}

/// The itinerary must make chronological sense: legs connect stop to
/// stop, nothing boards before the previous leg ended, and the last leg
/// ends exactly at the promised arrival.
fn assert_itinerary_consistent(it: &Itinerary, depart: u32) {
    let mut t = depart;
    let mut place: Option<&str> = None;
    let mut legs = 0;
    for leg in &it.legs {
        match leg {
            Leg::Walk { from_stop_id, to_stop_id, walk_secs, .. } => {
                if let Some(p) = place {
                    assert_eq!(p, from_stop_id, "walk starts where the last leg ended");
                }
                assert!(*walk_secs > 0, "zero-length walk");
                t += walk_secs;
                place = Some(to_stop_id);
            }
            Leg::Transit { board_stop_id, board_secs, alight_stop_id, alight_secs, .. } => {
                if let Some(p) = place {
                    assert_eq!(p, board_stop_id, "transit boards where the last leg ended");
                }
                assert!(*board_secs >= t, "boards before the previous leg ended");
                assert!(*alight_secs >= *board_secs, "alights before boarding");
                t = *alight_secs;
                place = Some(alight_stop_id);
                legs += 1;
            }
        }
    }
    assert!(legs <= MAX_LEGS, "too many transit legs");
    if let Some(p) = place {
        assert_eq!(p, it.to_stop_id, "itinerary must end at the destination");
    }
    assert_eq!(t, it.arrival_secs, "last leg must end at the promised arrival");
}
