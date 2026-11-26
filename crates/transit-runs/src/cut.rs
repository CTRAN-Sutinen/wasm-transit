//! The runs cut: a day's blocks into runs under the work rules and the
//! scheduling constraints. It is a heuristic, not an optimum, and it is
//! deterministic.
//!
//! A block that makes one good run stays whole. A run is good when it
//! breaks no rule, is in a class that the objective maximizes, and pays no
//! premium or guarantee. Any other block is cut at reliefs into the longest
//! pieces that break no rule and pay no premium alone, else the longest
//! that break no rule. Then each piece, earliest first, takes
//! the later piece that makes the best run with it, when that run is good
//! or pays less than the two apart. A piece that pairs with none is a run
//! alone. Every trip of every block that runs is in exactly one piece.
//!
//! A waivable limit is the operator's to waive, so the cut breaks one only
//! when no run it can make breaks none. Cost is total pay, because paid
//! intervening time costs as a premium does.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use transit_network::{MaterializedTrip, Network};
#[cfg(feature = "bindings")]
use ts_rs::TS;

use crate::rules::{Day, ShareOf, WorkRules};
use crate::run::{judge, meets, Judged, Piece, PieceTrip, Places};
use crate::SchedulingConstraints;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS))]
#[cfg_attr(feature = "bindings", ts(export, export_to = "transit_runs/"))]
pub struct Cut {
    pub runs: Vec<CutRun>,
    /// Trips that run on the day and that no block carries. The cut does
    /// not place them.
    pub unblocked: Vec<String>,
    pub mix: Vec<MixReport>,
    /// The date is outside the work rules' period, or another reason the
    /// cut may not reflect the rules in force.
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS))]
#[cfg_attr(feature = "bindings", ts(export, export_to = "transit_runs/"))]
pub struct CutRun {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub class: Option<String>,
    pub pieces: Vec<Piece>,
    pub judged: Judged,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS))]
#[cfg_attr(feature = "bindings", ts(export, export_to = "transit_runs/"))]
pub struct MixReport {
    pub classes: Vec<String>,
    pub of: ShareOf,
    /// A whole percent, rounded down.
    pub share_pct: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub min_pct: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub target_pct: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub max_pct: Option<u32>,
    /// Within the minimum and the maximum. A target is reported, never
    /// required.
    pub met: bool,
}

/// The network's stops as places: distances between them, and the next
/// trip of the day home.
pub struct NetworkPlaces<'a> {
    network: &'a Network,
    /// Each stop's departures: the trip's index in the day and the stop's
    /// position in that trip.
    departures: BTreeMap<&'a str, Vec<(u32, usize, usize)>>,
    day: &'a [MaterializedTrip],
}

impl<'a> NetworkPlaces<'a> {
    pub fn new(network: &'a Network, day: &'a [MaterializedTrip]) -> Self {
        let mut departures: BTreeMap<&str, Vec<(u32, usize, usize)>> = BTreeMap::new();
        for (i, t) in day.iter().enumerate() {
            for (k, st) in t.stop_times.iter().enumerate() {
                departures.entry(st.stop.as_str()).or_default().push((st.departure_secs, i, k));
            }
        }
        for list in departures.values_mut() {
            list.sort();
        }
        NetworkPlaces { network, departures, day }
    }

    fn position(&self, stop: &str) -> Option<(f64, f64)> {
        self.network.stops.get(stop).map(|s| (s.lat, s.lon))
    }
}

impl Places for NetworkPlaces<'_> {
    fn meters(&self, a: &str, b: &str) -> Option<f64> {
        let ((la, oa), (lb, ob)) = (self.position(a)?, self.position(b)?);
        Some(transit_geo::distance_m(la, oa, lb, ob))
    }

    fn next_trip_home(&self, from: &str, after_secs: u32, home: &str, within_m: u32) -> Option<u32> {
        let list = self.departures.get(from)?;
        let first = list.partition_point(|d| d.0 < after_secs);
        list[first..]
            .iter()
            .filter_map(|&(_, i, k)| {
                self.day[i].stop_times[k + 1..]
                    .iter()
                    .find(|st| self.meters(&st.stop, home).is_some_and(|m| m <= within_m as f64))
                    .map(|st| st.arrival_secs)
            })
            .min()
    }
}

