//! The trip-cancel laws (D-45): a cancel in the edit document takes a
//! trip out of the effective day, and the boards say so.
//!
//! 1. a cancel in force removes exactly the instances it names from
//!    the day — every other trip is the materialized trip, unchanged —
//!    and the input day is never mutated;
//! 2. a cancel holds on its one date only, and a retired one never
//!    holds (the day is then the plain materialization);
//! 3. a cancelled visit is never a board row; it is listed among the
//!    visits the edits took away, marked with the cancel's id (D-100),
//!    and a stop's display group keeps it;
//! 4. a frequency trip's cancel names one instance or all of them;
//! 5. a cancel record reports against the NETWORK: an unknown trip or
//!    an instance the trip never runs is invalid, reported, never
//!    rejected;
//! 6. the edit document's list shows a cancel by the trip's route and
//!    ends;
//! 7. a cancel is authored only for a trip instance that runs on its
//!    date (`cancel_of`): anything else names the problem.

use chrono::NaiveDate;
use proptest::prelude::*;
use transit_dispatch::{cancel_day, cancel_of, cancel_rows, day, grouped_board, report_against_network, stop_board};
use transit_network::{
    CalendarRule, DateRange, Departs, HeadwayBand, Network, Pattern, PatternStop, PatternTrips, Route, ServiceType, Stop,
    StopOffset, TimingProfile, Trip, WeekdaySet,
};
use transit_sync::{EditRecord, EditStatus, TripCancel};

fn date() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 10, 23).unwrap() // a Friday
}

fn stop(name: &str) -> Stop {
    Stop { name: name.into(), lat: 45.5, lon: -122.5, ..Default::default() }
}

fn timepoint(id: &str, distance_m: f64) -> PatternStop {
    PatternStop { stop: id.into(), timepoint: true, distance_m, ..Default::default() }
}

fn trip(id: &str, departs: Departs) -> Trip {
    Trip {
        id: id.into(),
        departs,
        times: [0, 600, 1200].iter().map(|&t| StopOffset { arrival_secs: Some(t), departure_secs: Some(t) }).collect(),
        ..Default::default()
    }
}

/// Route 10 A→C→B: two one-start trips (08:00 and 09:00) and a
/// frequency trip every 30 min 10:00–11:00 (instances 10:00, 10:30).
fn network() -> Network {
    let mut net = Network { name: "Cancel test".into(), ..Default::default() };
    for (id, name) in [("A", "Main & 1st"), ("B", "Main & 2nd"), ("C", "Main & 3rd")] {
        net.stops.insert(id.into(), stop(name));
    }
    net.routes.insert("10".into(), Route { name: "10".into(), mode: transit_network::Mode::Bus, ..Default::default() });
    net.patterns.insert(
        "p10".into(),
        Pattern {
            route: "10".into(),
            name: "route 10".into(),
            stops: vec![timepoint("A", 0.0), timepoint("C", 1000.0), timepoint("B", 2000.0)],
            timing: vec![TimingProfile { start_secs: 0, end_secs: 172_800, at_timepoints: vec![0, 600, 1200] }],
            ..Default::default()
        },
    );
    net.service_types.insert(
        "WK".into(),
        ServiceType {
            name: "Weekday".into(),
            rule: CalendarRule {
                days: WeekdaySet::weekdays(),
                ranges: vec![DateRange {
                    start: NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
                    end: NaiveDate::from_ymd_opt(2026, 12, 31).unwrap(),
                }],
                exceptions: vec![],
            },
        },
    );
    net.pattern_trips.insert(
        "s10".into(),
        PatternTrips {
            pattern: "p10".into(),
            service_type: "WK".into(),
            trips: vec![
                trip("t0800", Departs::At { start_secs: 28_800 }),
                trip("t0900", Departs::At { start_secs: 32_400 }),
                trip(
                    "tfreq",
                    Departs::Every {
                        bands: vec![HeadwayBand { start_secs: 36_000, end_secs: 39_600, headway_secs: 1_800 }],
                        exact: true,
                    },
                ),
            ],
        },
    );
    net
}

fn cancel(trip_id: &str, start_secs: Option<u32>, on: NaiveDate) -> TripCancel {
    TripCancel { id: format!("c-{trip_id}"), trip_id: trip_id.into(), start_secs, date: on }
}

