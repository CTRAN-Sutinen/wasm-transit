use serde::Serialize;
#[cfg(feature = "bindings")]
use ts_rs::TS;

use transit_date::Date;
use std::collections::{BTreeMap, BTreeSet};
use transit_network::Network;
use crate::{
    RouteStats, timed_transfers, family_stats, TransferWindow, missed_connections,
    longest_waits, CoverageWindow
};

#[cfg(feature = "bindings")]


/// Provisional thresholds (D-136).
#[derive(Debug, Clone, Default, PartialEq, Serialize, serde::Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS))]
#[cfg_attr(feature = "bindings", ts(export))]
pub struct FindingParams {
    #[serde(default)]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub hole_factor: Option<f64>,
    #[serde(default)]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub wait_secs: Option<u32>,
    #[serde(default)]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub miss_cost_secs: Option<u32>,
    #[serde(default)]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub window: Option<CoverageWindow>,
    #[serde(default)]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub limit: Option<u32>,
    #[serde(default)]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub watch: Option<Vec<String>>,
}

const HOLE_FACTOR: f64 = 1.5;
const LONG_WAIT_SECS: u32 = 3600;
const MISS_COST_SECS: u32 = 900;

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "bindings", derive(TS))]
#[cfg_attr(feature = "bindings", ts(export))]
pub struct FrequencyHole {
    /// The key is "hole|<route>|<hour>", so the same hole in another version
    /// has the same key.
    pub key: String,
    pub route_id: String,
    pub name: String,
    pub hour: u32,
    pub longest_gap_secs: u32,
    pub around_gap_secs: u32,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "bindings", derive(TS))]
#[cfg_attr(feature = "bindings", ts(export))]
pub struct LongWait {
    /// The key is "wait|<stop>".
    pub key: String,
    pub stop_id: String,
    pub name: String,
    pub longest_wait_secs: u32,
    pub wait_from_secs: u32,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "bindings", derive(TS))]
#[cfg_attr(feature = "bindings", ts(export))]
pub struct CostlyMiss {
    /// The key is "miss|<hub>|<from route>|<to route>".
    pub key: String,
    pub hub_id: String,
    pub hub_name: String,
    pub from_route: String,
    pub to_route: String,
    pub misses: u32,
    pub worst_wait_secs: Option<u32>,
    pub stranded: u32,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "bindings", derive(TS))]
#[cfg_attr(feature = "bindings", ts(export))]
pub struct HoleRow {
    pub now: FrequencyHole,
    pub base: Option<FrequencyHole>,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "bindings", derive(TS))]
#[cfg_attr(feature = "bindings", ts(export))]
pub struct WaitRow {
    pub now: LongWait,
    pub base: Option<LongWait>,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "bindings", derive(TS))]
#[cfg_attr(feature = "bindings", ts(export))]
pub struct MissRow {
    pub now: CostlyMiss,
    pub base: Option<CostlyMiss>,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "bindings", derive(TS))]
#[cfg_attr(feature = "bindings", ts(export))]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AnyFinding {
    Hole(FrequencyHole),
    Wait(LongWait),
    Miss(CostlyMiss),
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "bindings", derive(TS))]
#[cfg_attr(feature = "bindings", ts(export))]
pub struct Watched {
    pub key: String,
    pub now: Option<AnyFinding>,
    pub base: Option<AnyFinding>,
}

#[derive(Debug, Clone, Default, Serialize)]
#[cfg_attr(feature = "bindings", derive(TS))]
#[cfg_attr(feature = "bindings", ts(export))]
pub struct ResolvedFindings {
    pub holes: Vec<FrequencyHole>,
    pub waits: Vec<LongWait>,
    pub misses: Vec<CostlyMiss>,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "bindings", derive(TS))]
#[cfg_attr(feature = "bindings", ts(export))]
pub struct NetworkFindings {
    pub group: String,
    /// Every date that the combination holds answers alike, so this is
    /// the first.
    pub date: String,
    pub holes: Vec<HoleRow>,
    pub waits: Vec<WaitRow>,
    pub misses: Vec<MissRow>,
    pub resolved: ResolvedFindings,
    pub watched: Vec<Watched>,
}

