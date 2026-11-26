
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

/// An empty document is `{"kind":"work_rules","schema_version":2}`.
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
    #[serde(default, skip_serializing_if = "RunRules::is_empty")]
    #[cfg_attr(feature = "bindings", ts(optional, as = "Option<RunRules>"))]
    pub runs: RunRules,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub pay: Option<Pay>,
    #[serde(flatten)]
    #[cfg_attr(feature = "bindings", ts(skip))]
    pub extra: transit_version::Extra,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS, schemars::JsonSchema))]
#[cfg_attr(feature = "bindings", ts(export, export_to = "transit_runs/"))]
pub struct RunRules {
    /// The longest a run may be on the platform (driving and waiting).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub max_platform_secs: Option<u32>,
    /// The longest from report to clear, breaks included.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub max_spread_secs: Option<u32>,
    /// Paid time before the first piece (sign-on, pull-out).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub report_secs: Option<u32>,
    /// Paid time after the last piece (pull-in, sign-off).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub clear_secs: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub min_piece_secs: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub max_piece_secs: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub meal: Option<MealBreak>,
    #[serde(flatten)]
    #[cfg_attr(feature = "bindings", ts(skip))]
    pub extra: transit_version::Extra,
}

impl RunRules {
    fn is_empty(&self) -> bool {
        self == &RunRules::default()
    }
}

/// At least `min_secs` off within the first `within_secs` of work.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS, schemars::JsonSchema))]
#[cfg_attr(feature = "bindings", ts(export, export_to = "transit_runs/"))]
pub struct MealBreak {
    pub within_secs: u32,
    pub min_secs: u32,
    #[serde(flatten)]
    #[cfg_attr(feature = "bindings", ts(skip))]
    pub extra: transit_version::Extra,
}

/// Pay is in time, not dollars, because operators' rates vary by seniority.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "bindings", derive(TS, schemars::JsonSchema))]
#[cfg_attr(feature = "bindings", ts(export, export_to = "transit_runs/"))]
pub struct Pay {
    /// A run is paid at least this long.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub guarantee_secs: Option<u32>,
    /// Work past this is overtime, paid at `overtime_pct` percent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub overtime_after_secs: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub overtime_pct: Option<u32>,
    /// Spread past this earns a premium of `spread_pct` percent of the
    /// time past it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub spread_after_secs: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "bindings", ts(optional))]
    pub spread_pct: Option<u32>,
    #[serde(flatten)]
    #[cfg_attr(feature = "bindings", ts(skip))]
    pub extra: transit_version::Extra,
}

/// The current version. A missing `schema_version` reads as v1, so serde's
/// default for the field is not this.
impl Default for WorkRules {
    fn default() -> Self {
        WorkRules {
            kind: Default::default(),
            schema_version: transit_version::Version(SCHEMA_VERSION),
            name: String::new(),
            runs: RunRules::default(),
            pay: None,
            extra: Default::default(),
        }
    }
}

impl WorkRules {
    /// Refused: a version this build does not speak, and a minimum piece
    /// over the maximum.
    pub fn checked(self) -> Result<WorkRules, String> {
        if self.schema_version.0 != SCHEMA_VERSION {
            return Err(format!("work rules version {} (this build speaks version {SCHEMA_VERSION})", self.schema_version.0));
        }
        if let (Some(min), Some(max)) = (self.runs.min_piece_secs, self.runs.max_piece_secs) {
            if min > max {
                return Err(format!("min_piece_secs {min} is over max_piece_secs {max}"));
            }
        }
        Ok(self)
    }
}
