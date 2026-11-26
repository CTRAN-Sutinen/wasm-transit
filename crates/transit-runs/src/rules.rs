use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
#[cfg(feature = "bindings")]
use ts_rs::TS;

pub use crate::version::SCHEMA_VERSION;

transit_version::kind_tag!(
    WorkRulesKind = "work_rules"
);

impl transit_version::Versioned for WorkRules {
    const KIND: &'static str = "work_rules";
    const READS: u32 = SCHEMA_VERSION;
    fn schema_version(&self) -> u32 {
        self.schema_version.0
    }
}

/// The rules of one labor agreement, or of one reading of it. Every number
/// and every measure is data, and every rule list may be empty. A rule's
/// `cites` names the clause it comes from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS, schemars::JsonSchema))]
#[cfg_attr(feature = "bindings", ts(export, export_to = "transit_runs/"))]
pub struct WorkRules {
    /// A copy without a kind tag reads as work rules.
    #[serde(default)]
    #[cfg_attr(feature = "bindings", ts(type = "\"work_rules\""))]
    pub kind: WorkRulesKind,
    /// A copy without a version is version 1, which this build refuses.
    #[serde(default)]
    #[cfg_attr(feature = "bindings", ts(type = "number"))]
    pub schema_version: transit_version::Version<1>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    #[cfg_attr(feature = "bindings", ts(optional, as = "Option<String>"))]
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub source: Option<Source>,
    /// Each measure is a named set of activities. A rule names the measure
    /// that it limits, pays, or counts, so two readings of "drive" or
    /// "work" differ only here.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    #[cfg_attr(feature = "bindings", ts(optional, as = "Option<BTreeMap<String, Vec<Activity>>>"))]
    pub measures: BTreeMap<String, Vec<Activity>>,
    #[serde(flatten)]
    pub rules: RuleSet,
    #[serde(default, skip_serializing_if = "Combine::is_highest")]
    #[cfg_attr(feature = "bindings", ts(optional, as = "Option<Combine>"))]
    pub combine: Combine,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[cfg_attr(feature = "bindings", ts(optional, as = "Option<Vec<RunClass>>"))]
    pub classes: Vec<RunClass>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[cfg_attr(feature = "bindings", ts(optional, as = "Option<Vec<MixRule>>"))]
    pub mix: Vec<MixRule>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub objective: Option<Objective>,
    #[serde(flatten)]
    #[cfg_attr(feature = "bindings", ts(skip))]
    pub extra: transit_version::Extra,
}

/// The rule lists that a document holds, and that a run class replaces or
/// adds to.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS, schemars::JsonSchema))]
#[cfg_attr(feature = "bindings", ts(export, export_to = "transit_runs/"))]
pub struct RuleSet {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[cfg_attr(feature = "bindings", ts(optional, as = "Option<Vec<Allowance>>"))]
    pub allowances: Vec<Allowance>,
    /// A gap's share paid is the band with the largest `from_secs` at or
    /// under the gap. With no bands, a gap is unpaid.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[cfg_attr(feature = "bindings", ts(optional, as = "Option<Vec<GapBand>>"))]
    pub gaps: Vec<GapBand>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[cfg_attr(feature = "bindings", ts(optional, as = "Option<Vec<Limit>>"))]
    pub limits: Vec<Limit>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[cfg_attr(feature = "bindings", ts(optional, as = "Option<Vec<BreakRule>>"))]
    pub breaks: Vec<BreakRule>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[cfg_attr(feature = "bindings", ts(optional, as = "Option<Vec<Premium>>"))]
    pub premiums: Vec<Premium>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[cfg_attr(feature = "bindings", ts(optional, as = "Option<Vec<Guarantee>>"))]
    pub guarantees: Vec<Guarantee>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[cfg_attr(feature = "bindings", ts(optional, as = "Option<Vec<TravelRule>>"))]
    pub travel: Vec<TravelRule>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS, schemars::JsonSchema))]
#[cfg_attr(feature = "bindings", ts(export, export_to = "transit_runs/"))]
pub struct Source {
    pub title: String,
    /// "YYYY-MM-DD". A cut for a date outside the period warns.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub effective_from: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub effective_to: Option<String>,
    #[serde(flatten)]
    #[cfg_attr(feature = "bindings", ts(skip))]
    pub extra: transit_version::Extra,
}