struct Found {
    group: String,
    date: String,
    holes: Vec<FrequencyHole>,
    waits: Vec<LongWait>,
    misses: Vec<CostlyMiss>,
}

impl Found {
    fn find(&self, key: &str) -> Option<AnyFinding> {
        self.holes
            .iter()
            .find(|f| f.key == key)
            .map(|f| AnyFinding::Hole(f.clone()))
            .or_else(|| self.waits.iter().find(|f| f.key == key).map(|f| AnyFinding::Wait(f.clone())))
            .or_else(|| self.misses.iter().find(|f| f.key == key).map(|f| AnyFinding::Miss(f.clone())))
    }
}

fn findings_of(net: &Network, members: &[String], p: &FindingParams) -> Option<Found> {
    let combination = transit_network::service_combination(net, members)?;
    let date_text = combination.dates.first()?.clone();
    let date = Date::parse(&date_text).ok()?;
    let factor = p.hole_factor.unwrap_or(HOLE_FACTOR);
    let in_hour = |h: u32| p.window.map_or(true, |w| h * 3600 >= w.start_secs && h * 3600 < w.end_secs);
    let in_secs = |t: u32| p.window.map_or(true, |w| t >= w.start_secs && t <= w.end_secs);

    let mut holes = Vec::new();
    if let Some(grid) = route_frequency(net, &combination.members) {
        for row in grid.routes {
            let gaps: BTreeMap<u32, u32> =
                row.hours.iter().filter_map(|h| h.longest_gap_secs.map(|g| (h.hour, g))).collect();
            for (&h, &g) in &gaps {
                if h == 0 || !in_hour(h) {
                    continue;
                }
                let (Some(&a), Some(&b)) = (gaps.get(&(h - 1)), gaps.get(&(h + 1))) else { continue };
                let around = a.max(b);
                if g as f64 >= factor * around as f64 {
                    holes.push(FrequencyHole {
                        key: format!("hole|{}|{}", row.route_id, h),
                        route_id: row.route_id.clone(),
                        name: row.name.clone(),
                        hour: h,
                        longest_gap_secs: g,
                        around_gap_secs: around,
                    });
                }
            }
        }
    }
    holes.sort_by(|x, y| y.longest_gap_secs.cmp(&x.longest_gap_secs).then_with(|| x.key.cmp(&y.key)));

    let threshold = p.wait_secs.unwrap_or(LONG_WAIT_SECS);
    let waits: Vec<LongWait> = longest_waits(net, date, p.window, None)
        .into_iter()
        .filter(|w| w.longest_wait_secs >= threshold)
        .map(|w| LongWait {
            key: format!("wait|{}", w.stop_id),
            stop_id: w.stop_id,
            name: w.name,
            longest_wait_secs: w.longest_wait_secs,
            wait_from_secs: w.wait_from_secs,
        })
        .collect();

    let cost = p.miss_cost_secs.unwrap_or(MISS_COST_SECS);
    let mut folded: BTreeMap<(String, String, String), (u32, Option<u32>, u32)> = BTreeMap::new();
    for (hub, stop) in &net.stops {
        let station = net.stops.values().any(|c| c.parent.as_deref() == Some(hub.as_str()));
        if !station && !(stop.transfer && stop.parent.is_none()) {
            continue;
        }
        for m in missed_connections(net, date, hub, None, None) {
            if !in_secs(m.arrival_secs) {
                continue;
            }
            let wait = m.next_departure_secs.map(|t| t - m.arrival_secs);
            if wait.is_some_and(|w| w < cost) {
                continue;
            }
            let e = folded.entry((hub.clone(), m.from_route, m.to_route)).or_insert((0, None, 0));
            e.0 += 1;
            match wait {
                Some(w) => e.1 = Some(e.1.map_or(w, |x| x.max(w))),
                None => e.2 += 1,
            }
        }
    }
    let mut misses: Vec<CostlyMiss> = folded
        .into_iter()
        .map(|((hub, from_route, to_route), (misses, worst_wait_secs, stranded))| CostlyMiss {
            key: format!("miss|{hub}|{from_route}|{to_route}"),
            hub_name: net.stops.get(&hub).map(|s| s.name.clone()).unwrap_or_default(),
            hub_id: hub,
            from_route,
            to_route,
            misses,
            worst_wait_secs,
            stranded,
        })
        .collect();
    misses.sort_by(|x, y| y.misses.cmp(&x.misses).then_with(|| x.key.cmp(&y.key)));

    Some(Found { group: combination.id, date: date_text, holes, waits, misses })
}

