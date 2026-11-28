//! RAPTOR earliest arrival by rounds, then itinerary reconstruction.
//!
//! * `tau[k][s]` is the earliest arrival at stop `s` using at most `k`
//!   transit legs, in seconds since service-day midnight. `tau[0]` is the
//!   origin at its departure time, and every other stop is unreachable. Round
//!   `k` scans the patterns of the stops that round `k-1` improved, then
//!   relaxes footpaths. `tau[k]` starts as a copy of `tau[k-1]`, so "at most k
//!   legs" is exact.
//! * `parent[k][s]` records how round `k` improved stop `s`.

use serde::{Deserialize, Serialize};
#[cfg(feature = "bindings")]
use ts_rs::TS;

use crate::network::RouterNetwork;
use crate::{Result, RouterError};
use transit_streets::StreetPoint;

const INF: u32 = u32::MAX;

/// Hours are unbounded, and a value past 86400 is a trip after midnight.
pub(crate) fn fmt_time(secs: u32) -> String {
    let h = secs / 3600;
    let m = (secs % 3600) / 60;
    let s = secs % 60;
    format!("{h:02}:{m:02}:{s:02}")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS))]
#[cfg_attr(feature = "bindings", ts(export))]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Leg {
    /// Footpaths reach only nearby stops, so a longer walk is a chain of them,
    /// and the chain is one leg. `via_stop_ids` is empty for a single footpath.
    Walk {
        from_stop_id: String,
        from_stop_name: String,
        to_stop_id: String,
        to_stop_name: String,
        walk_secs: u32,
        via_stop_ids: Vec<String>,
    },
    Transit {
        trip: String,
        start_secs: u32,
        route_id: String,
        route_name: String,
        route_long_name: Option<String>,
        headsign: String,
        board_stop_id: String,
        board_stop_name: String,
        board_secs: u32,
        board_hhmmss: String,
        /// The day's own boarding time is `board_secs - delay`.
        board_delay_secs: Option<i32>,
        alight_stop_id: String,
        alight_stop_name: String,
        alight_secs: u32,
        alight_hhmmss: String,
        alight_delay_secs: Option<i32>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS))]
#[cfg_attr(feature = "bindings", ts(export))]
pub struct Itinerary {
    pub from_stop_id: String,
    pub from_stop_name: String,
    pub to_stop_id: String,
    pub to_stop_name: String,
    /// The moment that the plan was asked to leave at. For arrive-by, it is
    /// the origin's latest moment.
    pub depart_secs: u32,
    pub arrival_secs: u32,
    pub arrival_hhmmss: String,
    pub leave_secs: u32,
    pub duration_secs: u32,
    pub leave: transit_dispatch::Wait,
    pub legs: Vec<Leg>,
}

impl Itinerary {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        from_stop_id: String,
        from_stop_name: String,
        to_stop_id: String,
        to_stop_name: String,
        depart_secs: u32,
        arrival_secs: u32,
        legs: Vec<Leg>,
    ) -> Itinerary {
        let leave_secs = leave_secs(&legs, depart_secs);
        Itinerary {
            from_stop_id,
            from_stop_name,
            to_stop_id,
            to_stop_name,
            depart_secs,
            arrival_secs,
            arrival_hhmmss: fmt_time(arrival_secs),
            leave_secs,
            duration_secs: arrival_secs.saturating_sub(leave_secs),
            leave: transit_dispatch::wait(leave_secs, depart_secs),
            legs,
        }
    }
}

fn leave_secs(legs: &[Leg], depart_secs: u32) -> u32 {
    let mut walked = 0u32;
    for leg in legs {
        match leg {
            Leg::Walk { walk_secs, .. } => walked += walk_secs,
            Leg::Transit { board_secs, .. } => return board_secs.saturating_sub(walked),
        }
    }
    depart_secs
}

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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS))]
#[cfg_attr(feature = "bindings", ts(export))]
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

