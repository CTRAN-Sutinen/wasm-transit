//! When a stopped route next runs at a stop (D-42).
//!
//! The intent, as the user decided it (D-42, 2026-11-05): when a route
//! and destination has stopped for the day at a stop, the display says
//! when service resumes ("Next: 5:12 am tomorrow", "Next: Monday").
//! Look ahead day by day for up to a week, each day's in-force edits
//! read and never the live feed; say nothing past a week. "A week
//! sounds good to me."
//!
//! The laws, over generated weekly service, calendar ends, and
//! diversions that take the stop away on some dates:
//!
//! 1. **A group with departures left today has no next.**
//! 2. **A stopped group's next is the first later day that serves it**:
//!    the earliest departure of that route to that destination from
//!    the stop on the first of the next seven dates whose PATCHED day
//!    (the edits in force that date) carries one — its date, how many
//!    days ahead, the date's weekday, and the departure.
//! 3. **Nothing past a week**: no such departure on any of the seven
//!    dates, no next.
//! 4. **Never the live feed**: the live document changes today's rows
//!    only; a next is the same with or without it.
//! 5. **Nothing already gone**: a later day's departure counts only if
//!    it leaves at or after the moment asked (`from_secs`, which may
//!    run past midnight into the next calendar day).

use std::collections::BTreeMap;

use chrono::{Datelike, Days, NaiveDate};
use proptest::prelude::*;
use transit_dispatch::{grouped_board, with_next_service, Day, NEXT_SERVICE_DAYS};
use transit_live::{LiveDay, LiveStatus, LiveTrip, OnTimeWindow};
use transit_network::{
    CalendarRule, DateRange, Departs, Network, Pattern, PatternStop, PatternTrips, Route, ServiceType, Stop,
    StopOffset, TimingProfile, Trip, WeekdaySet,
};
use transit_sync::{Diversion, EditRecord, EditStatus};

const DAY_SECS: u32 = 86_400;

fn today() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 10, 23).unwrap() // a Friday
}

#[derive(Debug, Clone)]
struct GenRoute {
    /// The weekdays it runs besides Friday (today), Monday first.
    days: [bool; 7],
    /// The calendar's last date, days after today.
    ends_after: u64,
    starts: Vec<u32>,
    /// A diversion skipping X on these days after today (inclusive).
    diverted: Option<(u64, u64)>,
}

fn gen_route() -> impl Strategy<Value = GenRoute> {
    (
        prop::array::uniform7(prop::bool::weighted(0.4)),
        0u64..10,
        prop::collection::btree_set(18_000u32..90_000, 1..5),
        prop::option::of((0u64..9, 0u64..4)),
    )
        .prop_map(|(mut days, ends_after, starts, diverted)| {
            days[4] = true; // Friday: every route serves the stop today
            GenRoute {
                days,
                ends_after,
                starts: starts.into_iter().collect(),
                diverted: diverted.map(|(from, len)| (from, from + len)),
            }
        })
}

/// Route r runs A → X → T{r}; X is 600 s in.
fn build(routes: &[GenRoute]) -> Network {
    let plain = |name: &str| Stop { name: name.into(), lat: 45.5, lon: -122.5, ..Default::default() };
    let mut stops = BTreeMap::new();
    stops.insert("A".to_string(), plain("Origin"));
    stops.insert("X".to_string(), plain("The Stop"));
    for r in 0..routes.len() {
        stops.insert(format!("T{r}"), plain(&format!("Terminal {r}")));
    }
    let mut net = Network { name: "Next service laws".into(), stops, ..Default::default() };
    for (r, route) in routes.iter().enumerate() {
        let st = format!("S{r}");
        let d = route.days;
        let days = WeekdaySet { mon: d[0], tue: d[1], wed: d[2], thu: d[3], fri: d[4], sat: d[5], sun: d[6] };
        net.service_types.insert(
            st.clone(),
            ServiceType {
                name: st.clone(),
                rule: CalendarRule {
                    days,
                    ranges: vec![DateRange {
                        start: today(),
                        end: today().checked_add_days(Days::new(route.ends_after)).unwrap(),
                    }],
                    exceptions: vec![],
                },
            },
        );
        let rid = format!("r{r}");
        net.routes.insert(rid.clone(), Route { name: format!("{r}"), ..Default::default() });
        let pid = format!("p{r}");
        let ids = ["A".to_string(), "X".to_string(), format!("T{r}")];
        let offsets = vec![0, 600, 1_200];
        net.patterns.insert(
            pid.clone(),
            Pattern {
                direction: None,
                route: rid.clone(),
                name: pid.clone(),
                alignment: None,
                stops: ids
                    .iter()
                    .enumerate()
                    .map(|(i, s)| PatternStop { stop: s.clone(), timepoint: true, distance_m: i as f64 * 1000.0, ..Default::default() })
                    .collect(),
                timing: vec![TimingProfile { start_secs: 0, end_secs: 172_800, at_timepoints: offsets.clone() }],
            },
        );
        net.pattern_trips.insert(
            pid.clone(),
            PatternTrips {
                pattern: pid.clone(),
                service_type: st,
                trips: route
                    .starts
                    .iter()
                    .map(|&s| Trip {
                        id: format!("r{r}@{s}"),
                        departs: Departs::At { start_secs: s },
                        times: offsets.iter().map(|&t| StopOffset { arrival_secs: Some(t), departure_secs: Some(t) }).collect(),
                        ..Default::default()
                    })
                    .collect(),
            },
        );
    }
    net
}

