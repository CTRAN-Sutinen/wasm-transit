//! The runs cut, in two steps that run alone or together. Who composes a
//! run does not matter here: the agency, an operator at the bid, or a
//! planner by hand. Each step is a heuristic, not an optimum, and each is
//! deterministic.
//!
//! [`cut_pieces`] keeps whole a block that makes one good run. A run is
//! good when it breaks no rule, is in a class that the objective maximizes,
//! and pays no premium or guarantee. Any other block is cut at reliefs into
//! the longest pieces that break no rule and pay no premium alone, else the
//! longest that break no rule. Every trip of every block that runs is in
//! exactly one piece.
//!
//! [`compose_runs`] pairs pieces into runs (its doc has the pairing), and
//! [`judge_runs`] judges a set of runs that anyone composed, with the mix
//! over the set.
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

/// The day's blocks cut into pieces: a block that makes one good run stays
/// whole, and any other is cut at reliefs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS))]
#[cfg_attr(feature = "bindings", ts(export, export_to = "transit_runs/"))]
pub struct Pieces {
    pub pieces: Vec<Piece>,
    pub unblocked: Vec<String>,
    pub warnings: Vec<String>,
}

fn period_warnings(rules: &WorkRules, date: &str) -> Vec<String> {
    let Some(source) = &rules.source else { return Vec::new() };
    let outside = source.effective_from.as_deref().is_some_and(|f| date < f) || source.effective_to.as_deref().is_some_and(|t| date > t);
    if !outside {
        return Vec::new();
    }
    vec![format!(
        "{date} is outside the period of the work rules ({} to {})",
        source.effective_from.as_deref().unwrap_or("any"),
        source.effective_to.as_deref().unwrap_or("any")
    )]
}

/// Cuts the day's blocks into pieces. `day` is the service date's weekday,
/// and `date` its "YYYY-MM-DD", which the rules' period is read against.
pub fn cut_pieces(
    network: &Network,
    trips: &[MaterializedTrip],
    rules: &WorkRules,
    constraints: Option<&SchedulingConstraints>,
    date: &str,
    day: Day,
    places: &dyn Places,
) -> Pieces {
    let chooser = Chooser { rules, day, places };
    let (blocks, unblocked) = day_blocks(network, trips);
    let mut pieces = Vec::new();
    for (block, list) in &blocks {
        let one = piece(block, list, list);
        if good_whole(&chooser, &one) {
            pieces.push(one);
        } else {
            pieces.extend(split(block, list, constraints, &chooser));
        }
    }
    pieces.sort_by(|a, b| (a.start_secs(), &a.block).cmp(&(b.start_secs(), &b.block)));
    Pieces { pieces, unblocked, warnings: period_warnings(rules, date) }
}

/// A whole block that makes a good run alone. Composing keeps it a run of
/// its own.
fn good_whole(chooser: &Chooser, p: &Piece) -> bool {
    p.pull_out && p.pull_in && !chooser.best(std::slice::from_ref(p)).2 .2
}

/// Composes pieces into runs, a suggestion. A whole block that makes a good
/// run alone stays a run. Each other piece, earliest first, takes the later
/// piece that makes the best run with it, when that run is good or pays less
/// than the two apart. Runs are numbered from 1 by their first piece.
pub fn compose_runs(pieces: &[Piece], rules: &WorkRules, day: Day, places: &dyn Places) -> Vec<CutRun> {
    let chooser = Chooser { rules, day, places };
    let mut runs = Vec::new();
    let mut open = Vec::new();
    for p in pieces {
        if good_whole(&chooser, p) {
            let (class, judged, _) = chooser.best(std::slice::from_ref(p));
            runs.push((vec![p.clone()], class, judged));
        } else {
            open.push(p.clone());
        }
    }
    let mut pieces = open;
    pieces.sort_by(|a, b| (a.start_secs(), &a.block).cmp(&(b.start_secs(), &b.block)));
    let mut used = vec![false; pieces.len()];
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
    runs.into_iter().enumerate().map(|(n, (pieces, class, judged))| CutRun { id: format!("{}", n + 1), class, pieces, judged }).collect()
}

