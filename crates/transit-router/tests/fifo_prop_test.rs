//! A RAPTOR law the oracle tests don't state: the network is FIFO —
//! leaving later never gets you there earlier. Two departures from
//! the same stop, the later one at least as late as the earlier:
//!
//! * when both reach, `arrival(later) >= arrival(earlier)` — a later
//!   start cannot beat an earlier one's best arrival;
//! * when the earlier reaches and the later doesn't (missed the only
//!   bus), that is legitimate and not a FIFO violation.
//!
//! This is a different bug class than the brute-force fixpoint
//! oracle: a board/alight window off-by-one can be consistently wrong
//! in both, but not violate monotonicity in one and not the other.

use chrono::NaiveDate;
use proptest::collection::vec;
use proptest::prelude::*;
use transit_router::{earliest_arrival, RouterNetwork};

#[path = "prop_support.rs"]
mod support;
use support::{build_zip, model_day, TripSpec};

const DATE: NaiveDate = NaiveDate::from_ymd_opt(2026, 10, 23).unwrap();
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

    /// FIFO: a later departure never arrives earlier than an earlier
    /// one's optimal arrival (when both reach at all).
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
