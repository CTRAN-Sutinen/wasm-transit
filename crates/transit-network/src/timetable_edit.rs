//! Every gesture here keeps the trips that it moves coherent. A trip that
//! stays keeps its id, running times, and blocks. A trip that goes leaves its
//! blocks with the layovers beside it, and validation reports a gap that no
//! longer fits. A new trip runs the pattern of the trip before it, which is
//! the bay variant that it slots between, and joins the busiest pattern trips
//! of that pattern. Pattern trips that a gesture empties go, because the model
//! keeps no empty trip list.

use std::collections::{BTreeMap, BTreeSet};

use crate::edit::{prune_layovers, CreatedEntity};
use crate::materialize::produce_times;
use crate::model::{Block, Departs, Network, PatternStop, PatternTrips, StopOffset, Trip, TripRef};

/// (pattern trips, trip id, start) of a one-start trip in the timetable.
type Row = (String, String, u32);

/// `None` selects the patterns with no direction.
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

fn rows(net: &Network, patterns: &[String], service_type: &str) -> Result<Vec<Row>, String> {
    if !net.service_types.contains_key(service_type) {
        return Err(format!("unknown service type {service_type}"));
    }
    let mut out = Vec::new();
    for (id, pt) in &net.pattern_trips {
        if pt.service_type != service_type || !patterns.contains(&pt.pattern) {
            continue;
        }
        for t in &pt.trips {
            if let Departs::At { start_secs } = t.departs {
                out.push((id.clone(), t.id.clone(), start_secs));
            }
        }
    }
    out.sort_by(|a, b| a.2.cmp(&b.2).then_with(|| a.0.cmp(&b.0)).then_with(|| a.1.cmp(&b.1)));
    Ok(out)
}

/// A layover stays only while the two trips that it sat between are still
/// adjacent.
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

pub(crate) fn fresh_trip_id(net: &Network, taken: &BTreeSet<String>, base: &str) -> String {
    let exists: BTreeSet<&str> = net.trips().map(|(_, _, t)| t.id.as_str()).collect();
    let free = |id: &str| !exists.contains(id) && !taken.contains(id);
    if free(base) {
        return base.to_string();
    }
    (2..).map(|n| format!("{base}-{n}")).find(|id| free(id)).expect("ids are unbounded")
}

pub(crate) fn remove_trips(net: &mut Network, ids: &BTreeSet<String>) {
    if ids.is_empty() {
        return;
    }
    for pt in net.pattern_trips.values_mut() {
        pt.trips.retain(|t| !ids.contains(&t.id));
    }
    net.pattern_trips.retain(|_, pt| !pt.trips.is_empty());
    for block in net.blocks.values_mut() {
        if block.trips.iter().any(|t| ids.contains(&t.trip)) {
            let before = block.trips.clone();
            block.trips.retain(|t| !ids.contains(&t.trip));
            reindex_layovers(block, &before);
        }
    }
    net.blocks.retain(|_, b| !b.trips.is_empty());
}

pub(crate) fn pattern_trips_mut<'a>(net: &'a mut Network, pattern: &'a str) -> impl Iterator<Item = &'a mut Trip> + 'a {
    net.pattern_trips.values_mut().filter(move |pt| pt.pattern == pattern).flat_map(|pt| pt.trips.iter_mut())
}

/// `moves` pairs a trip id with its new start, and `added` pairs a pattern
/// with a start. Answers the first pattern trips that it had to create.
fn rewrite(
    net: &mut Network,
    service_type: &str,
    moves: &[(String, u32)],
    removed: &[String],
    added: &[(String, u32)],
) -> Result<Option<CreatedEntity>, String> {
    let mut timed = Vec::new();
    for (pattern, start) in added {
        let times = net.patterns.get(pattern).and_then(|p| produce_times(p, *start)).ok_or_else(|| {
            format!(
                "pattern {pattern} has no timing profile to time a trip at {:02}:{:02} — extend its timing first",
                start / 3600,
                start % 3600 / 60
            )
        })?;
        timed.push((pattern.clone(), *start, times));
    }
    let taken: BTreeSet<String> = net.trips().map(|(_, _, t)| t.id.clone()).collect();
    for (id, to) in moves {
        let trip = net
            .pattern_trips
            .values_mut()
            .flat_map(|pt| pt.trips.iter_mut())
            .find(|t| &t.id == id)
            .ok_or_else(|| format!("unknown trip {id}"))?;
        trip.departs = Departs::At { start_secs: *to };
    }
    remove_trips(net, &removed.iter().cloned().collect());
    let mut created = None;
    let mut minted = taken;
    for (pattern, start, times) in timed {
        let busiest = net
            .pattern_trips
            .iter()
            .filter(|(_, t)| t.pattern == pattern && t.service_type == service_type)
            .map(|(id, t)| (t.trips.len(), id.clone()))
            .max_by(|a, b| a.0.cmp(&b.0).then_with(|| b.1.cmp(&a.1)))
            .map(|(_, id)| id);
        let key = match busiest {
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
                    PatternTrips { pattern: pattern.clone(), service_type: service_type.to_string(), trips: Vec::new(), extra: Default::default() },
                );
                created.get_or_insert(CreatedEntity { kind: "pattern_trips".into(), id: id.clone() });
                id
            }
        };
        let id = fresh_trip_id(net, &minted, &format!("{key}@{start}"));
        minted.insert(id.clone());
        net.pattern_trips
            .get_mut(&key)
            .expect("found or created")
            .trips
            .push(Trip { id, departs: Departs::At { start_secs: start }, times, ..Default::default() });
    }
    Ok(created)
}