/// A spot off the network, such as a tapped spot or the rider's location. Its
/// id is never a stop's id.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS))]
#[cfg_attr(feature = "bindings", ts(export))]
pub struct Place {
    pub id: String,
    pub name: String,
    pub walks: Vec<PlaceWalk>,
}

/// A place's walk to one stop and back (walks are symmetric). Planning reads
/// the seconds, and [`crate::walk_path`] draws the path, which is empty when
/// the walk is given by its seconds alone.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS))]
#[cfg_attr(feature = "bindings", ts(export))]
pub struct PlaceWalk {
    pub stop_id: String,
    pub walk_secs: u32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub path: Vec<StreetPoint>,
}

/// A stop walks to itself in no time.
struct Side {
    id: String,
    name: String,
    place: bool,
    stops: Vec<(usize, u32)>,
}

fn resolve(net: &RouterNetwork, end: &End) -> Result<Side> {
    let stop = |id: &str| {
        net.stop_index(id)
            .map(|i| i as usize)
            .ok_or_else(|| RouterError::UnknownStop(id.into()))
    };
    match end {
        End::Stop(id) => {
            let i = stop(id)?;
            Ok(Side {
                id: id.clone(),
                name: net.stops[i].stop_name.clone(),
                place: false,
                stops: vec![(i, 0)],
            })
        }
        End::Place(p) => {
            if net.stop_index(&p.id).is_some() {
                return Err(RouterError::PlaceIsStop(p.id.clone()));
            }
            let stops = p
                .walks
                .iter()
                .map(|w| Ok((stop(&w.stop_id)?, w.walk_secs)))
                .collect::<Result<_>>()?;
            Ok(Side {
                id: p.id.clone(),
                name: p.name.clone(),
                place: true,
                stops,
            })
        }
    }
}

impl Side {
    fn walk(&self, net: &RouterNetwork, s: usize, walk_secs: u32, to_stop: bool) -> Leg {
        let (stop_id, stop_name) = (net.stops[s].stop_id.clone(), net.stops[s].stop_name.clone());
        let (from_stop_id, from_stop_name, to_stop_id, to_stop_name) = if to_stop {
            (self.id.clone(), self.name.clone(), stop_id, stop_name)
        } else {
            (stop_id, stop_name, self.id.clone(), self.name.clone())
        };
        Leg::Walk {
            from_stop_id,
            from_stop_name,
            to_stop_id,
            to_stop_name,
            walk_secs,
            via_stop_ids: Vec::new(),
        }
    }
}

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

