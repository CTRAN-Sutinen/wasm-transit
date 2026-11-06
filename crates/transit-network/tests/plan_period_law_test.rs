//! The fork-carries-the-calendar laws (D-41): `set_plan` with a period
//! end (`through`) extends every WEEKLY service type to that date, so
//! a pick framed past the import's calendar runs its service.
//!
//! Written BEFORE the implementation, from the decision, with every
//! expected answer recomputed here from the generated calendars:
//!
//! 1. **Coverage** — on every date from the pick's `effective_from`
//!    through `through` past the type's own last day, a weekly type
//!    runs wherever its weekday is flagged and no exception says
//!    otherwise (the fork EXTENDS a calendar; a type whose service
//!    starts after the period is not pulled back).
//!    Only a type still running at the document's end is carried
//!    (D-112): its last range ends in the calendar's final week — the
//!    week before the latest date any type names (a weekly type's last
//!    range end, or an added date). A closed period (C-TRAN's
//!    1-WKDY/30, ended in October) is left where it ended.
//! 2. **Nothing earlier moves** — a date a type already decided (on or
//!    before its last range's end, or outside the new window) runs
//!    exactly as before: the edit only ADDS the dates between the
//!    type's old end and `through`.
//! 3. **Dated service is not carried** — a type with no weekly rule
//!    (no flagged day, or no range: holidays, one-off events) is
//!    untouched; no exception is added, moved, or removed anywhere,
//!    and no weekday flag changes.
//! 4. **Idempotent**, and **without `through`** the edit is the frame
//!    alone — no service type changes.
//! 5. **Refusal** — a `through` before the pick's `effective_from`, or
//!    not a date, is a named error; the input is untouched.
//!
//! The API these laws require: `NetworkEdit::SetPlan { plan, through:
//! Option<String> }` ("YYYY-MM-DD").

use chrono::{Datelike, Duration, NaiveDate, Weekday};
use proptest::prelude::*;
use transit_network::{
    apply_edits, CalendarException, CalendarRule, DateRange, DayState, Network, NetworkEdit, PlanMeta, ServiceType,
    WeekdaySet,
};

fn day0() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 9, 1).unwrap()
}

fn d(offset: i64) -> NaiveDate {
    day0() + Duration::days(offset)
}

fn iso(date: NaiveDate) -> String {
    date.format("%Y-%m-%d").to_string()
}

#[derive(Clone, Debug)]
struct GenType {
    days: [bool; 7],
    /// (start offset, length) per range.
    ranges: Vec<(i64, i64)>,
    /// (offset, added) per exception; dates deduplicated.
    exceptions: Vec<(i64, bool)>,
}

fn gen_type() -> impl Strategy<Value = GenType> {
    (
        proptest::array::uniform7(any::<bool>()),
        proptest::collection::vec((0i64..200, 0i64..120), 0..3),
        proptest::collection::vec((0i64..400, any::<bool>()), 0..4),
    )
        .prop_map(|(days, ranges, exceptions)| GenType { days, ranges, exceptions })
}

fn service_type(g: &GenType) -> ServiceType {
    let [mon, tue, wed, thu, fri, sat, sun] = g.days;
    let mut exceptions: Vec<CalendarException> = Vec::new();
    for &(o, added) in &g.exceptions {
        if exceptions.iter().all(|e| e.date != d(o)) {
            exceptions.push(CalendarException { date: d(o), state: if added { DayState::Added } else { DayState::Removed } });
        }
    }
    exceptions.sort_by_key(|e| e.date);
    ServiceType {
        name: "T".into(),
        rule: CalendarRule {
            days: WeekdaySet { mon, tue, wed, thu, fri, sat, sun },
            ranges: g.ranges.iter().map(|&(s, len)| DateRange { start: d(s), end: d(s + len) }).collect(),
            exceptions,
        },
    }
}

