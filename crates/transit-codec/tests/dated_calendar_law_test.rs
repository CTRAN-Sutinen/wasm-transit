//! The dated-calendar laws (D-105): a feed may publish weekly service
//! as dates alone — no weekday flagged in calendar.txt (or no row at
//! all), and every day it runs an ADDED row in calendar_dates.txt, as
//! C-TRAN does. The import reads the weekly rule out of those dates,
//! so the model sees a weekday service as one (and the fork can carry
//! it, D-41); the export writes the dates back as the feed had them.
//!
//! Written BEFORE the implementation, from the decision. The
//! recognition threshold is D-111 (encoded provisionally):
//!
//! 1. **Exact** — whatever the import reads, the imported rule runs on
//!    exactly the feed's dates: no date gained, none lost.
//! 2. **Recognition** (D-111) — over the span of the dates (first to
//!    last), a weekday is part of the rule when the service runs on at
//!    least 4 of them AND on at least three quarters of that weekday's
//!    occurrences in the span. Fewer, and that weekday's dates stay
//!    dated (added exceptions). No weekday qualifying: the service
//!    stays dated, imported as the feed wrote it.
//! 3. **The shape of the rule** — the qualifying weekdays as the flags,
//!    one range from the first to the last date ON those weekdays (so a
//!    closed period ends where its dates end), a REMOVED exception for
//!    each of those weekdays in the range it does not run, an ADDED
//!    exception for each date the flags and range do not cover (a
//!    holiday running Sunday service) — and nothing else.
//! 4. **Only dated service is read** — a service with a flagged
//!    calendar row, or any removed date, imports as the feed wrote it.
//! 5. **Lossless** — export(import(feed)) writes calendar.txt and
//!    calendar_dates.txt back as the feed had them.
//! 6. **The publisher's form after an edit** — a carried (extended)
//!    rule still exports as dates: the same calendar row, and every
//!    date the rule now runs as an added date.

use chrono::{Datelike, Duration, NaiveDate};
use proptest::prelude::*;
use std::collections::BTreeSet;
use transit_codec::feed::{AgencyRow, CalendarDateRow, CalendarRow};
use transit_codec::{export, import, Feed};
use transit_network::{CalendarRule, DayState};

fn day0() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 9, 7).unwrap() // a Monday
}

fn gtfs(d: NaiveDate) -> String {
    d.format("%Y%m%d").to_string()
}

fn s(v: &str) -> Option<String> {
    Some(v.to_string())
}

#[derive(Clone, Debug)]
struct Dated {
    dates: BTreeSet<NaiveDate>,
    /// A zero-flag calendar row and its range (offsets from day0), or
    /// no calendar row at all.
    row: Option<(i64, i64)>,
}

/// A weekly rhythm with holes and extras, or sparse dates.
fn dated() -> impl Strategy<Value = Dated> {
    let rhythm = (
        0i64..14,                                  // start offset
        1i64..20,                                  // weeks
        proptest::array::uniform7(proptest::bool::weighted(0.4)),
        proptest::collection::vec(0i64..140, 0..6), // holes (offsets)
        proptest::collection::vec(0i64..150, 0..4), // extras (offsets)
    )
        .prop_map(|(start, weeks, mask, holes, extras)| {
            let mut dates = BTreeSet::new();
            for k in 0..weeks * 7 {
                let d = day0() + Duration::days(start + k);
                if mask[d.weekday().num_days_from_monday() as usize] {
                    dates.insert(d);
                }
            }
            for h in holes {
                dates.remove(&(day0() + Duration::days(h)));
            }
            for e in extras {
                dates.insert(day0() + Duration::days(e));
            }
            dates
        });
    let sparse = proptest::collection::btree_set(0i64..150, 1..8)
        .prop_map(|o| o.into_iter().map(|o| day0() + Duration::days(o)).collect::<BTreeSet<_>>());
    (prop_oneof![3 => rhythm, 1 => sparse], proptest::option::of((0i64..10, 100i64..200)))
        .prop_filter("a service runs somewhere", |(d, _)| !d.is_empty())
        .prop_map(|(dates, row)| Dated { dates, row })
}

fn feed(services: &[Dated]) -> Feed {
    let mut feed = Feed::default();
    feed.agency.push(AgencyRow {
        agency_name: s("T"),
        agency_url: s("https://example.org"),
        agency_timezone: s("UTC"),
        ..Default::default()
    });
    for (i, svc) in services.iter().enumerate() {
        let id = format!("svc{i}");
        if let Some((a, b)) = svc.row {
            feed.calendar.push(CalendarRow {
                service_id: Some(id.clone()),
                monday: s("0"),
                tuesday: s("0"),
                wednesday: s("0"),
                thursday: s("0"),
                friday: s("0"),
                saturday: s("0"),
                sunday: s("0"),
                start_date: Some(gtfs(day0() + Duration::days(a))),
                end_date: Some(gtfs(day0() + Duration::days(b))),
            });
        }
        for d in &svc.dates {
            feed.calendar_dates.push(CalendarDateRow {
                service_id: Some(id.clone()),
                date: Some(gtfs(*d)),
                exception_type: s("1"),
            });
        }
    }
    feed
}

