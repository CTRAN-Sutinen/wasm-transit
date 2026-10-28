//! What a rider is told about a compared plan — the one place the
//! notice is computed.
//!
//! A compared plan is the same query over two worlds: the day with
//! the diversions in force (active) and the unpatched day (baseline).
//! The rider hears about ANY change the diversion makes to their
//! trip: no trip at all, a later arrival, or a ride on a diverted
//! trip (a trip instance whose patched stop times differ from its
//! unpatched instance) — not only when the active world has no trip.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use transit_network::{MaterializedTrip, Network};
use ts_rs::TS;

use crate::raptor::{fmt_time, Itinerary, Leg};

/// Which change the notice reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum NoticeKind {
    /// No trip in either world within the legs searched.
    NoTrip,
    /// No trip while the diversion is active; the unpatched day has
    /// one.
    Unreachable,
    /// The active trip arrives later than the unpatched day's.
    Later,
    /// The active trip arrives no later, but rides a diverted trip.
    Diverted,
}

/// The rider's notice: structured, plus one text line.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PlanNotice {
    pub kind: NoticeKind,
    /// `later`: seconds the diversion costs (active arrival minus
    /// baseline arrival).
    pub delay_secs: Option<u32>,
    /// `later`: the same loss in whole minutes, rounded up.
    pub minutes_lost: Option<u32>,
    /// The unpatched day's arrival, when it has a trip.
    pub baseline_arrival_secs: Option<u32>,
    /// The route labels of the diverted trips either plan rides, in
    /// order of first appearance.
    pub routes: Vec<String>,
    /// The stop ids those trips skip, in order of first appearance.
    pub skipped_stops: Vec<String>,
    pub text: String,
}

/// The day's diverted trip instances: (trip, start) → the stops its
/// patched instance no longer visits (empty when it only runs at
/// other times).
#[derive(Debug, Clone, Default)]
pub struct DivertedTrips {
    trips: HashMap<(String, u32), Vec<String>>,
}

impl DivertedTrips {
    pub fn is_empty(&self) -> bool {
        self.trips.is_empty()
    }

    /// The stops a diverted instance skips; `None` when the instance
    /// is not diverted.
    pub fn skipped(&self, trip: &str, start_secs: u32) -> Option<&[String]> {
        self.trips.get(&(trip.to_string(), start_secs)).map(Vec::as_slice)
    }
}

/// Compare the unpatched day with the patched one: every trip
/// instance whose stop times differ is diverted. A patched-away
/// instance (absent from the patched day) is not ridden by any plan,
/// so it needs no entry.
pub fn diverted_instances(baseline: &[MaterializedTrip], patched: &[MaterializedTrip]) -> DivertedTrips {
    let base: HashMap<(&str, u32), &MaterializedTrip> =
        baseline.iter().map(|t| ((t.trip.as_str(), t.start_secs), t)).collect();
    let mut trips = HashMap::new();
    for p in patched {
        let Some(b) = base.get(&(p.trip.as_str(), p.start_secs)) else { continue };
        if b.stop_times == p.stop_times {
            continue;
        }
        // The multiset difference: a loop's second visit is its own.
        let mut left: Vec<&str> = p.stop_times.iter().map(|s| s.stop.as_str()).collect();
        let mut skipped = Vec::new();
        for st in &b.stop_times {
            match left.iter().position(|s| *s == st.stop) {
                Some(i) => {
                    left.remove(i);
                }
                None => skipped.push(st.stop.clone()),
            }
        }
        trips.insert((p.trip.clone(), p.start_secs), skipped);
    }
    DivertedTrips { trips }
}

/// The notice for a compared plan, or `None` when the diversion
/// changes nothing for this trip: the active plan exists, arrives no
/// later than the baseline (or there is no baseline), and rides no
/// diverted trip.
pub fn plan_notice(
    network: &Network,
    active: Option<&Itinerary>,
    baseline: Option<&Itinerary>,
    max_legs: u32,
    diverted: &DivertedTrips,
) -> Option<PlanNotice> {
    // The diverted trips either plan rides: their routes and skips.
    let mut routes: Vec<String> = Vec::new();
    let mut skipped_stops: Vec<String> = Vec::new();
    let mut rides_diverted = false;
    for (it, is_active) in [(active, true), (baseline, false)] {
        for leg in it.iter().flat_map(|i| &i.legs) {
            let Leg::Transit { trip, start_secs, route_name, .. } = leg else { continue };
            let Some(skipped) = diverted.skipped(trip, *start_secs) else { continue };
            rides_diverted |= is_active;
            if !routes.contains(route_name) {
                routes.push(route_name.clone());
            }
            for s in skipped {
                if !skipped_stops.contains(s) {
                    skipped_stops.push(s.clone());
                }
            }
        }
    }

    let clock = |secs: u32| fmt_time(secs)[..5].to_string();
    let detour = || {
        let mut line = String::new();
        if !routes.is_empty() {
            line.push_str(&format!(" Route {} is on detour", routes.join(", ")));
            if !skipped_stops.is_empty() {
                let names: Vec<&str> = skipped_stops
                    .iter()
                    .map(|s| network.stops.get(s).map(|st| st.name.as_str()).unwrap_or(s))
                    .collect();
                line.push_str(&format!(" and skips {}", names.join(", ")));
            }
            line.push('.');
        }
        line
    };
    let baseline_arrival_secs = baseline.map(|b| b.arrival_secs);
    let notice = |kind, delay_secs: Option<u32>, text: String| PlanNotice {
        kind,
        delay_secs,
        minutes_lost: delay_secs.map(|d| d.div_ceil(60)),
        baseline_arrival_secs,
        routes: routes.clone(),
        skipped_stops: skipped_stops.clone(),
        text,
    };

    match (active, baseline) {
        (None, Some(base)) => Some(notice(
            NoticeKind::Unreachable,
            None,
            format!(
                "No trip found while the diversion is active — without it you would arrive {}.{}",
                clock(base.arrival_secs),
                detour()
            ),
        )),
        (None, None) => Some(notice(
            NoticeKind::NoTrip,
            None,
            format!("No trip found within {max_legs} legs — try a different time or closer stops."),
        )),
        (Some(a), Some(b)) if a.arrival_secs > b.arrival_secs => {
            let lost = a.arrival_secs - b.arrival_secs;
            Some(notice(
                NoticeKind::Later,
                Some(lost),
                format!(
                    "The diversion makes this trip {} min later — you arrive {} instead of {}.{}",
                    lost.div_ceil(60),
                    clock(a.arrival_secs),
                    clock(b.arrival_secs),
                    detour()
                ),
            ))
        }
        (Some(_), _) if rides_diverted => Some(notice(
            NoticeKind::Diverted,
            None,
            format!("This trip rides a diverted bus.{}", detour()),
        )),
        (Some(_), _) => None,
    }
}
