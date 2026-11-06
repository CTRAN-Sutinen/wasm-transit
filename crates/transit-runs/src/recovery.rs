//! The recovery view: what each wait requires, and the slack
//! above it.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use transit_network::{recovery_floors, required_recovery, MaterializedTrip, Network, RecoveryAt, RecoverySource};
use ts_rs::TS;

use crate::WorkRules;

/// The day's waits and their per-pattern fold.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "transit_runs/")]
pub struct Recovery {
    /// Every wait between two running trips of a block, in block
    /// order (block id, then time).
    pub waits: Vec<RecoveryWait>,
    /// Per pattern of the trip before the wait, by pattern id.
    pub patterns: Vec<PatternRecovery>,
}

/// One wait: the trip before it, where the vehicle waits, how long,
/// what is required there, and the slack.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "transit_runs/")]
pub struct RecoveryWait {
    pub block_id: String,
    /// The trip before the wait, as the block's index.
    pub after_trip: usize,
    pub trip: String,
    pub route: String,
    pub pattern: String,
    /// Where the vehicle waits: the layover's `at`, else the stop the
    /// trip ends at.
    pub terminal: String,
    pub start_secs: u32,
    /// The trip's run on the day (its last arrival less its start).
    pub run_secs: u32,
    /// From the trip's arrival to the next trip's start.
    pub gap_secs: u32,
    /// The recovery required (D-9): the rules' when one applies, else
    /// the terminal's floor where the block authors a layover; null
    /// when nothing is required (no rule, no authored layover, or the
    /// trips are not adjacent in the block).
    pub required_secs: Option<u32>,
    pub required_by: Option<RecoverySource>,
    /// The gap less the requirement — negative when the wait is short
    /// (validation's `layover_infeasible`).
    #[ts(type = "number | null")]
    pub slack_secs: Option<i64>,
    /// The gap as a whole percent of the run (layover-to-runtime).
    pub recovery_pct: u32,
}

/// One pattern's waits, folded.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "transit_runs/")]
pub struct PatternRecovery {
    pub pattern: String,
    pub route: String,
    pub waits: u32,
    #[ts(type = "number")]
    pub run_secs: u64,
    #[ts(type = "number")]
    pub gap_secs: u64,
    /// Total gap as a whole percent of total run.
    pub recovery_pct: u32,
    /// Waits below their requirement.
    pub short: u32,
    /// The smallest slack among waits with a requirement.
    #[ts(type = "number | null")]
    pub min_slack_secs: Option<i64>,
}

fn pct(part: u64, whole: u64) -> u32 {
    if whole == 0 {
        0
    } else {
        (part * 100 / whole) as u32
    }
}

/// The day's recovery: every wait between consecutive running trips
/// of each authored block, under `rules` (none: the terminals' floors
/// judge).
pub fn recovery(network: &Network, day: &[MaterializedTrip], rules: Option<&WorkRules>) -> Recovery {
    let floors = recovery_floors(network);
    let mut waits = Vec::new();
    for (block_id, block) in &network.blocks {
        let position: BTreeMap<&str, usize> =
            block.trips.iter().enumerate().map(|(i, t)| (t.trip.as_str(), i)).collect();
        let mut running: Vec<(u32, usize, &MaterializedTrip)> = day
            .iter()
            .filter_map(|t| position.get(t.trip.as_str()).map(|&i| (t.start_secs, i, t)))
            .collect();
        running.sort_by_key(|&(start, i, _)| (start, i));
        for pair in running.windows(2) {
            let ((start, pi, before), (next_start, i, _)) = (pair[0], pair[1]);
            if pi == i {
                continue; // a frequency trip's next instance: no wait between trips
            }
            let arrival = end(before);
            if arrival > next_start {
                continue; // infeasible order — validation's block_infeasible, not a wait
            }
            let gap = next_start - arrival;
            let authored = block.layovers.iter().find(|l| l.after_trip == pi);
            let pattern = network.patterns.get(&before.pattern);
            let last = pattern.and_then(|p| p.stops.last()).map(|s| s.stop.as_str()).unwrap_or("");
            let terminal = authored.and_then(|l| l.at.as_deref()).unwrap_or(last);
            let service_type =
                network.pattern_trips.get(&before.pattern_trips).map(|pt| pt.service_type.as_str()).unwrap_or("");
            let model_run = network.trip(&before.trip).and_then(|(_, _, t)| t.run_secs()).unwrap_or(0);
            let required = if pi + 1 == i {
                let at = RecoveryAt { route: &before.route, service_type, run_secs: model_run, terminal };
                required_recovery(&floors, rules.and_then(|r| r.recovery_secs(&at)), authored.is_some(), terminal)
            } else {
                None
            };
            let run = arrival.saturating_sub(start);
            waits.push(RecoveryWait {
                block_id: block_id.clone(),
                after_trip: pi,
                trip: before.trip.clone(),
                route: before.route.clone(),
                pattern: before.pattern.clone(),
                terminal: terminal.to_string(),
                start_secs: start,
                run_secs: run,
                gap_secs: gap,
                required_secs: required.map(|r| r.0),
                required_by: required.map(|r| r.1),
                slack_secs: required.map(|(r, _)| gap as i64 - r as i64),
                recovery_pct: pct(gap as u64, run as u64),
            });
        }
    }
    let mut folded: BTreeMap<&str, PatternRecovery> = BTreeMap::new();
    for w in &waits {
        let p = folded.entry(w.pattern.as_str()).or_insert_with(|| PatternRecovery {
            pattern: w.pattern.clone(),
            route: w.route.clone(),
            waits: 0,
            run_secs: 0,
            gap_secs: 0,
            recovery_pct: 0,
            short: 0,
            min_slack_secs: None,
        });
        p.waits += 1;
        p.run_secs += w.run_secs as u64;
        p.gap_secs += w.gap_secs as u64;
        if w.slack_secs.is_some_and(|s| s < 0) {
            p.short += 1;
        }
        if let Some(s) = w.slack_secs {
            p.min_slack_secs = Some(p.min_slack_secs.map_or(s, |m| m.min(s)));
        }
    }
    let patterns = folded
        .into_values()
        .map(|mut p| {
            p.recovery_pct = pct(p.gap_secs, p.run_secs);
            p
        })
        .collect();
    Recovery { waits, patterns }
}

/// A trip instance's arrival at its last stop — what validation
/// judges a wait from.
pub(crate) fn end(trip: &MaterializedTrip) -> u32 {
    trip.stop_times.last().map(|st| st.arrival_secs).unwrap_or(trip.start_secs)
}
