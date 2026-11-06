//! Suggested blocking (D-46).

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use transit_network::{
    recovery_floors, required_recovery, Departs, MaterializedTrip, Network, NetworkEdit, RecoveryAt, RecoverySource, TripRef,
};
use ts_rs::TS;

use crate::recovery::end;
use crate::WorkRules;

/// The suggested blocking of a day's unassigned trips.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "transit_runs/")]
pub struct Suggestion {
    /// New blocks, each a chain of trips one vehicle can run.
    pub blocks: Vec<SuggestedBlock>,
    /// Unassigned trips the suggestion cannot place, with why.
    pub unblocked: Vec<Unblocked>,
    /// The day's peak vehicles over the authored blocks alone.
    pub authored_peak_vehicles: u32,
    /// The day's peak with the suggestion adopted, and the first time
    /// it is reached.
    pub peak_vehicles: u32,
    pub peak_at_secs: Option<u32>,
    /// What the suggestion adds to the authored peak.
    pub added_vehicles: u32,
    /// The edits that adopt it (author `apply`): a new block per
    /// suggested block, its trips, and each wait as its layover.
    pub edits: Vec<NetworkEdit>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "transit_runs/")]
pub struct SuggestedBlock {
    /// A block id the network does not have.
    pub block_id: String,
    pub trips: Vec<SuggestedTrip>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "transit_runs/")]
pub struct SuggestedTrip {
    pub trip: String,
    pub route: String,
    pub start_secs: u32,
    /// Its arrival at its last stop.
    pub end_secs: u32,
    pub from_stop: String,
    pub to_stop: String,
    /// The wait before the block's next trip; null on the last.
    pub wait: Option<Wait>,
}

/// One suggested wait: the gap, what is required there, and the slack.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "transit_runs/")]
pub struct Wait {
    pub gap_secs: u32,
    /// 0 when nothing is required there (no rule, no floor).
    pub required_secs: u32,
    pub required_by: Option<RecoverySource>,
    pub slack_secs: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "transit_runs/")]
pub struct Unblocked {
    pub trip: String,
    pub reason: UnblockedReason,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "transit_runs/")]
#[serde(rename_all = "snake_case")]
pub enum UnblockedReason {
    /// A frequency trip: a block runs every instance back to back,
    /// which the suggestion does not chain.
    Frequency,
}

struct Candidate<'a> {
    trip: &'a MaterializedTrip,
    start: u32,
    end: u32,
    from: &'a str,
    to: &'a str,
    /// The recovery required after it, and by what.
    required: Option<(u32, RecoverySource)>,
}

