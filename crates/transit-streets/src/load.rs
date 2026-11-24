//! The kernel's load of a walk table and a streets document (D-239). Its
//! line in the kind list (`crates/cap-abi/src/kinds.rs`) names this impl.

/// What `load_walks` answers.
#[derive(Debug, Clone, serde::Serialize)]
pub struct WalksLoaded {
    pub walks_hash: String,
}

/// Walk tables (cap-streets `walks` makes them).
impl transit_version::Load for crate::walking::WalkTable {
    const ID: &'static str = "walks";
    const NAME: &'static str = "walk table";
    const LOAD_OP: &'static str = "load_walks";
    const CODE: u32 = 262;
    const FORM: transit_version::Form = transit_version::Form::Json { field: "walks" };
    type Answer = WalksLoaded;

    fn parse(bytes: &[u8]) -> Result<Self, String> {
        transit_version::parse_versioned(bytes)
    }

    fn canonical(&self) -> Option<Result<String, String>> {
        Some(transit_version::canonical_hash(self))
    }

    fn answer(&self, hash: String) -> WalksLoaded {
        WalksLoaded { walks_hash: hash }
    }
}

/// What `load_streets` answers: the document's size as its graph will be:
/// its streets, their segments, and their distinct points.
#[derive(Debug, Clone, serde::Serialize)]
pub struct StreetsLoaded {
    pub streets_hash: String,
    pub ways: u32,
    pub segments: u32,
    pub nodes: u32,
}

/// Streets documents, refused unless valid, so a registered document
/// hashes as its import did.
impl transit_version::Load for crate::Streets {
    const ID: &'static str = "streets";
    const NAME: &'static str = "street network";
    const LOAD_OP: &'static str = "load_streets";
    const CODE: u32 = 263;
    const FORM: transit_version::Form = transit_version::Form::Json { field: "streets" };
    type Answer = StreetsLoaded;

    fn parse(bytes: &[u8]) -> Result<Self, String> {
        let document: crate::Streets = transit_version::parse_versioned(bytes).map_err(|e| format!("streets document: {e}"))?;
        document.validate().map_err(|e| format!("streets document: {}", e.0))?;
        Ok(document)
    }

    fn canonical(&self) -> Option<Result<String, String>> {
        Some(transit_version::canonical_hash(self))
    }

    fn answer(&self, hash: String) -> StreetsLoaded {
        let (ways, segments, nodes) = self.counts();
        StreetsLoaded { streets_hash: hash, ways, segments, nodes }
    }
}
