#[derive(Debug, Clone, serde::Serialize)]
pub struct WorkRulesLoaded {
    pub work_rules_hash: String,
    pub version: u32,
}

impl transit_version::Load for crate::WorkRules {
    const ID: &'static str = "work_rules";
    const NAME: &'static str = "work rules";
    const LOAD_OP: &'static str = "load_work_rules";
    const CODE: u32 = 269;
    const FORM: transit_version::Form = transit_version::Form::Json {
        field: "work_rules",
    };
    type Answer = WorkRulesLoaded;

    fn parse(bytes: &[u8]) -> Result<Self, transit_version::Refusal> {
        let document: crate::WorkRules =
            transit_version::parse_versioned(bytes).map_err(|e| e.context("work rules"))?;
        document
            .checked()
            .map_err(|e| format!("work rules: {e}").into())
    }

    fn canonical(&self) -> Option<Result<String, String>> {
        Some(transit_version::canonical_hash(self))
    }

    fn answer(&self, hash: String) -> WorkRulesLoaded {
        WorkRulesLoaded {
            work_rules_hash: hash,
            version: self.schema_version.0,
        }
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct SchedulingConstraintsLoaded {
    pub scheduling_constraints_hash: String,
    pub version: u32,
}

impl transit_version::Load for crate::SchedulingConstraints {
    const ID: &'static str = "scheduling_constraints";
    const NAME: &'static str = "scheduling constraints";
    const LOAD_OP: &'static str = "load_scheduling_constraints";
    const CODE: u32 = 275;
    const FORM: transit_version::Form = transit_version::Form::Json {
        field: "scheduling_constraints",
    };
    type Answer = SchedulingConstraintsLoaded;

    fn parse(bytes: &[u8]) -> Result<Self, transit_version::Refusal> {
        let document: crate::SchedulingConstraints = transit_version::parse_versioned(bytes)
            .map_err(|e| e.context("scheduling constraints"))?;
        document
            .checked()
            .map_err(|e| format!("scheduling constraints: {e}").into())
    }

    fn canonical(&self) -> Option<Result<String, String>> {
        Some(transit_version::canonical_hash(self))
    }

    fn answer(&self, hash: String) -> SchedulingConstraintsLoaded {
        SchedulingConstraintsLoaded {
            scheduling_constraints_hash: hash,
            version: self.schema_version.0,
        }
    }
}
