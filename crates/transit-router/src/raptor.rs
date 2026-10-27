//! RAPTOR: earliest arrival by rounds, then itinerary reconstruction.
//!
//! How to read this file:
//!
//! * `tau[k][s]` — the earliest arrival at stop `s` using **at most k**
//!   transit legs, as seconds since service-day midnight. `tau[0]` is
//!   the initial state (origin = departure time, everything else
//!   unreachable). Each round `k` scans patterns reachable from stops
//!   improved in round `k-1`, then relaxes footpaths. Because
//!   `tau[k]` starts as a copy of `tau[k-1]`, improvements are
//!   cumulative and "at most k legs" is exact.
//! * `parent[k][s]` — when round `k` improved stop `s`, how: which trip
//!   from which boarding position, or which footpath. Reconstruction
//!   walks these back from the destination.
//! * The answer is `tau[rounds][destination]`; the itinerary is
//!   rebuilt by following parents backward in time.

use ts_rs::TS;
use serde::Serialize;

use crate::network::RouterNetwork;
use crate::{Result, RouterError};

const INF: u32 = u32::MAX;

/// Seconds since service-day midnight as "HH:MM:SS" — hours
/// unbounded (past-86400 values are after-midnight trips, the
/// GTFS reading everywhere on the bus).
fn fmt_time(secs: u32) -> String {
    let h = secs / 3600;
    let m = (secs % 3600) / 60;
    let s = secs % 60;
    format!("{h:02}:{m:02}:{s:02}")
}

/// One leg of a planned itinerary. Internally tagged for JSON
/// serialization: `{"type": "transit", ...}` / `{"type": "walk", ...}`.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Leg {
    /// One continuous walk. Footpaths reach only nearby stops, so a
    /// longer walk is a chain of them; the chain is ONE leg (a rider
    /// walks once), its time the sum, and the stops it passes are
    /// `via_stop_ids`, in order (empty for a single footpath).
    Walk {
        from_stop_id: String,
        from_stop_name: String,
        to_stop_id: String,
        to_stop_name: String,
        walk_secs: u32,
        via_stop_ids: Vec<String>,
    },
    Transit {
        /// The derived trip's identity: its schedule and start. The
        /// model names no trips; (schedule, start_secs) is the key
        /// every day view and the codec's trip-id map agree on.
        schedule: String,
        start_secs: u32,
        route_id: String,
        route_name: String,
        headsign: Option<String>,
        board_stop_id: String,
        board_stop_name: String,
        board_secs: u32,
        board_hhmmss: String,
        alight_stop_id: String,
        alight_stop_name: String,
        alight_secs: u32,
        alight_hhmmss: String,
    },
}

/// A full plan: origin, destination, and legs in chronological order.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct Itinerary {
    pub from_stop_id: String,
    pub from_stop_name: String,
    pub to_stop_id: String,
    pub to_stop_name: String,
    pub depart_secs: u32,
    pub arrival_secs: u32,
    pub arrival_hhmmss: String,
    pub legs: Vec<Leg>,
}

/// How `tau[k][s]` was improved — the reconstruction pointers.
/// `Unset` is the initial value ("no label yet").
#[derive(Debug, Clone, Copy)]
enum Parent {
    Unset,
    Transit {
        pattern: usize,
        trip: usize,
        board_pos: usize,
        alight_pos: usize,
    },
    Walk {
        from: u32,
        walk_secs: u32,
    },
}

