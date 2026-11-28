//! A run judged and paid under the work rules. The cut builds runs and asks
//! this module what each one costs and breaks, and run legality asks the
//! same of a run a planner cut by hand.
//!
//! Pay is time, weighted by rate, rounded down to the second. Each second
//! of the timeline is paid at a base of 100 percent, times its gap band's
//! share for a gap. Premiums raise the rate of the seconds they trigger:
//! under `highest` the highest rate applies, and under `sum` each premium
//! adds its rate above 100. A guarantee tops up at the base rate.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
#[cfg(feature = "bindings")]
use ts_rs::TS;

use crate::rules::{
    Activity, BreakFrom, Combine, Day, Event, Limit, Per, Quantity, RuleSet, Strength, Timing,
    TravelAt, TriggerOn, Window, WorkRules,
};

/// One stretch of a block that one operator drives, from a relief or a
/// pull-out to a relief or a pull-in.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS))]
#[cfg_attr(feature = "bindings", ts(export, export_to = "transit_runs/"))]
pub struct Piece {
    pub block: String,
    pub trips: Vec<PieceTrip>,
    /// The piece starts at the block's first trip of the day.
    pub pull_out: bool,
    /// The piece ends at the block's last trip of the day.
    pub pull_in: bool,
    /// The garage the block leaves from and returns to, on a piece that
    /// pulls out or in, when the scheduling constraints name one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub garage: Option<String>,
    /// The drive from the garage to the first trip, when it is known. On a
    /// piece that pulls out, absent means unknown, never none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub deadhead_out_secs: Option<u32>,
    /// The drive from the last trip to the garage, when it is known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub deadhead_in_secs: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS))]
#[cfg_attr(feature = "bindings", ts(export, export_to = "transit_runs/"))]
pub struct PieceTrip {
    pub trip: String,
    /// Seconds after service-day midnight.
    pub start_secs: u32,
    pub end_secs: u32,
    pub from: String,
    pub to: String,
}

impl Piece {
    pub fn start_secs(&self) -> u32 {
        self.trips.first().map_or(0, |t| t.start_secs)
    }

    pub fn end_secs(&self) -> u32 {
        self.trips.last().map_or(0, |t| t.end_secs)
    }

    pub(crate) fn from(&self) -> &str {
        self.trips.first().map_or("", |t| t.from.as_str())
    }

    pub(crate) fn to(&self) -> &str {
        self.trips.last().map_or("", |t| t.to.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS))]
#[cfg_attr(feature = "bindings", ts(export, export_to = "transit_runs/"))]
pub struct Span {
    pub activity: Activity,
    pub from_secs: u32,
    pub to_secs: u32,
}

/// A rule that the run breaks, or that the cut could not judge.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS))]
#[cfg_attr(feature = "bindings", ts(export, export_to = "transit_runs/"))]
pub struct Finding {
    pub rule: String,
    pub strength: Strength,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub cites: Option<String>,
}

/// A time, and the place: a garage, or the stop of a relief. The place is
/// absent when the run pulls out or in and no garage is known.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS))]
#[cfg_attr(feature = "bindings", ts(export, export_to = "transit_runs/"))]
pub struct Moment {
    pub secs: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub place: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS))]
#[cfg_attr(feature = "bindings", ts(export, export_to = "transit_runs/"))]
pub struct Judged {
    /// Where and when the run starts: its first report.
    pub report: Moment,
    /// Where and when the run ends: its last clear.
    pub clear: Moment,
    /// Time on the vehicle: revenue, deadhead, and layover.
    pub platform_secs: u32,
    /// The run's drives to and from the garage. Absent when one is unknown.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub deadhead_secs: Option<u32>,
    pub timeline: Vec<Span>,
    pub spread_secs: u32,
    pub measures: BTreeMap<String, u32>,
    /// The time paid at the base rate: every second, times its gap band's
    /// share for a gap.
    pub paid_secs: u32,
    /// The pay above the base rate that the premiums add, in seconds.
    pub premium_secs: u32,
    /// The pay that the guarantees add, in seconds.
    pub guarantee_secs: u32,
    /// `paid_secs`, `premium_secs`, and `guarantee_secs`, together.
    pub pay_secs: u32,
    /// Every limit the run breaks and every break rule it misses.
    pub broken: Vec<Finding>,
    /// Rules the run alone cannot answer: a window that spans days, or
    /// travel with no trip found home.
    pub not_judged: Vec<Finding>,
}