/// The spec's reading of a rule, written out here.
fn runs(rule: &CalendarRule, date: NaiveDate) -> bool {
    if let Some(e) = rule.exceptions.iter().find(|e| e.date == date) {
        return e.state == DayState::Added;
    }
    let w = rule.days;
    let on = [w.mon, w.tue, w.wed, w.thu, w.fri, w.sat, w.sun][date.weekday().num_days_from_monday() as usize];
    on && rule.ranges.iter().any(|r| r.start <= date && date <= r.end)
}

fn flags(rule: &CalendarRule) -> [bool; 7] {
    let w = rule.days;
    [w.mon, w.tue, w.wed, w.thu, w.fri, w.sat, w.sun]
}

/// D-111, recomputed: which weekdays qualify.
fn expected_flags(dates: &BTreeSet<NaiveDate>) -> [bool; 7] {
    let (first, last) = (*dates.first().unwrap(), *dates.last().unwrap());
    let mut out = [false; 7];
    for (w, slot) in out.iter_mut().enumerate() {
        let mut occurrences = 0;
        let mut ran = 0;
        let mut d = first;
        while d <= last {
            if d.weekday().num_days_from_monday() as usize == w {
                occurrences += 1;
                ran += dates.contains(&d) as u32;
            }
            d += Duration::days(1);
        }
        *slot = ran >= 4 && 4 * ran >= 3 * occurrences;
    }
    out
}

/// The calendar files as sets of rows (the CSV's order is not the
/// feed's meaning).
fn calendar_rows(feed: &Feed) -> (BTreeSet<String>, BTreeSet<String>) {
    let cal = feed.calendar.iter().map(|r| format!("{r:?}")).collect();
    let dates = feed.calendar_dates.iter().map(|r| format!("{r:?}")).collect();
    (cal, dates)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn the_import_reads_a_weekly_rule_out_of_dated_service(services in proptest::collection::vec(dated(), 1..4)) {
        let f = feed(&services);
        let net = import(&f).network;
        for (i, svc) in services.iter().enumerate() {
            let rule = &net.service_types[&format!("svc{i}")].rule;
            let (first, last) = (*svc.dates.first().unwrap(), *svc.dates.last().unwrap());
            // 1: exact.
            let mut d = first - Duration::days(21);
            while d <= last + Duration::days(21) {
                prop_assert_eq!(runs(rule, d), svc.dates.contains(&d), "svc{} on {}", i, d);
                d += Duration::days(1);
            }
            // 2: recognition.
            let want = expected_flags(&svc.dates);
            if want.iter().any(|&b| b) {
                prop_assert_eq!(flags(rule), want);
                // 3: one range over the dates on the flagged weekdays.
                let on: Vec<NaiveDate> =
                    svc.dates.iter().copied().filter(|d| want[d.weekday().num_days_from_monday() as usize]).collect();
                prop_assert_eq!(rule.ranges.len(), 1);
                prop_assert_eq!(rule.ranges[0].start, on[0]);
                prop_assert_eq!(rule.ranges[0].end, *on.last().unwrap());
                // …and only the exceptions exactness needs.
                for e in &rule.exceptions {
                    let flagged = want[e.date.weekday().num_days_from_monday() as usize];
                    let inside = rule.ranges[0].start <= e.date && e.date <= rule.ranges[0].end;
                    match e.state {
                        DayState::Removed => prop_assert!(flagged && inside && !svc.dates.contains(&e.date)),
                        DayState::Added => prop_assert!(!(flagged && inside) && svc.dates.contains(&e.date)),
                    }
                }
                prop_assert!(rule.exceptions.windows(2).all(|w| w[0].date < w[1].date), "date order");
            } else {
                // Stays dated, as the feed wrote it.
                prop_assert_eq!(flags(rule), [false; 7]);
                prop_assert!(rule.exceptions.iter().all(|e| e.state == DayState::Added));
                prop_assert_eq!(rule.exceptions.len(), svc.dates.len());
            }
        }
        // 5: lossless.
        let back = export(&net, &import(&f).boundary).expect("exports");
        prop_assert_eq!(calendar_rows(&back), calendar_rows(&f));
    }

    #[test]
    fn a_carried_rule_exports_in_the_publishers_form(svc in dated(), weeks in 1i64..10) {
        // Only a service whose rule was read (a dated one that stays
        // dated has no rule to carry: no weekday is flagged).
        prop_assume!(expected_flags(&svc.dates).iter().any(|&b| b));
        let f = feed(std::slice::from_ref(&svc));
        let outcome = import(&f);
        let mut net = outcome.network.clone();
        let rule = &mut net.service_types.get_mut("svc0").unwrap().rule;
        if let Some(r) = rule.ranges.last_mut() {
            r.end += Duration::weeks(weeks);
        }
        let rule = rule.clone();
        let back = export(&net, &outcome.boundary).unwrap();
        // 6: the calendar row as the feed had it (zero flags, its
        // range), and every running date as an added date.
        let (cal, _) = calendar_rows(&back);
        prop_assert_eq!(cal, calendar_rows(&f).0);
        let exported: BTreeSet<NaiveDate> = back
            .calendar_dates
            .iter()
            .map(|r| {
                assert_eq!(r.exception_type.as_deref(), Some("1"));
                NaiveDate::parse_from_str(r.date.as_deref().unwrap(), "%Y%m%d").unwrap()
            })
            .collect();
        let first = *svc.dates.first().unwrap();
        let mut runs_on = BTreeSet::new();
        let mut d = first - Duration::days(7);
        while d <= first + Duration::days(400) {
            if runs(&rule, d) {
                runs_on.insert(d);
            }
            d += Duration::days(1);
        }
        prop_assert_eq!(exported, runs_on);
    }
}

