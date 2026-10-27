//! The timetable's gestures: changes a planner states against a
//! TIMETABLE (one route, one direction, one service type — see
//! `listing::timetable`), applied across every pattern of the
//! direction (bay and short-turn variants together). Each carries the
//! coherence rule of the trips it moves:
//!
//! * a trip that stays is RETIMED, never re-created — its identity is
//!   (pattern trips, start), and every block carrying it moves with
//!   it (the `retime_trip` rule);
//! * a trip that goes leaves its blocks, and the layovers either side
//!   of it go with it (the chain closes up — validation names a gap
//!   that no longer fits);
//! * a new trip runs the pattern of the trip before it in the
//!   timetable (the bay variant it slots between), under that
//!   pattern's busiest pattern trips for the type (minted when there
//!   is none); it is unassigned until a block takes it (the
//!   `unassigned_trips` warning);
//! * pattern trips a gesture empties are removed (the model keeps no
//!   empty trip list);
//! * a trip never lands where its pattern has no timing profile (it
//!   would not materialize) — that is a named refusal.
//!
//! Only exact-times trips are moved; a direction whose trips come from
//! headway bands under the type is refused (edit its bands instead).

use std::collections::{BTreeMap, BTreeSet};

use crate::edit::{prune_layovers, CreatedEntity};
use crate::model::{Block, Departures, Network, PatternStop, PatternTrips, TimingProfile, TripRef};

/// A trip of the timetable: (pattern trips, start).
type Trip = (String, u32);

/// The patterns of `route` in `direction` (`None`: those in none), in
/// id order; refused for an unknown route or direction, or none.
pub(crate) fn direction_patterns(net: &Network, route: &str, direction: Option<&str>) -> Result<Vec<String>, String> {
    let r = net.routes.get(route).ok_or_else(|| format!("unknown route {route}"))?;
    if let Some(d) = direction {
        if !r.directions.contains_key(d) {
            return Err(format!("route {route} has no direction {d}"));
        }
    }
    let ids: Vec<String> = net
        .patterns
        .iter()
        .filter(|(_, p)| p.route == route && p.direction.as_deref() == direction)
        .map(|(id, _)| id.clone())
        .collect();
    if ids.is_empty() {
        return Err(format!("route {route} runs no pattern in {}", direction.unwrap_or("no direction")));
    }
    Ok(ids)
}

/// The timetable's trips, by start (then pattern trips id).
fn trips(net: &Network, patterns: &[String], service_type: &str) -> Result<Vec<Trip>, String> {
    if !net.service_types.contains_key(service_type) {
        return Err(format!("unknown service type {service_type}"));
    }
    let mut out = Vec::new();
    for (id, pt) in &net.pattern_trips {
        if pt.service_type != service_type || !patterns.contains(&pt.pattern) {
            continue;
        }
        match &pt.departures {
            Departures::ExactTimes { starts } => out.extend(starts.iter().map(|s| (id.clone(), *s))),
            Departures::HeadwayBands { .. } => {
                return Err(format!("pattern trips {id} run headway bands — edit its bands instead"))
            }
        }
    }
    out.sort_by(|a, b| a.1.cmp(&b.1).then_with(|| a.0.cmp(&b.0)));
    Ok(out)
}

/// Re-attach a block's layovers after its chain changed: a layover
/// stays only while the two trips it sat between are still adjacent.
pub(crate) fn reindex_layovers(block: &mut Block, before: &[TripRef]) {
    let layovers = std::mem::take(&mut block.layovers);
    for mut l in layovers {
        let (Some(a), Some(b)) = (before.get(l.after_trip), before.get(l.after_trip + 1)) else { continue };
        if let Some(i) = block.trips.iter().position(|t| t == a) {
            if block.trips.get(i + 1) == Some(b) {
                l.after_trip = i;
                block.layovers.push(l);
            }
        }
    }
    block.layovers.sort_by_key(|l| l.after_trip);
    prune_layovers(block);
}