impl Judged {
    pub fn legal(&self) -> bool {
        self.broken.iter().all(|f| f.strength == Strength::Waivable)
    }
}

/// What the run's places need from the network: distances, and the trip
/// home.
pub trait Places {
    /// Meters between two stops, or None when either has no position.
    fn meters(&self, a: &str, b: &str) -> Option<f64>;
    /// The earliest arrival within `within_m` meters of `home`, by a trip
    /// that leaves `from` at or after `after_secs`.
    fn next_trip_home(&self, from: &str, after_secs: u32, home: &str, within_m: u32)
        -> Option<u32>;
}

/// The run's timeline, from its first report to its last clear. The
/// allowances at each event come first and last. Between two trips of a
/// piece is layover, and between two pieces is a gap. Trips never move: an
/// allowance that would overlap the next piece's first trip, or the span
/// before it, is cut short.
pub fn timeline(pieces: &[Piece], rules: &RuleSet) -> Vec<Span> {
    let allowance = |event: Event| {
        rules
            .allowances
            .iter()
            .filter(|a| a.event == event)
            .collect::<Vec<_>>()
    };
    let mut out: Vec<Span> = Vec::new();
    for (i, piece) in pieces.iter().enumerate() {
        let first = piece.start_secs();
        let out_secs = if piece.pull_out {
            piece.deadhead_out_secs.unwrap_or(0)
        } else {
            0
        };
        let mut before: Vec<_> = allowance(if piece.pull_out {
            Event::PullOut
        } else {
            Event::ReliefOn
        });
        before.extend(allowance(Event::PieceStart));
        let lead: u32 = before.iter().map(|a| a.secs).sum();
        let prev = out.last().map(|s| s.to_secs);
        let mut at = first
            .saturating_sub(out_secs + lead)
            .max(prev.unwrap_or(0))
            .min(first);
        // The allowances come before the drive, and both give way to the
        // span before them.
        let start = first.saturating_sub(out_secs).max(at);
        if let Some(prev) = prev.filter(|&p| at > p) {
            out.push(Span {
                activity: Activity::Gap,
                from_secs: prev,
                to_secs: at,
            });
        }
        for a in before {
            let to = (at + a.secs).min(start);
            if to > at {
                out.push(Span {
                    activity: a.activity,
                    from_secs: at,
                    to_secs: to,
                });
            }
            at = to;
        }
        if out_secs > 0 && first > at {
            out.push(Span {
                activity: Activity::Deadhead,
                from_secs: at,
                to_secs: first,
            });
            at = first;
        }
        for (j, t) in piece.trips.iter().enumerate() {
            if j > 0 && t.start_secs > at {
                out.push(Span {
                    activity: Activity::Layover,
                    from_secs: at,
                    to_secs: t.start_secs,
                });
            }
            let from = t.start_secs.max(at);
            if t.end_secs > from {
                out.push(Span {
                    activity: Activity::Revenue,
                    from_secs: from,
                    to_secs: t.end_secs,
                });
            }
            at = at.max(t.end_secs);
        }
        let stop = pieces.get(i + 1).map_or(u32::MAX, |next| next.start_secs());
        let in_secs = if piece.pull_in {
            piece.deadhead_in_secs.unwrap_or(0)
        } else {
            0
        };
        let to = at.saturating_add(in_secs).min(stop);
        if to > at {
            out.push(Span {
                activity: Activity::Deadhead,
                from_secs: at,
                to_secs: to,
            });
            at = to;
        }
        let mut after: Vec<_> = allowance(Event::PieceEnd);
        after.extend(allowance(if piece.pull_in {
            Event::PullIn
        } else {
            Event::ReliefOff
        }));
        for a in after {
            let to = at.saturating_add(a.secs).min(stop);
            if to > at {
                out.push(Span {
                    activity: a.activity,
                    from_secs: at,
                    to_secs: to,
                });
            }
            at = to.max(at);
        }
    }
    out
}