fn record(id: &str, c: TripCancel, status: EditStatus) -> EditRecord {
    EditRecord { id: id.into(), diversion: None, cancel: Some(c), status, rev: 1, updated_at: 1 }
}

fn instances(trips: &[transit_network::MaterializedTrip]) -> Vec<(String, u32)> {
    trips.iter().map(|t| (t.trip.clone(), t.start_secs)).collect()
}

const ALL: [(&str, u32); 4] = [("t0800", 28_800), ("t0900", 32_400), ("tfreq", 36_000), ("tfreq", 37_800)];

proptest! {
    /// Law 1 (and 4): any set of cancels removes exactly the instances
    /// they name; every other trip is the materialized trip, whole.
    #[test]
    fn a_cancel_removes_exactly_its_instances(picks in proptest::collection::vec((0usize..3, proptest::option::of(0usize..4)), 0..4)) {
        let net = network();
        let base = transit_network::materialize(&net, date()).unwrap();
        let before = base.clone();
        let ids = ["t0800", "t0900", "tfreq"];
        let cancels: Vec<TripCancel> = picks
            .iter()
            .map(|&(t, s)| cancel(ids[t], s.map(|i| ALL[i].1), date()))
            .collect();
        let out = cancel_day(&base, &cancels);
        prop_assert_eq!(&base, &before, "the input day is never mutated");
        let expected: Vec<_> = base
            .iter()
            .filter(|t| !cancels.iter().any(|c| c.cancels(&t.trip, t.start_secs)))
            .cloned()
            .collect();
        prop_assert_eq!(out, expected);
    }
}

#[test]
fn the_effective_day_drops_a_cancelled_trip_and_keeps_the_rest() {
    let net = network();
    let records = vec![record("e1", cancel("t0800", None, date()), EditStatus::Submitted)];
    let d = day(&net, date(), &records).unwrap();
    let edits = d.edits.as_ref().expect("a cancel in force is an edit");
    assert_eq!(edits.base, transit_network::materialize(&net, date()).unwrap(), "the base is the plain day");
    assert_eq!(edits.cancels.len(), 1);
    assert_eq!(instances(&d.trips), ALL[1..].iter().map(|(t, s)| (t.to_string(), *s)).collect::<Vec<_>>());
}

#[test]
fn a_cancel_holds_on_its_date_only_and_never_retired() {
    let net = network();
    let plain = transit_network::materialize(&net, date()).unwrap();
    let other = record("e1", cancel("t0800", None, NaiveDate::from_ymd_opt(2026, 10, 22).unwrap()), EditStatus::Accepted);
    let retired = record("e2", cancel("t0800", None, date()), EditStatus::Retired);
    let d = day(&net, date(), &[other, retired]).unwrap();
    assert!(d.edits.is_none(), "nothing is in force");
    assert_eq!(d.trips, plain);
}

#[test]
fn a_frequency_cancel_names_one_instance_or_all() {
    let net = network();
    let one = day(&net, date(), &[record("e1", cancel("tfreq", Some(37_800), date()), EditStatus::Draft)]).unwrap();
    assert!(instances(&one.trips).contains(&("tfreq".into(), 36_000)), "the 10:00 instance still runs");
    assert!(!instances(&one.trips).contains(&("tfreq".into(), 37_800)));
    let all = day(&net, date(), &[record("e1", cancel("tfreq", None, date()), EditStatus::Draft)]).unwrap();
    assert!(all.trips.iter().all(|t| t.trip != "tfreq"));
}