/// The run events of TODS, and `gap`, the time between two pieces.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS, schemars::JsonSchema))]
#[cfg_attr(feature = "bindings", ts(export, export_to = "transit_runs/"))]
#[serde(rename_all = "snake_case")]
pub enum Activity {
    Report,
    Clear,
    Revenue,
    Deadhead,
    Layover,
    Travel,
    Standby,
    Gap,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS, schemars::JsonSchema))]
#[cfg_attr(feature = "bindings", ts(export, export_to = "transit_runs/"))]
#[serde(rename_all = "snake_case")]
pub enum Event {
    PullOut,
    PullIn,
    ReliefOn,
    ReliefOff,
    PieceStart,
    PieceEnd,
}

/// Paid time added at an event, as the given activity.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS, schemars::JsonSchema))]
#[cfg_attr(feature = "bindings", ts(export, export_to = "transit_runs/"))]
pub struct Allowance {
    pub event: Event,
    pub secs: u32,
    pub activity: Activity,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub cites: Option<String>,
    #[serde(flatten)]
    #[cfg_attr(feature = "bindings", ts(skip))]
    pub extra: transit_version::Extra,
}

/// Gaps of at least `from_secs`, up to the next band, are paid at
/// `paid_pct` percent (0 to 100).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS, schemars::JsonSchema))]
#[cfg_attr(feature = "bindings", ts(export, export_to = "transit_runs/"))]
pub struct GapBand {
    pub from_secs: u32,
    pub paid_pct: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub cites: Option<String>,
    #[serde(flatten)]
    #[cfg_attr(feature = "bindings", ts(skip))]
    pub extra: transit_version::Extra,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS, schemars::JsonSchema))]
#[cfg_attr(feature = "bindings", ts(export, export_to = "transit_runs/"))]
#[serde(rename_all = "snake_case")]
pub enum Quantity {
    /// The total of the limit's `measure`, in seconds.
    Measure,
    /// From the first report to the last clear, in seconds.
    Spread,
    /// One piece's length, in seconds.
    Piece,
    /// The number of pieces.
    Pieces,
    /// One gap's length, in seconds.
    Gap,
    /// The days with work in the window, a count. A roster judges it.
    DaysWorked,
    /// The time off from one day's last clear to the next day's first
    /// report, in seconds. A roster judges it.
    Rest,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS, schemars::JsonSchema))]
#[cfg_attr(feature = "bindings", ts(export, export_to = "transit_runs/"))]
#[serde(rename_all = "snake_case")]
pub enum Window {
    #[default]
    Run,
    /// Any 24 hours. It spans days, so a roster judges it, not the cut.
    Rolling24h,
    Week,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS, schemars::JsonSchema))]
#[cfg_attr(feature = "bindings", ts(export, export_to = "transit_runs/"))]
#[serde(rename_all = "snake_case")]
pub enum Strength {
    /// A run that breaks it is not legal.
    #[default]
    Hard,
    /// A run that breaks it is legal and flagged, as when an operator may
    /// waive the limit.
    Waivable,
}

/// `min` and `max` are seconds, or a count for `pieces` and `days_worked`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS, schemars::JsonSchema))]
#[cfg_attr(feature = "bindings", ts(export, export_to = "transit_runs/"))]
pub struct Limit {
    pub quantity: Quantity,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub measure: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub min: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub max: Option<u32>,
    #[serde(default, skip_serializing_if = "is_default")]
    #[cfg_attr(feature = "bindings", ts(optional, as = "Option<Window>"))]
    pub over: Window,
    #[serde(default, skip_serializing_if = "is_default")]
    #[cfg_attr(feature = "bindings", ts(optional, as = "Option<Strength>"))]
    pub strength: Strength,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub cites: Option<String>,
    #[serde(flatten)]
    #[cfg_attr(feature = "bindings", ts(skip))]
    pub extra: transit_version::Extra,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS, schemars::JsonSchema))]
#[cfg_attr(feature = "bindings", ts(export, export_to = "transit_runs/"))]
#[serde(rename_all = "snake_case")]
pub enum BreakFrom {
    #[default]
    Report,
    /// The first minute of the break rule's `measure`.
    MeasureStart,
}

