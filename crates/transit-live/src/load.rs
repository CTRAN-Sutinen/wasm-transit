//! The kernel's load of a live document (D-239). Its line in the kind list
//! (`crates/cap-abi/src/kinds.rs`) names this impl.

/// What `load_live` answers.
#[derive(Debug, Clone, serde::Serialize)]
pub struct LiveLoaded {
    pub live_hash: String,
}

/// Live documents are short-lived. They are made again every poll. The last
/// four are kept (D-70).
impl transit_version::Load for crate::LiveDay {
    const ID: &'static str = "live";
    const NAME: &'static str = "live document";
    const LOAD_OP: &'static str = "load_live";
    const CODE: u32 = 260;
    const FORM: transit_version::Form = transit_version::Form::Json { field: "live" };
    const RETAIN: transit_version::Retain = transit_version::Retain::Last(4);
    type Answer = LiveLoaded;

    fn parse(bytes: &[u8]) -> Result<Self, String> {
        transit_version::parse_versioned(bytes)
    }

    fn canonical(&self) -> Option<Result<String, String>> {
        Some(transit_version::canonical_hash(self))
    }

    fn answer(&self, hash: String) -> LiveLoaded {
        LiveLoaded { live_hash: hash }
    }
}