fn edits(routes: &[GenRoute]) -> Vec<EditRecord> {
    routes
        .iter()
        .enumerate()
        .filter_map(|(r, route)| {
            let (from, to) = route.diverted?;
            Some(EditRecord {
                id: format!("e{r}"),
                diversion: Some(Diversion {
                    id: format!("d{r}"),
                    route_id: format!("r{r}"),
                    direction_id: None,
                    first_kept_stop: "A".into(),
                    last_kept_stop: format!("T{r}"),
                    skipped_stops: vec!["X".into()],
                    detour: None,
                    added_secs: 0,
                    start_date: today().checked_add_days(Days::new(from)).unwrap(),
                    end_date: today().checked_add_days(Days::new(to)).unwrap(),
                }),
                cancel: None,
                status: EditStatus::Accepted,
                rev: 1,
                updated_at: 1,
            })
        })
        .collect()
}

/// The oracle: route r's next departure from X after today, from the
/// generated calendar alone — (days ahead, departure secs).
fn oracle(route: &GenRoute, from_secs: u32) -> Option<(u32, u32)> {
    for k in 1..=7u64 {
        let d = today().checked_add_days(Days::new(k)).unwrap();
        let wd = d.weekday().num_days_from_monday() as usize;
        let runs = route.days[wd] && k <= route.ends_after;
        let diverted = route.diverted.is_some_and(|(a, b)| a <= k && k <= b);
        if !runs || diverted {
            continue;
        }
        let shift = k as u32 * DAY_SECS;
        if let Some(dep) = route.starts.iter().map(|s| s + 600).filter(|dep| dep + shift >= from_secs).min() {
            return Some((k as u32, dep));
        }
    }
    None
}

/// Every trip cancelled live: today's rows go (D-73) — the next must
/// not move.
fn all_cancelled(net: &Network, day: &Day) -> LiveDay {
    let _ = net;
    LiveDay { kind: Default::default(), schema_version: Default::default(),
        trips: day
            .trips
            .iter()
            .map(|t| LiveTrip {
                trip: t.trip.clone(),
                start_secs: t.start_secs,
                route: None,
                route_name: None,
                worst_delay_secs: None,
                status: LiveStatus::NoPrediction,
                canceled: true,
                stops: vec![],
            })
            .collect(),
        window: OnTimeWindow::default(),
        vehicles: vec![],
        unmatched_trips: 0,
        alerts: vec![],
        unmatched_alerts: 0,
    }
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 128, ..ProptestConfig::default() })]

    #[test]
    fn a_stopped_group_says_when_it_next_runs(
        routes in prop::collection::vec(gen_route(), 1..4),
        from_secs in 0u32..100_000,
        cancel_live in any::<bool>(),
    ) {
        let net = build(&routes);
        let records = edits(&routes);
        let day_on = |d: NaiveDate| transit_dispatch::day(&net, d, &records).map_err(|e| e.to_string()).map(Box::new);
        let today_day = day_on(today()).unwrap();
        let live = cancel_live.then(|| all_cancelled(&net, &today_day));
        let mut board = grouped_board(&net, &today_day.trips, "X", from_secs, 3, live.as_ref(), today_day.edits.as_ref());
        with_next_service(&net, &mut board, "X", today(), from_secs, day_on).unwrap();

        prop_assert_eq!(board.groups.len(), routes.len(), "every route serves X today");
        for group in &board.groups {
            let r: usize = group.route[1..].parse().unwrap();
            if !group.departures.is_empty() {
                prop_assert!(group.next.is_none(), "law 1: a group with departures left has no next");
                continue;
            }
            let expected = oracle(&routes[r], from_secs);
            match (&group.next, expected) {
                (None, None) => {}
                (Some(next), Some((k, dep))) => {
                    let date = today().checked_add_days(Days::new(k as u64)).unwrap();
                    prop_assert_eq!(next.days_ahead, k, "law 2: the first later day that serves it");
                    prop_assert!(next.days_ahead >= 1 && next.days_ahead <= NEXT_SERVICE_DAYS);
                    prop_assert_eq!(&next.date, &date.format("%Y-%m-%d").to_string());
                    prop_assert_eq!(&next.weekday, &date.format("%A").to_string());
                    prop_assert_eq!(next.departure_secs, dep, "law 2: its earliest departure");
                    prop_assert_eq!(&next.stop, "X");
                }
                (got, want) => prop_assert!(false, "route {r}: next {got:?}, oracle {want:?}"),
            }
        }
    }
}

/// The worked case: the last trip has left Friday night; the route
/// runs weekdays only, so the next is Monday at 05:10.
#[test]
fn a_weekday_route_stopped_on_friday_night_next_runs_monday() {
    let mut days = [false; 7];
    days[..5].fill(true);
    let routes = vec![GenRoute { days, ends_after: 30, starts: vec![18_000, 79_200], diverted: None }];
    let net = build(&routes);
    let day_on = |d: NaiveDate| transit_dispatch::day(&net, d, &[]).map_err(|e| e.to_string()).map(Box::new);
    let today_day = day_on(today()).unwrap();
    let mut board = grouped_board(&net, &today_day.trips, "X", 82_000, 3, None, None);
    with_next_service(&net, &mut board, "X", today(), 82_000, day_on).unwrap();
    let next = board.groups[0].next.as_ref().expect("a next");
    assert_eq!((next.date.as_str(), next.days_ahead, next.weekday.as_str(), next.departure_secs), ("2026-10-26", 3, "Monday", 18_600));
}