fn total(timeline: &[Span], activities: &[Activity]) -> u32 {
    timeline
        .iter()
        .filter(|s| activities.contains(&s.activity))
        .map(|s| s.to_secs - s.from_secs)
        .sum()
}

/// The instant the measure's running total passes `after`, if it does.
fn crossing(timeline: &[Span], activities: &[Activity], after: u32) -> Option<u32> {
    let mut sum = 0;
    for s in timeline.iter().filter(|s| activities.contains(&s.activity)) {
        let len = s.to_secs - s.from_secs;
        if sum + len > after {
            return Some(s.from_secs + (after - sum));
        }
        sum += len;
    }
    None
}

fn finding(rule: String, strength: Strength, cites: &Option<String>) -> Finding {
    Finding {
        rule,
        strength,
        cites: cites.clone(),
    }
}

/// Every value the limit bounds on this run, each to check against it.
fn quantities(
    l: &Limit,
    timeline: &[Span],
    pieces: &[Piece],
    measures: &BTreeMap<String, Vec<Activity>>,
) -> Vec<u32> {
    match l.quantity {
        Quantity::Measure => vec![l
            .measure
            .as_ref()
            .and_then(|m| measures.get(m))
            .map_or(0, |a| total(timeline, a))],
        Quantity::Spread => vec![spread(timeline)],
        Quantity::Piece => pieces
            .iter()
            .map(|p| p.end_secs() - p.start_secs())
            .collect(),
        Quantity::Pieces => vec![pieces.len() as u32],
        Quantity::Gap => timeline
            .iter()
            .filter(|s| s.activity == Activity::Gap)
            .map(|s| s.to_secs - s.from_secs)
            .collect(),
        Quantity::DaysWorked | Quantity::Rest => vec![],
    }
}

fn spread(timeline: &[Span]) -> u32 {
    match (timeline.first(), timeline.last()) {
        (Some(a), Some(b)) => b.to_secs - a.from_secs,
        _ => 0,
    }
}

/// The limit's breaks on this run, as text naming the value.
fn breaks_limit(
    l: &Limit,
    timeline: &[Span],
    pieces: &[Piece],
    measures: &BTreeMap<String, Vec<Activity>>,
) -> Option<String> {
    let what = match (&l.quantity, &l.measure) {
        (Quantity::Measure, Some(m)) => format!("{m} time"),
        (q, _) => format!("{q:?}").to_lowercase(),
    };
    quantities(l, timeline, pieces, measures)
        .into_iter()
        .find_map(|v| {
            if l.max.is_some_and(|max| v > max) {
                Some(format!("{what} {v} is over its max {}", l.max.unwrap()))
            } else if l.min.is_some_and(|min| v < min) {
                Some(format!("{what} {v} is under its min {}", l.min.unwrap()))
            } else {
                None
            }
        })
}

/// The run meets every one of the class's conditions.
pub fn meets(rules: &WorkRules, class: &str, pieces: &[Piece]) -> bool {
    let Some(c) = rules.classes.iter().find(|c| c.id == class) else {
        return false;
    };
    let timeline = timeline(pieces, &rules.rules_for(Some(class)));
    c.conditions
        .iter()
        .all(|l| breaks_limit(l, &timeline, pieces, &rules.measures).is_none())
}