/// Earliest-arrival query. Times are seconds since midnight of the
/// network's service date. Returns `Ok(None)` when the destination is
/// not reachable within `max_legs` transit legs (plus any number of
/// walks).
pub fn earliest_arrival(
    net: &RouterNetwork,
    from_stop_id: &str,
    to_stop_id: &str,
    depart_secs: u32,
    max_legs: usize,
) -> Result<Option<Itinerary>> {
    let from = net
        .stop_index(from_stop_id)
        .ok_or_else(|| RouterError::UnknownStop(from_stop_id.into()))?
        as usize;
    let to = net
        .stop_index(to_stop_id)
        .ok_or_else(|| RouterError::UnknownStop(to_stop_id.into()))?
        as usize;

    if from == to {
        return Ok(Some(Itinerary {
            from_stop_id: net.stops[from].stop_id.clone(),
            from_stop_name: net.stops[from].stop_name.clone(),
            to_stop_id: net.stops[to].stop_id.clone(),
            to_stop_name: net.stops[to].stop_name.clone(),
            depart_secs,
            arrival_secs: depart_secs,
            arrival_hhmmss: fmt_time(depart_secs),
            legs: Vec::new(),
        }));
    }

    let n = net.stops.len();
    let rounds = max_legs.max(1);
    let mut tau: Vec<Vec<u32>> = vec![vec![INF; n]; rounds + 1];
    let mut parent: Vec<Vec<Parent>> = vec![vec![Parent::Unset; n]; rounds + 1];
    tau[0][from] = depart_secs;

    let mut marked = vec![false; n];
    let mut marked_list: Vec<usize> = vec![from];
    marked[from] = true;

    for k in 1..=rounds {
        if marked_list.is_empty() {
            break;
        }

        // Copy: tau[k] means "at most k legs".
        tau[k] = tau[k - 1].clone();

        let mut improved: Vec<usize> = Vec::new();
        let mut improved_set = vec![false; n];
        let mark = |stop: usize, tau_k: &mut [u32], parent_k: &mut [Parent],
                     improved: &mut Vec<usize>, improved_set: &mut [bool],
                     label: Parent, arrival: u32| {
            if arrival < tau_k[stop] {
                tau_k[stop] = arrival;
                parent_k[stop] = label;
                if !improved_set[stop] {
                    improved_set[stop] = true;
                    improved.push(stop);
                }
            }
        };

        // ---- transit pass. For every pattern serving a stop marked in
        // the previous round: at every marked position, board the
        // earliest feasible trip and ride it to the end of the pattern.
        // (Riding from *each* marked position is what makes patterns
        // with several marked stops correct — a trip boarded at the
        // first covers more stops, but a later marked stop may allow an
        // earlier trip that only helps further downstream.)
        let mut pattern_seen = vec![false; net.patterns.len()];
        for &s in &marked_list {
            for &(pi, _) in &net.patterns_at_stop[s] {
                if pattern_seen[pi as usize] {
                    continue;
                }
                pattern_seen[pi as usize] = true;
                let pattern = &net.patterns[pi as usize];
                let len = pattern.stop_positions.len();

                for p in 0..len - 1 {
                    let board_stop = pattern.stop_positions[p] as usize;
                    if !marked[board_stop] {
                        continue;
                    }
                    let t = tau[k - 1][board_stop];
                    if t == INF {
                        continue;
                    }
                    // Earliest trip departing this position at/after the
                    // label. Linear scan, not binary search: trips are
                    // sorted by their *first* departure, but overtaking
                    // trips (an express passing a local) make later
                    // positions non-monotonic, which would break a
                    // search assuming sorted departures.
                    let mut best: Option<(usize, u32)> = None;
                    for (ti, tr) in pattern.trips.iter().enumerate() {
                        let dep = tr.departures[p];
                        if dep >= t && best.is_none_or(|(_, d)| dep < d) {
                            best = Some((ti, dep));
                        }
                    }
                    let Some((ti, _)) = best else { continue };
                    let trip = &pattern.trips[ti];
                    for q in p + 1..len {
                        let alight_stop = pattern.stop_positions[q] as usize;
                        let label = Parent::Transit {
                            pattern: pi as usize,
                            trip: ti,
                            board_pos: p,
                            alight_pos: q,
                        };
                        mark(alight_stop, &mut tau[k], &mut parent[k], &mut improved,
                            &mut improved_set, label, trip.arrivals[q]);
                    }
                }
            }
        }

        // ---- footpath pass, from every stop improved this round (and,
        // at k == 1, from the origin: walking to a nearby stop is how
        // you board without having arrived by transit).
        if k == 1 {
            improved.push(from);
            improved_set[from] = true;
        }
        let mut i = 0;
        while i < improved.len() {
            let s = improved[i];
            i += 1;
            let base = tau[k][s];
            if base == INF {
                continue;
            }
            for edge in &net.stops[s].footpaths {
                let candidate = base + edge.walk_secs;
                let t = edge.to as usize;
                if candidate < tau[k][t] {
                    // Labels can improve multiple times within a round
                    // (walk chains), so re-queue on every improvement —
                    // the loop converges because labels strictly
                    // decrease. `improved_set` still dedupes the next
                    // round's marked set.
                    tau[k][t] = candidate;
                    parent[k][t] =
                        Parent::Walk { from: s as u32, walk_secs: edge.walk_secs };
                    improved.push(t);
                    improved_set[t] = true;
                }
            }
        }

        // Stops improved this round drive the next one.
        for m in marked.iter_mut() {
            *m = false;
        }
        for &s in &improved {
            marked[s] = true;
        }
        marked_list = improved;
    }

    // The loop may have converged (or leg-capped) before the last
    // round ran, and unvisited rounds were never given their cumulative
    // copy — so the answer is the minimum across ALL rounds, not
    // tau[rounds].
    let arrival = (0..=rounds).map(|k| tau[k][to]).min().unwrap_or(INF);
    if arrival == INF {
        return Ok(None);
    }

    // ---- reconstruction: walk parents back from the destination.
    let mut legs_rev: Vec<Leg> = Vec::new();
    let mut stop = to;
    let mut k = rounds;
    loop {
        // The label for `stop` at round `k` was set in the latest round
        // r <= k that improved it (tau is cumulative, so tau[r][stop]
        // still equals tau[k][stop]).
        let mut label = None;
        for r in (0..=k).rev() {
            match parent[r][stop] {
                Parent::Unset => {}
                other => {
                    label = Some((r, other));
                    break;
                }
            }
        }
        match label {
            None => {
                // No parent: this must be the origin with its initial
                // label.
                debug_assert_eq!(stop, from);
                debug_assert_eq!(tau[k][stop], depart_secs);
                break;
            }
            Some((r, Parent::Transit { pattern, trip, board_pos, alight_pos })) => {
                let pat = &net.patterns[pattern];
                let pt = &pat.trips[trip];
                let board_stop = pat.stop_positions[board_pos] as usize;
                legs_rev.push(Leg::Transit {
                    schedule: pt.schedule.clone(),
                    start_secs: pt.start_secs,
                    route_id: pt.route_id.clone(),
                    route_name: pt.route_name.clone(),
                    headsign: pt.headsign.clone(),
                    board_stop_id: net.stops[board_stop].stop_id.clone(),
                    board_stop_name: net.stops[board_stop].stop_name.clone(),
                    board_secs: pt.departures[board_pos],
                    board_hhmmss: fmt_time(pt.departures[board_pos]),
                    alight_stop_id: net.stops[stop].stop_id.clone(),
                    alight_stop_name: net.stops[stop].stop_name.clone(),
                    alight_secs: pt.arrivals[alight_pos],
                    alight_hhmmss: fmt_time(pt.arrivals[alight_pos]),
                });
                stop = board_stop;
                // The boarding used tau[k-1] at the board stop.
                k = r - 1;
            }
            Some((r, Parent::Walk { from, walk_secs })) => {
                let from = from as usize;
                legs_rev.push(Leg::Walk {
                    from_stop_id: net.stops[from].stop_id.clone(),
                    from_stop_name: net.stops[from].stop_name.clone(),
                    to_stop_id: net.stops[stop].stop_id.clone(),
                    to_stop_name: net.stops[stop].stop_name.clone(),
                    walk_secs,
                    via_stop_ids: Vec::new(),
                });
                stop = from;
                // The walk used tau[r] at its source stop — same round.
                k = r;
            }
            Some((_, Parent::Unset)) => unreachable!(),
        }
    }

    legs_rev.reverse();
    Ok(Some(Itinerary {
        from_stop_id: net.stops[from].stop_id.clone(),
        from_stop_name: net.stops[from].stop_name.clone(),
        to_stop_id: net.stops[to].stop_id.clone(),
        to_stop_name: net.stops[to].stop_name.clone(),
        depart_secs,
        arrival_secs: arrival,
        arrival_hhmmss: fmt_time(arrival),
        legs: merge_walks(legs_rev),
    }))
}

/// Consecutive walk legs are one walk: from the first leg's start to
/// the last leg's end, the times summed, the stops in between kept as
/// `via_stop_ids`. Transit legs pass through untouched.
fn merge_walks(legs: Vec<Leg>) -> Vec<Leg> {
    let mut out: Vec<Leg> = Vec::with_capacity(legs.len());
    for leg in legs {
        if let (
            Some(Leg::Walk { to_stop_id, to_stop_name, walk_secs, via_stop_ids, .. }),
            Leg::Walk { to_stop_id: next_id, to_stop_name: next_name, walk_secs: next_secs, .. },
        ) = (out.last_mut(), &leg)
        {
            via_stop_ids.push(std::mem::replace(to_stop_id, next_id.clone()));
            *to_stop_name = next_name.clone();
            *walk_secs += next_secs;
            continue;
        }
        out.push(leg);
    }
    out
}