/// Retime, remove, and add trips in one step: `moves` carry their
/// blocks, `removed` leave theirs, `added` (pattern, start) mint into
/// the pattern's busiest pattern trips for the type. Answers the
/// first pattern trips it had to create.
fn rewrite(
    net: &mut Network,
    service_type: &str,
    moves: &[(Trip, u32)],
    removed: &[Trip],
    added: &[(String, u32)],
) -> Result<Option<CreatedEntity>, String> {
    // A trip only runs where its pattern has timing: refuse a new or
    // moved one no profile covers (it would not materialize).
    let pattern_of = |pt: &str| net.pattern_trips.get(pt).map(|t| t.pattern.clone()).unwrap_or_default();
    let landing = moves.iter().map(|((pt, _), to)| (pattern_of(pt), *to)).chain(added.iter().cloned());
    for (pattern, start) in landing.collect::<Vec<_>>() {
        if net.patterns.get(&pattern).and_then(|p| p.profile_for(start)).is_none() {
            return Err(format!(
                "pattern {pattern} has no timing profile covering {:02}:{:02} — extend its timing first",
                start / 3600,
                start % 3600 / 60
            ));
        }
    }
    // Starts: take every moved and removed trip out, put moves back.
    let touched: BTreeSet<String> =
        moves.iter().map(|((pt, _), _)| pt.clone()).chain(removed.iter().map(|(pt, _)| pt.clone())).collect();
    for pt in touched {
        let entry = net.pattern_trips.get_mut(&pt).ok_or_else(|| format!("unknown pattern trips {pt}"))?;
        let Departures::ExactTimes { starts } = &mut entry.departures else {
            return Err(format!("pattern trips {pt} run headway bands — edit its bands instead"));
        };
        starts.retain(|s| {
            !moves.iter().any(|((p, f), _)| *p == pt && f == s) && !removed.iter().any(|(p, f)| *p == pt && f == s)
        });
        for ((p, _), to) in moves {
            if *p == pt {
                starts.push(*to);
            }
        }
    }
    // New trips: the pattern's busiest pattern trips for the type.
    let mut created = None;
    for (pattern, start) in added {
        let busiest = net
            .pattern_trips
            .iter()
            .filter(|(_, t)| &t.pattern == pattern && t.service_type == service_type)
            .filter_map(|(id, t)| match &t.departures {
                Departures::ExactTimes { starts } => Some((starts.len(), id.clone())),
                Departures::HeadwayBands { .. } => None,
            })
            .max_by(|a, b| a.0.cmp(&b.0).then_with(|| b.1.cmp(&a.1)))
            .map(|(_, id)| id);
        let id = match busiest {
            Some(id) => id,
            None => {
                let base = format!("{service_type}:{pattern}");
                let mut id = base.clone();
                let mut n = 2;
                while net.pattern_trips.contains_key(&id) {
                    id = format!("{base}-{n}");
                    n += 1;
                }
                net.pattern_trips.insert(
                    id.clone(),
                    PatternTrips {
                        pattern: pattern.clone(),
                        service_type: service_type.to_string(),
                        departures: Departures::ExactTimes { starts: Vec::new() },
                    },
                );
                created.get_or_insert(CreatedEntity { kind: "pattern_trips".into(), id: id.clone() });
                id
            }
        };
        if let Some(PatternTrips { departures: Departures::ExactTimes { starts }, .. }) = net.pattern_trips.get_mut(&id) {
            starts.push(*start);
        }
    }
    // Sorted, unique; refuse a collision rather than merge two trips.
    let mut emptied = Vec::new();
    for (id, pt) in net.pattern_trips.iter_mut() {
        if let Departures::ExactTimes { starts } = &mut pt.departures {
            starts.sort_unstable();
            if starts.windows(2).any(|w| w[0] == w[1]) {
                return Err(format!("pattern trips {id} would run two trips at one start"));
            }
            if starts.is_empty() {
                emptied.push(id.clone());
            }
        }
    }
    // Blocks: moves carry them, removals leave them.
    for block in net.blocks.values_mut() {
        let before = block.trips.clone();
        block.trips.retain(|t| !removed.iter().any(|(p, s)| *p == t.pattern_trips && *s == t.start_secs));
        for t in block.trips.iter_mut() {
            if let Some((_, to)) = moves.iter().find(|((p, f), _)| *p == t.pattern_trips && *f == t.start_secs) {
                t.start_secs = *to;
            }
        }
        if block.trips != before {
            let renamed: Vec<TripRef> = before
                .iter()
                .map(|t| match moves.iter().find(|((p, f), _)| *p == t.pattern_trips && *f == t.start_secs) {
                    Some((_, to)) => TripRef { pattern_trips: t.pattern_trips.clone(), start_secs: *to },
                    None => t.clone(),
                })
                .collect();
            reindex_layovers(block, &renamed);
        }
    }
    for id in emptied {
        if !net.blocks.values().any(|b| b.trips.iter().any(|t| t.pattern_trips == id)) {
            net.pattern_trips.remove(&id);
        }
    }
    Ok(created)
}

