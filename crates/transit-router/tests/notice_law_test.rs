//! Detour notice laws (QAF-74: notices on ANY change).
//!
//! The intent, as the user stated it: a compared plan's notice fires
//! whenever the active (diverted) itinerary arrives later than the
//! baseline OR rides a diverted trip — a trip instance whose patched
//! stop times differ from its unpatched instance — not only when the
//! active world has no trip. The notice is structured (its kind, the
//! time lost, the diverted routes and the stops they skip) and
//! carries one text line. `plan_notice` is the one place it is
//! computed.
//!
//! The laws, over generated days and a generated diversion of one
//! trip (an interior stop skipped, or the trip delayed from a visit
//! on — the two things a diversion does to a day):
//!
//! 1. no notice exactly when the active plan exists, arrives no later
//!    than the baseline (or there is no baseline), and rides no
//!    diverted trip;
//! 2. the kind: no active plan → `unreachable` when the baseline has
//!    one, `no_trip` when neither does; an active plan arriving later
//!    → `later`, with the seconds lost and the minutes (rounded up —
//!    a rider is never told a loss is smaller than it is); otherwise
//!    `diverted`;
//! 3. every diverted trip the active plan rides is named: its route,
//!    and the stops it skips;
//! 4. the notice always says something.

use chrono::NaiveDate;
use proptest::collection::vec;
use proptest::prelude::*;
use transit_network::MaterializedTrip;
use transit_router::{diverted_instances, earliest_arrival, plan_notice, Leg, NoticeKind, RouterNetwork};

#[path = "prop_support.rs"]
mod support;
use support::{build_zip_spaced, model_day, TripSpec};

const DATE: NaiveDate = NaiveDate::from_ymd_opt(2026, 10, 23).unwrap();
const MAX_LEGS: usize = 6;

#[derive(Debug, Clone)]
enum Divert {
    /// Skip the interior visit at this index (mod the interior).
    Skip(usize),
    /// Delay every visit from this index on by this many seconds.
    Delay(usize, u32),
}

prop_compose! {
    fn trip_spec(n_stops: usize)
        (stops in vec(0u32..(n_stops as u32), 2..=5),
         start in 0u32..86_000,
         step in 100u32..900)
    -> TripSpec {
        TripSpec { stops, start, step }
    }
}

fn divert() -> impl Strategy<Value = Divert> {
    prop_oneof![
        (0usize..5).prop_map(Divert::Skip),
        (0usize..5, 0u32..1_200).prop_map(|(i, s)| Divert::Delay(i, s)),
    ]
}

#[allow(clippy::type_complexity)]
fn world() -> impl Strategy<Value = (usize, f64, Vec<TripSpec>, usize, Divert, usize, usize, u32)> {
    (3usize..=8).prop_flat_map(|n_stops| {
        (
            Just(n_stops),
            prop_oneof![Just(0.0008), Just(0.003)],
            vec(trip_spec(n_stops), 1..=8),
            0usize..8,
            divert(),
            0usize..n_stops,
            0usize..n_stops,
            0u32..86_000,
        )
    })
}

/// The patched day: one trip diverted, everything else untouched.
fn patch(day: &[MaterializedTrip], which: usize, how: &Divert) -> Vec<MaterializedTrip> {
    let mut out = day.to_vec();
    let target = which % out.len();
    let st = &mut out[target].stop_times;
    match *how {
        Divert::Skip(i) => {
            if st.len() > 2 {
                st.remove(1 + i % (st.len() - 2));
            }
        }
        Divert::Delay(i, secs) => {
            let from = i % st.len();
            for v in st.iter_mut().skip(from) {
                v.arrival_secs += secs;
                v.departure_secs += secs;
            }
        }
    }
    out
}

/// The stops a patched trip no longer visits (multiset difference) —
/// recomputed here from the two days, not from the router.
fn skipped(base: &MaterializedTrip, patched: &MaterializedTrip) -> Vec<String> {
    let mut left: Vec<&str> = patched.stop_times.iter().map(|s| s.stop.as_str()).collect();
    let mut out = Vec::new();
    for s in &base.stop_times {
        match left.iter().position(|x| *x == s.stop) {
            Some(i) => {
                left.remove(i);
            }
            None => out.push(s.stop.clone()),
        }
    }
    out
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn a_notice_fires_on_any_change((n_stops, spacing, trips, which, how, from, to, depart) in world()) {
        let (network, day) = model_day(&build_zip_spaced(n_stops, spacing, &trips), DATE);
        prop_assume!(!day.is_empty());
        let patched = patch(&day, which, &how);
        let base_net = RouterNetwork::build_from_day(&network, &day, DATE);
        let active_net = RouterNetwork::build_from_day(&network, &patched, DATE);
        let (f, t) = (format!("s{from}"), format!("s{to}"));
        let active = earliest_arrival(&active_net, &f, &t, depart, MAX_LEGS).unwrap();
        let baseline = earliest_arrival(&base_net, &f, &t, depart, MAX_LEGS).unwrap();

        // The diverted instances, by definition: patched stop times
        // differ from the unpatched instance's.
        let diverted_here: Vec<(&MaterializedTrip, &MaterializedTrip)> = day
            .iter()
            .zip(&patched)
            .filter(|(b, p)| b.stop_times != p.stop_times)
            .collect();
        let rides_diverted: Vec<(&MaterializedTrip, &MaterializedTrip)> = active
            .iter()
            .flat_map(|it| &it.legs)
            .filter_map(|l| match l {
                Leg::Transit { trip, start_secs, .. } => diverted_here
                    .iter()
                    .find(|(b, _)| &b.trip == trip && b.start_secs == *start_secs)
                    .copied(),
                _ => None,
            })
            .collect();

        let diverted = diverted_instances(&day, &patched);
        let notice = plan_notice(&network, active.as_ref(), baseline.as_ref(), MAX_LEGS as u32, &diverted);

        let later = match (&active, &baseline) {
            (Some(a), Some(b)) => a.arrival_secs > b.arrival_secs,
            _ => false,
        };
        // Law 1.
        let quiet = active.is_some() && !later && rides_diverted.is_empty();
        prop_assert_eq!(notice.is_none(), quiet, "notice {:?}", notice);
        let Some(notice) = notice else { return Ok(()) };

        // Law 2.
        let kind = match (&active, &baseline) {
            (None, Some(_)) => NoticeKind::Unreachable,
            (None, None) => NoticeKind::NoTrip,
            _ if later => NoticeKind::Later,
            _ => NoticeKind::Diverted,
        };
        prop_assert_eq!(notice.kind, kind);
        if kind == NoticeKind::Later {
            let lost = active.as_ref().unwrap().arrival_secs - baseline.as_ref().unwrap().arrival_secs;
            prop_assert_eq!(notice.delay_secs, Some(lost));
            prop_assert_eq!(notice.minutes_lost, Some(lost.div_ceil(60)));
            prop_assert!(notice.text.contains(&format!("{} min", lost.div_ceil(60))), "{}", notice.text);
        }

        // Law 3.
        for (b, p) in rides_diverted {
            let route = network.routes.get(&b.route).map(|r| r.label().to_string()).unwrap();
            prop_assert!(notice.routes.contains(&route), "route {route} unnamed: {:?}", notice.routes);
            for s in skipped(b, p) {
                prop_assert!(notice.skipped_stops.contains(&s), "skipped {s} unnamed: {:?}", notice.skipped_stops);
            }
        }

        // Law 4.
        prop_assert!(!notice.text.trim().is_empty());
    }
}
