use serde::{Deserialize, Serialize};
use transit_network::RecoveryAt;
#[cfg(feature = "bindings")]
use ts_rs::TS;

pub const CONSTRAINTS_VERSION: u32 = 1;

transit_version::kind_tag!(
    SchedulingConstraintsKind = "scheduling_constraints"
);

impl transit_version::Versioned for SchedulingConstraints {
    const KIND: &'static str = "scheduling_constraints";
    const READS: u32 = CONSTRAINTS_VERSION;
    fn schema_version(&self) -> u32 {
        self.schema_version.0
    }
}

/// An empty document is `{"kind":"scheduling_constraints","schema_version":1}`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS, schemars::JsonSchema))]
#[cfg_attr(feature = "bindings", ts(export, export_to = "transit_runs/"))]
pub struct SchedulingConstraints {
    /// A copy without a kind tag reads as scheduling constraints.
    #[serde(default)]
    #[cfg_attr(feature = "bindings", ts(type = "\"scheduling_constraints\""))]
    pub kind: SchedulingConstraintsKind,
    /// A copy without a version is version 1.
    #[serde(default)]
    #[cfg_attr(feature = "bindings", ts(type = "number"))]
    pub schema_version: transit_version::Version<1>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    #[cfg_attr(feature = "bindings", ts(optional, as = "Option<String>"))]
    pub name: String,
    /// The most specific rule answers. Specificity runs from route with
    /// service type, to route, to service type, to neither. With no rule,
    /// the terminal's floor applies, which is the smallest authored layover
    /// there.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[cfg_attr(feature = "bindings", ts(optional, as = "Option<Vec<RecoveryRule>>"))]
    pub recovery: Vec<RecoveryRule>,
    /// Set per pair, in either order. A route follows itself. An
    /// unlisted pair never interlines.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[cfg_attr(feature = "bindings", ts(optional, as = "Option<Vec<RoutePair>>"))]
    pub interline: Vec<RoutePair>,
    /// Stop ids where one operator may relieve another. When empty, only at
    /// the ends of a block's trips.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[cfg_attr(feature = "bindings", ts(optional, as = "Option<Vec<String>>"))]
    pub relief_points: Vec<String>,
    #[serde(flatten)]
    #[cfg_attr(feature = "bindings", ts(skip))]
    pub extra: transit_version::Extra,
}

/// The larger of `min_secs` and `pct_of_run` percent of the run before the
/// wait, rounded up to the second. With no route or service type, the rule
/// covers every trip.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS, schemars::JsonSchema))]
#[cfg_attr(feature = "bindings", ts(export, export_to = "transit_runs/"))]
pub struct RecoveryRule {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub route: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub service_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub min_secs: Option<u32>,
    /// A whole percent of the run time (0 to 1000).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub pct_of_run: Option<u32>,
    #[serde(flatten)]
    #[cfg_attr(feature = "bindings", ts(skip))]
    pub extra: transit_version::Extra,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS, schemars::JsonSchema))]
#[cfg_attr(feature = "bindings", ts(export, export_to = "transit_runs/"))]
pub struct RoutePair {
    pub a: String,
    pub b: String,
    #[serde(flatten)]
    #[cfg_attr(feature = "bindings", ts(skip))]
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

impl SchedulingConstraints {
    /// Rules, pairs, and relief points are sorted, and each pair's routes
    /// are in order, so two orderings of one document are one identity.
    /// Refused: a version this build does not speak, a rule that requires
    /// nothing, two rules over one scope, a share over 1000 percent, a pair
    /// that is one route or listed twice, and a relief point listed twice.
    pub fn checked(mut self) -> Result<SchedulingConstraints, String> {
        if self.schema_version.0 != CONSTRAINTS_VERSION {
            return Err(format!(
                "scheduling constraints version {} (this build speaks version {CONSTRAINTS_VERSION})",
                self.schema_version.0
            ));
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
        self.relief_points.sort();
        if let Some(w) = self.relief_points.windows(2).find(|w| w[0] == w[1]) {
            return Err(format!("relief point {} is given twice", w[0]));
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

/// The requirement that `validation_issues_under` takes: the constraints',
/// else the terminal's floor judges.
pub fn required(constraints: Option<&SchedulingConstraints>) -> impl Fn(&RecoveryAt) -> Option<u32> + '_ {
    move |at| constraints.and_then(|c| c.recovery_secs(at))
}
