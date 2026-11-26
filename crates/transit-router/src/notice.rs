//! A compared plan is the same query over two worlds: the day with the edits
//! in force (the active world) and the unpatched day (the baseline). A
//! diverted trip is a trip instance whose patched stop times differ from its
//! unpatched instance.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use transit_network::{MaterializedTrip, Network};
#[cfg(feature = "bindings")]
use ts_rs::TS;

use crate::raptor::{fmt_time, Itinerary, Leg};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS))]
#[cfg_attr(feature = "bindings", ts(export))]
#[serde(rename_all = "snake_case")]
pub enum NoticeKind {
    NoTrip,
    Unreachable,
    Later,
    Diverted,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS))]
#[cfg_attr(feature = "bindings", ts(export))]
pub struct PlanNotice {
    pub kind: NoticeKind,
    pub delay_secs: Option<u32>,
    pub minutes_lost: Option<u32>,
    pub baseline_arrival_secs: Option<u32>,
    pub routes: Vec<String>,
    pub skipped_stops: Vec<String>,
    pub text: String,
}

/// Maps each diverted `(trip, start)` to the stops that its patched instance
/// no longer visits. The list is empty when the instance only runs at other
/// times.
#[derive(Debug, Clone, Default)]
pub struct DivertedTrips {
    trips: HashMap<(String, u32), Vec<String>>,
}

impl DivertedTrips {
    pub fn is_empty(&self) -> bool {
        self.trips.is_empty()
    }

    pub fn skipped(&self, trip: &str, start_secs: u32) -> Option<&[String]> {
        self.trips.get(&(trip.to_string(), start_secs)).map(Vec::as_slice)
    }
}

pub fn diverted_instances(baseline: &[MaterializedTrip], patched: &[MaterializedTrip]) -> DivertedTrips {
    let base: HashMap<(&str, u32), &MaterializedTrip> =
        baseline.iter().map(|t| ((t.trip.as_str(), t.start_secs), t)).collect();
    let mut trips = HashMap::new();
    for p in patched {
        let Some(b) = base.get(&(p.trip.as_str(), p.start_secs)) else { continue };
        if b.stop_times == p.stop_times {
            continue;
        }
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

pub fn plan_notice(
    network: &Network,
    active: Option<&Itinerary>,
    baseline: Option<&Itinerary>,
    max_legs: u32,
    diverted: &DivertedTrips,
) -> Option<PlanNotice> {
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
