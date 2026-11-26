//! The service date's laws, with chrono as the oracle over years
//! 0000-9999.

use chrono::{Datelike, NaiveDate};
use proptest::prelude::*;
use transit_date::{Date, DateError, Days, Weekday};

fn oracle(d: Date) -> NaiveDate {
    NaiveDate::from_ymd_opt(d.year(), d.month(), d.day()).unwrap()
}

fn any_date() -> impl Strategy<Value = Date> {
    let lo = NaiveDate::from_ymd_opt(0, 1, 1).unwrap().num_days_from_ce();
    let hi = NaiveDate::from_ymd_opt(9999, 12, 31).unwrap().num_days_from_ce();
    (lo..=hi).prop_map(|n| {
        let c = NaiveDate::from_num_days_from_ce_opt(n).unwrap();
        Date::from_ymd_opt(c.year(), c.month(), c.day()).unwrap()
    })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(4096))]

    #[test]
    fn a_valid_date_reads_back_as_written(d in any_date()) {
        let text = d.to_string();
        prop_assert_eq!(&text, &oracle(d).format("%Y-%m-%d").to_string());
        prop_assert_eq!(Date::parse(&text), Ok(d));
        let json = serde_json::to_string(&d).unwrap();
        prop_assert_eq!(serde_json::from_str::<Date>(&json).unwrap(), d);
    }

    #[test]
    fn from_ymd_agrees_with_chrono(y in -2i32..10_002, m in 0u32..14, day in 0u32..33) {
        let ours = Date::from_ymd_opt(y, m, day).map(|d| (d.year(), d.month(), d.day()));
        let theirs = NaiveDate::from_ymd_opt(y, m, day)
            .filter(|_| (0..=9999).contains(&y))
            .map(|d| (d.year(), d.month(), d.day()));
        prop_assert_eq!(ours, theirs);
    }

    #[test]
    fn the_weekday_agrees_with_chrono(d in any_date()) {
        let theirs = oracle(d).weekday();
        prop_assert_eq!(d.weekday().num_days_from_monday(), theirs.num_days_from_monday());
        prop_assert_eq!(d.weekday().name(), oracle(d).format("%A").to_string());
        prop_assert_eq!(d.weekday().number_from_monday().to_string(), oracle(d).format("%u").to_string());
    }

    #[test]
    fn day_arithmetic_agrees_with_chrono(d in any_date(), n in -800_000i64..800_000) {
        let ours = d.checked_add_days(Days::new(n));
        let theirs = oracle(d)
            .checked_add_signed(chrono::Duration::days(n))
            .filter(|t| (0..=9999).contains(&t.year()));
        prop_assert_eq!(ours.map(oracle), theirs);
        if let Some(later) = ours {
            prop_assert_eq!((later - d).num_days(), n);
            prop_assert_eq!(later - Days::new(n), d);
        }
    }

    #[test]
    fn order_is_the_calendar_order(a in any_date(), b in any_date()) {
        prop_assert_eq!(a.cmp(&b), oracle(a).cmp(&oracle(b)));
        prop_assert_eq!(a.cmp(&b), a.to_string().cmp(&b.to_string()));
    }

    /// What parse reads is exactly the strict shape that chrono also reads.
    #[test]
    fn parse_reads_only_the_strict_shape(text in "[0-9 +-]{0,4}-?[0-9 ]{0,3}-?[0-9 ]{0,3}[ ]?") {
        let strict = text.len() == 10
            && text.bytes().enumerate().all(|(i, c)| if i == 4 || i == 7 { c == b'-' } else { c.is_ascii_digit() });
        let theirs = NaiveDate::parse_from_str(&text, "%Y-%m-%d").ok().filter(|_| strict);
        prop_assert_eq!(Date::parse(&text).ok().map(oracle), theirs);
    }

    #[test]
    fn a_strict_shape_reads_when_chrono_finds_the_day(text in "[0-9]{4}-[01][0-9]-[0-3][0-9]") {
        let theirs = NaiveDate::parse_from_str(&text, "%Y-%m-%d").ok();
        prop_assert_eq!(Date::parse(&text).ok().map(oracle), theirs);
        prop_assert_eq!(Date::parse(&text).err().is_some_and(|e| e == DateError::NoSuchDay), theirs.is_none());
    }
}

#[test]
fn parse_refuses_what_is_not_a_date() {
    for bad in ["", "2026-1-05", "2026-01-5", " 2026-01-05", "2026-01-05 ", "2026/01/05", "20260105", "+2026-01-05", "２０２６-01-05", "2026-01-05T00:00"] {
        assert_eq!(Date::parse(bad), Err(DateError::Shape), "{bad:?}");
    }
    for bad in ["2026-02-29", "2026-13-01", "2026-00-10", "2026-04-31", "2026-01-00"] {
        assert_eq!(Date::parse(bad), Err(DateError::NoSuchDay), "{bad:?}");
    }
    assert_eq!(Date::parse("2024-02-29").map(|d| d.weekday()), Ok(Weekday::Thu));
    let err = serde_json::from_str::<Date>("\"2026-1-5\"").unwrap_err().to_string();
    assert!(err.contains("invalid date \"2026-1-5\": not a YYYY-MM-DD date"), "{err}");
}

#[test]
fn the_ends_are_the_four_digit_years() {
    assert_eq!(Date::MIN.to_string(), "0000-01-01");
    assert_eq!(Date::MAX.to_string(), "9999-12-31");
    assert_eq!(Date::MAX.succ_opt(), None);
    assert_eq!(Date::MIN.pred_opt(), None);
    assert_eq!(format!("{:>12}|{:?}", Date::MIN, Date::MAX), "  0000-01-01|9999-12-31");
}
