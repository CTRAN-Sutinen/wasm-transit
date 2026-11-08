//! The previous service day's after-midnight trips (docs/units/
//! deploy-rider-apps.md). GTFS times past 24:00 belong to the service
//! date the trip started on: at 00:30 on Saturday, Friday's 24:45
//! departure leaves in 15 minutes. A board answers its date and, when
//! the host gives the same instant on the previous date's clock
//! (`Overnight`: that clock's seconds and the previous day's length,
//! anchor to anchor — 25 h on the night the clocks go back), the
//! previous date's departures still to leave.
//!
//! The laws:
//!
//! 1. **The previous day's after-midnight departures are on the
//!    board** when they have not left, each on its own date's clock
//!    (`departure_secs` past 86 400) and carrying its date; without
//!    `previous` the board is its date alone.
//! 2. **One order across both days**: rows by how long until each
//!    leaves, the wait counted on each row's own clock.
//! 3. **Live data joins by date**: a live trip stating the previous
//!    date (GTFS-rt `start_date`) predicts, cancels, or skips the
//!    previous day's instance; a live trip stating no date is the
//!    current service day's (the spec's reading) and never touches
//!    the previous day's.
//! 4. **The previous day's edits are the previous day's**: a cancel on
//!    the previous date takes its departure off the board and lists
//!    it as skipped.
//! 5. **The fall-back hour**: before a fall-back day's anchor the
//!    instant precedes the date's clock; waits on that date count from
//!    the true instant (previous seconds minus the previous day's
//!    length), not from the clamped zero.
//! 6. **A route only the previous day still runs has its group** on
//!    the grouped board.

use std::collections::BTreeMap;

use chrono::NaiveDate;
use transit_dispatch::{grouped_board_at, stop_board_at, BoardAt, Overnight, ServiceDay, Wait};
use transit_live::{LiveDay, LiveStatus, LiveStop, LiveTrip, OnTimeWindow};
use transit_network::{
    CalendarRule, DateRange, Departs, Network, Pattern, PatternStop, PatternTrips, Route, ServiceType, Stop,
    StopOffset, TimingProfile, Trip, WeekdaySet,
};
use transit_sync::{EditRecord, EditStatus, TripCancel};

const FRI: &str = "2026-10-23";
const SAT: &str = "2026-10-24";

fn date(s: &str) -> NaiveDate {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
}

