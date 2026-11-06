//! The work-rule document (D-50).

use serde::{Deserialize, Serialize};
use transit_network::RecoveryAt;
use ts_rs::TS;

/// The document version this build speaks.
pub const SCHEMA_VERSION: u32 = 1;

/// The rules work is cut under: generic, every rule optional, an
/// agency's values as data. Nothing unset is written, so an empty
/// document is `{"version":1}`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export, export_to = "transit_runs/")]
pub struct WorkRules {
    pub version: u32,
    /// What the agency calls this rule set ("Winter 2027 agreement").
    #[serde(default, skip_serializing_if = "String::is_empty")]
    #[ts(optional, as = "Option<String>")]
    pub name: String,
    /// The recovery a wait after a trip requires (D-9). The most
    /// specific rule answers: route and service type, then route, then
    /// service type, then neither. No rule applies → the terminal's
    /// floor (the smallest authored layover there).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[ts(optional, as = "Option<Vec<RecoveryRule>>")]
    pub recovery: Vec<RecoveryRule>,
    /// The route pairs one vehicle may run back to back (D-46: set per
    /// pair, either order). A route always follows itself; a pair not
    /// listed is not interlined.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[ts(optional, as = "Option<Vec<RoutePair>>")]
    pub interline: Vec<RoutePair>,
    /// The rules a run is cut under.
    #[serde(default, skip_serializing_if = "RunRules::is_empty")]
    #[ts(optional, as = "Option<RunRules>")]
    pub runs: RunRules,
    /// What a run is paid.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub pay: Option<Pay>,
}

/// One recovery requirement: the larger of `min_secs` and
/// `pct_of_run` percent of the run before the wait (rounded up to the
/// second). Scoped by route and/or service type; neither = every trip.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export, export_to = "transit_runs/")]
pub struct RecoveryRule {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub route: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub service_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub min_secs: Option<u32>,
    /// A whole percent of the run time (0–1000).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub pct_of_run: Option<u32>,
}

/// Two routes one vehicle may run back to back, either order.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export, export_to = "transit_runs/")]
pub struct RoutePair {
    pub a: String,
    pub b: String,
}

/// The rules a run is cut under — each optional; the runs cut reads
/// them (`docs/units/runs-layer.md`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export, export_to = "transit_runs/")]
pub struct RunRules {
    /// The longest a run may be on the platform (driving and waiting).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub max_platform_secs: Option<u32>,
    /// The longest from report to clear, breaks included.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub max_spread_secs: Option<u32>,
    /// Paid time before the first piece (sign-on, pull-out).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub report_secs: Option<u32>,
    /// Paid time after the last piece (pull-in, sign-off).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub clear_secs: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub min_piece_secs: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub max_piece_secs: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub meal: Option<MealBreak>,
    /// Where one operator may relieve another (stop ids). Empty: at
    /// the ends of a block's trips only.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[ts(optional, as = "Option<Vec<String>>")]
    pub relief_points: Vec<String>,
}

impl RunRules {
    fn is_empty(&self) -> bool {
        self == &RunRules::default()
    }
}

/// A meal break: at least `min_secs` off within the first
/// `within_secs` of work.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export, export_to = "transit_runs/")]
pub struct MealBreak {
    pub within_secs: u32,
    pub min_secs: u32,
}

/// What a run is paid, in time (D-51: no dollars — operators' rates
/// vary by seniority).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export, export_to = "transit_runs/")]
pub struct Pay {
    /// A run is paid at least this long.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub guarantee_secs: Option<u32>,
    /// Work past this is overtime, paid at `overtime_pct` percent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub overtime_after_secs: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub overtime_pct: Option<u32>,
    /// Spread past this earns a premium of `spread_pct` percent of the
    /// time past it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub spread_after_secs: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub spread_pct: Option<u32>,
}

impl RecoveryRule {
    fn applies(&self, at: &RecoveryAt) -> bool {
        self.route.as_deref().is_none_or(|r| r == at.route)
            && self.service_type.as_deref().is_none_or(|s| s == at.service_type)
    }

    fn specificity(&self) -> u8 {
        2 * u8::from(self.route.is_some()) + u8::from(self.service_type.is_some())
    }

    fn secs(&self, run_secs: u32) -> Option<u32> {
        let share = self.pct_of_run.map(|p| ((run_secs as u64 * p as u64).div_ceil(100)) as u32);
        self.min_secs.into_iter().chain(share).max()
    }
}

impl WorkRules {
    /// The document validated and normalized (rules and pairs sorted,
    /// each pair's routes in order) — what the kernel registers, so
    /// two orderings of one document are one identity. Refused: a
    /// version this build does not speak, a rule that requires
    /// nothing, two rules over one scope, a share over 1000 percent, a
    /// pair that is one route or listed twice, a minimum piece over
    /// the maximum.
    pub fn checked(mut self) -> Result<WorkRules, String> {
        if self.version != SCHEMA_VERSION {
            return Err(format!("work rules version {} (this build speaks version {SCHEMA_VERSION})", self.version));
        }
        for r in &self.recovery {
            if r.min_secs.is_none() && r.pct_of_run.is_none() {
                return Err(format!("recovery rule {} requires nothing (give min_secs and/or pct_of_run)", scope(r)));
            }
            if r.pct_of_run.is_some_and(|p| p > 1000) {
                return Err(format!("recovery rule {}: pct_of_run is a percent of the run, at most 1000", scope(r)));
            }
        }
        self.recovery.sort();
        if let Some(w) = self.recovery.windows(2).find(|w| w[0].route == w[1].route && w[0].service_type == w[1].service_type) {
            return Err(format!("recovery rule {} is given twice", scope(&w[0])));
        }
        for p in &mut self.interline {
            if p.a == p.b {
                return Err(format!("an interline pair names two routes ({} twice)", p.a));
            }
            if p.a > p.b {
                std::mem::swap(&mut p.a, &mut p.b);
            }
        }
        self.interline.sort();
        if let Some(w) = self.interline.windows(2).find(|w| w[0] == w[1]) {
            return Err(format!("interline pair {}–{} is given twice", w[0].a, w[0].b));
        }
        if let (Some(min), Some(max)) = (self.runs.min_piece_secs, self.runs.max_piece_secs) {
            if min > max {
                return Err(format!("min_piece_secs {min} is over max_piece_secs {max}"));
            }
        }
        Ok(self)
    }

    /// The recovery the most specific applicable rule requires after a
    /// trip, or `None` when no rule applies.
    pub fn recovery_secs(&self, at: &RecoveryAt) -> Option<u32> {
        self.recovery.iter().filter(|r| r.applies(at)).max_by_key(|r| r.specificity()).and_then(|r| r.secs(at.run_secs))
    }

    /// May one vehicle run route `b` right after route `a`?
    pub fn interlines(&self, a: &str, b: &str) -> bool {
        a == b || self.interline.iter().any(|p| (p.a == a && p.b == b) || (p.a == b && p.b == a))
    }
}

fn scope(r: &RecoveryRule) -> String {
    match (&r.route, &r.service_type) {
        (Some(route), Some(st)) => format!("for route {route} under {st}"),
        (Some(route), None) => format!("for route {route}"),
        (None, Some(st)) => format!("under {st}"),
        (None, None) => "for every trip".into(),
    }
}

/// The requirement `transit_network::validation_issues_under` takes:
/// the rules' when there are rules, else none (the terminal's floor
/// judges).
pub fn required(rules: Option<&WorkRules>) -> impl Fn(&RecoveryAt) -> Option<u32> + '_ {
    move |at| rules.and_then(|r| r.recovery_secs(at))
}