fn network(types: &[GenType]) -> Network {
    let mut net = Network { name: "Generated".into(), ..Default::default() };
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

/// The spec's reading of a rule, written out here (an exception wins;
/// else a flagged weekday inside a range).
fn runs(rule: &CalendarRule, date: NaiveDate) -> bool {
    if let Some(e) = rule.exceptions.iter().find(|e| e.date == date) {
        return e.state == DayState::Added;
    }
    flagged(&rule.days, date.weekday()) && rule.ranges.iter().any(|r| r.start <= date && date <= r.end)
}

fn weekly(rule: &CalendarRule) -> bool {
    let w = rule.days;
    (w.mon || w.tue || w.wed || w.thu || w.fri || w.sat || w.sun) && !rule.ranges.is_empty()
}

fn set_plan(from: i64, through: Option<NaiveDate>) -> NetworkEdit {
    NetworkEdit::SetPlan {
        plan: PlanMeta { id: "winter".into(), name: "Winter".into(), effective_from: Some(d(from)) },
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

        // D-112: the calendar's end, recomputed.
        let calendar_end = net
            .service_types
            .values()
            .flat_map(|t| {
                let weekly_end = weekly(&t.rule).then(|| t.rule.ranges.iter().map(|r| r.end).max()).flatten();
                weekly_end.into_iter().chain(t.rule.exceptions.iter().filter(|e| e.state == DayState::Added).map(|e| e.date))
            })
            .max();
        for (id, before) in &net.service_types {
            let after = &next.service_types[id];
            // 3: flags, exceptions and names never change.
            prop_assert_eq!(after.rule.days, before.rule.days);
            prop_assert_eq!(&after.rule.exceptions, &before.rule.exceptions);
            prop_assert_eq!(&after.name, &before.name);
            if !weekly(&before.rule) {
                prop_assert_eq!(after, before, "dated service is not carried");
                continue;
            }
            let old_end = before.rule.ranges.iter().map(|r| r.end).max().unwrap();
            if old_end < calendar_end.unwrap() - Duration::days(6) {
                prop_assert_eq!(after, before, "{} ended before the final week: not carried", id);
                continue;
            }
            for offset in -10..(from + len + 30) {
                let date = d(offset);
                // 1: coverage over the pick's period, past the type's
                // own end (a type whose service starts later is not
                // pulled back: the fork extends, it never backfills).
                if d(from) <= date && date <= through && date > old_end {
                    prop_assert_eq!(runs(&after.rule, date), runs(&CalendarRule {
                        ranges: vec![DateRange { start: date, end: date }],
                        ..before.rule.clone()
                    }, date), "{} on {}", id, date);
                }
                // 2: only the dates after the old end through `through`
                // can change.
                if date <= old_end || date > through {
                    prop_assert_eq!(runs(&after.rule, date), runs(&before.rule, date), "{} on {} moved", id, date);
                }
            }
        }

        // 4: idempotent.
        let again = apply_edits(&next, &[set_plan(from, Some(through))]).unwrap();
        prop_assert_eq!(&again, &next);
    }

    #[test]
    fn without_a_period_end_the_fork_is_the_frame_alone(
        types in proptest::collection::vec(gen_type(), 1..5),
        from in 0i64..400,
    ) {
        let net = network(&types);
        let next = apply_edits(&net, &[set_plan(from, None)]).unwrap();
        prop_assert_eq!(&next.service_types, &net.service_types);
        prop_assert_eq!(next.plan.effective_from, Some(d(from)));
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
        plan: PlanMeta { id: "w".into(), name: "W".into(), effective_from: Some(d(0)) },
        through: Some("2027-02-30".into()),
    };
    assert!(apply_edits(&net, &[edit]).expect_err("refused").reason.contains("invalid date"));
}

#[test]
fn the_winter_fork_runs_past_the_import_calendar() {
    // The C-TRAN shape of QAF-54: a weekday type ending 2027-01-09, a
    // holiday type dated 2026-12-31 only, and a pick on 2027-01-25.
    let mut net = Network::default();
    net.service_types.insert(
        "1-WKDY".into(),
        ServiceType {
            name: "Weekday".into(),
            rule: CalendarRule {
                days: WeekdaySet::weekdays(),
                ranges: vec![DateRange {
                    start: NaiveDate::from_ymd_opt(2026, 9, 13).unwrap(),
                    end: NaiveDate::from_ymd_opt(2027, 1, 9).unwrap(),
                }],
                exceptions: vec![],
            },
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
                    date: NaiveDate::from_ymd_opt(2026, 12, 31).unwrap(),
                    state: DayState::Added,
                }],
            },
        },
    );
    net.service_types.insert(
        "1-WKDY/30".into(),
        ServiceType {
            name: "Closure period".into(),
            rule: CalendarRule {
                days: WeekdaySet::weekdays(),
                ranges: vec![DateRange {
                    start: NaiveDate::from_ymd_opt(2026, 9, 14).unwrap(),
                    end: NaiveDate::from_ymd_opt(2026, 10, 9).unwrap(),
                }],
                exceptions: vec![],
            },
        },
    );
    let pick = NaiveDate::from_ymd_opt(2027, 1, 25).unwrap();
    let edit = NetworkEdit::SetPlan {
        plan: PlanMeta { id: "winter-2027".into(), name: "Winter 2027".into(), effective_from: Some(pick) },
        through: Some("2027-05-29".into()),
    };
    let next = apply_edits(&net, &[edit]).unwrap();
    assert!(!net.service_types["1-WKDY"].rule.runs_on(pick));
    assert!(next.service_types["1-WKDY"].rule.runs_on(pick), "the weekday runs on the pick's date");
    assert_eq!(next.service_types["1-WKDY"].rule.ranges[0].end.to_string(), "2027-05-29");
    assert_eq!(next.service_types["4-NYE"], net.service_types["4-NYE"], "next year's holiday is the planner's");
    assert_eq!(next.service_types["1-WKDY/30"], net.service_types["1-WKDY/30"], "a closed period is not carried");
}