/// Chain the day's trips no authored block carries into suggested
/// blocks: by start, each trip follows the vehicle that leaves it the
/// least slack among those waiting where it starts (the same stop, or
/// a bay of the same station — no deadheads), on a route the rules let
/// it interline with, after the recovery required (`rules`', else the
/// terminal's floor); else it takes a new vehicle. Authored blocks
/// are kept as authored.
pub fn suggest_blocks(network: &Network, day: &[MaterializedTrip], rules: Option<&WorkRules>) -> Suggestion {
    let floors = recovery_floors(network);
    let blocked: BTreeSet<&str> =
        network.blocks.values().flat_map(|b| b.trips.iter().map(|t| t.trip.as_str())).collect();
    let mut instances: BTreeMap<&str, Vec<&MaterializedTrip>> = BTreeMap::new();
    for t in day.iter().filter(|t| !blocked.contains(t.trip.as_str())) {
        instances.entry(t.trip.as_str()).or_default().push(t);
    }
    let station = |stop: &str| -> String {
        network.stops.get(stop).and_then(|s| s.parent.clone()).unwrap_or_else(|| stop.to_string())
    };

    let mut unblocked = Vec::new();
    let mut candidates: Vec<Candidate> = Vec::new();
    for (id, runs) in &instances {
        let model = network.trip(id);
        let frequency = runs.len() > 1 || model.is_some_and(|(_, _, t)| matches!(t.departs, Departs::Every { .. }));
        if frequency {
            unblocked.push(Unblocked { trip: id.to_string(), reason: UnblockedReason::Frequency });
            continue;
        }
        let trip = runs[0];
        let pattern = network.patterns.get(&trip.pattern);
        let from = pattern.and_then(|p| p.stops.first()).map(|s| s.stop.as_str()).unwrap_or("");
        let to = pattern.and_then(|p| p.stops.last()).map(|s| s.stop.as_str()).unwrap_or("");
        let service_type = model.map(|(_, pt, _)| pt.service_type.as_str()).unwrap_or("");
        let at = RecoveryAt {
            route: &trip.route,
            service_type,
            run_secs: model.and_then(|(_, _, t)| t.run_secs()).unwrap_or(0),
            terminal: to,
        };
        // A suggested wait is authored as a layover, so the floor judges
        // it where the rules do not.
        let required = required_recovery(&floors, rules.and_then(|r| r.recovery_secs(&at)), true, to);
        candidates.push(Candidate { trip, start: trip.start_secs, end: end(trip), from, to, required });
    }
    candidates.sort_by(|a, b| (a.start, &a.trip.trip).cmp(&(b.start, &b.trip.trip)));

    let interlines = |a: &str, b: &str| a == b || rules.is_some_and(|r| r.interlines(a, b));
    let mut chains: Vec<Vec<usize>> = Vec::new();
    for (k, c) in candidates.iter().enumerate() {
        let best = chains
            .iter()
            .enumerate()
            .filter_map(|(ci, chain)| {
                let last = &candidates[*chain.last().expect("chains are never empty")];
                let ready = last.end.checked_add(last.required.map_or(0, |r| r.0))?;
                (station(last.to) == station(c.from) && interlines(&last.trip.route, &c.trip.route) && ready <= c.start)
                    .then(|| (c.start - ready, ci))
            })
            .min();
        match best {
            Some((_, ci)) => chains[ci].push(k),
            None => chains.push(vec![k]),
        }
    }

    let mut taken: BTreeSet<String> = BTreeSet::new();
    let mut next_id = 1u32;
    let mut fresh = || loop {
        let id = format!("auto-{next_id}");
        next_id += 1;
        if !network.blocks.contains_key(&id) && taken.insert(id.clone()) {
            return id;
        }
    };
    let mut blocks = Vec::new();
    let mut edits = Vec::new();
    for chain in &chains {
        let block_id = fresh();
        edits.push(NetworkEdit::AddBlock { id: Some(block_id.clone()) });
        let mut trips = Vec::new();
        for (pos, &k) in chain.iter().enumerate() {
            let c = &candidates[k];
            edits.push(NetworkEdit::AddBlockTrip { block: block_id.clone(), trip: TripRef { trip: c.trip.trip.clone() } });
            let wait = chain.get(pos + 1).map(|&n| {
                let gap = candidates[n].start - c.end;
                let required = c.required.map_or(0, |r| r.0);
                Wait { gap_secs: gap, required_secs: required, required_by: c.required.map(|r| r.1), slack_secs: gap - required }
            });
            trips.push(SuggestedTrip {
                trip: c.trip.trip.clone(),
                route: c.trip.route.clone(),
                start_secs: c.start,
                end_secs: c.end,
                from_stop: c.from.to_string(),
                to_stop: c.to.to_string(),
                wait,
            });
        }
        for (pos, t) in trips.iter().enumerate() {
            if let Some(w) = &t.wait {
                edits.push(NetworkEdit::SetLayover { block: block_id.clone(), after_trip: pos, secs: Some(w.gap_secs) });
            }
        }
        blocks.push(SuggestedBlock { block_id, trips });
    }

    // The peak: the authored blocks' spans (as the fleet summary
    // reads them) and the suggested chains', swept together.
    let authored = transit_dispatch::fleet_summary(network, day);
    let mut events: Vec<(u32, i64)> = authored
        .spans
        .iter()
        .flat_map(|s| [(s.first_start_secs, 1), (s.last_end_secs, -1)])
        .collect();
    for chain in &chains {
        let first = candidates[chain[0]].trip;
        let last_end = chain.iter().map(|&k| span_end(candidates[k].trip)).max().expect("chains are never empty");
        events.push((first.start_secs, 1));
        events.push((last_end, -1));
    }
    events.sort();
    let (mut current, mut peak, mut peak_at) = (0i64, 0u32, None);
    for (time, delta) in events {
        current += delta;
        if current > peak as i64 {
            peak = current as u32;
            peak_at = Some(time);
        }
    }
    Suggestion {
        blocks,
        unblocked,
        authored_peak_vehicles: authored.peak_vehicles,
        peak_vehicles: peak,
        peak_at_secs: peak_at,
        added_vehicles: peak.saturating_sub(authored.peak_vehicles),
        edits,
    }
}

/// Where a vehicle's span ends after a trip, as the fleet summary
/// reads it (the later of its last arrival and departure).
fn span_end(trip: &MaterializedTrip) -> u32 {
    trip.stop_times.last().map(|st| st.arrival_secs.max(st.departure_secs)).unwrap_or(trip.start_secs)
}