/// Judges and pays a run of `class` (or of no class) on a service day whose
/// weekday is `day`.
pub fn judge(
    rules: &WorkRules,
    class: Option<&str>,
    pieces: &[Piece],
    day: Day,
    places: &dyn Places,
) -> Judged {
    let set = rules.rules_for(class);
    let mut timeline = timeline(pieces, &set);
    let mut broken = Vec::new();
    let mut not_judged = Vec::new();

    for t in &set.travel {
        let pairs: Vec<(&str, &str, u32)> = match t.at {
            TravelAt::DayEnd => pieces
                .first()
                .zip(pieces.last())
                .map(|(a, b)| (b.to(), a.from(), b.end_secs()))
                .into_iter()
                .collect(),
            TravelAt::BetweenPieces => pieces
                .windows(2)
                .map(|w| (w[0].to(), w[1].from(), w[0].end_secs()))
                .collect(),
        };
        for (from, home, after) in pairs {
            let Some(far) = places.meters(from, home) else {
                not_judged.push(finding(
                    format!("travel from {from}: the stop has no position"),
                    Strength::Hard,
                    &t.cites,
                ));
                continue;
            };
            if far <= t.beyond_m as f64 {
                continue;
            }
            let secs = match t.timed {
                Timing::NextTrip => places
                    .next_trip_home(from, after, home, t.within_m)
                    .map(|arrive| arrive.saturating_sub(after)),
                Timing::Fixed => t.fixed_secs,
                Timing::Speed => t
                    .speed_kph
                    .map(|kph| (far * 3.6 / kph as f64).ceil() as u32),
            };
            let Some(secs) = secs.filter(|&s| s > 0) else {
                not_judged.push(finding(
                    format!("travel from {from} to {home}: no trip found"),
                    Strength::Hard,
                    &t.cites,
                ));
                continue;
            };
            insert_travel(&mut timeline, after, secs);
        }
    }

    let measures: BTreeMap<String, u32> = rules
        .measures
        .iter()
        .map(|(m, a)| (m.clone(), total(&timeline, a)))
        .collect();
    let spread_secs = spread(&timeline);

    for l in &set.limits {
        if l.over != Window::Run {
            not_judged.push(finding(
                format!("a {:?} limit over {:?}", l.quantity, l.over).to_lowercase(),
                l.strength,
                &l.cites,
            ));
            continue;
        }
        if let Some(why) = breaks_limit(l, &timeline, pieces, &rules.measures) {
            broken.push(finding(why, l.strength, &l.cites));
        }
    }

    let report = timeline.first().map_or(0, |s| s.from_secs);
    for b in &set.breaks {
        if let (Some(m), Some(over)) = (&b.when_measure, b.when_over_secs) {
            if measures.get(m).copied().unwrap_or(0) <= over {
                continue;
            }
        }
        let anchor = match b.from {
            BreakFrom::Report => Some(report),
            BreakFrom::MeasureStart => b
                .measure
                .as_ref()
                .and_then(|m| rules.measures.get(m))
                .and_then(|a| {
                    timeline
                        .iter()
                        .find(|s| a.contains(&s.activity))
                        .map(|s| s.from_secs)
                }),
        };
        let Some(anchor) = anchor else { continue };
        let (from, to) = (anchor + b.window_from_secs, anchor + b.window_to_secs);
        let off =
            |s: &&Span| s.activity == Activity::Gap || (b.paid && s.activity == Activity::Layover);
        let taken = timeline
            .iter()
            .filter(off)
            .filter(|s| s.to_secs.min(to).saturating_sub(s.from_secs.max(from)) >= b.min_secs)
            .count() as u32;
        if taken < b.count {
            broken.push(finding(
                format!(
                    "{} break of {} s between {} s and {} s after the start, {} taken",
                    b.count, b.min_secs, b.window_from_secs, b.window_to_secs, taken
                ),
                Strength::Hard,
                &b.cites,
            ));
        }
    }

    let (paid_secs, weighted) = pay(rules, &set, &timeline, day, class);
    let premium_secs = weighted.saturating_sub(paid_secs);

    let mut guarantee_secs = 0u32;
    for g in &set.guarantees {
        let Some(counts) = rules.measures.get(&g.counts) else {
            continue;
        };
        let groups: Vec<(u32, u32)> = match g.per {
            Per::Run | Per::Day => vec![(report, timeline.last().map_or(report, |s| s.to_secs))],
            Per::Report => reports(&timeline, &set),
            Per::Week => {
                not_judged.push(finding(
                    "a guarantee per week".into(),
                    Strength::Hard,
                    &g.cites,
                ));
                continue;
            }
        };
        for (from, to) in groups {
            let counted = paid_within(&timeline, &set, counts, from, to);
            guarantee_secs = guarantee_secs.max(g.min_secs.saturating_sub(counted));
        }
    }

    for p in pieces {
        if p.pull_out && p.deadhead_out_secs.is_none() {
            not_judged.push(finding(
                format!(
                    "the deadhead to {} at the pull-out of {}",
                    p.from(),
                    p.block
                ),
                Strength::Hard,
                &None,
            ));
        }
        if p.pull_in && p.deadhead_in_secs.is_none() {
            not_judged.push(finding(
                format!("the deadhead from {} at the pull-in of {}", p.to(), p.block),
                Strength::Hard,
                &None,
            ));
        }
    }
    let ends = |p: &Piece| {
        [
            (p.pull_out, p.deadhead_out_secs),
            (p.pull_in, p.deadhead_in_secs),
        ]
    };
    let deadhead_secs = pieces
        .iter()
        .flat_map(ends)
        .filter(|e| e.0)
        .map(|e| e.1)
        .sum::<Option<u32>>();
    let report = Moment {
        secs: timeline.first().map_or(0, |s| s.from_secs),
        place: pieces.first().and_then(|p| {
            if p.pull_out {
                p.garage.clone()
            } else {
                Some(p.from().to_string())
            }
        }),
    };
    let clear = Moment {
        secs: timeline.last().map_or(0, |s| s.to_secs),
        place: pieces.last().and_then(|p| {
            if p.pull_in {
                p.garage.clone()
            } else {
                Some(p.to().to_string())
            }
        }),
    };
    let platform_secs = total(
        &timeline,
        &[Activity::Revenue, Activity::Deadhead, Activity::Layover],
    );
    broken.sort();
    not_judged.sort();
    not_judged.dedup();
    Judged {
        report,
        clear,
        platform_secs,
        deadhead_secs,
        spread_secs,
        measures,
        paid_secs,
        premium_secs,
        guarantee_secs,
        pay_secs: weighted + guarantee_secs,
        broken,
        not_judged,
        timeline,
    }
}