/// Cuts the day's blocks into pieces and composes them into runs.
pub fn cut_runs(
    network: &Network,
    trips: &[MaterializedTrip],
    rules: &WorkRules,
    constraints: Option<&SchedulingConstraints>,
    date: &str,
    day: Day,
    places: &dyn Places,
) -> Cut {
    let Pieces { pieces, unblocked, warnings } = cut_pieces(network, trips, rules, constraints, date, day, places);
    let runs = compose_runs(&pieces, rules, day, places);
    let mix = mix(rules, &runs);
    Cut { runs, unblocked, mix, warnings }
}

/// A piece named by its block and its first and last trips. A trip that the
/// block runs more than once on the day names its start too.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS))]
#[cfg_attr(feature = "bindings", ts(export, export_to = "transit_runs/"))]
pub struct PieceRef {
    pub block: String,
    pub first_trip: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub first_start_secs: Option<u32>,
    pub last_trip: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub last_start_secs: Option<u32>,
}

impl PieceRef {
    pub fn of(p: &Piece) -> PieceRef {
        let (first, last) = (p.trips.first(), p.trips.last());
        PieceRef {
            block: p.block.clone(),
            first_trip: first.map(|t| t.trip.clone()).unwrap_or_default(),
            first_start_secs: first.map(|t| t.start_secs),
            last_trip: last.map(|t| t.trip.clone()).unwrap_or_default(),
            last_start_secs: last.map(|t| t.start_secs),
        }
    }
}

/// A run that someone composed: its pieces, and its class when they chose
/// one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS))]
#[cfg_attr(feature = "bindings", ts(export, export_to = "transit_runs/"))]
pub struct RunRef {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub class: Option<String>,
    pub pieces: Vec<PieceRef>,
}

/// A set of runs judged together: each run, the mix over the set, and the
/// trips the set places twice or not at all.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS))]
#[cfg_attr(feature = "bindings", ts(export, export_to = "transit_runs/"))]
pub struct RunSet {
    pub runs: Vec<CutRun>,
    pub mix: Vec<MixReport>,
    /// Trips in more than one run, as `trip@start_secs`.
    pub doubled: Vec<String>,
    /// Trips that a block carries on the day and that no run holds, as
    /// `trip@start_secs`.
    pub uncovered: Vec<String>,
    pub warnings: Vec<String>,
}

/// The day's blocks, ready to resolve piece references against.
pub struct DayBlocks {
    blocks: BTreeMap<String, Vec<PieceTrip>>,
}

impl DayBlocks {
    pub fn new(network: &Network, trips: &[MaterializedTrip]) -> Self {
        DayBlocks { blocks: day_blocks(network, trips).0 }
    }

    /// The piece that `r` names, or why it names none.
    pub fn resolve(&self, r: &PieceRef) -> Result<Piece, String> {
        let list = self.blocks.get(&r.block).ok_or_else(|| format!("block {} runs no trip on the day", r.block))?;
        let at = |from: usize, trip: &str, start: Option<u32>| {
            list.iter()
                .enumerate()
                .skip(from)
                .find(|(_, t)| t.trip == trip && start.is_none_or(|s| s == t.start_secs))
                .map(|(i, _)| i)
                .ok_or_else(|| match start {
                    Some(s) => format!("block {} runs no trip {trip} at {s} on the day", r.block),
                    None => format!("block {} runs no trip {trip} on the day", r.block),
                })
        };
        let first = at(0, &r.first_trip, r.first_start_secs)?;
        let last = at(first, &r.last_trip, r.last_start_secs)?;
        Ok(piece(&r.block, &list[first..=last], list))
    }

    /// The block's trip before the piece ends at a relief point, and so does
    /// the piece, unless it is the block's first or last.
    fn reliefs(&self, p: &Piece, relief_points: &[String]) -> Vec<String> {
        if relief_points.is_empty() {
            return Vec::new();
        }
        let at = |stop: &str| relief_points.iter().any(|r| r == stop);
        let mut out = Vec::new();
        if !p.pull_out {
            let list = &self.blocks[&p.block];
            let i = list.iter().position(|t| Some(t) == p.trips.first()).expect("a resolved piece is in its block");
            if !at(&list[i - 1].to) {
                out.push(format!("the piece of {} starts after a trip that ends at {}, not a relief point", p.block, list[i - 1].to));
            }
        }
        if !p.pull_in {
            let to = p.to();
            if !at(to) {
                out.push(format!("the piece of {} ends at {to}, not a relief point", p.block));
            }
        }
        out
    }