/// Each block's running trips on the day, in order, as piece trips.
fn day_blocks(network: &Network, day: &[MaterializedTrip]) -> (BTreeMap<String, Vec<PieceTrip>>, Vec<String>) {
    let mut placed = std::collections::BTreeSet::new();
    let mut blocks = BTreeMap::new();
    for (block_id, block) in &network.blocks {
        let position: BTreeMap<&str, usize> = block.trips.iter().enumerate().map(|(i, t)| (t.trip.as_str(), i)).collect();
        let mut running: Vec<(u32, usize, &MaterializedTrip)> =
            day.iter().filter_map(|t| position.get(t.trip.as_str()).map(|&i| (t.start_secs, i, t))).collect();
        running.sort_by_key(|&(start, i, t)| (start, i, t.trip.clone()));
        let trips: Vec<PieceTrip> = running
            .into_iter()
            .map(|(_, _, t)| {
                placed.insert((t.trip.clone(), t.start_secs));
                PieceTrip {
                    trip: t.trip.clone(),
                    start_secs: t.start_secs,
                    end_secs: crate::recovery::end(t),
                    from: t.stop_times.first().map(|s| s.stop.clone()).unwrap_or_default(),
                    to: t.stop_times.last().map(|s| s.stop.clone()).unwrap_or_default(),
                }
            })
            .collect();
        if !trips.is_empty() {
            blocks.insert(block_id.clone(), trips);
        }
    }
    let mut unblocked: Vec<String> =
        day.iter().filter(|t| !placed.contains(&(t.trip.clone(), t.start_secs))).map(|t| format!("{}@{}", t.trip, t.start_secs)).collect();
    unblocked.sort();
    (blocks, unblocked)
}

/// Lower is better: legal before not, then a run that breaks nothing, then
/// a good run, then the least pay, then the fewest broken rules, then the
/// class's place in the document.
type Score = (bool, bool, bool, u32, usize, usize);

struct Chooser<'a> {
    rules: &'a WorkRules,
    day: Day,
    places: &'a dyn Places,
}

impl Chooser<'_> {
    fn best(&self, pieces: &[Piece]) -> (Option<String>, Judged, Score) {
        let maximize = self.rules.objective.as_ref().map(|o| o.maximize.clone()).unwrap_or_default();
        let classes = self.rules.classes.iter().map(|c| Some(c.id.as_str())).chain(std::iter::once(None));
        let mut best: Option<(Option<String>, Judged, Score)> = None;
        for (order, class) in classes.enumerate() {
            if class.is_some_and(|c| !meets(self.rules, c, pieces)) {
                continue;
            }
            let judged = judge(self.rules, class, pieces, self.day, self.places);
            let clean = judged.broken.is_empty();
            let good = clean && class.is_some_and(|c| maximize.iter().any(|m| m == c)) && judged.premium_secs + judged.guarantee_secs == 0;
            let score = (!judged.legal(), !clean, !good, judged.pay_secs, judged.broken.len(), order);
            if best.as_ref().is_none_or(|b| score < b.2) {
                best = Some((class.map(String::from), judged, score));
            }
        }
        best.expect("a run of no class is always judged")
    }
}

fn piece(block: &str, trips: &[PieceTrip], all: &[PieceTrip]) -> Piece {
    Piece {
        block: block.to_string(),
        trips: trips.to_vec(),
        pull_out: trips.first() == all.first(),
        pull_in: trips.last() == all.last(),
    }
}

/// The block's trips cut at reliefs into the longest pieces legal alone. A
/// relief is after a trip that ends at a relief point, or after any trip
/// when the constraints name none.
fn split(block: &str, trips: &[PieceTrip], constraints: Option<&SchedulingConstraints>, chooser: &Chooser) -> Vec<Piece> {
    let relief_points = constraints.map(|c| c.relief_points.as_slice()).unwrap_or(&[]);
    let relief = |i: usize| relief_points.is_empty() || relief_points.iter().any(|p| *p == trips[i].to);
    // Breaks nothing alone, and pays no premium alone: a piece that stops
    // where premiums start leaves pairing room to build a full day.
    let fits = |from: usize, to: usize| {
        let (_, judged, score) = chooser.best(std::slice::from_ref(&piece(block, &trips[from..=to], trips)));
        (!score.1, !score.1 && judged.premium_secs == 0)
    };
    let mut out = Vec::new();
    let mut start = 0;
    while start < trips.len() {
        let ends: Vec<usize> = (start..trips.len()).filter(|&e| e + 1 == trips.len() || relief(e)).collect();
        let (mut clean, mut cheap) = (None, None);
        for &e in &ends {
            let (is_clean, is_cheap) = fits(start, e);
            if is_cheap {
                cheap = Some(e);
            }
            if is_clean {
                clean = Some(e);
            } else if clean.is_some() {
                break;
            }
        }
        let chosen = cheap.or(clean);
        let stop = chosen.unwrap_or(ends[0]);
        out.push(piece(block, &trips[start..=stop], trips));
        start = stop + 1;
    }
    out
}

