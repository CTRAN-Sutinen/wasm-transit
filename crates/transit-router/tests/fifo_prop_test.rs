//! Pins that a later departure from the same stop never arrives earlier. The
//! property needs no oracle, so it also catches a boarding-window off-by-one
//! that the fixpoint oracle shares.

use proptest::collection::vec;
use proptest::prelude::*;
use transit_date::Date;
use transit_router::{earliest_arrival, RouterNetwork};

#[path = "prop_support.rs"]
mod support;
use support::{build_zip, model_day, TripSpec};

const DATE: Date = Date::from_ymd_opt(2026, 10, 23).unwrap();
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

fn feed() -> impl Strategy<Value = (usize, Vec<TripSpec>, usize, usize, u32, u32)> {
    (3usize..=8).prop_flat_map(|n_stops| {
        (
            Just(n_stops),
            vec(trip_spec(n_stops), 1..=10),
            0usize..n_stops,
            0usize..n_stops,
            0u32..86_000,
            1u32..3_600, // how much later the second departure is
        )
    })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn later_departures_never_arrive_earlier(
        (n_stops, trips, from, to, depart, later) in feed(),
    ) {
        let (network, day) = model_day(&build_zip(n_stops, &trips), DATE);
        let net = RouterNetwork::build_from_day(&network, &day, DATE);
        let early = earliest_arrival(&net, &format!("s{from}"), &format!("s{to}"), depart, MAX_LEGS).expect("valid query");
        let late = earliest_arrival(&net, &format!("s{from}"), &format!("s{to}"), depart + later, MAX_LEGS).expect("valid query");

        if let (Some(e), Some(l)) = (early, late) {
            prop_assert!(l.arrival_secs >= e.arrival_secs,
                "departing {}s later arrived {}s earlier",
                later, e.arrival_secs - l.arrival_secs);
        }
    }
}
