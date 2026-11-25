#[derive(Debug, Clone, serde::Serialize)]
pub struct WalksLoaded {
    pub walks_hash: String,
}

impl transit_version::Load for crate::walking::WalkTable {
    const ID: &'static str = "walks";
    const NAME: &'static str = "walk table";
    const LOAD_OP: &'static str = "load_walks";
    const CODE: u32 = 262;
    const FORM: transit_version::Form = transit_version::Form::Json { field: "walks" };
    type Answer = WalksLoaded;

    fn parse(bytes: &[u8]) -> Result<Self, transit_version::Refusal> {
        transit_version::parse_versioned(bytes)
    }

    fn canonical(&self) -> Option<Result<String, String>> {
        Some(transit_version::canonical_hash(self))
    }

    fn answer(&self, hash: String) -> WalksLoaded {
        WalksLoaded { walks_hash: hash }
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct StreetsLoaded {
    pub streets_hash: String,
    pub ways: u32,
    pub segments: u32,
    pub nodes: u32,
}

impl transit_version::Load for crate::Streets {
    const ID: &'static str = "streets";
    const NAME: &'static str = "street network";
    const LOAD_OP: &'static str = "load_streets";
    const CODE: u32 = 263;
    const FORM: transit_version::Form = transit_version::Form::Json { field: "streets" };
    type Answer = StreetsLoaded;

    fn parse(bytes: &[u8]) -> Result<Self, transit_version::Refusal> {
        let document: crate::Streets = transit_version::parse_versioned(bytes).map_err(|e| e.context("streets document"))?;
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