/// Match `small` into `large` in order, minimizing the total distance
/// (each of `small` takes one of `large`): `large` indices, one per
/// `small` item.
fn match_in_order(small: &[u32], large: &[u32]) -> Vec<usize> {
    let (n, m) = (small.len(), large.len());
    // cost[i][j]: the best for small[..i] within large[..j].
    let mut cost = vec![vec![u64::MAX; m + 1]; n + 1];
    for j in 0..=m {
        cost[0][j] = 0;
    }
    for i in 1..=n {
        for j in i..=m {
            let skip = cost[i][j - 1];
            let take = cost[i - 1][j - 1].saturating_add(small[i - 1].abs_diff(large[j - 1]) as u64);
            cost[i][j] = skip.min(take);
        }
    }
    let mut out = vec![0; n];
    let (mut i, mut j) = (n, m);
    while i > 0 {
        if j > i && cost[i][j] == cost[i][j - 1] {
            j -= 1;
        } else {
            out[i - 1] = j - 1;
            i -= 1;
            j -= 1;
        }
    }
    out
}

/// The pattern a new trip at `start` runs: the trip before it in the
/// timetable, else the one after, else the direction's busiest.
fn neighbour_pattern(net: &Network, kept: &[(Trip, u32)], start: u32, fallback: &str) -> String {
    let pattern_of = |pt: &str| net.pattern_trips.get(pt).map(|t| t.pattern.clone());
    let before = kept.iter().filter(|(_, s)| *s <= start).max_by_key(|(_, s)| *s);
    let after = kept.iter().filter(|(_, s)| *s > start).min_by_key(|(_, s)| *s);
    before
        .or(after)
        .and_then(|((pt, _), _)| pattern_of(pt))
        .unwrap_or_else(|| fallback.to_string())
}

fn busiest_pattern(net: &Network, timetable: &[Trip]) -> String {
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for (pt, _) in timetable {
        if let Some(t) = net.pattern_trips.get(pt) {
            *counts.entry(t.pattern.clone()).or_default() += 1;
        }
    }
    counts.into_iter().max_by(|a, b| a.1.cmp(&b.1).then_with(|| b.0.cmp(&a.0))).map(|(p, _)| p).unwrap_or_default()
}

/// Frequency by period: trips every `headway_secs` from `from_secs`
/// (the first departure) until before `to_secs`. Existing trips in
/// the window move to the nearest slots in order (carrying their
/// blocks); slots left over are new trips (`pattern` when given, else
/// the neighbour's); trips left over — a lower frequency — go.
#[allow(clippy::too_many_arguments)]
pub(crate) fn set_headway(
    net: &mut Network,
    route: &str,
    direction: Option<&str>,
    service_type: &str,
    from_secs: u32,
    to_secs: u32,
    headway_secs: u32,
    pattern: Option<&str>,
) -> Result<Option<CreatedEntity>, String> {
    if headway_secs == 0 || from_secs >= to_secs {
        return Err("a headway needs a positive interval and a window that ends after it starts".into());
    }
    let patterns = direction_patterns(net, route, direction)?;
    if let Some(p) = pattern {
        if !patterns.iter().any(|x| x == p) {
            return Err(format!("pattern {p} is not in this direction of route {route}"));
        }
    }
    let all = trips(net, &patterns, service_type)?;
    let fallback = pattern.map(str::to_string).unwrap_or_else(|| busiest_pattern(net, &all));
    let window: Vec<Trip> = all.iter().filter(|(_, s)| (from_secs..to_secs).contains(s)).cloned().collect();
    let slots: Vec<u32> = (from_secs..to_secs).step_by(headway_secs as usize).collect();
    let starts: Vec<u32> = window.iter().map(|(_, s)| *s).collect();

    let (mut moves, mut removed, mut open) = (Vec::new(), Vec::new(), Vec::new());
    if window.len() <= slots.len() {
        let taken = match_in_order(&starts, &slots);
        for (trip, &j) in window.iter().zip(&taken) {
            moves.push((trip.clone(), slots[j]));
        }
        open = (0..slots.len()).filter(|j| !taken.contains(j)).map(|j| slots[j]).collect();
    } else {
        let kept = match_in_order(&slots, &starts);
        for (i, trip) in window.iter().enumerate() {
            match kept.iter().position(|&k| k == i) {
                Some(slot) => moves.push((trip.clone(), slots[slot])),
                None => removed.push(trip.clone()),
            }
        }
    }
    // Every trip that stays, at its new time: the neighbours.
    let mut staying: Vec<(Trip, u32)> =
        all.iter().filter(|(_, s)| !(from_secs..to_secs).contains(s)).map(|t| (t.clone(), t.1)).collect();
    staying.extend(moves.iter().cloned());
    let added: Vec<(String, u32)> = open
        .iter()
        .map(|&s| (pattern.map(str::to_string).unwrap_or_else(|| neighbour_pattern(net, &staying, s, &fallback)), s))
        .collect();
    let moves: Vec<(Trip, u32)> = moves.into_iter().filter(|((_, f), t)| f != t).collect();
    rewrite(net, service_type, &moves, &removed, &added)
}

