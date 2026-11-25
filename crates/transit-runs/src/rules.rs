
use serde::{Deserialize, Serialize};
use transit_network::RecoveryAt;
use ts_rs::TS;

pub const SCHEMA_VERSION: u32 = 1;

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

/// An empty document is `{"kind":"work_rules","schema_version":1}`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export, export_to = "transit_runs/")]
pub struct WorkRules {
    /// A copy without a kind tag reads as work rules (D-115).
    #[serde(default)]
    #[ts(type = "\"work_rules\"")]
    pub kind: WorkRulesKind,
    /// A copy without a version is version 1.
    #[serde(default)]
    #[ts(type = "number")]
    pub schema_version: transit_version::Version<1>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    #[ts(optional, as = "Option<String>")]
    pub name: String,
    /// The most specific rule answers. Specificity runs from route with
    /// service type, to route, to service type, to neither. With no rule,
    /// the terminal's floor applies, which is the smallest authored layover
    /// there (D-9).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[ts(optional, as = "Option<Vec<RecoveryRule>>")]
    pub recovery: Vec<RecoveryRule>,
    /// Set per pair, in either order (D-46). A route follows itself. An
    /// unlisted pair never interlines.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[ts(optional, as = "Option<Vec<RoutePair>>")]
    pub interline: Vec<RoutePair>,
    #[serde(default, skip_serializing_if = "RunRules::is_empty")]
    #[ts(optional, as = "Option<RunRules>")]
    pub runs: RunRules,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub pay: Option<Pay>,
    #[serde(flatten)]
    #[ts(skip)]
    pub extra: transit_version::Extra,
}

/// The larger of `min_secs` and `pct_of_run` percent of the run before the
/// wait, rounded up to the second. With no route or service type, the rule
/// covers every trip.
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
    /// A whole percent of the run time (0 to 1000).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub pct_of_run: Option<u32>,
    #[serde(flatten)]
    #[ts(skip)]
    pub extra: transit_version::Extra,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export, export_to = "transit_runs/")]
pub struct RoutePair {
    pub a: String,
    pub b: String,
    #[serde(flatten)]
    #[ts(skip)]
    pub extra: transit_version::Extra,
}

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
    /// Stop ids where one operator may relieve another. When empty, only at
    /// the ends of a block's trips.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[ts(optional, as = "Option<Vec<String>>")]
    pub relief_points: Vec<String>,
    #[serde(flatten)]
    #[ts(skip)]
    pub extra: transit_version::Extra,
}

impl RunRules {
    fn is_empty(&self) -> bool {
        self == &RunRules::default()
    }
}

/// At least `min_secs` off within the first `within_secs` of work.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export, export_to = "transit_runs/")]
pub struct MealBreak {
    pub within_secs: u32,
    pub min_secs: u32,
    #[serde(flatten)]
    #[ts(skip)]
    pub extra: transit_version::Extra,
}

/// Pay is in time, not dollars, because operators' rates vary by seniority
/// (D-51).
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
    #[serde(flatten)]
    #[ts(skip)]
    pub extra: transit_version::Extra,
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
    /// Rules and pairs are sorted, and each pair's routes are in order, so
    /// two orderings of one document are one identity. Refused: a version
    /// this build does not speak, a rule that requires nothing, two rules
    /// over one scope, a share over 1000 percent, a pair that is one route or
    /// listed twice, and a minimum piece over the maximum.
    pub fn checked(mut self) -> Result<WorkRules, String> {
        if self.schema_version.0 != SCHEMA_VERSION {
            return Err(format!("work rules version {} (this build speaks version {SCHEMA_VERSION})", self.schema_version.0));
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

    pub fn recovery_secs(&self, at: &RecoveryAt) -> Option<u32> {
        self.recovery.iter().filter(|r| r.applies(at)).max_by_key(|r| r.specificity()).and_then(|r| r.secs(at.run_secs))
    }

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

/// The requirement that `validation_issues_under` takes: the rules', else the
/// terminal's floor judges.
pub fn required(rules: Option<&WorkRules>) -> impl Fn(&RecoveryAt) -> Option<u32> + '_ {
    move |at| rules.and_then(|r| r.recovery_secs(at))
}
