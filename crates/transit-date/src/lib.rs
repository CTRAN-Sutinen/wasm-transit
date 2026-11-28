//! The service date. Every library reads and writes a date as
//! "YYYY-MM-DD" through [`Date::parse`] and [`Date`]'s `Display` and
//! serde impls, so one rule decides what a date is: exactly ten
//! characters, four digits, a hyphen, two digits, a hyphen, two digits,
//! naming a day that exists. Nothing else is read: no spaces, no short
//! month, no other separator. GTFS's "YYYYMMDD" is the codec's format at
//! the boundary, and transit-codec and transit-rt read it themselves.
//!
//! Years run from 0000 to 9999, the years that four digits write.
//! Arithmetic that leaves them panics; [`Date::checked_add_days`] does
//! not.
//!
//! A library with no workspace dependency: transit-network may use it
//! (the arch gates, `crates/cap-abi/src/bin/common/gates.rs`).

use std::fmt;
use std::ops::{Add, AddAssign, Sub, SubAssign};

/// A day, as days after 1970-01-01.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Date(i32);

/// A count of days, for [`Date`]'s arithmetic.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Days(i64);

impl Days {
    pub const fn new(n: i64) -> Days {
        Days(n)
    }

    pub const fn weeks(n: i64) -> Days {
        Days(n * 7)
    }

    pub const fn num_days(self) -> i64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Weekday {
    Mon,
    Tue,
    Wed,
    Thu,
    Fri,
    Sat,
    Sun,
}

impl Weekday {
    const ALL: [Weekday; 7] = [
        Weekday::Mon,
        Weekday::Tue,
        Weekday::Wed,
        Weekday::Thu,
        Weekday::Fri,
        Weekday::Sat,
        Weekday::Sun,
    ];

    /// Monday is 0.
    pub const fn num_days_from_monday(self) -> u32 {
        self as u32
    }

    /// Monday is 1, as ISO 8601 numbers the days.
    pub const fn number_from_monday(self) -> u32 {
        self as u32 + 1
    }

    /// "Monday" through "Sunday".
    pub const fn name(self) -> &'static str {
        match self {
            Weekday::Mon => "Monday",
            Weekday::Tue => "Tuesday",
            Weekday::Wed => "Wednesday",
            Weekday::Thu => "Thursday",
            Weekday::Fri => "Friday",
            Weekday::Sat => "Saturday",
            Weekday::Sun => "Sunday",
        }
    }
}

/// Why a text is not a date.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DateError {
    /// Not "YYYY-MM-DD".
    Shape,
    /// The shape, but no such day (a month 13, a February 30).
    NoSuchDay,
}

impl fmt::Display for DateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            DateError::Shape => "not a YYYY-MM-DD date",
            DateError::NoSuchDay => "no such day",
        })
    }
}

impl std::error::Error for DateError {}

const fn is_leap(y: i32) -> bool {
    y % 4 == 0 && (y % 100 != 0 || y % 400 == 0)
}

const fn days_in_month(y: i32, m: u32) -> u32 {
    match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        _ if is_leap(y) => 29,
        _ => 28,
    }
}

// Howard Hinnant's days_from_civil and civil_from_days.
const fn days_from_civil(y: i32, m: u32, d: u32) -> i32 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = (y - era * 400) as u32;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe as i32 - 719468
}