/// Travel starts where the day or the piece ends. Spans after it move
/// later only when they would overlap it.
fn insert_travel(timeline: &mut Vec<Span>, at: u32, secs: u32) {
    let end = at + secs;
    let pos = timeline
        .iter()
        .position(|s| s.from_secs >= at)
        .unwrap_or(timeline.len());
    let mut kept = Vec::with_capacity(timeline.len() + 1);
    kept.extend(timeline.drain(..pos));
    let rest: Vec<Span> = std::mem::take(timeline);
    kept.push(Span {
        activity: Activity::Travel,
        from_secs: at,
        to_secs: end,
    });
    for s in rest {
        if s.to_secs <= end {
            if s.activity != Activity::Gap {
                kept.push(Span {
                    activity: s.activity,
                    from_secs: end,
                    to_secs: end + (s.to_secs - s.from_secs),
                });
            }
        } else if s.from_secs < end {
            kept.push(Span {
                activity: s.activity,
                from_secs: end,
                to_secs: s.to_secs,
            });
        } else {
            kept.push(s);
        }
    }
    *timeline = kept;
}

/// Each report runs until a gap that is not paid in full.
fn reports(timeline: &[Span], set: &RuleSet) -> Vec<(u32, u32)> {
    let mut out = Vec::new();
    let mut start = None;
    let mut last = 0;
    for s in timeline {
        let breaks = s.activity == Activity::Gap && set.gap_paid_pct(s.to_secs - s.from_secs) < 100;
        if breaks {
            if let Some(from) = start.take() {
                out.push((from, last));
            }
            continue;
        }
        start.get_or_insert(s.from_secs);
        last = s.to_secs;
    }
    if let Some(from) = start {
        out.push((from, last));
    }
    out
}