/// `count` breaks of at least `min_secs` each, inside the window from
/// `window_from_secs` to `window_to_secs` after `from`. The rule applies
/// only to a run whose `when_measure` totals more than `when_over_secs`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS, schemars::JsonSchema))]
#[cfg_attr(feature = "bindings", ts(export, export_to = "transit_runs/"))]
pub struct BreakRule {
    pub min_secs: u32,
    pub window_from_secs: u32,
    pub window_to_secs: u32,
    #[serde(default, skip_serializing_if = "is_default")]
    #[cfg_attr(feature = "bindings", ts(optional, as = "Option<BreakFrom>"))]
    pub from: BreakFrom,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub measure: Option<String>,
    #[serde(default = "one", skip_serializing_if = "is_one")]
    #[cfg_attr(feature = "bindings", ts(optional, as = "Option<u32>"))]
    pub count: u32,
    #[serde(default, skip_serializing_if = "is_default")]
    #[cfg_attr(feature = "bindings", ts(optional, as = "Option<bool>"))]
    pub paid: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub when_measure: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub when_over_secs: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub cites: Option<String>,
    #[serde(flatten)]
    #[cfg_attr(feature = "bindings", ts(skip))]
    pub extra: transit_version::Extra,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS, schemars::JsonSchema))]
#[cfg_attr(feature = "bindings", ts(export, export_to = "transit_runs/"))]
#[serde(rename_all = "snake_case")]
pub enum TriggerOn {
    /// The minutes of `measure` past its first `after_secs`.
    Measure,
    /// The minutes past `after_secs` of spread.
    Spread,
    /// The minutes from `from_secs` to `to_secs` after service-day midnight.
    TimeOfDay,
    /// Every minute on one of `days`.
    DayOfWeek,
    /// Every minute of a run in `class`.
    Class,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS, schemars::JsonSchema))]
#[cfg_attr(feature = "bindings", ts(export, export_to = "transit_runs/"))]
#[serde(rename_all = "snake_case")]
pub enum Day {
    Mon,
    Tue,
    Wed,
    Thu,
    Fri,
    Sat,
    Sun,
}

/// Pays the triggered minutes of the `pays` measure at `pct` percent of
/// the base rate (150 is time and a half, 0 to 1000).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS, schemars::JsonSchema))]
#[cfg_attr(feature = "bindings", ts(export, export_to = "transit_runs/"))]
pub struct Premium {
    pub on: TriggerOn,
    pub pays: String,
    pub pct: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub measure: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub after_secs: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub from_secs: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub to_secs: Option<u32>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[cfg_attr(feature = "bindings", ts(optional, as = "Option<Vec<Day>>"))]
    pub days: Vec<Day>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub class: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub cites: Option<String>,
    #[serde(flatten)]
    #[cfg_attr(feature = "bindings", ts(skip))]
    pub extra: transit_version::Extra,
}

/// Where premiums overlap on a minute: only the highest rate, or the
/// rates added.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS, schemars::JsonSchema))]
#[cfg_attr(feature = "bindings", ts(export, export_to = "transit_runs/"))]
#[serde(rename_all = "snake_case")]
pub enum Combine {
    #[default]
    Highest,
    Sum,
}

impl Combine {
    fn is_highest(&self) -> bool {
        *self == Combine::Highest
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS, schemars::JsonSchema))]
#[cfg_attr(feature = "bindings", ts(export, export_to = "transit_runs/"))]
#[serde(rename_all = "snake_case")]
pub enum Per {
    Run,
    Report,
    Day,
    Week,
}

/// At least `min_secs` paid per `per`, counting the `counts` measure.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS, schemars::JsonSchema))]
#[cfg_attr(feature = "bindings", ts(export, export_to = "transit_runs/"))]
pub struct Guarantee {
    pub per: Per,
    pub min_secs: u32,
    pub counts: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub cites: Option<String>,
    #[serde(flatten)]
    #[cfg_attr(feature = "bindings", ts(skip))]
    pub extra: transit_version::Extra,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS, schemars::JsonSchema))]
#[cfg_attr(feature = "bindings", ts(export, export_to = "transit_runs/"))]
#[serde(rename_all = "snake_case")]
pub enum TravelAt {
    DayEnd,
    BetweenPieces,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS, schemars::JsonSchema))]
#[cfg_attr(feature = "bindings", ts(export, export_to = "transit_runs/"))]
#[serde(rename_all = "snake_case")]
pub enum Timing {
    /// The wait and the ride on the next trip back.
    NextTrip,
    /// `fixed_secs`, however far.
    Fixed,
    /// The distance at `speed_kph`.
    Speed,
}