#[test]
fn a_flagged_or_removing_calendar_is_read_as_written() {
    // 4: a flagged row with added dates, and a zero row with a removed
    // date, are not dated service.
    let mut f = feed(&[]);
    f.calendar.push(CalendarRow {
        service_id: s("wk"),
        monday: s("1"),
        tuesday: s("0"),
        wednesday: s("0"),
        thursday: s("0"),
        friday: s("0"),
        saturday: s("0"),
        sunday: s("0"),
        start_date: s("20260907"),
        end_date: s("20261231"),
    });
    let mut dated = BTreeSet::new();
    for k in 0..10 {
        let d = day0() + Duration::weeks(k) + Duration::days(1); // Tuesdays
        dated.insert(d);
        for id in ["wk", "zr"] {
            f.calendar_dates.push(CalendarDateRow { service_id: s(id), date: Some(gtfs(d)), exception_type: s("1") });
        }
    }
    f.calendar_dates.push(CalendarDateRow { service_id: s("zr"), date: s("20260908"), exception_type: s("2") });
    let net = import(&f).network;
    let wk = &net.service_types["wk"].rule;
    assert_eq!(flags(wk), [true, false, false, false, false, false, false]);
    assert_eq!(wk.exceptions.len(), 10);
    let zr = &net.service_types["zr"].rule;
    assert_eq!(flags(zr), [false; 7], "a removed date: the feed's own reading");
    assert_eq!(zr.exceptions.len(), 11);
}

#[test]
fn the_ctran_shapes_read_as_the_decision_says() {
    // The shapes D-105 names (measured on C-TRAN's feed): a weekday
    // service with holiday gaps, Sunday service with holidays running
    // it, a closed period, a Tuesday-only overlay, and a one-day type.
    let span = |from: NaiveDate, to: NaiveDate, days: &[u32]| {
        let mut out = BTreeSet::new();
        let mut d = from;
        while d <= to {
            if days.contains(&d.weekday().num_days_from_monday()) {
                out.insert(d);
            }
            d += Duration::days(1);
        }
        out
    };
    let ymd = |y, m, d| NaiveDate::from_ymd_opt(y, m, d).unwrap();
    let mut weekday = span(ymd(2026, 9, 14), ymd(2027, 1, 8), &[0, 1, 2, 3, 4]);
    for gap in [ymd(2026, 11, 26), ymd(2026, 11, 27), ymd(2026, 12, 24), ymd(2026, 12, 25), ymd(2027, 1, 1)] {
        weekday.remove(&gap);
    }
    let mut sunday = span(ymd(2026, 9, 13), ymd(2027, 1, 3), &[6]);
    sunday.extend([ymd(2026, 11, 26), ymd(2026, 12, 25), ymd(2027, 1, 1)]);
    let closed = span(ymd(2026, 9, 14), ymd(2026, 10, 9), &[0, 1, 2, 3, 4]);
    let tuesdays: BTreeSet<_> =
        [ymd(2026, 9, 22), ymd(2026, 10, 6), ymd(2026, 10, 20), ymd(2026, 11, 3), ymd(2026, 11, 17), ymd(2026, 12, 1), ymd(2026, 12, 15), ymd(2027, 1, 5)].into();
    let nye: BTreeSet<_> = [ymd(2026, 12, 31)].into();
    let row = Some((6, 124)); // 2026-09-13 → 2027-01-09
    let services: Vec<Dated> =
        [weekday, sunday, closed, tuesdays, nye].into_iter().map(|dates| Dated { dates, row }).collect();
    let net = import(&feed(&services)).network;
    let rule = |i: usize| net.service_types[&format!("svc{i}")].rule.clone();
    assert_eq!(flags(&rule(0)), [true, true, true, true, true, false, false]);
    assert_eq!(rule(0).ranges[0].end, ymd(2027, 1, 8));
    assert_eq!(rule(0).exceptions.len(), 5, "the holiday gaps, removed");
    assert_eq!(flags(&rule(1)), [false, false, false, false, false, false, true]);
    assert_eq!(rule(1).exceptions.iter().filter(|e| e.state == DayState::Added).count(), 3, "holidays run Sunday service");
    assert_eq!(rule(2).ranges[0].end, ymd(2026, 10, 9), "a closed period ends where its dates end");
    assert_eq!(flags(&rule(3)), [false; 7], "every other Tuesday stays dated");
    assert_eq!(flags(&rule(4)), [false; 7], "one date stays dated");
}