#[test]
fn a_cancelled_visit_is_listed_cancelled_never_a_row() {
    let net = network();
    let records = vec![record("e1", cancel("t0800", None, date()), EditStatus::Accepted)];
    let d = day(&net, date(), &records).unwrap();
    let board = stop_board(&net, &d.trips, "A", 0, 10, None, d.edits.as_ref());
    assert!(board.rows.iter().all(|r| r.departure.trip != "t0800"), "never a departure");
    assert_eq!(board.rows.len(), 3, "the rest still leave");
    assert_eq!(board.skipped.len(), 1);
    let s = &board.skipped[0];
    assert_eq!((s.departure.trip.as_str(), s.departure.departure_secs), ("t0800", 28_800));
    assert_eq!((s.cancel.as_deref(), s.diversion.as_deref()), (Some("c-t0800"), None));

    // The display group keeps it among the visits taken away, and it
    // takes no place among the next departures.
    let grouped = grouped_board(&net, &d.trips, "A", 0, 3, None, d.edits.as_ref());
    assert_eq!(grouped.groups.len(), 1);
    let g = &grouped.groups[0];
    assert!(g.departures.iter().all(|r| r.departure.trip != "t0800"));
    assert_eq!(g.departures.len(), 3);
    assert_eq!(g.skipped.iter().filter_map(|s| s.cancel.as_deref()).collect::<Vec<_>>(), vec!["c-t0800"]);
}

#[test]
fn a_group_whose_only_trip_is_cancelled_stays_with_it() {
    let mut net = network();
    net.pattern_trips.get_mut("s10").unwrap().trips.truncate(1);
    let records = vec![record("e1", cancel("t0800", None, date()), EditStatus::Accepted)];
    let d = day(&net, date(), &records).unwrap();
    let grouped = grouped_board(&net, &d.trips, "A", 0, 3, None, d.edits.as_ref());
    assert_eq!(grouped.groups.len(), 1, "the route still serves the stop that day");
    assert!(grouped.groups[0].departures.is_empty());
    assert_eq!(grouped.groups[0].skipped[0].cancel.as_deref(), Some("c-t0800"));
}

#[test]
fn a_cancel_reports_against_the_network() {
    let net = network();
    let records = vec![
        record("ok", cancel("t0800", None, date()), EditStatus::Draft),
        record("freq", cancel("tfreq", None, date()), EditStatus::Draft),
        record("one", cancel("tfreq", Some(36_000), date()), EditStatus::Draft),
        record("ghost", cancel("nope", None, date()), EditStatus::Draft),
        record("never", cancel("t0800", Some(3_600), date()), EditStatus::Draft),
    ];
    let reports = report_against_network(&net, &records);
    let got: Vec<(&str, bool, usize)> = reports.iter().map(|r| (r.id.as_str(), r.valid, r.affected_trips)).collect();
    assert_eq!(
        got,
        vec![("ok", true, 1), ("freq", true, 2), ("one", true, 1), ("ghost", false, 0), ("never", false, 0)]
    );
    assert!(reports[3].error.as_deref().unwrap().contains("unknown trip"));
    assert!(reports[4].error.as_deref().unwrap().contains("no instance"));
}

#[test]
fn the_list_shows_a_cancel_by_route_and_ends() {
    let net = network();
    let records = vec![record("e1", cancel("t0800", None, date()), EditStatus::Retired)];
    let rows = cancel_rows(&net, &records);
    assert_eq!(rows.len(), 1);
    let r = &rows[0];
    assert_eq!(
        (r.record_id.as_str(), r.in_force, r.route_name.as_str(), r.from_name.as_str(), r.to_name.as_str()),
        ("e1", false, "10", "Main & 1st", "Main & 2nd")
    );
    assert_eq!((r.trip_id.as_str(), r.start_secs, r.date.as_str()), ("t0800", Some(28_800), "2026-10-23"));
    assert!(transit_dispatch::diversion_rows(&net, &records).is_empty(), "a cancel is not a diversion row");
}

#[test]
fn a_cancel_is_authored_only_for_an_instance_that_runs_that_day() {
    let net = network();
    assert_eq!(cancel_of(&net, "c1", "t0800", None, "2026-10-23"), Ok(TripCancel { id: "c1".into(), ..cancel("t0800", None, date()) }));
    assert_eq!(cancel_of(&net, "c1", "tfreq", Some(37_800), "2026-10-23").unwrap().start_secs, Some(37_800));
    let err = |trip: &str, start: Option<u32>, on: &str| cancel_of(&net, "c1", trip, start, on).unwrap_err();
    assert!(err("nope", None, "2026-10-23").contains("unknown trip"));
    assert!(err("t0800", Some(3_600), "2026-10-23").contains("no instance"));
    assert!(err("t0800", None, "2026-10-25").contains("does not run on 2026-10-25"), "a Sunday");
    assert!(err("t0800", None, "soon").contains("date"));
}
