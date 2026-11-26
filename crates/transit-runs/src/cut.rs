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
//! [`compose_runs`] pairs pieces into runs and moves them toward the mix
//! (its doc and [`toward_mix`]'s have how), and
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
    /// The pay that moving toward the mix added over the least-pay
    /// composition, in seconds. Negative when it saved pay.
    pub mix_cost_secs: i64,
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

fn piece(block: &str, trips: &[PieceTrip], all: &[PieceTrip], deadheads: &Deadheads) -> Piece {
    let (pull_out, pull_in) = (trips.first() == all.first(), trips.last() == all.last());
    let drive = |end: bool, stop: Option<&PieceTrip>, to_garage: bool| {
        end.then(|| stop.and_then(|t| deadheads.secs.get(&(if to_garage { t.to.clone() } else { t.from.clone() }, to_garage)).copied())).flatten()
    };
    Piece {
        block: block.to_string(),
        trips: trips.to_vec(),
        pull_out,
        pull_in,
        garage: (pull_out || pull_in).then(|| deadheads.garage.clone()).flatten(),
        deadhead_out_secs: drive(pull_out, trips.first(), false),
        deadhead_in_secs: drive(pull_in, trips.last(), true),
    }
}

/// The roads between two positions, for the drives to and from a garage.
pub trait Roads {
    /// Meters along the streets, or None when either end is off them.
    fn meters(&self, from: (f64, f64), to: (f64, f64)) -> Option<f64>;
}

/// The first garage, and the drive between it and each block's first and
/// last stop, keyed by the stop and whether the drive is to the garage.
#[derive(Default)]
struct Deadheads {
    garage: Option<String>,
    secs: BTreeMap<(String, bool), u32>,
}

fn deadheads(
    network: &Network,
    blocks: &BTreeMap<String, Vec<PieceTrip>>,
    constraints: Option<&SchedulingConstraints>,
    roads: Option<&dyn Roads>,
) -> Deadheads {
    let Some(garage) = constraints.and_then(|c| c.garages.first()) else { return Deadheads::default() };
    let mut secs = BTreeMap::new();
    if let (Some(roads), Some(kph)) = (roads, constraints.and_then(|c| c.deadhead_kph)) {
        let at = (garage.lat, garage.lon);
        let ends = blocks.values().flat_map(|l| [(l.first().map(|t| t.from.clone()), false), (l.last().map(|t| t.to.clone()), true)]);
        for (stop, to_garage) in ends {
            let Some(stop) = stop else { continue };
            if secs.contains_key(&(stop.clone(), to_garage)) {
                continue;
            }
            let Some(s) = network.stops.get(&stop) else { continue };
            let m = if to_garage { roads.meters((s.lat, s.lon), at) } else { roads.meters(at, (s.lat, s.lon)) };
            if let Some(m) = m {
                secs.insert((stop, to_garage), (m * 3.6 / kph as f64).ceil() as u32);
            }
        }
    }
    Deadheads { garage: Some(garage.id.clone()), secs }
}

/// A stop where blocks lay over: a trip of the block ends there, and the
/// block's next trip starts there or at another stop of its station.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS))]
#[cfg_attr(feature = "bindings", ts(export, export_to = "transit_runs/"))]
pub struct ReliefPoint {
    pub stop: String,
    /// The blocks that lay over there.
    pub blocks: u32,
    /// The routes of the trips on either side of those layovers.
    pub routes: Vec<String>,
}

/// The stops where two blocks or more lay over: transit centers and shared
/// terminals, where one operator can most likely relieve another. A guess
/// from the blocks, for when the constraints name no relief point.
fn infer_relief_points(network: &Network, day: &[MaterializedTrip], blocks: &BTreeMap<String, Vec<PieceTrip>>) -> Vec<ReliefPoint> {
    let route: BTreeMap<(&str, u32), &str> = day.iter().map(|t| ((t.trip.as_str(), t.start_secs), t.route.as_str())).collect();
    let station = |stop: &str| network.stops.get(stop).and_then(|s| s.parent.clone()).unwrap_or_else(|| stop.to_string());
    let mut at: BTreeMap<String, (std::collections::BTreeSet<&str>, std::collections::BTreeSet<&str>)> = BTreeMap::new();
    for (block, list) in blocks {
        for w in list.windows(2) {
            if w[0].to != w[1].from && station(&w[0].to) != station(&w[1].from) {
                continue;
            }
            let entry = at.entry(w[0].to.clone()).or_default();
            entry.0.insert(block.as_str());
            for t in w {
                if let Some(r) = route.get(&(t.trip.as_str(), t.start_secs)) {
                    entry.1.insert(r);
                }
            }
        }
    }
    at.into_iter()
        .filter(|(_, (b, _))| b.len() >= 2)
        .map(|(stop, (b, r))| ReliefPoint { stop, blocks: b.len() as u32, routes: r.into_iter().map(String::from).collect() })
        .collect()
}