/// Pays the trip home when the place it applies at is more than
/// `beyond_m` meters from home. The trip ends within `within_m` meters of
/// home. Home is a place in the scheduling constraints, else where the
/// day began.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS, schemars::JsonSchema))]
#[cfg_attr(feature = "bindings", ts(export, export_to = "transit_runs/"))]
pub struct TravelRule {
    pub at: TravelAt,
    pub beyond_m: u32,
    pub within_m: u32,
    pub timed: Timing,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub fixed_secs: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub speed_kph: Option<u32>,
    /// A percent of the base rate (0 to 1000). With none, 100.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub pct: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub cites: Option<String>,
    #[serde(flatten)]
    #[cfg_attr(feature = "bindings", ts(skip))]
    pub extra: transit_version::Extra,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS, schemars::JsonSchema))]
#[cfg_attr(feature = "bindings", ts(export, export_to = "transit_runs/"))]
#[serde(rename_all = "snake_case")]
pub enum RuleKind {
    Allowances,
    Gaps,
    Limits,
    Breaks,
    Premiums,
    Guarantees,
    Travel,
}

/// A run in the class meets every limit in `conditions`. The class's
/// `rules` replace the document's list of each kind in `replaces`, and add
/// to the others. Gap bands always replace, because two sets of bands
/// would pay one gap twice.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS, schemars::JsonSchema))]
#[cfg_attr(feature = "bindings", ts(export, export_to = "transit_runs/"))]
pub struct RunClass {
    pub id: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[cfg_attr(feature = "bindings", ts(optional, as = "Option<Vec<Limit>>"))]
    pub conditions: Vec<Limit>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[cfg_attr(feature = "bindings", ts(optional, as = "Option<Vec<RuleKind>>"))]
    pub replaces: Vec<RuleKind>,
    #[serde(flatten)]
    pub rules: RuleSet,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub cites: Option<String>,
    #[serde(flatten)]
    #[cfg_attr(feature = "bindings", ts(skip))]
    pub extra: transit_version::Extra,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS, schemars::JsonSchema))]
#[cfg_attr(feature = "bindings", ts(export, export_to = "transit_runs/"))]
#[serde(rename_all = "snake_case")]
pub enum ShareOf {
    #[default]
    Runs,
    PaidTime,
}

/// Bounds the share of runs, or of paid time, in `classes`. Each percent
/// is 0 to 100. The cut reports a miss and does not refuse it.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS, schemars::JsonSchema))]
#[cfg_attr(feature = "bindings", ts(export, export_to = "transit_runs/"))]
pub struct MixRule {
    pub classes: Vec<String>,
    #[serde(default, skip_serializing_if = "is_default")]
    #[cfg_attr(feature = "bindings", ts(optional, as = "Option<ShareOf>"))]
    pub of: ShareOf,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub min_pct: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub target_pct: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub max_pct: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub cites: Option<String>,
    #[serde(flatten)]
    #[cfg_attr(feature = "bindings", ts(skip))]
    pub extra: transit_version::Extra,
}

/// The cut makes the most runs in `maximize`, then the least premium
/// time when `minimize_premium` is set.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS, schemars::JsonSchema))]
#[cfg_attr(feature = "bindings", ts(export, export_to = "transit_runs/"))]
pub struct Objective {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[cfg_attr(feature = "bindings", ts(optional, as = "Option<Vec<String>>"))]
    pub maximize: Vec<String>,
    #[serde(default, skip_serializing_if = "is_default")]
    #[cfg_attr(feature = "bindings", ts(optional, as = "Option<bool>"))]
    pub minimize_premium: bool,
    #[serde(flatten)]
    #[cfg_attr(feature = "bindings", ts(skip))]
    pub extra: transit_version::Extra,
}

fn is_default<T: Default + PartialEq>(v: &T) -> bool {
    *v == T::default()
}

fn one() -> u32 {
    1
}

fn is_one(v: &u32) -> bool {
    *v == 1
}

/// The current version. A missing `schema_version` reads as v1, so serde's
/// default for the field is not this.
impl Default for WorkRules {
    fn default() -> Self {
        WorkRules {
            kind: Default::default(),
            schema_version: transit_version::Version(SCHEMA_VERSION),
            name: String::new(),
            source: None,
            measures: BTreeMap::new(),
            rules: RuleSet::default(),
            combine: Combine::Highest,
            classes: Vec::new(),
            mix: Vec::new(),
            objective: None,
            extra: Default::default(),
        }
    }
}