/// Span: trips before `first_secs` / after `last_secs` go; a bound
/// past the current first / last trip becomes the new first / last
/// departure, reached in the even steps nearest `headway_secs` (else
/// the lower median of the edge's last three gaps), each new trip
/// running the edge trip's pattern.
pub(crate) fn set_span(
    net: &mut Network,
    route: &str,
    direction: Option<&str>,
    service_type: &str,
    first_secs: Option<u32>,
    last_secs: Option<u32>,
    headway_secs: Option<u32>,
) -> Result<Option<CreatedEntity>, String> {
    let patterns = direction_patterns(net, route, direction)?;
    let all = trips(net, &patterns, service_type)?;
    let (Some(first), Some(last)) = (all.first().cloned(), all.last().cloned()) else {
        return Err(format!("route {route} runs no trip in this direction under {service_type} — add one first"));
    };
    if let (Some(f), Some(l)) = (first_secs, last_secs) {
        if f > l {
            return Err("the span's first departure is after its last".into());
        }
    }
    let edge_headway = |gaps: Vec<u32>| -> Result<u32, String> {
        if let Some(h) = headway_secs {
            return if h == 0 { Err("a headway must be positive".into()) } else { Ok(h) };
        }
        let mut gaps: Vec<u32> = gaps.into_iter().filter(|g| *g > 0).collect();
        gaps.sort_unstable();
        gaps.get(gaps.len().saturating_sub(1) / 2).copied().ok_or_else(|| "one trip has no headway to extend at — give headway_secs".into())
    };
    let starts: Vec<u32> = all.iter().map(|(_, s)| *s).collect();
    let pattern_of = |pt: &str| net.pattern_trips[pt].pattern.clone();
    let mut removed: Vec<Trip> = Vec::new();
    let mut added: Vec<(String, u32)> = Vec::new();
    if let Some(l) = last_secs {
        if l < last.1 {
            removed.extend(all.iter().filter(|(_, s)| *s > l).cloned());
        } else {
            let tail = &starts[starts.len().saturating_sub(4)..];
            let h = edge_headway(tail.windows(2).map(|w| w[1] - w[0]).collect())?;
            added.extend(steps(last.1, l, h).into_iter().map(|t| (pattern_of(&last.0), t)));
        }
    }
    if let Some(f) = first_secs {
        if f > first.1 {
            removed.extend(all.iter().filter(|(_, s)| *s < f).cloned());
        } else {
            let head = &starts[..starts.len().min(4)];
            let h = edge_headway(head.windows(2).map(|w| w[1] - w[0]).collect())?;
            added.extend(steps(first.1, f, h).into_iter().map(|t| (pattern_of(&first.0), t)));
        }
    }
    if removed.len() == all.len() {
        return Err("the span leaves this direction no trip — remove its pattern trips instead".into());
    }
    rewrite(net, service_type, &[], &removed, &added)
}

/// The departures from `edge` (exclusive) to `bound` (inclusive), in
/// the whole number of even steps nearest `headway` — none when the
/// bound is within half a headway of the edge. Either direction.
fn steps(edge: u32, bound: u32, headway: u32) -> Vec<u32> {
    let span = edge.abs_diff(bound);
    let n = (span + headway / 2) / headway;
    (1..=n)
        .map(|k| {
            let offset = (span as u64 * k as u64 / n as u64) as u32;
            if bound >= edge { edge + offset } else { edge - offset }
        })
        .collect()
}