/// Match `small` into `large` in order, with the least total distance (each
/// item of `small` takes one item of `large`). Answers the `large` index of
/// each `small` item.
fn match_in_order(small: &[u32], large: &[u32]) -> Vec<usize> {
    let (n, m) = (small.len(), large.len());
    // cost[i][j] is the best total for small[..i] within large[..j].
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

fn neighbour_pattern(net: &Network, kept: &[(Row, u32)], start: u32, fallback: &str) -> String {
    let pattern_of = |pt: &str| net.pattern_trips.get(pt).map(|t| t.pattern.clone());
    let before = kept.iter().filter(|(_, s)| *s <= start).max_by_key(|(_, s)| *s);
    let after = kept.iter().filter(|(_, s)| *s > start).min_by_key(|(_, s)| *s);
    before
        .or(after)
        .and_then(|((pt, _, _), _)| pattern_of(pt))
        .unwrap_or_else(|| fallback.to_string())
}

fn busiest_pattern(net: &Network, timetable: &[Row], patterns: &[String]) -> String {
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for (pt, _, _) in timetable {
        if let Some(t) = net.pattern_trips.get(pt) {
            *counts.entry(t.pattern.clone()).or_default() += 1;
        }
    }
    counts
        .into_iter()
        .max_by(|a, b| a.1.cmp(&b.1).then_with(|| b.0.cmp(&a.0)))
        .map(|(p, _)| p)
        .or_else(|| patterns.first().cloned())
        .unwrap_or_default()
}

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
    let all = rows(net, &patterns, service_type)?;
    let fallback = pattern.map(str::to_string).unwrap_or_else(|| busiest_pattern(net, &all, &patterns));
    let window: Vec<Row> = all.iter().filter(|r| (from_secs..to_secs).contains(&r.2)).cloned().collect();
    let slots: Vec<u32> = (from_secs..to_secs).step_by(headway_secs as usize).collect();
    let starts: Vec<u32> = window.iter().map(|r| r.2).collect();

    let (mut moves, mut removed, mut open) = (Vec::new(), Vec::new(), Vec::new());
    if window.len() <= slots.len() {
        let taken = match_in_order(&starts, &slots);
        for (row, &j) in window.iter().zip(&taken) {
            moves.push((row.clone(), slots[j]));
        }
        open = (0..slots.len()).filter(|j| !taken.contains(j)).map(|j| slots[j]).collect();
    } else {
        let kept = match_in_order(&slots, &starts);
        for (i, row) in window.iter().enumerate() {
            match kept.iter().position(|&k| k == i) {
                Some(slot) => moves.push((row.clone(), slots[slot])),
                None => removed.push(row.1.clone()),
            }
        }
    }
    let mut staying: Vec<(Row, u32)> =
        all.iter().filter(|r| !(from_secs..to_secs).contains(&r.2)).map(|r| (r.clone(), r.2)).collect();
    staying.extend(moves.iter().cloned());
    let added: Vec<(String, u32)> = open
        .iter()
        .map(|&s| (pattern.map(str::to_string).unwrap_or_else(|| neighbour_pattern(net, &staying, s, &fallback)), s))
        .collect();
    let moves: Vec<(String, u32)> =
        moves.into_iter().filter(|((_, _, f), t)| f != t).map(|((_, id, _), t)| (id, t)).collect();
    rewrite(net, service_type, &moves, &removed, &added)
}

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
    let all = rows(net, &patterns, service_type)?;
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
    let starts: Vec<u32> = all.iter().map(|r| r.2).collect();
    let pattern_of = |pt: &str| net.pattern_trips[pt].pattern.clone();
    let mut removed: Vec<String> = Vec::new();
    let mut added: Vec<(String, u32)> = Vec::new();
    if let Some(l) = last_secs {
        if l < last.2 {
            removed.extend(all.iter().filter(|r| r.2 > l).map(|r| r.1.clone()));
        } else {
            let tail = &starts[starts.len().saturating_sub(4)..];
            let h = edge_headway(tail.windows(2).map(|w| w[1] - w[0]).collect())?;
            added.extend(steps(last.2, l, h).into_iter().map(|t| (pattern_of(&last.0), t)));
        }
    }
    if let Some(f) = first_secs {
        if f > first.2 {
            removed.extend(all.iter().filter(|r| r.2 < f).map(|r| r.1.clone()));
        } else {
            let head = &starts[..starts.len().min(4)];
            let h = edge_headway(head.windows(2).map(|w| w[1] - w[0]).collect())?;
            added.extend(steps(first.2, f, h).into_iter().map(|t| (pattern_of(&first.0), t)));
        }
    }
    if removed.len() == all.len() {
        return Err("the span leaves this direction no trip — remove its pattern trips instead".into());
    }
    rewrite(net, service_type, &[], &removed, &added)
}