impl RuleSet {
    fn sort(&mut self) {
        self.allowances.sort();
        self.gaps.sort();
        self.limits.sort();
        self.breaks.sort();
        self.premiums.sort();
        self.guarantees.sort();
        self.travel.sort();
        for p in &mut self.premiums {
            p.days.sort();
            p.days.dedup();
        }
    }

    fn has(&self, kind: RuleKind) -> bool {
        match kind {
            RuleKind::Allowances => !self.allowances.is_empty(),
            RuleKind::Gaps => !self.gaps.is_empty(),
            RuleKind::Limits => !self.limits.is_empty(),
            RuleKind::Breaks => !self.breaks.is_empty(),
            RuleKind::Premiums => !self.premiums.is_empty(),
            RuleKind::Guarantees => !self.guarantees.is_empty(),
            RuleKind::Travel => !self.travel.is_empty(),
        }
    }
}

fn twice<T: PartialEq>(list: &[T]) -> bool {
    list.windows(2).any(|w| w[0] == w[1])
}

fn pct(what: &str, value: u32, most: u32) -> Result<(), String> {
    if value > most {
        return Err(format!("{what} is {value} percent, at most {most}"));
    }
    Ok(())
}

struct Names<'a> {
    measures: &'a BTreeMap<String, Vec<Activity>>,
    classes: BTreeSet<&'a str>,
}