/// Running time by period: every pattern of the direction runs its
/// trips departing in [`from_secs`, `to_secs`) `delta_secs` longer
/// (shorter, when negative) end to end, spread over its segments in
/// proportion. Profiles are split at the window's bounds. Timing is
/// the pattern's, so the change holds under every service type that
/// runs these patterns.
pub(crate) fn adjust_running_time(
    net: &mut Network,
    route: &str,
    direction: Option<&str>,
    from_secs: u32,
    to_secs: u32,
    delta_secs: i32,
) -> Result<Option<CreatedEntity>, String> {
    if from_secs >= to_secs {
        return Err("the window must end after it starts".into());
    }
    for pid in direction_patterns(net, route, direction)? {
        let p = net.patterns.get_mut(&pid).expect("listed");
        let mut split: Vec<TimingProfile> = Vec::new();
        for profile in &p.timing {
            let mut cuts = vec![profile.start_secs];
            cuts.extend([from_secs, to_secs].into_iter().filter(|c| *c > profile.start_secs && *c < profile.end_secs));
            cuts.push(profile.end_secs);
            for w in cuts.windows(2) {
                split.push(TimingProfile { start_secs: w[0], end_secs: w[1], at_timepoints: profile.at_timepoints.clone() });
            }
        }
        for profile in split.iter_mut().filter(|t| t.start_secs >= from_secs && t.end_secs <= to_secs) {
            let total = profile.at_timepoints.last().copied().unwrap_or(0);
            let next = total as i64 + delta_secs as i64;
            if total == 0 || next <= 0 {
                return Err(format!("pattern {pid} runs {total} s end to end — {delta_secs:+} s leaves no running time"));
            }
            for v in profile.at_timepoints.iter_mut() {
                *v = ((*v as i64 * next) as f64 / total as f64).round() as u32;
            }
        }
        p.timing = split;
    }
    Ok(None)
}

/// Visit `stop` right after `after` on every pattern of the direction
/// that visits `after` before its last stop (a non-timepoint, halfway
/// along that leg). Patterns already visiting `stop` are left alone.
pub(crate) fn insert_direction_stop(
    net: &mut Network,
    route: &str,
    direction: Option<&str>,
    stop: &str,
    after: &str,
) -> Result<Option<CreatedEntity>, String> {
    if !net.stops.contains_key(stop) {
        return Err(format!("unknown stop {stop}"));
    }
    let mut placed = 0;
    for pid in direction_patterns(net, route, direction)? {
        let p = net.patterns.get_mut(&pid).expect("listed");
        if p.stops.iter().any(|s| s.stop == stop) {
            continue;
        }
        let Some(i) = p.stops.iter().position(|s| s.stop == after) else { continue };
        if i + 1 >= p.stops.len() {
            continue;
        }
        let distance_m = (p.stops[i].distance_m + p.stops[i + 1].distance_m) / 2.0;
        p.stops.insert(i + 1, PatternStop { stop: stop.to_string(), timepoint: false, distance_m });
        placed += 1;
    }
    if placed == 0 {
        return Err(format!("no pattern of route {route} in this direction visits {after} before its last stop (without {stop})"));
    }
    Ok(None)
}

/// Stop visiting `stop` on every pattern of the direction (a
/// timepoint's entry leaves every profile with it). Refused when it
/// ends a pattern — endpoints stay.
pub(crate) fn remove_direction_stop(
    net: &mut Network,
    route: &str,
    direction: Option<&str>,
    stop: &str,
) -> Result<Option<CreatedEntity>, String> {
    let patterns = direction_patterns(net, route, direction)?;
    for pid in &patterns {
        let p = &net.patterns[pid];
        if p.stops.first().is_some_and(|s| s.stop == stop) || p.stops.last().is_some_and(|s| s.stop == stop) {
            return Err(format!("stop {stop} ends pattern {pid} — endpoints stay"));
        }
    }
    let mut removed = 0;
    for pid in &patterns {
        let p = net.patterns.get_mut(pid).expect("listed");
        while let Some(i) = p.stops.iter().position(|s| s.stop == stop) {
            let position = p.stops[..i].iter().filter(|s| s.timepoint).count();
            if p.stops.remove(i).timepoint {
                for profile in &mut p.timing {
                    if position < profile.at_timepoints.len() {
                        profile.at_timepoints.remove(position);
                    }
                }
            }
            removed += 1;
        }
    }
    if removed == 0 {
        return Err(format!("no pattern of route {route} in this direction visits {stop}"));
    }
    Ok(None)
}
