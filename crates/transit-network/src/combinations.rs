//! A combination is what a rider gets on a day. Types that never
//! share a day are never pooled. C-TRAN's 1-WKDY/30 and /31 run in
//! successive periods, so they meet 1-WKDY in two combinations, never in
//! one. A dated exception is a combination of its own.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use transit_date::{Date, Days};
#[cfg(feature = "bindings")]
use ts_rs::TS;

use crate::{DayState, Network};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS, schemars::JsonSchema))]
#[cfg_attr(feature = "bindings", ts(export))]
pub struct ServiceCombination {
    /// The members joined by " + ", as in "1-WKDY + 1-WKDY/30".
    pub id: String,
    /// Sorted. Only types with trips count (D-94).
    pub members: Vec<String>,
    /// "YYYY-MM-DD" strings, sorted.
    pub dates: Vec<String>,
    pub spans: Vec<DateSpan>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS, schemars::JsonSchema))]
#[cfg_attr(feature = "bindings", ts(export))]
pub struct DateSpan {
    pub start: String,
    pub end: String,
    pub days: u32,
}

/// The longest gap between two dates of one span (D-94).
const SPAN_GAP_DAYS: i64 = 7;

pub fn date_spans(dates: &[Date]) -> Vec<DateSpan> {
    let mut spans: Vec<(Date, Date, u32)> = Vec::new();
    for &d in dates {
        match spans.last_mut() {
            Some(s) if (d - s.1).num_days() <= SPAN_GAP_DAYS => {
                s.1 = d;
                s.2 += 1;
            }
            _ => spans.push((d, d, 1)),
        }
    }
    let iso = |d: Date| d.to_string();
    spans
        .into_iter()
        .map(|(start, end, days)| DateSpan {
            start: iso(start),
            end: iso(end),
            days,
        })
        .collect()
}

fn candidate_dates(network: &Network) -> BTreeSet<Date> {
    let mut out = BTreeSet::new();
    for st in network.service_types.values() {
        for r in &st.rule.ranges {
            let mut d = r.start;
            while d <= r.end {
                out.insert(d);
                d += Days::new(1);
            }
        }
        out.extend(
            st.rule
                .exceptions
                .iter()
                .filter(|e| e.state == DayState::Added)
                .map(|e| e.date),
        );
    }
    out
}

/// An imported feed picks on this date.
pub fn first_service_date(network: &Network) -> Option<Date> {
    service_combinations(network)
        .iter()
        .filter_map(|c| c.dates.first())
        .filter_map(|d| Date::parse(d).ok())
        .min()
}

/// One `set_plan` effective from the first service date.
/// Empty when the document has a plan id already, or runs no service.
pub fn stamp_as_pick(network: &Network, name: &str, id: &str) -> Vec<crate::NetworkEdit> {
    if !network.plan.id.is_empty() {
        return Vec::new();
    }
    let Some(first) = first_service_date(network) else {
        return Vec::new();
    };
    let plan = crate::model::PlanMeta {
        id: id.into(),
        name: name.into(),
        effective_from: Some(first),
        ..Default::default()
    };
    vec![crate::NetworkEdit::SetPlan {
        plan,
        through: None,
    }]
}

/// Only types with trips count (D-94). The order is days held (most first),
/// then first date, then id.
pub fn service_combinations(network: &Network) -> Vec<ServiceCombination> {
    let with_trips: BTreeSet<&str> = network
        .pattern_trips
        .values()
        .filter(|pt| !pt.trips.is_empty())
        .map(|pt| pt.service_type.as_str())
        .collect();
    let mut by_members: BTreeMap<Vec<String>, Vec<Date>> = BTreeMap::new();
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
            dates: dates.iter().map(|d| d.to_string()).collect(),
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

pub fn service_combination(network: &Network, members: &[String]) -> Option<ServiceCombination> {
    let mut wanted: Vec<String> = members.to_vec();
    wanted.sort();
    wanted.dedup();
    if wanted.is_empty() {
        return None;
    }
    service_combinations(network)
        .into_iter()
        .find(|c| c.members == wanted)
}
