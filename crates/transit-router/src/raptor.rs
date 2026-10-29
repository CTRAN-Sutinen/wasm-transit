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
//!
//! Arrive-by ([`latest_departure`]) is the same rounds run backward
//! in time from the destination: the LATEST moment at each stop from
//! which the destination is still reached by the deadline, then the
//! forward plan from the origin's latest moment is the itinerary.

use ts_rs::TS;
use serde::{Deserialize, Serialize};

use crate::network::RouterNetwork;
use crate::{Result, RouterError};

const INF: u32 = u32::MAX;

/// Seconds since service-day midnight as "HH:MM:SS" — hours
/// unbounded (past-86400 values are after-midnight trips, the
/// GTFS reading everywhere on the bus).
pub(crate) fn fmt_time(secs: u32) -> String {
    let h = secs / 3600;
    let m = (secs % 3600) / 60;
    let s = secs % 60;
    format!("{h:02}:{m:02}:{s:02}")
}

/// One leg of a planned itinerary. Internally tagged for JSON
/// serialization: `{"type": "transit", ...}` / `{"type": "walk", ...}`.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Leg {
    /// One continuous walk. Footpaths reach only nearby stops, so a
    /// longer walk is a chain of them; the chain is ONE leg (a rider
    /// walks once), its time the sum, and the stops it passes are
    /// `via_stop_ids`, in order (empty for a single footpath). A walk
    /// from or to a [`Place`] names the place's id and name.
    Walk {
        from_stop_id: String,
        from_stop_name: String,
        to_stop_id: String,
        to_stop_name: String,
        walk_secs: u32,
        via_stop_ids: Vec<String>,
    },
    /// One ride. Times are the ones the trip is expected to keep:
    /// predicted where a live document states them, the day's own
    /// elsewhere — the delays say which.
    Transit {
        /// The trip instance ridden: the network's trip and its start
        /// — the key every day view agrees on.
        trip: String,
        start_secs: u32,
        route_id: String,
        /// The route's label: its short name, else its long name.
        route_name: String,
        /// The route's long name, when it has one.
        route_long_name: Option<String>,
        /// What the bus's sign says where the rider boards
        /// (`transit_network::visit_destination` at the boarding
        /// visit).
        headsign: String,
        board_stop_id: String,
        board_stop_name: String,
        board_secs: u32,
        board_hhmmss: String,
        /// The boarding departure is a live prediction: how far it
        /// moved from the day's own time (`board_secs - delay` is that
        /// time). `None` = the day's own time, no prediction.
        board_delay_secs: Option<i32>,
        alight_stop_id: String,
        alight_stop_name: String,
        alight_secs: u32,
        alight_hhmmss: String,
        /// The alighting arrival's prediction, read the same way.
        alight_delay_secs: Option<i32>,
    },
}

/// A full plan: origin, destination (a stop, or a [`Place`] by its id
/// and name), and legs in chronological order.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
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

/// Where a plan starts or ends: a stop of the network (its id — the
/// JSON is the bare string), or a [`Place`] off it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(untagged)]
pub enum End {
    Stop(String),
    Place(Place),
}

impl From<&str> for End {
    fn from(stop_id: &str) -> End {
        End::Stop(stop_id.into())
    }
}

/// Somewhere off the network — a tapped spot, the rider's location —
/// named by its walks to the stops near it (cap-streets `access`
/// answers them along the streets). Its id and name stand where an
/// itinerary names a stop; the id is never a stop's.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Place {
    pub id: String,
    pub name: String,
    pub walks: Vec<PlaceWalk>,
}

/// A place's walk to (and, walks being symmetric, from) one stop.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct PlaceWalk {
    pub stop_id: String,
    pub walk_secs: u32,
}

/// An end resolved against the network: what an itinerary names it,
/// and the stops it walks to with the seconds (a stop: itself, in no
/// time).
struct Side {
    id: String,
    name: String,
    place: bool,
    stops: Vec<(usize, u32)>,
}

fn resolve(net: &RouterNetwork, end: &End) -> Result<Side> {
    let stop = |id: &str| {
        net.stop_index(id).map(|i| i as usize).ok_or_else(|| RouterError::UnknownStop(id.into()))
    };
    match end {
        End::Stop(id) => {
            let i = stop(id)?;
            Ok(Side { id: id.clone(), name: net.stops[i].stop_name.clone(), place: false, stops: vec![(i, 0)] })
        }
        End::Place(p) => {
            if net.stop_index(&p.id).is_some() {
                return Err(RouterError::PlaceIsStop(p.id.clone()));
            }
            let stops = p.walks.iter().map(|w| Ok((stop(&w.stop_id)?, w.walk_secs))).collect::<Result<_>>()?;
            Ok(Side { id: p.id.clone(), name: p.name.clone(), place: true, stops })
        }
    }
}