fn share(s: &Span, set: &RuleSet) -> u64 {
    if s.activity == Activity::Gap {
        set.gap_paid_pct(s.to_secs - s.from_secs) as u64
    } else {
        100
    }
}

fn paid_within(timeline: &[Span], set: &RuleSet, counts: &[Activity], from: u32, to: u32) -> u32 {
    let hundredths: u64 = timeline
        .iter()
        .filter(|s| counts.contains(&s.activity))
        .map(|s| s.to_secs.min(to).saturating_sub(s.from_secs.max(from)) as u64 * share(s, set))
        .sum();
    (hundredths / 100) as u32
}

/// The base pay and the weighted pay, in seconds.
fn pay(
    rules: &WorkRules,
    set: &RuleSet,
    timeline: &[Span],
    day: Day,
    class: Option<&str>,
) -> (u32, u32) {
    let report = timeline.first().map_or(0, |s| s.from_secs);
    // Each premium as the window of instants it triggers in, and the
    // activities whose seconds it pays.
    let mut windows: Vec<(u32, u32, &[Activity], u32)> = Vec::new();
    for p in &set.premiums {
        let Some(pays) = rules.measures.get(&p.pays) else {
            continue;
        };
        let window = match p.on {
            TriggerOn::Measure => p
                .measure
                .as_ref()
                .and_then(|m| rules.measures.get(m))
                .and_then(|a| crossing(timeline, a, p.after_secs.unwrap_or(0)))
                .map(|at| (at, u32::MAX)),
            TriggerOn::Spread => p.after_secs.map(|a| (report + a, u32::MAX)),
            TriggerOn::TimeOfDay => p.from_secs.zip(p.to_secs),
            TriggerOn::DayOfWeek => p.days.contains(&day).then_some((0, u32::MAX)),
            TriggerOn::Class => {
                (p.class.as_deref() == class && class.is_some()).then_some((0, u32::MAX))
            }
        };
        if let Some((from, to)) = window {
            windows.push((from, to, pays.as_slice(), p.pct));
        }
    }
    let mut cuts: Vec<u32> = windows.iter().flat_map(|w| [w.0, w.1]).collect();
    let (mut base, mut weighted) = (0u64, 0u64);
    for s in timeline {
        let mut points: Vec<u32> = cuts
            .iter()
            .copied()
            .filter(|&c| c > s.from_secs && c < s.to_secs)
            .collect();
        points.push(s.from_secs);
        points.push(s.to_secs);
        points.sort();
        points.dedup();
        let share = share(s, set);
        for w in points.windows(2) {
            let (a, b) = (w[0], w[1]);
            let len = (b - a) as u64;
            let rates = windows
                .iter()
                .filter(|p| p.0 <= a && b <= p.1 && p.2.contains(&s.activity))
                .map(|p| p.3 as u64);
            let rate = match rules.combine {
                Combine::Highest => rates.max().unwrap_or(100).max(100),
                Combine::Sum => 100 + rates.map(|r| r.saturating_sub(100)).sum::<u64>(),
            };
            base += len * share;
            weighted += len * share * rate / 100;
        }
    }
    cuts.clear();
    ((base / 100) as u32, (weighted / 100) as u32)
}