impl Names<'_> {
    fn measure(&self, what: &str, name: Option<&str>) -> Result<(), String> {
        match name {
            None => Err(format!("{what} names no measure")),
            Some(m) if self.measures.contains_key(m) => Ok(()),
            Some(m) => Err(format!("{what} names measure {m:?}, which the document does not define")),
        }
    }

    fn class(&self, what: &str, id: &str) -> Result<(), String> {
        if self.classes.contains(id) {
            Ok(())
        } else {
            Err(format!("{what} names class {id:?}, which the document does not define"))
        }
    }

    fn limit(&self, what: &str, l: &Limit) -> Result<(), String> {
        match (l.quantity, &l.measure) {
            (Quantity::Measure, m) => self.measure(what, m.as_deref())?,
            (_, Some(m)) => return Err(format!("{what} limits {:?} and names measure {m:?}, which only a measure limit takes", l.quantity)),
            _ => {}
        }
        if l.min.is_none() && l.max.is_none() {
            return Err(format!("{what} gives neither min nor max"));
        }
        if let (Some(min), Some(max)) = (l.min, l.max) {
            if min > max {
                return Err(format!("{what}: min {min} is over max {max}"));
            }
        }
        match (l.quantity, l.over) {
            (Quantity::DaysWorked | Quantity::Rest, Window::Run) => {
                return Err(format!("{what} spans days, so it is limited over a rolling 24 hours or a week, not a run"))
            }
            (Quantity::Piece | Quantity::Pieces | Quantity::Gap, Window::Rolling24h | Window::Week) => {
                return Err(format!("{what} is limited over a run only"))
            }
            _ => {}
        }
        Ok(())
    }

    fn rules(&self, at: &str, r: &RuleSet) -> Result<(), String> {
        for a in &r.allowances {
            if a.secs == 0 {
                return Err(format!("{at}: an allowance at {:?} adds no time", a.event));
            }
        }
        for g in &r.gaps {
            pct(&format!("{at}: the gap band from {} s", g.from_secs), g.paid_pct, 100)?;
        }
        if let Some(first) = r.gaps.first() {
            if first.from_secs != 0 {
                return Err(format!("{at}: the first gap band starts at {} s, not 0", first.from_secs));
            }
        }
        if let Some(w) = r.gaps.windows(2).find(|w| w[0].from_secs == w[1].from_secs) {
            return Err(format!("{at}: two gap bands start at {} s", w[0].from_secs));
        }
        for l in &r.limits {
            self.limit(&format!("{at}: a {:?} limit", l.quantity), l)?;
        }
        for b in &r.breaks {
            let what = format!("{at}: a break rule");
            if b.min_secs == 0 || b.count == 0 {
                return Err(format!("{what} requires no time off"));
            }
            if b.window_from_secs >= b.window_to_secs {
                return Err(format!("{what}: its window ends at or before it starts"));
            }
            match b.from {
                BreakFrom::MeasureStart => self.measure(&what, b.measure.as_deref())?,
                BreakFrom::Report if b.measure.is_some() => return Err(format!("{what} names a measure but counts from the report")),
                BreakFrom::Report => {}
            }
            match (&b.when_measure, b.when_over_secs) {
                (Some(m), Some(_)) => self.measure(&what, Some(m))?,
                (None, None) => {}
                _ => return Err(format!("{what} gives half a trigger (when_measure and when_over_secs go together)")),
            }
        }
        for p in &r.premiums {
            let what = format!("{at}: a {:?} premium", p.on);
            self.measure(&what, Some(&p.pays))?;
            pct(&what, p.pct, 1000)?;
            let given = (p.measure.is_some(), p.after_secs.is_some(), p.from_secs.is_some() || p.to_secs.is_some(), !p.days.is_empty(), p.class.is_some());
            let fits = match p.on {
                TriggerOn::Measure => given == (true, true, false, false, false),
                TriggerOn::Spread => given == (false, true, false, false, false),
                TriggerOn::TimeOfDay => given == (false, false, true, false, false) && p.from_secs.is_some() && p.to_secs.is_some(),
                TriggerOn::DayOfWeek => given == (false, false, false, true, false),
                TriggerOn::Class => given == (false, false, false, false, true),
            };
            if !fits {
                return Err(format!("{what} gives fields its trigger does not take, or lacks one it needs"));
            }
            if let Some(m) = &p.measure {
                self.measure(&what, Some(m))?;
            }
            if let (Some(from), Some(to)) = (p.from_secs, p.to_secs) {
                if from >= to {
                    return Err(format!("{what}: its window ends at or before it starts"));
                }
            }
            if let Some(c) = &p.class {
                self.class(&what, c)?;
            }
        }
        for g in &r.guarantees {
            let what = format!("{at}: a guarantee per {:?}", g.per);
            if g.min_secs == 0 {
                return Err(format!("{what} guarantees nothing"));
            }
            self.measure(&what, Some(&g.counts))?;
        }
        for t in &r.travel {
            let what = format!("{at}: a travel rule at {:?}", t.at);
            let fits = match t.timed {
                Timing::NextTrip => t.fixed_secs.is_none() && t.speed_kph.is_none(),
                Timing::Fixed => t.fixed_secs.is_some() && t.speed_kph.is_none(),
                Timing::Speed => t.speed_kph.is_some_and(|s| s > 0) && t.fixed_secs.is_none(),
            };
            if !fits {
                return Err(format!("{what} is timed {:?} but gives other timing fields", t.timed));
            }
            if let Some(p) = t.pct {
                pct(&what, p, 1000)?;
            }
        }
        for (name, list) in [
            ("allowance", twice(&r.allowances)),
            ("gap band", twice(&r.gaps)),
            ("limit", twice(&r.limits)),
            ("break rule", twice(&r.breaks)),
            ("premium", twice(&r.premiums)),
            ("guarantee", twice(&r.guarantees)),
            ("travel rule", twice(&r.travel)),
        ] {
            if list {
                return Err(format!("{at}: a {name} is given twice"));
            }
        }
        Ok(())
    }
}