    fn uncovered(&self, placed: &std::collections::BTreeSet<(String, u32)>) -> Vec<String> {
        let mut out: Vec<String> = self
            .blocks
            .values()
            .flatten()
            .filter(|t| !placed.contains(&(t.trip.clone(), t.start_secs)))
            .map(|t| format!("{}@{}", t.trip, t.start_secs))
            .collect();
        out.sort();
        out
    }
}

/// Composes the pieces that `refs` name, and reports the set as
/// [`judge_runs`] does.
pub fn compose_refs(blocks: &DayBlocks, refs: &[PieceRef], rules: &WorkRules, date: &str, day: Day, places: &dyn Places) -> Result<RunSet, String> {
    let pieces = refs.iter().map(|r| blocks.resolve(r)).collect::<Result<Vec<_>, _>>()?;
    Ok(run_set(blocks, compose_runs(&pieces, rules, day, places), rules, date))
}

fn run_set(blocks: &DayBlocks, runs: Vec<CutRun>, rules: &WorkRules, date: &str) -> RunSet {
    let mut seen: BTreeMap<(String, u32), u32> = BTreeMap::new();
    for t in runs.iter().flat_map(|r| &r.pieces).flat_map(|p| &p.trips) {
        *seen.entry((t.trip.clone(), t.start_secs)).or_default() += 1;
    }
    let doubled = seen.iter().filter(|(_, &n)| n > 1).map(|((trip, start), _)| format!("{trip}@{start}")).collect();
    let placed = seen.into_keys().collect();
    let mix = mix(rules, &runs);
    RunSet { runs, mix, doubled, uncovered: blocks.uncovered(&placed), warnings: period_warnings(rules, date) }
}

/// Judges each run as the cut judges its own, whoever composed it, and the
/// mix over the set. A run with no class takes the class the cut would give
/// it. A run whose class's conditions it does not meet, whose pieces
/// overlap, or whose pieces start or end away from a relief point breaks a
/// hard rule.
pub fn judge_runs(
    blocks: &DayBlocks,
    runs: &[RunRef],
    rules: &WorkRules,
    constraints: Option<&SchedulingConstraints>,
    date: &str,
    day: Day,
    places: &dyn Places,
) -> Result<RunSet, String> {
    let chooser = Chooser { rules, day, places };
    let relief_points = constraints.map(|c| c.relief_points.as_slice()).unwrap_or(&[]);
    let mut out = Vec::new();
    for r in runs {
        let mut pieces = r.pieces.iter().map(|p| blocks.resolve(p)).collect::<Result<Vec<_>, _>>().map_err(|e| format!("run {}: {e}", r.id))?;
        pieces.sort_by(|a, b| (a.start_secs(), &a.block).cmp(&(b.start_secs(), &b.block)));
        let (class, mut judged) = match &r.class {
            Some(c) => {
                if !rules.classes.iter().any(|k| &k.id == c) {
                    return Err(format!("run {}: the work rules have no class {c}", r.id));
                }
                let mut judged = judge(rules, Some(c), &pieces, day, places);
                if !meets(rules, c, &pieces) {
                    judged.broken.push(hard(format!("the run does not meet the conditions of class {c}")));
                }
                (Some(c.clone()), judged)
            }
            None => {
                let (class, judged, _) = chooser.best(&pieces);
                (class, judged)
            }
        };
        if pieces.windows(2).any(|w| w[0].end_secs() > w[1].start_secs()) {
            judged.broken.push(hard("the run's pieces overlap".to_string()));
        }
        for p in &pieces {
            judged.broken.extend(blocks.reliefs(p, relief_points).into_iter().map(hard));
        }
        judged.broken.sort();
        out.push(CutRun { id: r.id.clone(), class, pieces, judged });
    }
    Ok(run_set(blocks, out, rules, date))
}

fn hard(rule: String) -> crate::run::Finding {
    crate::run::Finding { rule, strength: crate::rules::Strength::Hard, cites: None }
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
