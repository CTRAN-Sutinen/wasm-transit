//! The kernel's load of an edit document (D-239): its line in the kind list
//! (`crates/cap-abi/src/kinds.rs`) names this impl.

/// What `load_edits` answers.
#[derive(Debug, Clone, serde::Serialize)]
pub struct EditsLoaded {
    pub edits_hash: String,
}

/// Operator edit documents, which the host replaces whole, in the version
/// this build reads. An older or newer one is refused at the load, naming
/// both versions. Validation against a network is a question (cap-divert
/// `reports`), never the load's.
impl transit_version::Load for crate::EditDocument {
    const ID: &'static str = "edits";
    const NAME: &'static str = "edits";
    const LOAD_OP: &'static str = "load_edits";
    const CODE: u32 = 259;
    const FORM: transit_version::Form = transit_version::Form::Json { field: "edits" };
    type Answer = EditsLoaded;

    fn parse(bytes: &[u8]) -> Result<Self, String> {
        transit_version::parse_versioned(bytes)
    }

    fn canonical(&self) -> Option<Result<String, String>> {
        Some(transit_version::canonical_hash(self))
    }

    fn answer(&self, hash: String) -> EditsLoaded {
        EditsLoaded { edits_hash: hash }
    }
}