impl WorkRules {
    /// Every list is sorted, so two orderings of one document are one
    /// identity. Refused: a version this build does not speak, a name that
    /// the document does not define, a rule that gives fields it does not
    /// take or lacks one it needs, and a rule given twice. Each refusal
    /// names the rule.
    pub fn checked(mut self) -> Result<WorkRules, String> {
        if self.schema_version.0 != SCHEMA_VERSION {
            return Err(format!("work rules version {} (this build speaks version {SCHEMA_VERSION})", self.schema_version.0));
        }
        if let Some(s) = &self.source {
            let from = s.effective_from.as_deref().map(transit_date::Date::read).transpose()?;
            let to = s.effective_to.as_deref().map(transit_date::Date::read).transpose()?;
            if let (Some(from), Some(to)) = (from, to) {
                if from > to {
                    return Err(format!("the source's period ends before it starts ({from} to {to})"));
                }
            }
        }
        for (name, activities) in &mut self.measures {
            if name.is_empty() {
                return Err("a measure has no name".into());
            }
            activities.sort();
            activities.dedup();
            if activities.is_empty() {
                return Err(format!("measure {name:?} holds no activity"));
            }
        }
        self.rules.sort();
        self.classes.sort_by(|a, b| a.id.cmp(&b.id));
        for c in &mut self.classes {
            if c.id.is_empty() {
                return Err("a run class has no id".into());
            }
            c.conditions.sort();
            c.replaces.sort();
            c.replaces.dedup();
            c.rules.sort();
        }
        if let Some(w) = self.classes.windows(2).find(|w| w[0].id == w[1].id) {
            return Err(format!("run class {:?} is given twice", w[0].id));
        }
        for m in &mut self.mix {
            m.classes.sort();
            m.classes.dedup();
        }
        self.mix.sort();
        if let Some(o) = &mut self.objective {
            o.maximize.sort();
            o.maximize.dedup();
        }

        let names = Names { measures: &self.measures, classes: self.classes.iter().map(|c| c.id.as_str()).collect() };
        names.rules("the document", &self.rules)?;
        for c in &self.classes {
            let at = format!("run class {:?}", c.id);
            for l in &c.conditions {
                names.limit(&format!("{at}: a {:?} condition", l.quantity), l)?;
            }
            if twice(&c.conditions) {
                return Err(format!("{at}: a condition is given twice"));
            }
            names.rules(&at, &c.rules)?;
            if let Some(kind) = c.replaces.iter().find(|&&k| k == RuleKind::Gaps && !c.rules.has(k)) {
                return Err(format!("{at} replaces the {kind:?} with none. To leave every gap unpaid, give a band at 0 s paid at 0 percent."));
            }
            names.rules(&format!("{at}, in force"), &self.rules_for(Some(&c.id)))?;
        }
        for m in &self.mix {
            let what = "a mix rule";
            if m.classes.is_empty() {
                return Err(format!("{what} names no class"));
            }
            for c in &m.classes {
                names.class(what, c)?;
            }
            if m.min_pct.is_none() && m.target_pct.is_none() && m.max_pct.is_none() {
                return Err(format!("{what} over {:?} bounds nothing", m.classes));
            }
            for p in [m.min_pct, m.target_pct, m.max_pct].into_iter().flatten() {
                pct(what, p, 100)?;
            }
            let given: Vec<u32> = [m.min_pct, m.target_pct, m.max_pct].into_iter().flatten().collect();
            if given.windows(2).any(|w| w[0] > w[1]) {
                return Err(format!("{what} over {:?}: min, target, and max are out of order", m.classes));
            }
        }
        if twice(&self.mix) {
            return Err("a mix rule is given twice".into());
        }
        if let Some(o) = &self.objective {
            for c in &o.maximize {
                names.class("the objective", c)?;
            }
        }
        Ok(self)
    }

    /// The rules in force for a run of `class`, or for a run in no class.
    /// A class's list of each kind in its `replaces` stands in for the
    /// document's list, and its other lists add to the document's.
    pub fn rules_for(&self, class: Option<&str>) -> RuleSet {
        let mut out = self.rules.clone();
        let Some(c) = class.and_then(|id| self.classes.iter().find(|c| c.id == id)) else {
            return out;
        };
        fn merge<T: Clone + Ord>(base: &mut Vec<T>, class: &[T], replace: bool) {
            if replace {
                base.clear();
            }
            base.extend(class.iter().cloned());
            base.sort();
            base.dedup();
        }
        let r = |k: RuleKind| c.replaces.contains(&k);
        merge(&mut out.allowances, &c.rules.allowances, r(RuleKind::Allowances));
        merge(&mut out.gaps, &c.rules.gaps, r(RuleKind::Gaps) || !c.rules.gaps.is_empty());
        merge(&mut out.limits, &c.rules.limits, r(RuleKind::Limits));
        merge(&mut out.breaks, &c.rules.breaks, r(RuleKind::Breaks));
        merge(&mut out.premiums, &c.rules.premiums, r(RuleKind::Premiums));
        merge(&mut out.guarantees, &c.rules.guarantees, r(RuleKind::Guarantees));
        merge(&mut out.travel, &c.rules.travel, r(RuleKind::Travel));
        out
    }
}

impl RuleSet {
    /// The percent of a gap of `gap_secs` that is paid: the band with the
    /// largest `from_secs` at or under the gap. With no bands, 0.
    pub fn gap_paid_pct(&self, gap_secs: u32) -> u32 {
        self.gaps.iter().rev().find(|b| b.from_secs <= gap_secs).map_or(0, |b| b.paid_pct)
    }
}