/// Route r1 runs A → X → T every day (X is 600 s in): "late" starts at
/// 24:35 (X at 24:45), "dawn" at 00:00 (X at 00:10), "early" at 00:40
/// (X at 00:50). Route r2 runs Friday only: "owl" at 24:50 (X at 25:00).
fn network() -> Network {
    let plain = |name: &str| Stop { name: name.into(), lat: 45.5, lon: -122.5, ..Default::default() };
    let stops: BTreeMap<String, Stop> =
        [("A", "Origin"), ("X", "The Stop"), ("T", "Terminal")].iter().map(|(id, n)| (id.to_string(), plain(n))).collect();
    let mut net = Network { name: "Overnight laws".into(), stops, ..Default::default() };
    let every = WeekdaySet { mon: true, tue: true, wed: true, thu: true, fri: true, sat: true, sun: true };
    let friday = WeekdaySet { fri: true, ..Default::default() };
    for (st, days) in [("daily", every), ("friday", friday)] {
        net.service_types.insert(
            st.into(),
            ServiceType {
                name: st.into(),
                rule: CalendarRule {
                    days,
                    ranges: vec![DateRange { start: date("2026-10-01"), end: date("2026-11-30") }],
                    exceptions: vec![],
                },
            },
        );
    }
    let offsets = [0u32, 600, 1_200];
    for (rid, st, trips) in [("r1", "daily", vec![("late", 88_500u32), ("dawn", 0), ("early", 2_400)]), ("r2", "friday", vec![("owl", 89_400)])] {
        net.routes.insert(rid.into(), Route { name: rid[1..].into(), ..Default::default() });
        net.patterns.insert(
            rid.into(),
            Pattern {
                direction: None,
                route: rid.into(),
                name: rid.into(),
                alignment: None,
                stops: ["A", "X", "T"]
                    .iter()
                    .enumerate()
                    .map(|(i, s)| PatternStop { stop: s.to_string(), timepoint: true, distance_m: i as f64 * 1000.0, ..Default::default() })
                    .collect(),
                timing: vec![TimingProfile { start_secs: 0, end_secs: 172_800, at_timepoints: offsets.to_vec() }],
            },
        );
        net.pattern_trips.insert(
            rid.into(),
            PatternTrips {
                pattern: rid.into(),
                service_type: st.into(),
                trips: trips
                    .into_iter()
                    .map(|(id, s)| Trip {
                        id: id.into(),
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

fn live(trips: Vec<LiveTrip>) -> LiveDay {
    LiveDay {
        kind: Default::default(),
        schema_version: Default::default(),
        window: OnTimeWindow::default(),
        trips,
        vehicles: vec![],
        unmatched_trips: 0,
        alerts: vec![],
        unmatched_alerts: 0,
    }
}

/// "late" (start 24:35) live: `delay` at X, or cancelled; on `date`.
fn late_live(date: Option<&str>, delay: i32, canceled: bool) -> LiveTrip {
    LiveTrip {
        trip: "late".into(),
        date: date.map(Into::into),
        start_secs: 88_500,
        route: Some("r1".into()),
        route_name: None,
        worst_delay_secs: Some(delay),
        status: LiveStatus::NoPrediction,
        canceled,
        stops: vec![LiveStop {
            stop: "X".into(),
            scheduled_departure: Some(89_100),
            arrival_delay: Some(delay),
            departure_delay: Some(delay),
            predicted_departure: Some((89_100 + delay) as u32),
            skipped: false,
        }],
    }
}

fn cancel_late_on(on: &str) -> EditRecord {
    EditRecord {
        id: "c-late".into(),
        diversion: None,
        cancel: Some(TripCancel { id: "cancel-late".into(), trip_id: "late".into(), start_secs: None, date: date(on) }),
        status: EditStatus::Accepted,
        rev: 1,
        updated_at: 1,
    }
}

/// Saturday's board over Friday's night: `prev_secs` on Friday's
/// clock, Friday `length` seconds long.
fn ask<'a>(
    sat: &'a transit_dispatch::Day,
    fri: &'a transit_dispatch::Day,
    prev_secs: u32,
    length: u32,
    from_secs: u32,
) -> BoardAt<'a> {
    BoardAt {
        today: ServiceDay { date: SAT, trips: &sat.trips, edits: sat.edits.as_ref() },
        from_secs,
        previous: Some(Overnight {
            day: ServiceDay { date: FRI, trips: &fri.trips, edits: fri.edits.as_ref() },
            from_secs: prev_secs,
            length_secs: length,
        }),
    }
}

fn days(net: &Network, records: &[EditRecord]) -> (transit_dispatch::Day, transit_dispatch::Day) {
    (transit_dispatch::day(net, date(SAT), records).unwrap(), transit_dispatch::day(net, date(FRI), records).unwrap())
}

fn rows(board: &transit_dispatch::StopBoard) -> Vec<(String, String, u32)> {
    board.rows.iter().map(|r| (r.departure.trip.clone(), r.departure.date.clone().unwrap(), r.departure.departure_secs)).collect()
}

#[test]
fn law1_the_previous_days_after_midnight_departures_are_on_the_board() {
    let net = network();
    let (sat, fri) = days(&net, &[]);
    // Saturday 00:30: Friday's clock reads 24:30.
    let board = stop_board_at(&net, &ask(&sat, &fri, 88_200, 86_400, 1_800), "X", 10, None);
    assert_eq!(
        rows(&board)[..4],
        [
            ("late".into(), FRI.into(), 89_100),
            ("early".into(), SAT.into(), 3_000),
            ("owl".into(), FRI.into(), 90_000),
            ("late".into(), SAT.into(), 89_100),
        ],
        "Friday's 24:45 and 25:00 are Friday's; Saturday's 00:10 has left"
    );
    // Without the previous date, the board is Saturday alone.
    let alone = stop_board_at(
        &net,
        &BoardAt { today: ServiceDay { date: SAT, trips: &sat.trips, edits: None }, from_secs: 1_800, previous: None },
        "X",
        10,
        None,
    );
    assert!(alone.rows.iter().all(|r| r.departure.date.as_deref() == Some(SAT)));
    assert_eq!(alone.rows[0].departure.trip, "early");
}

#[test]
fn law1_a_previous_day_departure_that_has_left_is_not_on_the_board() {
    let net = network();
    let (sat, fri) = days(&net, &[]);
    // 00:55 Saturday: Friday's 24:45 has left, its 25:00 has not.
    let board = stop_board_at(&net, &ask(&sat, &fri, 89_700, 86_400, 3_300), "X", 10, None);
    let r = rows(&board);
    assert!(!r.contains(&("late".into(), FRI.into(), 89_100)));
    assert_eq!(r[0], ("owl".into(), FRI.into(), 90_000));
}

#[test]
fn law2_rows_order_by_the_wait_on_each_rows_own_clock() {
    let net = network();
    let (sat, fri) = days(&net, &[]);
    let board = stop_board_at(&net, &ask(&sat, &fri, 88_200, 86_400, 1_800), "X", 10, None);
    assert_eq!(board.rows[0].wait, Wait::Minutes { minutes: 15 }, "Friday's 24:45 at Saturday 00:30");
    assert_eq!(board.rows[1].wait, Wait::Minutes { minutes: 20 }, "Saturday's 00:50");
    assert_eq!(board.rows[2].wait, Wait::Minutes { minutes: 30 }, "Friday's 25:00");
}

#[test]
fn law3_live_data_joins_the_previous_days_instance_by_its_date() {
    let net = network();
    let (sat, fri) = days(&net, &[]);
    let at = ask(&sat, &fri, 88_200, 86_400, 1_800);
    let friday_late = |board: &transit_dispatch::StopBoard| {
        board.rows.iter().find(|r| r.departure.trip == "late" && r.departure.date.as_deref() == Some(FRI)).cloned()
    };

    // Stating Friday: it predicts Friday's instance.
    let dated = live(vec![late_live(Some(FRI), 120, false)]);
    let board = stop_board_at(&net, &at, "X", 10, Some(&dated));
    let row = friday_late(&board).expect("Friday's late is on the board");
    assert_eq!((row.expected_secs, row.delay_secs), (Some(89_220), Some(120)));

    // Stating no date: the current service day's — Saturday's — never
    // Friday's.
    let undated = live(vec![late_live(None, 120, false)]);
    let board = stop_board_at(&net, &at, "X", 10, Some(&undated));
    let row = friday_late(&board).expect("Friday's late is on the board");
    assert_eq!(row.expected_secs, None, "an undated live trip is not the previous day's");
    let saturday = board.rows.iter().find(|r| r.departure.trip == "late" && r.departure.date.as_deref() == Some(SAT)).unwrap();
    assert_eq!(saturday.delay_secs, Some(120), "it is Saturday's instance");

    // A cancel stating Friday takes Friday's instance off (D-73); one
    // stating no date does not.
    let board = stop_board_at(&net, &at, "X", 10, Some(&live(vec![late_live(Some(FRI), 0, true)])));
    assert!(friday_late(&board).is_none());
    let board = stop_board_at(&net, &at, "X", 10, Some(&live(vec![late_live(None, 0, true)])));
    assert!(friday_late(&board).is_some());
}

#[test]
fn law4_the_previous_days_edits_are_the_previous_days() {
    let net = network();
    let records = [cancel_late_on(FRI)];
    let (sat, fri) = days(&net, &records);
    let board = stop_board_at(&net, &ask(&sat, &fri, 88_200, 86_400, 1_800), "X", 10, None);
    assert!(!rows(&board).contains(&("late".into(), FRI.into(), 89_100)), "Friday's cancel takes Friday's late off");
    assert!(rows(&board).contains(&("late".into(), SAT.into(), 89_100)), "Saturday's late runs");
    let skipped = board.skipped.iter().find(|s| s.departure.trip == "late").expect("listed as skipped");
    assert_eq!((skipped.departure.date.as_deref(), skipped.cancel.as_deref()), (Some(FRI), Some("cancel-late")));
}

#[test]
fn law5_the_fall_back_hour_counts_waits_from_the_true_instant() {
    let net = network();
    let (sat, fri) = days(&net, &[]);
    // The clocks go back at Saturday 02:00, so Friday is 25 h anchor to
    // anchor and Saturday's anchor is 01:00 on the wall. At 00:30 on
    // the wall, Friday's clock reads 24:30 and Saturday's -00:30 —
    // clamped to 0 by the host.
    let board = stop_board_at(&net, &ask(&sat, &fri, 88_200, 90_000, 0), "X", 10, None);
    let dawn = board.rows.iter().find(|r| r.departure.trip == "dawn").expect("Saturday's 00:10 has not left");
    assert_eq!(dawn.wait, Wait::Minutes { minutes: 40 }, "00:10 on Saturday's clock is 40 minutes off, not 10");
    assert_eq!(board.rows[0].departure.trip, "late");
    assert_eq!(board.rows[0].wait, Wait::Minutes { minutes: 15 });
}

#[test]
fn law6_a_route_only_the_previous_day_runs_has_its_group() {
    let net = network();
    let (sat, fri) = days(&net, &[]);
    let board = grouped_board_at(&net, &ask(&sat, &fri, 88_200, 86_400, 1_800), "X", 3, None);
    let owl = board.groups.iter().find(|g| g.route == "r2").expect("Friday's owl route still runs");
    assert_eq!(owl.departures.len(), 1);
    assert_eq!((owl.departures[0].departure.trip.as_str(), owl.departures[0].departure.date.as_deref()), ("owl", Some(FRI)));
    let r1 = board.groups.iter().find(|g| g.route == "r1").unwrap();
    let order: Vec<_> = r1.departures.iter().map(|r| (r.departure.trip.as_str(), r.departure.date.as_deref().unwrap())).collect();
    assert_eq!(order, [("late", FRI), ("early", SAT), ("late", SAT)]);

    // Once Friday's night is done, the owl's group is gone from
    // Saturday's board (Saturday does not run it).
    let later = grouped_board_at(&net, &ask(&sat, &fri, 100_000, 86_400, 13_600), "X", 3, None);
    assert!(later.groups.iter().all(|g| g.route != "r2"));
}
