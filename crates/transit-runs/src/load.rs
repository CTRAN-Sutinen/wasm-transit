//! The kernel's load of work rules (D-239): its line in the kind list
//! (`crates/cap-abi/src/kinds.rs`) names this impl.

/// What `load_work_rules` answers: the hash, and the version the rules speak.
#[derive(Debug, Clone, serde::Serialize)]
pub struct WorkRulesLoaded {
    pub work_rules_hash: String,
    pub version: u32,
}

/// Work rules: the rules work is cut under, checked and normalized at the
/// load, so two orderings of one document are one identity.
impl transit_version::Load for crate::WorkRules {
    const ID: &'static str = "work_rules";
    const NAME: &'static str = "work rules";
    const LOAD_OP: &'static str = "load_work_rules";
    const CODE: u32 = 269;
    const FORM: transit_version::Form = transit_version::Form::Json { field: "work_rules" };
    type Answer = WorkRulesLoaded;

    fn parse(bytes: &[u8]) -> Result<Self, String> {
        let document: crate::WorkRules = transit_version::parse_versioned(bytes).map_err(|e| format!("work rules: {e}"))?;
        document.checked().map_err(|e| format!("work rules: {e}"))
    }

    fn canonical(&self) -> Option<Result<String, String>> {
        Some(transit_version::canonical_hash(self))
    }

    fn answer(&self, hash: String) -> WorkRulesLoaded {
        WorkRulesLoaded { work_rules_hash: hash, version: self.schema_version.0 }
    }
}
