//! `set_plan` with `through` carries the calendar into the fork (D-112;
//! cap-author/OPS.md). The expected answers are recomputed from the
//! generated calendars.
//!
//! 1. A weekly type that still runs at the calendar's end runs on every date
//!    of the period past its old last day. A type whose service starts after
//!    the period is not pulled back.
//! 2. Every range and exception lies in the period, and nothing a type
//!    already decided inside it moves.
//! 3. A type with no weekly rule gains nothing, and every type stays.
//! 4. The edit is idempotent. Without `through`, only the dates before
//!    `effective_from` go.
//! 5. A `through` before `effective_from`, or not a date, is a named error,
//!    and the input is untouched.

use proptest::prelude::*;
use transit_date::{Date, Days, Weekday};
use transit_network::{
    apply_edits, CalendarException, CalendarRule, DateRange, DayState, Network, NetworkEdit,
    PlanMeta, ServiceType, WeekdaySet,
};

fn day0() -> Date {
    Date::from_ymd_opt(2026, 9, 1).unwrap()
}

fn d(offset: i64) -> Date {
    day0() + Days::new(offset)
}

fn iso(date: Date) -> String {
    date.to_string()
}

#[derive(Clone, Debug)]
struct GenType {
    days: [bool; 7],
    ranges: Vec<(i64, i64)>,
    exceptions: Vec<(i64, bool)>,
}

fn gen_type() -> impl Strategy<Value = GenType> {
    (
        proptest::array::uniform7(any::<bool>()),
        proptest::collection::vec((0i64..200, 0i64..120), 0..3),
        proptest::collection::vec((0i64..400, any::<bool>()), 0..4),
    )
        .prop_map(|(days, ranges, exceptions)| GenType {
            days,
            ranges,
            exceptions,
        })
}

fn service_type(g: &GenType) -> ServiceType {
    let [mon, tue, wed, thu, fri, sat, sun] = g.days;
    let mut exceptions: Vec<CalendarException> = Vec::new();
    for &(o, added) in &g.exceptions {
        if exceptions.iter().all(|e| e.date != d(o)) {
            exceptions.push(CalendarException {
                date: d(o),
                state: if added {
                    DayState::Added
                } else {
                    DayState::Removed
                },
                extra: Default::default(),
            });
        }
    }
    exceptions.sort_by_key(|e| e.date);
    ServiceType {
        name: "T".into(),
        rule: CalendarRule {
            days: WeekdaySet {
                mon,
                tue,
                wed,
                thu,
                fri,
                sat,
                sun,
            },
            ranges: g
                .ranges
                .iter()
                .map(|&(s, len)| DateRange {
                    start: d(s),
                    end: d(s + len),
                    extra: Default::default(),
                })
                .collect(),
            exceptions,
            extra: Default::default(),
        },
        extra: Default::default(),
    }
}

fn network(types: &[GenType]) -> Network {
    let mut net = Network {
        name: "Generated".into(),
        ..Default::default()
    };
    for (i, g) in types.iter().enumerate() {
        net.service_types.insert(format!("t{i}"), service_type(g));
    }
    net
}

fn flagged(days: &WeekdaySet, wd: Weekday) -> bool {
    match wd {
        Weekday::Mon => days.mon,
        Weekday::Tue => days.tue,
        Weekday::Wed => days.wed,
        Weekday::Thu => days.thu,
        Weekday::Fri => days.fri,
        Weekday::Sat => days.sat,
        Weekday::Sun => days.sun,
    }
}

/// An exception wins. Otherwise a flagged weekday inside a range runs.
fn runs(rule: &CalendarRule, date: Date) -> bool {
    if let Some(e) = rule.exceptions.iter().find(|e| e.date == date) {
        return e.state == DayState::Added;
    }
    flagged(&rule.days, date.weekday())
        && rule.ranges.iter().any(|r| r.start <= date && date <= r.end)
}

fn weekly(rule: &CalendarRule) -> bool {
    let w = rule.days;
    (w.mon || w.tue || w.wed || w.thu || w.fri || w.sat || w.sun) && !rule.ranges.is_empty()
}

/// A flagged day and a range that starts by the period's end.
fn weekly_by(rule: &CalendarRule, through: Date) -> bool {
    weekly(rule) && rule.ranges.iter().any(|r| r.start <= through)
}