/// Cuts the day's blocks into runs. `day` is the service date's weekday,
/// and `date` its "YYYY-MM-DD", which the rules' period is read against.
pub fn cut_runs(
    network: &Network,
    trips: &[MaterializedTrip],
    rules: &WorkRules,
    constraints: Option<&SchedulingConstraints>,
    date: &str,
    day: Day,
    places: &dyn Places,
) -> Cut {
    let chooser = Chooser { rules, day, places };
    let (blocks, unblocked) = day_blocks(network, trips);
    let mut warnings = Vec::new();
    if let Some(source) = &rules.source {
        let outside = source.effective_from.as_deref().is_some_and(|f| date < f) || source.effective_to.as_deref().is_some_and(|t| date > t);
        if outside {
            warnings.push(format!(
                "{date} is outside the period of the work rules ({} to {})",
                source.effective_from.as_deref().unwrap_or("any"),
                source.effective_to.as_deref().unwrap_or("any")
            ));
        }
    }

    let mut whole = Vec::new();
    let mut pieces = Vec::new();
    for (block, list) in &blocks {
        let one = piece(block, list, list);
        let (class, judged, score) = chooser.best(std::slice::from_ref(&one));
        if !score.2 {
            whole.push((vec![one], class, judged));
        } else {
            pieces.extend(split(block, list, constraints, &chooser));
        }
    }

    pieces.sort_by(|a, b| (a.start_secs(), &a.block).cmp(&(b.start_secs(), &b.block)));
    let mut used = vec![false; pieces.len()];
    let mut runs = whole;
    for i in 0..pieces.len() {
        if used[i] {
            continue;
        }
        used[i] = true;
        let alone = chooser.best(std::slice::from_ref(&pieces[i]));
        let mut pick: Option<(usize, (Option<String>, Judged, Score))> = None;
        for j in i + 1..pieces.len() {
            if used[j] || pieces[j].start_secs() < pieces[i].end_secs() {
                continue;
            }
            let pair = [pieces[i].clone(), pieces[j].clone()];
            let made = chooser.best(&pair);
            if made.2 .1 {
                continue;
            }
            let other = chooser.best(std::slice::from_ref(&pieces[j]));
            let apart = alone.2 .3 + other.2 .3;
            let better = !made.2 .2 || made.2 .3 < apart;
            if better && pick.as_ref().is_none_or(|p| made.2 < p.1 .2) {
                pick = Some((j, made));
            }
        }
        match pick {
            Some((j, (class, judged, _))) => {
                used[j] = true;
                runs.push((vec![pieces[i].clone(), pieces[j].clone()], class, judged));
            }
            None => runs.push((vec![pieces[i].clone()], alone.0, alone.1)),
        }
    }

    runs.sort_by(|a, b| (a.0[0].start_secs(), &a.0[0].block).cmp(&(b.0[0].start_secs(), &b.0[0].block)));
    let runs: Vec<CutRun> =
        runs.into_iter().enumerate().map(|(n, (pieces, class, judged))| CutRun { id: format!("{}", n + 1), class, pieces, judged }).collect();
    let mix = mix(rules, &runs);
    Cut { runs, unblocked, mix, warnings }
}

fn mix(rules: &WorkRules, runs: &[CutRun]) -> Vec<MixReport> {
    rules
        .mix
        .iter()
        .map(|m| {
            let inside = |r: &&CutRun| r.class.as_ref().is_some_and(|c| m.classes.contains(c));
            let (part, whole) = match m.of {
                ShareOf::Runs => (runs.iter().filter(inside).count() as u64, runs.len() as u64),
                ShareOf::PaidTime => (
                    runs.iter().filter(inside).map(|r| r.judged.pay_secs as u64).sum(),
                    runs.iter().map(|r| r.judged.pay_secs as u64).sum(),
                ),
            };
            let share_pct = if whole == 0 { 0 } else { (part * 100 / whole) as u32 };
            let met = m.min_pct.is_none_or(|min| share_pct >= min) && m.max_pct.is_none_or(|max| share_pct <= max);
            MixReport { classes: m.classes.clone(), of: m.of, share_pct, min_pct: m.min_pct, target_pct: m.target_pct, max_pct: m.max_pct, met }
        })
        .collect()
}