/// The block's trips cut at reliefs into the longest pieces legal alone. A
/// relief is after a trip that ends at a relief point, or after any trip
/// when the constraints name none.
fn split(block: &str, trips: &[PieceTrip], relief_points: &[String], deadheads: &Deadheads, chooser: &Chooser) -> Vec<Piece> {
    let relief = |i: usize| relief_points.is_empty() || relief_points.iter().any(|p| *p == trips[i].to);
    let piece = |a: &[PieceTrip], all: &[PieceTrip]| piece(block, a, all, deadheads);
    // Breaks nothing alone, and pays no premium alone: a piece that stops
    // where premiums start leaves pairing room to build a full day.
    let fits = |from: usize, to: usize| {
        let (_, judged, score) = chooser.best(std::slice::from_ref(&piece(&trips[from..=to], trips)));
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
        out.push(piece(&trips[start..=stop], trips));
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
    /// The relief points the cut guessed from the blocks, because the
    /// constraints name none. Empty when the constraints name some.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[cfg_attr(feature = "bindings", ts(optional, as = "Option<Vec<ReliefPoint>>"))]
    pub inferred_relief_points: Vec<ReliefPoint>,
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
/// and `date` its "YYYY-MM-DD", which the rules' period is read against. A
/// relief is at a relief point that the constraints name, else at one the
/// blocks suggest, else after any trip.
pub fn cut_pieces(
    blocks: &DayBlocks,
    rules: &WorkRules,
    constraints: Option<&SchedulingConstraints>,
    date: &str,
    day: Day,
    places: &dyn Places,
) -> Pieces {
    let chooser = Chooser { rules, day, places };
    let named = constraints.map(|c| c.relief_points.clone()).unwrap_or_default();
    let inferred = if named.is_empty() { blocks.inferred.clone() } else { Vec::new() };
    let relief_points: Vec<String> = if named.is_empty() { inferred.iter().map(|r| r.stop.clone()).collect() } else { named };
    let mut pieces = Vec::new();
    for (block, list) in &blocks.blocks {
        let one = piece(block, list, list, &blocks.deadheads);
        if good_whole(&chooser, &one) {
            pieces.push(one);
        } else {
            pieces.extend(split(block, list, &relief_points, &blocks.deadheads, &chooser));
        }
    }
    pieces.sort_by(|a, b| (a.start_secs(), &a.block).cmp(&(b.start_secs(), &b.block)));
    Pieces { pieces, unblocked: blocks.unblocked.clone(), inferred_relief_points: inferred, warnings: period_warnings(rules, date) }
}

/// A whole block that makes a good run alone. Composing keeps it a run of
/// its own.
fn good_whole(chooser: &Chooser, p: &Piece) -> bool {
    p.pull_out && p.pull_in && !chooser.best(std::slice::from_ref(p)).2 .2
}

/// A composition, and the pay its moves toward the mix added.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Composition {
    pub runs: Vec<CutRun>,
    /// The composition's pay minus the least-pay composition's, in
    /// seconds. Negative when a move toward the mix also saved pay.
    pub mix_cost_secs: i64,
}

/// Composes pieces into runs, a suggestion. A whole block that makes a good
/// run alone stays a run. Each other piece, earliest first, takes the later
/// piece that makes the best run with it, when that run is good or pays less
/// than the two apart. That is the least-pay composition. When it misses a
/// mix rule, [`toward_mix`] moves it toward the mix. Runs are numbered from
/// 1 by their first piece.
pub fn compose_runs(pieces: &[Piece], rules: &WorkRules, day: Day, places: &dyn Places) -> Composition {
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
    let least: i64 = runs.iter().map(|r| r.2.pay_secs as i64).sum();
    let mut runs = toward_mix(runs, &chooser);
    let mix_cost_secs = runs.iter().map(|r| r.2.pay_secs as i64).sum::<i64>() - least;
    runs.sort_by(|a, b| (a.0[0].start_secs(), &a.0[0].block).cmp(&(b.0[0].start_secs(), &b.0[0].block)));
    let runs = runs.into_iter().enumerate().map(|(n, (pieces, class, judged))| CutRun { id: format!("{}", n + 1), class, pieces, judged }).collect();
    Composition { runs, mix_cost_secs }
}

type Drafted = (Vec<Piece>, Option<String>, Judged);

enum Move {
    /// Two one-piece runs become one run of the class.
    Merge(usize, usize, String, Judged),
    /// A run takes another class.
    Reclass(usize, String, Judged),
}

/// The runs and the pay in each class, and over the set.
#[derive(Default)]
struct Tally {
    count: BTreeMap<Option<String>, i64>,
    pay: BTreeMap<Option<String>, i64>,
    runs: i64,
    total_pay: i64,
}

impl Tally {
    fn add(&mut self, class: &Option<String>, pay: u32, sign: i64) {
        *self.count.entry(class.clone()).or_default() += sign;
        *self.pay.entry(class.clone()).or_default() += sign * pay as i64;
        self.runs += sign;
        self.total_pay += sign * pay as i64;
    }

    fn parts(&self, m: &crate::rules::MixRule) -> (i64, i64) {
        let of = |map: &BTreeMap<Option<String>, i64>| m.classes.iter().map(|c| map.get(&Some(c.clone())).copied().unwrap_or(0)).sum();
        match m.of {
            ShareOf::Runs => (of(&self.count), self.runs),
            ShareOf::PaidTime => (of(&self.pay), self.total_pay),
        }
    }

    /// How far the set is outside the mix: each rule's distance below its
    /// minimum or above its maximum, in percentage points, summed.
    fn off(&self, rules: &WorkRules) -> f64 {
        rules
            .mix
            .iter()
            .map(|m| {
                let (part, whole) = self.parts(m);
                let share = if whole <= 0 { 0.0 } else { part as f64 * 100.0 / whole as f64 };
                m.min_pct.map_or(0.0, |min| (min as f64 - share).max(0.0)) + m.max_pct.map_or(0.0, |max| (share - max as f64).max(0.0))
            })
            .sum()
    }

    /// As [`mix`] reports `met`.
    fn met(&self, rules: &WorkRules) -> bool {
        rules.mix.iter().all(|m| {
            let (part, whole) = self.parts(m);
            let pct = if whole <= 0 { 0 } else { (part * 100 / whole) as u32 };
            m.min_pct.is_none_or(|min| pct >= min) && m.max_pct.is_none_or(|max| pct <= max)
        })
    }
}

/// Moves a composition toward the rules' mix, cheapest first. A move merges
/// two one-piece runs into one run of a class, or gives a run another class,
/// and the run it makes breaks no rule. Each step takes the move with the
/// least added pay for each point it closes, and the steps stop when the
/// mix is met or no move closes any of it. So the set is never further from
/// the mix than the least-pay composition.
fn toward_mix(runs: Vec<Drafted>, chooser: &Chooser) -> Vec<Drafted> {
    let rules = chooser.rules;
    let mut tally = Tally::default();
    for r in &runs {
        tally.add(&r.1, r.2.pay_secs, 1);
    }
    if rules.mix.is_empty() || tally.met(rules) {
        return runs;
    }
    let clean_as = |pieces: &[Piece], class: &str| {
        if !meets(rules, class, pieces) {
            return None;
        }
        let judged = judge(rules, Some(class), pieces, chooser.day, chooser.places);
        judged.broken.is_empty().then_some(judged)
    };
    let mut moves = Vec::new();
    for (i, a) in runs.iter().enumerate().filter(|(_, r)| r.0.len() == 1) {
        for (j, b) in runs.iter().enumerate().filter(|(_, r)| r.0.len() == 1) {
            if i == j || b.0[0].start_secs() < a.0[0].end_secs() {
                continue;
            }
            let pair = [a.0[0].clone(), b.0[0].clone()];
            for c in &rules.classes {
                if let Some(judged) = clean_as(&pair, &c.id) {
                    moves.push(Move::Merge(i, j, c.id.clone(), judged));
                }
            }
        }
    }
    for (i, r) in runs.iter().enumerate() {
        for c in rules.classes.iter().filter(|c| r.1.as_deref() != Some(c.id.as_str())) {
            if let Some(judged) = clean_as(&r.0, &c.id) {
                moves.push(Move::Reclass(i, c.id.clone(), judged));
            }
        }
    }

    let mut set: Vec<Option<Drafted>> = runs.into_iter().map(Some).collect();
    loop {
        let off = tally.off(rules);
        let mut best: Option<(f64, i64, usize)> = None;
        for (k, mv) in moves.iter().enumerate() {
            let (gone, made): (Vec<&Drafted>, (Option<String>, u32)) = match mv {
                Move::Merge(i, j, c, judged) => match (&set[*i], &set[*j]) {
                    (Some(a), Some(b)) => (vec![a, b], (Some(c.clone()), judged.pay_secs)),
                    _ => continue,
                },
                Move::Reclass(i, c, judged) => match &set[*i] {
                    Some(a) if a.1.as_deref() != Some(c.as_str()) => (vec![a], (Some(c.clone()), judged.pay_secs)),
                    _ => continue,
                },
            };
            for g in &gone {
                tally.add(&g.1, g.2.pay_secs, -1);
            }
            tally.add(&made.0, made.1, 1);
            let gain = off - tally.off(rules);
            tally.add(&made.0, made.1, -1);
            for g in &gone {
                tally.add(&g.1, g.2.pay_secs, 1);
            }
            if gain <= 1e-9 {
                continue;
            }
            let added = made.1 as i64 - gone.iter().map(|g| g.2.pay_secs as i64).sum::<i64>();
            let rate = added as f64 / gain;
            let better = best.is_none_or(|(r, a, n)| rate.total_cmp(&r).then(added.cmp(&a)).then(k.cmp(&n)).is_lt());
            if better {
                best = Some((rate, added, k));
            }
        }
        let Some((_, _, k)) = best else { break };
        match &moves[k] {
            Move::Merge(i, j, c, judged) => {
                let (a, b) = (set[*i].take().expect("a live move"), set[*j].take().expect("a live move"));
                tally.add(&a.1, a.2.pay_secs, -1);
                tally.add(&b.1, b.2.pay_secs, -1);
                tally.add(&Some(c.clone()), judged.pay_secs, 1);
                set.push(Some((vec![a.0[0].clone(), b.0[0].clone()], Some(c.clone()), judged.clone())));
            }
            Move::Reclass(i, c, judged) => {
                let a = set[*i].take().expect("a live move");
                tally.add(&a.1, a.2.pay_secs, -1);
                tally.add(&Some(c.clone()), judged.pay_secs, 1);
                set[*i] = Some((a.0, Some(c.clone()), judged.clone()));
            }
        }
        if tally.met(rules) {
            break;
        }
    }
    set.into_iter().flatten().collect()
}

/// Cuts the day's blocks into pieces and composes them into runs.
pub fn cut_runs(
    blocks: &DayBlocks,
    rules: &WorkRules,
    constraints: Option<&SchedulingConstraints>,
    date: &str,
    day: Day,
    places: &dyn Places,
) -> Cut {
    let Pieces { pieces, unblocked, warnings, .. } = cut_pieces(blocks, rules, constraints, date, day, places);
    let Composition { runs, mix_cost_secs } = compose_runs(&pieces, rules, day, places);
    let mix = mix(rules, &runs);
    Cut { runs, unblocked, mix, mix_cost_secs, warnings }
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
    /// As in [`Cut`], for a set that `compose` made. Absent for a set
    /// judged as given.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub mix_cost_secs: Option<i64>,
    /// Trips in more than one run, as `trip@start_secs`.
    pub doubled: Vec<String>,
    /// Trips that a block carries on the day and that no run holds, as
    /// `trip@start_secs`.
    pub uncovered: Vec<String>,
    pub warnings: Vec<String>,
}

/// The day's blocks: their trips, the drives to and from the garage at
/// their ends, and the relief points they suggest.
pub struct DayBlocks {
    blocks: BTreeMap<String, Vec<PieceTrip>>,
    unblocked: Vec<String>,
    deadheads: Deadheads,
    inferred: Vec<ReliefPoint>,
}

impl DayBlocks {
    /// `roads` measures the drives to and from the constraints' first
    /// garage. Without roads, a garage, or a deadhead speed, those drives
    /// are unknown.
    pub fn new(network: &Network, trips: &[MaterializedTrip], constraints: Option<&SchedulingConstraints>, roads: Option<&dyn Roads>) -> Self {
        let (blocks, unblocked) = day_blocks(network, trips);
        let deadheads = deadheads(network, &blocks, constraints, roads);
        let inferred = infer_relief_points(network, trips, &blocks);
        DayBlocks { blocks, unblocked, deadheads, inferred }
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
        Ok(piece(&r.block, &list[first..=last], list, &self.deadheads))
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
    let Composition { runs, mix_cost_secs } = compose_runs(&pieces, rules, day, places);
    Ok(RunSet { mix_cost_secs: Some(mix_cost_secs), ..run_set(blocks, runs, rules, date) })
}

fn run_set(blocks: &DayBlocks, runs: Vec<CutRun>, rules: &WorkRules, date: &str) -> RunSet {
    let mut seen: BTreeMap<(String, u32), u32> = BTreeMap::new();
    for t in runs.iter().flat_map(|r| &r.pieces).flat_map(|p| &p.trips) {
        *seen.entry((t.trip.clone(), t.start_secs)).or_default() += 1;
    }
    let doubled = seen.iter().filter(|(_, &n)| n > 1).map(|((trip, start), _)| format!("{trip}@{start}")).collect();
    let placed = seen.into_keys().collect();
    let mix = mix(rules, &runs);
    RunSet { runs, mix, mix_cost_secs: None, doubled, uncovered: blocks.uncovered(&placed), warnings: period_warnings(rules, date) }
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