pub fn network_findings(
    net: &Network,
    base: Option<&Network>,
    members: &[String],
    params: &FindingParams,
) -> Option<NetworkFindings> {
    let now = findings_of(net, members, params)?;
    let then = base.and_then(|b| findings_of(b, members, params));
    let watched: Vec<Watched> = params
        .watch
        .iter()
        .flatten()
        .map(|key| Watched {
            key: key.clone(),
            now: now.find(key),
            base: then.as_ref().and_then(|t| t.find(key)),
        })
        .collect();
    let cut = |n: usize| params.limit.map_or(n, |l| n.min(l as usize));
    let mut resolved = ResolvedFindings::default();
    let (mut base_holes, mut base_waits, mut base_misses): (BTreeMap<_, _>, BTreeMap<_, _>, BTreeMap<_, _>) =
        Default::default();
    if let Some(t) = then {
        let hole_keys: BTreeSet<&String> = now.holes.iter().map(|f| &f.key).collect();
        let wait_keys: BTreeSet<&String> = now.waits.iter().map(|f| &f.key).collect();
        let miss_keys: BTreeSet<&String> = now.misses.iter().map(|f| &f.key).collect();
        for f in t.holes {
            if hole_keys.contains(&f.key) { base_holes.insert(f.key.clone(), f); } else { resolved.holes.push(f); }
        }
        for f in t.waits {
            if wait_keys.contains(&f.key) { base_waits.insert(f.key.clone(), f); } else { resolved.waits.push(f); }
        }
        for f in t.misses {
            if miss_keys.contains(&f.key) { base_misses.insert(f.key.clone(), f); } else { resolved.misses.push(f); }
        }
    }
    resolved.holes.truncate(cut(resolved.holes.len()));
    resolved.waits.truncate(cut(resolved.waits.len()));
    resolved.misses.truncate(cut(resolved.misses.len()));
    let n = (cut(now.holes.len()), cut(now.waits.len()), cut(now.misses.len()));
    Some(NetworkFindings {
        group: now.group,
        date: now.date,
        holes: now.holes.into_iter().take(n.0).map(|f| HoleRow { base: base_holes.remove(&f.key), now: f }).collect(),
        waits: now.waits.into_iter().take(n.1).map(|f| WaitRow { base: base_waits.remove(&f.key), now: f }).collect(),
        misses: now.misses.into_iter().take(n.2).map(|f| MissRow { base: base_misses.remove(&f.key), now: f }).collect(),
        resolved,
        watched,
    })
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "bindings", derive(TS))]
#[cfg_attr(feature = "bindings", ts(export))]
pub struct RouteConnections {
    pub from_route: String,
    pub to_route: String,
    pub connections: u32,
    pub shortest_wait_secs: u32,
    pub longest_wait_secs: u32,
    pub median_wait_secs: Option<u32>,
}

pub fn hub_connections(
    net: &Network,
    date: Date,
    stop_id: &str,
    window: Option<TransferWindow>,
) -> Vec<RouteConnections> {
    let mut pairs: BTreeMap<(String, String), Vec<u32>> = BTreeMap::new();
    for t in timed_transfers(net, date, stop_id, window) {
        pairs.entry((t.from_route, t.to_route)).or_default().push(t.wait_secs);
    }
    pairs
        .into_iter()
        .map(|((from_route, to_route), mut waits)| {
            waits.sort_unstable();
            RouteConnections {
                from_route,
                to_route,
                connections: waits.len() as u32,
                shortest_wait_secs: waits[0],
                longest_wait_secs: waits[waits.len() - 1],
                median_wait_secs: (waits.len() >= 4).then(|| waits[waits.len() / 2]),
            }
        })
        .collect()
}