const fn civil_from_days(z: i32) -> (i32, u32, u32) {
    let z = z + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u32;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe as i32 + era * 400;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

impl Date {
    pub const MIN: Date = Date(days_from_civil(0, 1, 1));
    pub const MAX: Date = Date(days_from_civil(9999, 12, 31));

    /// None for a day that does not exist or a year outside 0000-9999.
    pub const fn from_ymd_opt(year: i32, month: u32, day: u32) -> Option<Date> {
        if year < 0
            || year > 9999
            || month < 1
            || month > 12
            || day < 1
            || day > days_in_month(year, month)
        {
            return None;
        }
        Some(Date(days_from_civil(year, month, day)))
    }

    /// Reads "YYYY-MM-DD", and nothing else.
    pub fn parse(text: &str) -> Result<Date, DateError> {
        let b = text.as_bytes();
        let digits = |r: std::ops::Range<usize>| -> Option<u32> {
            b[r].iter().try_fold(0u32, |n, c| {
                c.is_ascii_digit().then(|| n * 10 + (c - b'0') as u32)
            })
        };
        if b.len() != 10 || b[4] != b'-' || b[7] != b'-' {
            return Err(DateError::Shape);
        }
        let (Some(y), Some(m), Some(d)) = (digits(0..4), digits(5..7), digits(8..10)) else {
            return Err(DateError::Shape);
        };
        Date::from_ymd_opt(y as i32, m, d).ok_or(DateError::NoSuchDay)
    }

    /// [`Date::parse`], with the message an op or a read answers:
    /// `invalid date "2026-1-5": not a YYYY-MM-DD date`.
    pub fn read(text: &str) -> Result<Date, String> {
        Date::parse(text).map_err(|e| format!("invalid date {text:?}: {e}"))
    }

    fn civil(self) -> (i32, u32, u32) {
        civil_from_days(self.0)
    }

    pub fn year(self) -> i32 {
        self.civil().0
    }

    /// January is 1.
    pub fn month(self) -> u32 {
        self.civil().1
    }

    /// January is 0.
    pub fn month0(self) -> u32 {
        self.month() - 1
    }

    pub fn day(self) -> u32 {
        self.civil().2
    }

    pub fn weekday(self) -> Weekday {
        // 1970-01-01 was a Thursday.
        Weekday::ALL[(self.0 as i64 + 3).rem_euclid(7) as usize]
    }

    /// None outside 0000-01-01 to 9999-12-31.
    pub fn checked_add_days(self, days: Days) -> Option<Date> {
        let n = self.0 as i64 + days.0;
        (Date::MIN.0 as i64..=Date::MAX.0 as i64)
            .contains(&n)
            .then(|| Date(n as i32))
    }

    pub fn checked_sub_days(self, days: Days) -> Option<Date> {
        self.checked_add_days(Days(-days.0))
    }

    pub fn succ_opt(self) -> Option<Date> {
        self.checked_add_days(Days(1))
    }

    pub fn pred_opt(self) -> Option<Date> {
        self.checked_add_days(Days(-1))
    }
}

impl Add<Days> for Date {
    type Output = Date;
    fn add(self, days: Days) -> Date {
        self.checked_add_days(days)
            .expect("a date between 0000-01-01 and 9999-12-31")
    }
}

impl Sub<Days> for Date {
    type Output = Date;
    fn sub(self, days: Days) -> Date {
        self.checked_sub_days(days)
            .expect("a date between 0000-01-01 and 9999-12-31")
    }
}

impl AddAssign<Days> for Date {
    fn add_assign(&mut self, days: Days) {
        *self = *self + days;
    }
}

impl SubAssign<Days> for Date {
    fn sub_assign(&mut self, days: Days) {
        *self = *self - days;
    }
}

/// The days from `other` to `self`.
impl Sub<Date> for Date {
    type Output = Days;
    fn sub(self, other: Date) -> Days {
        Days(self.0 as i64 - other.0 as i64)
    }
}

impl fmt::Display for Date {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (y, m, d) = self.civil();
        // pad() so a width or alignment in the format string applies.
        f.pad(&format!("{y:04}-{m:02}-{d:02}"))
    }
}

impl fmt::Debug for Date {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

impl std::str::FromStr for Date {
    type Err = DateError;
    fn from_str(s: &str) -> Result<Date, DateError> {
        Date::parse(s)
    }
}

impl serde::Serialize for Date {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

impl<'de> serde::Deserialize<'de> for Date {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Date, D::Error> {
        struct Visit;
        impl serde::de::Visitor<'_> for Visit {
            type Value = Date;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a YYYY-MM-DD date")
            }
            fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<Date, E> {
                Date::read(v).map_err(E::custom)
            }
        }
        d.deserialize_str(Visit)
    }
}

#[cfg(feature = "bindings")]
impl schemars::JsonSchema for Date {
    fn inline_schema() -> bool {
        true
    }
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "Date".into()
    }
    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({ "type": "string", "format": "date" })
    }
}