impl Side {
    /// The walk between this side and its stop `s`, outward (`to_stop`)
    /// or inward.
    fn walk(&self, net: &RouterNetwork, s: usize, walk_secs: u32, to_stop: bool) -> Leg {
        let (stop_id, stop_name) = (net.stops[s].stop_id.clone(), net.stops[s].stop_name.clone());
        let (from_stop_id, from_stop_name, to_stop_id, to_stop_name) = if to_stop {
            (self.id.clone(), self.name.clone(), stop_id, stop_name)
        } else {
            (stop_id, stop_name, self.id.clone(), self.name.clone())
        };
        Leg::Walk { from_stop_id, from_stop_name, to_stop_id, to_stop_name, walk_secs, via_stop_ids: Vec::new() }
    }
}

/// Earliest-arrival query between stops — [`earliest_arrival_between`]
/// with two [`End::Stop`]s. Returns `Ok(None)` when the destination
/// is not reachable within `max_legs` transit legs (plus any number
/// of walks).
pub fn earliest_arrival(
    net: &RouterNetwork,
    from_stop_id: &str,
    to_stop_id: &str,
    depart_secs: u32,
    max_legs: usize,
) -> Result<Option<Itinerary>> {
    let (from, to) = (End::Stop(from_stop_id.into()), End::Stop(to_stop_id.into()));
    earliest_arrival_between(net, &from, &to, None, depart_secs, max_legs)
}

/// Earliest-arrival query. Times are seconds since midnight of the
/// network's service date. From a place, the rider walks to one of
/// its stops; to a place, from one of its stops; `direct_walk_secs`
/// is the walk straight between the two ends, when one joins them —
/// arriving as early on foot, the rider walks. Returns `Ok(None)`
/// when the destination is not reachable within `max_legs` transit
/// legs (plus any number of walks).
pub fn earliest_arrival_between(
    net: &RouterNetwork,
    from: &End,
    to: &End,
    direct_walk_secs: Option<u32>,
    depart_secs: u32,
    max_legs: usize,
) -> Result<Option<Itinerary>> {
    let origin = resolve(net, from)?;
    let target = resolve(net, to)?;
    let itinerary = |arrival: u32, legs: Vec<Leg>| Itinerary {
        from_stop_id: origin.id.clone(),
        from_stop_name: origin.name.clone(),
        to_stop_id: target.id.clone(),
        to_stop_name: target.name.clone(),
        depart_secs,
        arrival_secs: arrival,
        arrival_hhmmss: fmt_time(arrival),
        legs: merge_walks(legs),
    };

    // A stop to itself: no legs.
    if !origin.place && !target.place && origin.stops == target.stops {
        return Ok(Some(itinerary(depart_secs, Vec::new())));
    }

    let rounds = max_legs.max(1);
    let (tau, parent) = forward(net, &origin.stops, depart_secs, rounds);

    // The best stop to leave the network at: its label plus its walk.
    // The loop may have converged (or leg-capped) before the last
    // round ran, and unvisited rounds were never given their
    // cumulative copy — so a stop's label is the minimum across ALL
    // rounds, not tau[rounds].
    let mut best: Option<(u32, usize, u32)> = None;
    for &(e, walk) in &target.stops {
        let label = (0..=rounds).map(|k| tau[k][e]).min().unwrap_or(INF);
        if label != INF && best.is_none_or(|(arrival, _, _)| label + walk < arrival) {
            best = Some((label + walk, e, walk));
        }
    }
    if let Some(walk_secs) = direct_walk_secs {
        let arrival = depart_secs + walk_secs;
        if best.is_none_or(|(a, _, _)| arrival <= a) {
            let leg = Leg::Walk {
                from_stop_id: origin.id.clone(),
                from_stop_name: origin.name.clone(),
                to_stop_id: target.id.clone(),
                to_stop_name: target.name.clone(),
                walk_secs,
                via_stop_ids: Vec::new(),
            };
            return Ok(Some(itinerary(arrival, vec![leg])));
        }
    }
    let Some((arrival, exit, exit_walk)) = best else { return Ok(None) };

    let (mut legs, entry) = reconstruct(net, &tau, &parent, exit, rounds);
    if origin.place {
        legs.insert(0, origin.walk(net, entry, tau[0][entry] - depart_secs, true));
    }
    if target.place {
        legs.push(target.walk(net, exit, exit_walk, false));
    }
    Ok(Some(itinerary(arrival, legs)))
}

