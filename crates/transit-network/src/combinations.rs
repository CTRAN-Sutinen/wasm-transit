//! Service COMBINATIONS (D-55): the planner's service group is the set
//! of service types that run TOGETHER on the same dates, shown with the
//! dates on which it holds. Service ids key trips; a combination is
//! what a rider actually gets on a day. Types that never share a day
//! are never pooled (C-TRAN's 1-WKDY/30 and /31 run in successive
//! periods, so they meet 1-WKDY in two combinations, never in one),
//! and a dated exception (a holiday the base skips and a holiday type
//! runs) is a combination of its own.
//!
//! No date stands in for a combination: every date it holds on runs
//! exactly its members, so any one of them materializes the same day
//! (the analysis laws pin that equality), and the answer names the
//! combination by its members.

use std::collections::{BTreeMap, BTreeSet};

use chrono::{Duration, NaiveDate};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{DayState, Network};

/// One combination of service types and the dates on which it holds.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
pub struct ServiceCombination {
    /// The combination's name and address: its members joined " + "
    /// ("1-WKDY + 1-WKDY/30").
    pub id: String,
    /// The service types that run together, sorted. Only types that
    /// carry trips: a type with none runs no service (D-94).
    pub members: Vec<String>,
    /// Every date it holds on ("YYYY-MM-DD", sorted).
    pub dates: Vec<String>,
    /// The dates folded into spans: a new span starts where the next
    /// date is more than seven days after the last (D-94) — a weekly
    /// pattern reads as one span, a period that comes back later as
    /// another.
    pub spans: Vec<DateSpan>,
}

/// A run of the dates a combination holds on.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
pub struct DateSpan {
    pub start: String,
    pub end: String,
    /// The dates it holds on inside the span.
    pub days: u32,
}

/// The longest step between two dates of one span.
const SPAN_GAP_DAYS: i64 = 7;

/// Sorted dates folded into spans: a new span starts where the next
/// date is more than seven days after the last (D-94).
pub fn date_spans(dates: &[NaiveDate]) -> Vec<DateSpan> {
    let mut spans: Vec<(NaiveDate, NaiveDate, u32)> = Vec::new();
    for &d in dates {
        match spans.last_mut() {
            Some(s) if (d - s.1).num_days() <= SPAN_GAP_DAYS => {
                s.1 = d;
                s.2 += 1;
            }
            _ => spans.push((d, d, 1)),
        }
    }
    let iso = |d: NaiveDate| d.format("%Y-%m-%d").to_string();
    spans.into_iter().map(|(start, end, days)| DateSpan { start: iso(start), end: iso(end), days }).collect()
}

/// Every date any service type could run on: its ranges' dates and its
/// added exceptions.
fn candidate_dates(network: &Network) -> BTreeSet<NaiveDate> {
    let mut out = BTreeSet::new();
    for st in network.service_types.values() {
        for r in &st.rule.ranges {
            let mut d = r.start;
            while d <= r.end {
                out.insert(d);
                d += Duration::days(1);
            }
        }
        out.extend(st.rule.exceptions.iter().filter(|e| e.state == DayState::Added).map(|e| e.date));
    }
    out
}

/// The earliest date any type with trips runs (D-140): the date an
/// imported feed picks on. None when nothing runs.
pub fn first_service_date(network: &Network) -> Option<NaiveDate> {
    service_combinations(network)
        .iter()
        .filter_map(|c| c.dates.first())
        .filter_map(|d| NaiveDate::parse_from_str(d, "%Y-%m-%d").ok())
        .min()
}

/// The service combinations: one per distinct set of types (with
/// trips) that run on some date, with the dates it holds on. Sorted by
/// the days it holds (most first), then its first date, then its id.
pub fn service_combinations(network: &Network) -> Vec<ServiceCombination> {
    let with_trips: BTreeSet<&str> = network
        .pattern_trips
        .values()
        .filter(|pt| !pt.trips.is_empty())
        .map(|pt| pt.service_type.as_str())
        .collect();
    let mut by_members: BTreeMap<Vec<String>, Vec<NaiveDate>> = BTreeMap::new();
    for date in candidate_dates(network) {
        let members: Vec<String> = network
            .service_types
            .iter()
            .filter(|(id, st)| with_trips.contains(id.as_str()) && st.rule.runs_on(date))
            .map(|(id, _)| id.clone())
            .collect();
        if !members.is_empty() {
            by_members.entry(members).or_default().push(date);
        }
    }
    let mut out: Vec<ServiceCombination> = by_members
        .into_iter()
        .map(|(members, dates)| ServiceCombination {
            id: members.join(" + "),
            members,
            dates: dates.iter().map(|d| d.format("%Y-%m-%d").to_string()).collect(),
            spans: date_spans(&dates),
        })
        .collect();
    out.sort_by(|a, b| {
        b.dates
            .len()
            .cmp(&a.dates.len())
            .then_with(|| a.dates[0].cmp(&b.dates[0]))
            .then_with(|| a.id.cmp(&b.id))
    });
    out
}

/// The combination whose members are exactly `members` (any order),
/// or None when that set never runs together on a date.
pub fn service_combination(network: &Network, members: &[String]) -> Option<ServiceCombination> {
    let mut wanted: Vec<String> = members.to_vec();
    wanted.sort();
    wanted.dedup();
    if wanted.is_empty() {
        return None;
    }
    service_combinations(network).into_iter().find(|c| c.members == wanted)
}