/// A target headway that varies through the day reads as the range it runs,
/// never rounded to a grid.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "bindings", derive(TS))]
#[cfg_attr(feature = "bindings", ts(export))]
pub struct RouteHourGaps {
    pub hour: u32,
    pub departures: u32,
    pub shortest_gap_secs: Option<u32>,
    pub longest_gap_secs: Option<u32>,
    pub median_gap_secs: Option<u32>,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "bindings", derive(TS))]
#[cfg_attr(feature = "bindings", ts(export))]
pub struct RouteFrequencyRow {
    pub route_id: String,
    pub name: String,
    pub headway_stop: Option<String>,
    pub hours: Vec<RouteHourGaps>,
    pub stats: RouteStats,
}

#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "bindings", derive(TS))]
#[cfg_attr(feature = "bindings", ts(export))]
pub struct RouteFrequency {
    pub group: String,
    pub members: Vec<String>,
    pub routes: Vec<RouteFrequencyRow>,
}

pub fn route_frequency(net: &Network, members: &[String]) -> Option<RouteFrequency> {
    let combination = transit_network::service_combination(net, members)?;
    let group: BTreeSet<String> = combination.members.iter().cloned().collect();
    let mut stats: BTreeMap<String, RouteStats> =
        family_stats(net, &group).routes.into_iter().map(|r| (r.route_id.clone(), r)).collect();
    let mut departures: BTreeMap<String, BTreeMap<String, Vec<u32>>> = BTreeMap::new();
    for (id, entry) in &net.pattern_trips {
        if !group.contains(&entry.service_type) {
            continue;
        }
        let Ok(day) = transit_network::materialize_service(net, id) else {
            continue;
        };
        for trip in day {
            let Some(route) = net.patterns.get(&trip.pattern).map(|p| p.route.clone()) else {
                continue;
            };
            if trip.stop_times.is_empty() {
                continue;
            }
            for st in &trip.stop_times[..trip.stop_times.len() - 1] {
                departures
                    .entry(route.clone())
                    .or_default()
                    .entry(st.stop.clone())
                    .or_default()
                    .push(st.departure_secs);
            }
        }
    }
    let routes = departures
        .into_iter()
        .filter_map(|(route_id, stops)| {
            let (stop, times) = stops.into_iter().fold(None::<(String, Vec<u32>)>, |best, (stop, times)| match best {
                Some((_, ref b)) if b.len() >= times.len() => best,
                _ => Some((stop, times)),
            })?;
            let spine: Vec<u32> = times.iter().copied().collect::<BTreeSet<u32>>().into_iter().collect();
            let mut by_hour: BTreeMap<u32, (u32, Vec<u32>)> = BTreeMap::new();
            for t in &times {
                by_hour.entry(t / 3600).or_default().0 += 1;
            }
            for w in spine.windows(2) {
                by_hour.entry(w[0] / 3600).or_default().1.push(w[1] - w[0]);
            }
            let hours = by_hour
                .into_iter()
                .map(|(hour, (count, mut gaps))| {
                    gaps.sort_unstable();
                    RouteHourGaps {
                        hour,
                        departures: count,
                        shortest_gap_secs: gaps.first().copied(),
                        longest_gap_secs: gaps.last().copied(),
                        median_gap_secs: (gaps.len() >= 4).then(|| gaps[gaps.len() / 2]),
                    }
                })
                .collect();
            Some(RouteFrequencyRow {
                name: net.routes.get(&route_id).map_or(route_id.clone(), |r| r.label().to_string()),
                stats: stats.remove(&route_id)?,
                route_id,
                headway_stop: Some(stop),
                hours,
            })
        })
        .collect();
    Some(RouteFrequency {
        group: combination.id,
        members: combination.members,
        routes,
    })
}