/// The departures after `edge`, up to and including `bound`, in the whole
/// number of even steps nearest `headway`. Empty when the bound is within
/// half a headway of the edge.
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

pub(crate) fn adjust_running_time(
    net: &mut Network,
    route: &str,
    direction: Option<&str>,
    service_type: &str,
    from_secs: u32,
    to_secs: u32,
    delta_secs: i32,
) -> Result<Option<CreatedEntity>, String> {
    if from_secs >= to_secs {
        return Err("the window must end after it starts".into());
    }
    if !net.service_types.contains_key(service_type) {
        return Err(format!("unknown service type {service_type}"));
    }
    let patterns = direction_patterns(net, route, direction)?;
    for pt in net.pattern_trips.values_mut() {
        if pt.service_type != service_type || !patterns.contains(&pt.pattern) {
            continue;
        }
        for trip in &mut pt.trips {
            let Departs::At { start_secs } = trip.departs else { continue };
            if !(from_secs..to_secs).contains(&start_secs) {
                continue;
            }
            let base = trip.times.first().and_then(|o| o.departure_secs).unwrap_or(0);
            let end = trip.run_secs().unwrap_or(0);
            let total = end.saturating_sub(base);
            let next = total as i64 + delta_secs as i64;
            if total == 0 || next <= 0 {
                return Err(format!(
                    "trip {} runs {total} s end to end — {delta_secs:+} s leaves no running time",
                    trip.id
                ));
            }
            let scale = |v: u32| base + ((v.saturating_sub(base) as i64 * next) as f64 / total as f64).round() as u32;
            for (i, o) in trip.times.iter_mut().enumerate() {
                if i == 0 {
                    continue; // the start and the first dwell stand
                }
                o.arrival_secs = o.arrival_secs.map(scale);
                o.departure_secs = o.departure_secs.map(scale);
            }
        }
    }
    Ok(None)
}

/// The caller has already added the visit to the stops of the pattern.
pub(crate) fn insert_trip_stop(net: &mut Network, pattern: &str, index: usize) {
    for trip in pattern_trips_mut(net, pattern) {
        if index <= trip.times.len() {
            trip.times.insert(index, StopOffset::default());
        }
    }
}

pub(crate) fn remove_trip_stop(net: &mut Network, pattern: &str, index: usize) {
    for trip in pattern_trips_mut(net, pattern) {
        if index < trip.times.len() {
            trip.times.remove(index);
        }
    }
}

/// A pattern that already visits `stop` stays as it is.
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
    let mut placed = Vec::new();
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
        p.stops.insert(i + 1, PatternStop { stop: stop.to_string(), timepoint: false, distance_m, ..Default::default() });
        placed.push((pid, i + 1));
    }
    if placed.is_empty() {
        return Err(format!("no pattern of route {route} in this direction visits {after} before its last stop (without {stop})"));
    }
    for (pid, i) in placed {
        insert_trip_stop(net, &pid, i);
    }
    Ok(None)
}

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
        while let Some(i) = net.patterns[pid].stops.iter().position(|s| s.stop == stop) {
            let p = net.patterns.get_mut(pid).expect("listed");
            let position = p.stops[..i].iter().filter(|s| s.timepoint).count();
            if p.stops.remove(i).timepoint {
                for profile in &mut p.timing {
                    if position < profile.at_timepoints.len() {
                        profile.at_timepoints.remove(position);
                    }
                }
            }
            remove_trip_stop(net, pid, i);
            removed += 1;
        }
    }
    if removed == 0 {
        return Err(format!("no pattern of route {route} in this direction visits {stop}"));
    }
    Ok(None)
}