/// Times are seconds since midnight of the network's service date. If the rider
/// arrives as early on `direct_walk_secs`, the rider walks. Returns `Ok(None)`
/// when the destination is not reachable within `max_legs` transit legs (plus
/// any number of walks).
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
    let itinerary = |arrival: u32, legs: Vec<Leg>| {
        Itinerary::new(
            origin.id.clone(),
            origin.name.clone(),
            target.id.clone(),
            target.name.clone(),
            depart_secs,
            arrival,
            merge_walks(legs),
        )
    };

    if !origin.place && !target.place && origin.stops == target.stops {
        return Ok(Some(itinerary(depart_secs, Vec::new())));
    }

    let rounds = max_legs.max(1);
    let (tau, parent) = forward(net, &origin.stops, depart_secs, rounds);

    // The best stop to leave the network at has the least label plus walk.
    // The loop may stop before the last round (it converged, or it hit the leg
    // cap), and a skipped round never copies the one before it. So a stop's
    // label is the minimum across all rounds, not tau[rounds].
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
    let Some((arrival, exit, exit_walk)) = best else {
        return Ok(None);
    };

    let (hops, entry) = reconstruct(&tau, &parent, net, exit, rounds);
    let plan = walk_least(
        net,
        &hops,
        &origin.stops,
        depart_secs,
        &target.stops,
        arrival,
    )
    .unwrap_or(Plan {
        entry_walk: tau[0][entry] - depart_secs,
        entry,
        hops,
        exit,
        exit_walk,
    });
    let mut legs: Vec<Leg> = plan.hops.iter().map(|hop| hop.leg(net)).collect();
    if origin.place {
        legs.insert(0, origin.walk(net, plan.entry, plan.entry_walk, true));
    }
    if target.place {
        legs.push(target.walk(net, plan.exit, plan.exit_walk, false));
    }
    Ok(Some(itinerary(arrival, legs)))
}

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
    // The walk to the first boarding is no leg. The footpaths of the sources,
    // chains included, settle into round 0 before any ride, so a one-leg plan
    // may walk to its bus.
    let mut i = 0;
    while i < marked_list.len() {
        let s = marked_list[i];
        i += 1;
        for edge in &net.stops[s].footpaths {
            let (t, candidate) = (edge.to as usize, tau[0][s] + edge.walk_secs);
            if candidate < tau[0][t] {
                tau[0][t] = candidate;
                parent[0][t] = Parent::Walk {
                    from: s as u32,
                    walk_secs: edge.walk_secs,
                };
                marked[t] = true;
                marked_list.push(t);
            }
        }
    }
    marked_list.sort_unstable();
    marked_list.dedup();

    for k in 1..=rounds {
        if marked_list.is_empty() {
            break;
        }

        tau[k] = tau[k - 1].clone();

        let mut improved: Vec<usize> = Vec::new();
        let mut improved_set = vec![false; n];
        let mark = |stop: usize,
                    tau_k: &mut [u32],
                    parent_k: &mut [Parent],
                    improved: &mut Vec<usize>,
                    improved_set: &mut [bool],
                    label: Parent,
                    arrival: u32| {
            if arrival < tau_k[stop] {
                tau_k[stop] = arrival;
                parent_k[stop] = label;
                if !improved_set[stop] {
                    improved_set[stop] = true;
                    improved.push(stop);
                }
            }
        };

        // Every trip, not only the earliest to depart: an overtaking trip (an
        // express passing a local) leaves later and arrives earlier. A trip
        // boards at its first catchable marked position, since boarding it
        // later reaches nothing sooner.
        let mut pattern_seen = vec![false; net.patterns.len()];
        for &s in &marked_list {
            for &(pi, _) in &net.patterns_at_stop[s] {
                if pattern_seen[pi as usize] {
                    continue;
                }
                pattern_seen[pi as usize] = true;
                let pattern = &net.patterns[pi as usize];
                let len = pattern.stop_positions.len();

                for (ti, trip) in pattern.trips.iter().enumerate() {
                    let board = (0..len - 1).find(|&p| {
                        let stop = pattern.stop_positions[p] as usize;
                        marked[stop]
                            && tau[k - 1][stop] != INF
                            && trip.departures[p] >= tau[k - 1][stop]
                    });
                    let Some(p) = board else { continue };
                    for q in p + 1..len {
                        let alight_stop = pattern.stop_positions[q] as usize;
                        let label = Parent::Transit {
                            pattern: pi as usize,
                            trip: ti,
                            board_pos: p,
                            alight_pos: q,
                        };
                        mark(
                            alight_stop,
                            &mut tau[k],
                            &mut parent[k],
                            &mut improved,
                            &mut improved_set,
                            label,
                            trip.arrivals[q],
                        );
                    }
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
                    // A walk chain can improve a label several times in a
                    // round,
                    // so every improvement re-queues the stop. The loop ends
                    // because labels strictly decrease. `improved_set` dedupes
                    // the next round's marked set.
                    tau[k][t] = candidate;
                    parent[k][t] = Parent::Walk {
                        from: s as u32,
                        walk_secs: edge.walk_secs,
                    };
                    improved.push(t);
                    improved_set[t] = true;
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
    (tau, parent)
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Hop {
    Ride {
        pattern: usize,
        trip: usize,
        board: usize,
        alight: usize,
    },
    Walk {
        from: usize,
        to: usize,
        walk_secs: u32,
    },
}

impl Hop {
    fn leg(&self, net: &RouterNetwork) -> Leg {
        match *self {
            Hop::Ride {
                pattern,
                trip,
                board,
                alight,
            } => {
                let pat = &net.patterns[pattern];
                let pt = &pat.trips[trip];
                let (board_stop, alight_stop) = (
                    pat.stop_positions[board] as usize,
                    pat.stop_positions[alight] as usize,
                );
                Leg::Transit {
                    trip: pt.trip.clone(),
                    start_secs: pt.start_secs,
                    route_id: pt.route_id.clone(),
                    route_name: pt.route_name.clone(),
                    route_long_name: pt.route_long_name.clone(),
                    headsign: pt.headsign_at(board).to_string(),
                    board_stop_id: net.stops[board_stop].stop_id.clone(),
                    board_stop_name: net.stops[board_stop].stop_name.clone(),
                    board_secs: pt.departures[board],
                    board_hhmmss: fmt_time(pt.departures[board]),
                    board_delay_secs: pt.delay_at(board).1,
                    alight_stop_id: net.stops[alight_stop].stop_id.clone(),
                    alight_stop_name: net.stops[alight_stop].stop_name.clone(),
                    alight_secs: pt.arrivals[alight],
                    alight_hhmmss: fmt_time(pt.arrivals[alight]),
                    alight_delay_secs: pt.delay_at(alight).0,
                }
            }
            Hop::Walk {
                from,
                to,
                walk_secs,
            } => Leg::Walk {
                from_stop_id: net.stops[from].stop_id.clone(),
                from_stop_name: net.stops[from].stop_name.clone(),
                to_stop_id: net.stops[to].stop_id.clone(),
                to_stop_name: net.stops[to].stop_name.clone(),
                walk_secs,
                via_stop_ids: Vec::new(),
            },
        }
    }
}

fn reconstruct(
    tau: &[Vec<u32>],
    parent: &[Vec<Parent>],
    net: &RouterNetwork,
    exit: usize,
    rounds: usize,
) -> (Vec<Hop>, usize) {
    let mut hops_rev: Vec<Hop> = Vec::new();
    let mut stop = exit;
    let mut k = rounds;
    loop {
        // The latest round r <= k that improved `stop` set its label. `tau` is
        // cumulative, so tau[r][stop] equals tau[k][stop].
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
                debug_assert_ne!(tau[0][stop], INF);
                break;
            }
            Some((
                r,
                Parent::Transit {
                    pattern,
                    trip,
                    board_pos,
                    alight_pos,
                },
            )) => {
                hops_rev.push(Hop::Ride {
                    pattern,
                    trip,
                    board: board_pos,
                    alight: alight_pos,
                });
                stop = net.patterns[pattern].stop_positions[board_pos] as usize;
                k = r - 1;
            }
            Some((r, Parent::Walk { from, walk_secs })) => {
                hops_rev.push(Hop::Walk {
                    from: from as usize,
                    to: stop,
                    walk_secs,
                });
                stop = from as usize;
                k = r;
            }
            Some((_, Parent::Unset)) => unreachable!(),
        }
    }
    hops_rev.reverse();
    (hops_rev, stop)
}

struct Plan {
    entry: usize,
    entry_walk: u32,
    hops: Vec<Hop>,
    exit: usize,
    exit_walk: u32,
}

struct WalkTree {
    secs: Vec<u32>,
    prev: Vec<Option<(usize, u32)>>,
}

impl WalkTree {
    fn new(net: &RouterNetwork, sources: &[(usize, u32)], limit: u32) -> WalkTree {
        let n = net.stops.len();
        let (mut secs, mut prev) = (vec![INF; n], vec![None; n]);
        let mut heap = std::collections::BinaryHeap::new();
        for &(s, c) in sources {
            if c < secs[s] {
                secs[s] = c;
                heap.push(std::cmp::Reverse((c, s)));
            }
        }
        while let Some(std::cmp::Reverse((d, s))) = heap.pop() {
            if d > secs[s] || d >= limit {
                continue;
            }
            for e in &net.stops[s].footpaths {
                let (t, c) = (e.to as usize, d + e.walk_secs);
                if c < secs[t] {
                    secs[t] = c;
                    prev[t] = Some((s, e.walk_secs));
                    heap.push(std::cmp::Reverse((c, t)));
                }
            }
        }
        WalkTree { secs, prev }
    }

    fn path(&self, stop: usize) -> (Vec<Hop>, usize) {
        let mut hops = Vec::new();
        let mut at = stop;
        while let Some((from, walk_secs)) = self.prev[at] {
            hops.push(Hop::Walk {
                from,
                to: at,
                walk_secs,
            });
            at = from;
        }
        hops.reverse();
        (hops, at)
    }
}

/// Shortens each walk of a plan to the least that still connects (D-35),
/// keeping its rides, its arrival, and its leg count. The pass repeats until
/// nothing moves, and it ends because each move shortens one walk. A tie keeps
/// the search's choice. Returns `None` for a plan with no ride.
fn walk_least(
    net: &RouterNetwork,
    hops: &[Hop],
    sources: &[(usize, u32)],
    depart_secs: u32,
    targets: &[(usize, u32)],
    arrival: u32,
) -> Option<Plan> {
    let mut rides: Vec<(usize, usize, usize, usize)> = hops
        .iter()
        .filter_map(|h| match *h {
            Hop::Ride {
                pattern,
                trip,
                board,
                alight,
            } => Some((pattern, trip, board, alight)),
            Hop::Walk { .. } => None,
        })
        .collect();
    if rides.is_empty() {
        return None;
    }
    let stop = |r: (usize, usize, usize, usize), pos: usize| {
        net.patterns[r.0].stop_positions[pos] as usize
    };
    let dep =
        |r: (usize, usize, usize, usize), pos: usize| net.patterns[r.0].trips[r.1].departures[pos];
    let arr =
        |r: (usize, usize, usize, usize), pos: usize| net.patterns[r.0].trips[r.1].arrivals[pos];
    let len = |r: (usize, usize, usize, usize)| net.patterns[r.0].stop_positions.len();
    let onward = |tree: &WalkTree| {
        targets
            .iter()
            .filter(|(t, _)| tree.secs[*t] != INF)
            .map(|&(t, w)| (tree.secs[t] + w, t, w))
            .min_by_key(|&(total, _, _)| total)
    };

    let origin = WalkTree::new(net, sources, INF);
    loop {
        let mut moved = false;

        let r = rides[0];
        let mut best = (origin.secs[stop(r, r.2)], r.2);
        for p in 0..r.3 {
            let w = origin.secs[stop(r, p)];
            if w < best.0 && depart_secs + w <= dep(r, p) {
                best = (w, p);
            }
        }
        if best.1 != r.2 {
            rides[0].2 = best.1;
            moved = true;
        }

        for i in 0..rides.len() - 1 {
            let (a, b) = (rides[i], rides[i + 1]);
            let current = WalkTree::new(net, &[(stop(a, a.3), 0)], INF).secs[stop(b, b.2)];
            let mut best = (current, a.3, b.2);
            for q in a.2 + 1..len(a) {
                let tree = WalkTree::new(net, &[(stop(a, q), 0)], best.0);
                for p in 0..b.3 {
                    let w = tree.secs[stop(b, p)];
                    if w < best.0 && arr(a, q) + w <= dep(b, p) {
                        best = (w, q, p);
                    }
                }
            }
            if (best.1, best.2) != (a.3, b.2) {
                rides[i].3 = best.1;
                rides[i + 1].2 = best.2;
                moved = true;
            }
        }

        let last = rides.len() - 1;
        let r = rides[last];
        let current = onward(&WalkTree::new(net, &[(stop(r, r.3), 0)], INF)).map_or(INF, |o| o.0);
        let mut best = (current, r.3);
        for q in r.2 + 1..len(r) {
            if let Some((w, _, _)) = onward(&WalkTree::new(net, &[(stop(r, q), 0)], best.0)) {
                if w < best.0 && arr(r, q) + w <= arrival {
                    best = (w, q);
                }
            }
        }
        if best.1 != r.3 {
            rides[last].3 = best.1;
            moved = true;
        }

        if !moved {
            break;
        }
    }

    let (mut out, entry) = origin.path(stop(rides[0], rides[0].2));
    for (i, &r) in rides.iter().enumerate() {
        out.push(Hop::Ride {
            pattern: r.0,
            trip: r.1,
            board: r.2,
            alight: r.3,
        });
        if let Some(&next) = rides.get(i + 1) {
            out.extend(
                WalkTree::new(net, &[(stop(r, r.3), 0)], INF)
                    .path(stop(next, next.2))
                    .0,
            );
        }
    }
    let r = rides[rides.len() - 1];
    let tree = WalkTree::new(net, &[(stop(r, r.3), 0)], INF);
    let (_, exit, exit_walk) = onward(&tree)?;
    out.extend(tree.path(exit).0);
    Some(Plan {
        entry,
        entry_walk: origin.secs[entry],
        hops: out,
        exit,
        exit_walk,
    })
}

fn merge_walks(legs: Vec<Leg>) -> Vec<Leg> {
    let mut out: Vec<Leg> = Vec::with_capacity(legs.len());
    for leg in legs {
        if let (
            Some(Leg::Walk {
                to_stop_id,
                to_stop_name,
                walk_secs,
                via_stop_ids,
                ..
            }),
            Leg::Walk {
                to_stop_id: next_id,
                to_stop_name: next_name,
                walk_secs: next_secs,
                ..
            },
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS))]
#[cfg_attr(feature = "bindings", ts(export))]
pub struct ArriveBy {
    pub arrive_by_secs: u32,
    pub itinerary: Option<Itinerary>,
}

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

/// With no deadline, the deadline is the service day's end
/// ([`RouterNetwork::day_end`]), which answers "the last bus home".
///
/// The search is RAPTOR backward in time. `tau[k][s]` is the latest moment at
/// `s` from which the destination is reached by the deadline in at most `k`
/// legs. A trip is boardable at every position before its latest feasible
/// alight, so the search is exact when trips of a pattern overtake. The
/// itinerary is the forward earliest-arrival plan from the origin's latest
/// moment.
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
    let mut i = 0;
    while i < marked_list.len() {
        let t = marked_list[i];
        i += 1;
        for edge in &net.stops[t].footpaths {
            let (s, candidate) = (edge.to as usize, tau[0][t] - edge.walk_secs as i64);
            if candidate >= 0 && candidate > tau[0][s] {
                tau[0][s] = candidate;
                marked[s] = true;
                marked_list.push(s);
            }
        }
    }
    marked_list.sort_unstable();
    marked_list.dedup();

    for k in 1..=rounds {
        if marked_list.is_empty() {
            break;
        }
        tau[k] = tau[k - 1].clone();
        let mut improved: Vec<usize> = Vec::new();
        let mut improved_set = vec![false; n];

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
                        marked[stop]
                            && tau[k - 1][stop] != NONE
                            && trip.arrivals[q] as i64 <= tau[k - 1][stop]
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

    let mut latest = direct_walk_secs.map_or(NONE, |walk| by as i64 - walk as i64);
    for &(a, walk) in &origin.stops {
        let at = (0..=rounds).map(|k| tau[k][a]).max().unwrap_or(NONE);
        if at != NONE {
            latest = latest.max(at - walk as i64);
        }
    }
    if latest < 0 {
        return Ok(ArriveBy {
            arrive_by_secs: by,
            itinerary: None,
        });
    }
    let itinerary =
        earliest_arrival_between(net, from, to, direct_walk_secs, latest as u32, max_legs)?
            .filter(|it| it.arrival_secs <= by);
    Ok(ArriveBy {
        arrive_by_secs: by,
        itinerary,
    })
}
