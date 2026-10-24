//! Service calendar: which `service_id`s run on which dates.
//!
//! GTFS defines service in two layers:
//!
//! 1. `calendar.txt` — a weekly pattern (`monday`..`sunday` flags) plus a
//!    `start_date`..`end_date` range.
//! 2. `calendar_dates.txt` — one-off exceptions for specific dates:
//!    `exception_type = 1` adds the service that day, `2` removes it.
//!
//! The rule is: an explicit exception for a date always wins; otherwise
//! the weekly pattern applies if the date is in range and the weekday
//! flag is set; otherwise the service does not run.
//!
//! A feed must contain at least one of the two files; feeds with only
//! `calendar_dates.txt` (e.g. many North American feeds) define everything
//! as explicit "added" dates, which this handles naturally.

use chrono::{Datelike, NaiveDate};
use std::collections::HashMap;

use crate::model::{Calendar, CalendarDate};

#[derive(Debug, Default, Clone)]
pub struct ServiceCalendar {
    /// Patterns from `calendar.txt`, by service_id.
    pub regular: HashMap<String, Calendar>,
    /// Exceptions from `calendar_dates.txt`, by (service_id, date).
    pub exceptions: HashMap<(String, NaiveDate), u8>,
}

impl ServiceCalendar {
    pub fn new(calendars: Vec<Calendar>, dates: Vec<CalendarDate>) -> Self {
        let mut regular = HashMap::with_capacity(calendars.len());
        for c in calendars {
            regular.insert(c.service_id.clone(), c);
        }
        let mut exceptions = HashMap::with_capacity(dates.len());
        for d in dates {
            exceptions.insert((d.service_id.clone(), d.date), d.exception_type);
        }
        Self { regular, exceptions }
    }

    pub fn is_active(&self, service_id: &str, date: NaiveDate) -> bool {
        if let Some(&exception) = self.exceptions.get(&(service_id.to_string(), date)) {
            return exception == 1;
        }
        match self.regular.get(service_id) {
            Some(cal) => cal.start_date <= date && date <= cal.end_date && cal.runs_on(date.weekday()),
            None => false,
        }
    }

    /// All service_ids running on `date`.
    ///
    /// A single date routinely has several active services: the regular
    /// weekly pattern(s) *plus* any services explicitly added by
    /// `calendar_dates.txt` (holiday services, exception-only feeds, ...).
    /// This is the union, sorted, with duplicates removed.
    pub fn active_services(&self, date: NaiveDate) -> Vec<String> {
        let mut out: Vec<String> = self
            .regular
            .keys()
            .filter(|id| self.is_active(id, date))
            .cloned()
            .collect();
        for ((service_id, d), &exception) in &self.exceptions {
            if *d == date && exception == 1 && !out.contains(service_id) {
                out.push(service_id.clone());
            }
        }
        out.sort();
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pattern_service() -> Calendar {
        // Runs weekdays only, all of 2026.
        Calendar {
            service_id: "WK".into(),
            monday: true,
            tuesday: true,
            wednesday: true,
            thursday: true,
            friday: true,
            saturday: false,
            sunday: false,
            start_date: NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
            end_date: NaiveDate::from_ymd_opt(2026, 12, 31).unwrap(),
        }
    }

    fn cal(date: (i32, u32, u32)) -> NaiveDate {
        NaiveDate::from_ymd_opt(date.0, date.1, date.2).unwrap()
    }

    #[test]
    fn weekly_pattern_respects_weekday_and_range() {
        let sc = ServiceCalendar::new(vec![pattern_service()], vec![]);
        assert!(sc.is_active("WK", cal((2026, 10, 23)))); // Friday
        assert!(!sc.is_active("WK", cal((2026, 10, 24)))); // Saturday
        assert!(!sc.is_active("WK", cal((2027, 1, 15)))); // out of range
    }

    #[test]
    fn exceptions_override_patterns() {
        // Service removed on a Friday, added on a Saturday.
        let dates = vec![
            CalendarDate { service_id: "WK".into(), date: cal((2026, 10, 23)), exception_type: 2 },
            CalendarDate { service_id: "WK".into(), date: cal((2026, 10, 24)), exception_type: 1 },
        ];
        let sc = ServiceCalendar::new(vec![pattern_service()], dates);
        assert!(!sc.is_active("WK", cal((2026, 10, 23))));
        assert!(sc.is_active("WK", cal((2026, 10, 24))));
    }

    #[test]
    fn exception_only_services_work_without_calendar_txt() {
        let dates = vec![
            CalendarDate { service_id: "EXTRA".into(), date: cal((2026, 10, 23)), exception_type: 1 },
        ];
        let sc = ServiceCalendar::new(vec![], dates);
        assert!(sc.is_active("EXTRA", cal((2026, 10, 23))));
        assert!(!sc.is_active("EXTRA", cal((2026, 10, 26))));
    }

    #[test]
    fn a_date_can_have_multiple_active_services() {
        // A weekday pattern running, plus a holiday service added by
        // exception on the same date — both active that day.
        let dates = vec![
            CalendarDate { service_id: "HOL".into(), date: cal((2026, 10, 23)), exception_type: 1 },
            CalendarDate { service_id: "OLD".into(), date: cal((2026, 10, 23)), exception_type: 2 },
        ];
        let mut cal_old = pattern_service();
        cal_old.service_id = "OLD".into();
        let sc = ServiceCalendar::new(vec![pattern_service(), cal_old], dates);

        let active = sc.active_services(cal((2026, 10, 23)));
        assert_eq!(active, vec!["HOL".to_string(), "WK".to_string()]);
        // OLD was removed by exception on that date; on other weekdays it runs.
        assert!(!sc.is_active("OLD", cal((2026, 10, 23))));
        assert!(sc.is_active("OLD", cal((2026, 10, 30))));
    }
}