/// The rounds forward in time from `sources` (each stop reached at
/// `depart_secs` plus its walk): `tau` and the parents that rebuild
/// each label.
fn forward(
    net: &RouterNetwork,
    sources: &[(usize, u32)],
    depart_secs: u32,
    rounds: usize,
) -> (Vec<Vec<u32>>, Vec<Vec<Parent>>) {
    let n = net.stops.len();
    let mut tau: Vec<Vec<u32>> = vec![vec![INF; n]; rounds + 1];
    let mut parent: Vec<Vec<Parent>> = vec![vec![Parent::Unset; n]; rounds + 1];

    let mut marked = vec![false; n];
    let mut marked_list: Vec<usize> = Vec::new();
    for &(s, walk) in sources {
        tau[0][s] = tau[0][s].min(depart_secs + walk);
        if !marked[s] {
            marked[s] = true;
            marked_list.push(s);
        }
    }
    let sources = marked_list.clone();

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
        // at k == 1, from the sources: walking to a nearby stop is how
        // you board without having arrived by transit).
        if k == 1 {
            for &s in &sources {
                if !improved_set[s] {
                    improved_set[s] = true;
                    improved.push(s);
                }
            }
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
    (tau, parent)
}

/// The legs that reach `exit`, walking parents back from it — and the
/// source stop they start at.
fn reconstruct(
    net: &RouterNetwork,
    tau: &[Vec<u32>],
    parent: &[Vec<Parent>],
    exit: usize,
    rounds: usize,
) -> (Vec<Leg>, usize) {
    let mut legs_rev: Vec<Leg> = Vec::new();
    let mut stop = exit;
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
                // No parent: this must be a source with its initial
                // label.
                debug_assert_ne!(tau[0][stop], INF);
                break;
            }
            Some((r, Parent::Transit { pattern, trip, board_pos, alight_pos })) => {
                let pat = &net.patterns[pattern];
                let pt = &pat.trips[trip];
                let board_stop = pat.stop_positions[board_pos] as usize;
                legs_rev.push(Leg::Transit {
                    trip: pt.trip.clone(),
                    start_secs: pt.start_secs,
                    route_id: pt.route_id.clone(),
                    route_name: pt.route_name.clone(),
                    route_long_name: pt.route_long_name.clone(),
                    headsign: pt.headsign_at(board_pos).to_string(),
                    board_stop_id: net.stops[board_stop].stop_id.clone(),
                    board_stop_name: net.stops[board_stop].stop_name.clone(),
                    board_secs: pt.departures[board_pos],
                    board_hhmmss: fmt_time(pt.departures[board_pos]),
                    board_delay_secs: pt.delay_at(board_pos).1,
                    alight_stop_id: net.stops[stop].stop_id.clone(),
                    alight_stop_name: net.stops[stop].stop_name.clone(),
                    alight_secs: pt.arrivals[alight_pos],
                    alight_hhmmss: fmt_time(pt.arrivals[alight_pos]),
                    alight_delay_secs: pt.delay_at(alight_pos).0,
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
    (legs_rev, stop)
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

/// An arrive-by answer: the deadline it answered (the request's, or
/// the service day's end when none was given) and the itinerary
/// leaving as late as possible — `None` when nothing reaches the
/// destination by the deadline.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ArriveBy {
    pub arrive_by_secs: u32,
    pub itinerary: Option<Itinerary>,
}

/// Arrive-by between stops — [`latest_departure_between`] with two
/// [`End::Stop`]s.
pub fn latest_departure(
    net: &RouterNetwork,
    from_stop_id: &str,
    to_stop_id: &str,
    arrive_by: Option<u32>,
    max_legs: usize,
) -> Result<ArriveBy> {
    let (from, to) = (End::Stop(from_stop_id.into()), End::Stop(to_stop_id.into()));
    latest_departure_between(net, &from, &to, None, arrive_by, max_legs)
}

/// Arrive-by: the LATEST departure from `from` that reaches `to` by
/// `arrive_by` within `max_legs` transit legs (plus any walks; a
/// place's walks and the direct walk as in
/// [`earliest_arrival_between`]). With no deadline, the deadline is
/// the service day's end ([`RouterNetwork::day_end`]) — "the last bus
/// home": nothing arrives anywhere later than the day's last arrival.
///
/// The search is RAPTOR backward in time: `tau[k][s]` is the latest
/// moment at `s` from which the destination is reached by the
/// deadline in at most `k` legs. A trip is boardable at every
/// position before the last one it can alight at in time (its
/// latest feasible alight — exact even when trips of a pattern
/// overtake). The itinerary is then the forward earliest-arrival plan
/// leaving at the origin's latest moment: it arrives by the deadline
/// (the backward search proved a path exists), as early as it can.
pub fn latest_departure_between(
    net: &RouterNetwork,
    from: &End,
    to: &End,
    direct_walk_secs: Option<u32>,
    arrive_by: Option<u32>,
    max_legs: usize,
) -> Result<ArriveBy> {
    let origin = resolve(net, from)?;
    let target = resolve(net, to)?;
    let by = arrive_by.unwrap_or_else(|| net.day_end());

    const NONE: i64 = i64::MIN;
    let n = net.stops.len();
    let rounds = max_legs.max(1);
    let mut tau: Vec<Vec<i64>> = vec![vec![NONE; n]; rounds + 1];
    let mut marked = vec![false; n];
    let mut marked_list: Vec<usize> = Vec::new();
    for &(e, walk) in &target.stops {
        let latest = by as i64 - walk as i64;
        if latest >= 0 {
            tau[0][e] = tau[0][e].max(latest);
            if !marked[e] {
                marked[e] = true;
                marked_list.push(e);
            }
        }
    }
    let sinks = marked_list.clone();

    for k in 1..=rounds {
        if marked_list.is_empty() {
            break;
        }
        tau[k] = tau[k - 1].clone();
        let mut improved: Vec<usize> = Vec::new();
        let mut improved_set = vec![false; n];

        // ---- transit pass, backward: for every pattern serving a
        // stop marked last round, every trip boards at each position
        // before its latest position where it arrives in time.
        let mut pattern_seen = vec![false; net.patterns.len()];
        for &s in &marked_list {
            for &(pi, _) in &net.patterns_at_stop[s] {
                if pattern_seen[pi as usize] {
                    continue;
                }
                pattern_seen[pi as usize] = true;
                let pattern = &net.patterns[pi as usize];
                for trip in &pattern.trips {
                    let alight = (1..pattern.stop_positions.len()).rev().find(|&q| {
                        let stop = pattern.stop_positions[q] as usize;
                        marked[stop] && tau[k - 1][stop] != NONE && trip.arrivals[q] as i64 <= tau[k - 1][stop]
                    });
                    let Some(q) = alight else { continue };
                    for p in 0..q {
                        let board = pattern.stop_positions[p] as usize;
                        let dep = trip.departures[p] as i64;
                        if dep > tau[k][board] {
                            tau[k][board] = dep;
                            if !improved_set[board] {
                                improved_set[board] = true;
                                improved.push(board);
                            }
                        }
                    }
                }
            }
        }

        // ---- footpath pass, backward (footpaths are symmetric: the
        // same distance both ways), from every stop improved this
        // round — and at k == 1 from the stops the destination is
        // reached from.
        if k == 1 {
            for &s in &sinks {
                if !improved_set[s] {
                    improved_set[s] = true;
                    improved.push(s);
                }
            }
        }
        let mut i = 0;
        while i < improved.len() {
            let t = improved[i];
            i += 1;
            let base = tau[k][t];
            if base == NONE {
                continue;
            }
            for edge in &net.stops[t].footpaths {
                let candidate = base - edge.walk_secs as i64;
                let s = edge.to as usize;
                if candidate >= 0 && candidate > tau[k][s] {
                    tau[k][s] = candidate;
                    improved.push(s);
                    improved_set[s] = true;
                }
            }
        }

        for m in marked.iter_mut() {
            *m = false;
        }
        for &s in &improved {
            marked[s] = true;
        }
        marked_list = improved;
    }

    // The latest moment at the origin: each of its stops' latest,
    // less the walk there — or the direct walk's.
    let mut latest = direct_walk_secs.map_or(NONE, |walk| by as i64 - walk as i64);
    for &(a, walk) in &origin.stops {
        let at = (0..=rounds).map(|k| tau[k][a]).max().unwrap_or(NONE);
        if at != NONE {
            latest = latest.max(at - walk as i64);
        }
    }
    if latest < 0 {
        return Ok(ArriveBy { arrive_by_secs: by, itinerary: None });
    }
    let itinerary = earliest_arrival_between(net, from, to, direct_walk_secs, latest as u32, max_legs)?
        .filter(|it| it.arrival_secs <= by);
    Ok(ArriveBy { arrive_by_secs: by, itinerary })
}