fn set_plan(from: i64, through: Option<Date>) -> NetworkEdit {
    NetworkEdit::SetPlan {
        plan: PlanMeta {
            id: "winter".into(),
            name: "Winter".into(),
            effective_from: Some(d(from)),
            extra: Default::default(),
        },
        through: through.map(iso),
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn a_fork_with_a_period_end_carries_every_weekly_type_through_it(
        types in proptest::collection::vec(gen_type(), 1..5),
        from in 0i64..400,
        len in 0i64..200,
    ) {
        let net = network(&types);
        let through = d(from + len);
        let next = apply_edits(&net, &[set_plan(from, Some(through))]).expect("the fork applies");
        prop_assert_eq!(&next.plan.id, "winter");
        prop_assert_eq!(next.plan.effective_from, Some(d(from)));
        prop_assert_eq!(next.service_types.len(), net.service_types.len());

        // D-112: a date past `through` leaves the fork's calendar, so it
        // does not count toward the calendar's end.
        let calendar_end = net
            .service_types
            .values()
            .flat_map(|t| {
                let weekly_end = weekly_by(&t.rule, through)
                    .then(|| t.rule.ranges.iter().filter(|r| r.start <= through).map(|r| r.end.min(through)).max())
                    .flatten();
                weekly_end.into_iter().chain(t.rule.exceptions.iter().filter(|e| e.state == DayState::Added && e.date <= through).map(|e| e.date))
            })
            .max();
        let period = |date: Date| d(from) <= date && date <= through;
        for (id, before) in &net.service_types {
            let after = &next.service_types[id];
            for r in &after.rule.ranges {
                prop_assert!(r.start <= r.end && period(r.start) && period(r.end), "{} range {:?} outside the period", id, r);
            }
            prop_assert_eq!(after.rule.days, before.rule.days);
            let kept: Vec<_> = before.rule.exceptions.iter().filter(|e| period(e.date)).cloned().collect();
            prop_assert_eq!(&after.rule.exceptions, &kept);
            prop_assert_eq!(&after.name, &before.name);
            let last_end = before.rule.ranges.iter().filter(|r| r.start <= through).map(|r| r.end).max();
            let carried = weekly_by(&before.rule, through) && last_end.unwrap() >= calendar_end.unwrap() - Days::new(6);
            let old_end = if carried { last_end.unwrap() } else { through };
            for offset in -10..(from + len + 30) {
                let date = d(offset);
                if !period(date) {
                    prop_assert!(!runs(&after.rule, date), "{} runs on {}, outside the period", id, date);
                    continue;
                }
                if !carried {
                    prop_assert_eq!(runs(&after.rule, date), runs(&before.rule, date), "{} on {}: not carried, moved", id, date);
                    continue;
                }
                if d(from) <= date && date <= through && date > old_end {
                    prop_assert_eq!(runs(&after.rule, date), runs(&CalendarRule {
                        ranges: vec![DateRange { start: date, end: date, extra: Default::default() }],
                        ..before.rule.clone()
                    }, date), "{} on {}", id, date);
                }
                if date <= old_end {
                    prop_assert_eq!(runs(&after.rule, date), runs(&before.rule, date), "{} on {} moved", id, date);
                }
            }
        }

        let again = apply_edits(&next, &[set_plan(from, Some(through))]).unwrap();
        prop_assert_eq!(&again, &next);
    }

    #[test]
    fn without_a_period_end_the_fork_starts_at_its_pick(
        types in proptest::collection::vec(gen_type(), 1..5),
        from in 0i64..400,
    ) {
        let net = network(&types);
        let next = apply_edits(&net, &[set_plan(from, None)]).unwrap();
        prop_assert_eq!(next.plan.effective_from, Some(d(from)));
        prop_assert_eq!(next.service_types.len(), net.service_types.len());
        for (id, before) in &net.service_types {
            let after = &next.service_types[id];
            prop_assert_eq!(after.rule.days, before.rule.days);
            prop_assert!(after.rule.ranges.iter().all(|r| r.start >= d(from) && r.start <= r.end), "{}: a range before the pick", id);
            prop_assert!(after.rule.exceptions.iter().all(|e| e.date >= d(from)), "{}: an exception before the pick", id);
            for offset in -10..(from + 400) {
                let date = d(offset);
                let want = date >= d(from) && runs(&before.rule, date);
                prop_assert_eq!(runs(&after.rule, date), want, "{} on {}", id, date);
            }
        }
        let again = apply_edits(&next, &[set_plan(from, None)]).unwrap();
        prop_assert_eq!(&again, &next);
    }

    #[test]
    fn a_period_end_before_the_pick_is_refused(
        types in proptest::collection::vec(gen_type(), 1..3),
        from in 1i64..400,
        back in 1i64..30,
    ) {
        let net = network(&types);
        let err = apply_edits(&net, &[set_plan(from, Some(d(from - back)))]).expect_err("refused");
        prop_assert_eq!(err.index, 0);
        prop_assert!(err.reason.contains("before"), "{}", err.reason);
    }
}

#[test]
fn a_period_end_that_is_not_a_date_is_refused() {
    let net = network(&[]);
    let edit = NetworkEdit::SetPlan {
        plan: PlanMeta {
            id: "w".into(),
            name: "W".into(),
            effective_from: Some(d(0)),
            extra: Default::default(),
        },
        through: Some("2027-02-30".into()),
    };
    assert!(apply_edits(&net, &[edit])
        .expect_err("refused")
        .reason
        .contains("invalid date"));
}

#[test]
fn the_winter_fork_runs_past_the_import_calendar() {
    // The C-TRAN shape: a weekday type ending 2027-01-09, a
    // holiday type dated 2026-12-31 only, and a pick on 2027-01-25.
    let mut net = Network::default();
    net.service_types.insert(
        "1-WKDY".into(),
        ServiceType {
            name: "Weekday".into(),
            rule: CalendarRule {
                days: WeekdaySet::weekdays(),
                ranges: vec![DateRange {
                    start: Date::from_ymd_opt(2026, 9, 13).unwrap(),
                    end: Date::from_ymd_opt(2027, 1, 9).unwrap(),
                    extra: Default::default(),
                }],
                exceptions: vec![],
                extra: Default::default(),
            },
            extra: Default::default(),
        },
    );
    net.service_types.insert(
        "4-NYE".into(),
        ServiceType {
            name: "New Year's Eve".into(),
            rule: CalendarRule {
                days: WeekdaySet::none(),
                ranges: vec![],
                exceptions: vec![CalendarException {
                    date: Date::from_ymd_opt(2026, 12, 31).unwrap(),
                    state: DayState::Added,
                    extra: Default::default(),
                }],
                extra: Default::default(),
            },
            extra: Default::default(),
        },
    );
    net.service_types.insert(
        "1-WKDY/30".into(),
        ServiceType {
            name: "Closure period".into(),
            rule: CalendarRule {
                days: WeekdaySet::weekdays(),
                ranges: vec![DateRange {
                    start: Date::from_ymd_opt(2026, 9, 14).unwrap(),
                    end: Date::from_ymd_opt(2026, 10, 9).unwrap(),
                    extra: Default::default(),
                }],
                exceptions: vec![],
                extra: Default::default(),
            },
            extra: Default::default(),
        },
    );
    let pick = Date::from_ymd_opt(2027, 1, 25).unwrap();
    let edit = NetworkEdit::SetPlan {
        plan: PlanMeta {
            id: "winter-2027".into(),
            name: "Winter 2027".into(),
            effective_from: Some(pick),
            extra: Default::default(),
        },
        through: Some("2027-05-29".into()),
    };
    let next = apply_edits(&net, &[edit]).unwrap();
    assert!(!net.service_types["1-WKDY"].rule.runs_on(pick));
    assert!(
        next.service_types["1-WKDY"].rule.runs_on(pick),
        "the weekday runs on the pick's date"
    );
    let weekday = &next.service_types["1-WKDY"].rule.ranges;
    assert_eq!(
        (weekday.len(), weekday[0].start, weekday[0].end.to_string()),
        (1, pick, "2027-05-29".into()),
        "the weekday's calendar is the period"
    );
    assert!(
        next.service_types["4-NYE"].rule.exceptions.is_empty(),
        "last year's holiday leaves; next year's is the planner's"
    );
    assert!(
        next.service_types["1-WKDY/30"].rule.ranges.is_empty(),
        "a closed period before the pick leaves the calendar"
    );
    assert_eq!(
        next.service_types.len(),
        3,
        "every key stays: trips still name them"
    );
}
