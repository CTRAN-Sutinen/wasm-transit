#[derive(Debug, Clone, serde::Serialize)]
pub struct BoundaryLoaded {
    pub boundary_hash: String,
}

impl transit_version::Load for crate::Boundary {
    const ID: &'static str = "boundary";
    const NAME: &'static str = "boundary";
    const LOAD_OP: &'static str = "load_boundary";
    const CODE: u32 = 261;
    const FORM: transit_version::Form = transit_version::Form::Json { field: "boundary" };
    type Answer = BoundaryLoaded;

    fn parse(bytes: &[u8]) -> Result<Self, transit_version::Refusal> {
        transit_version::parse_versioned(bytes)
    }

    fn canonical(&self) -> Option<Result<String, String>> {
        Some(transit_version::canonical_hash(self))
    }

    fn answer(&self, hash: String) -> BoundaryLoaded {
        BoundaryLoaded {
            boundary_hash: hash,
        }
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct FeedLoaded {
    pub feed_hash: String,
    pub summary: crate::FeedSummary,
}

impl transit_version::Load for crate::Feed {
    const ID: &'static str = "feed";
    const NAME: &'static str = "feed";
    const LOAD_OP: &'static str = "load_feed";
    const CODE: u32 = 266;
    const FORM: transit_version::Form = transit_version::Form::Bytes;
    type Answer = FeedLoaded;

    fn parse(bytes: &[u8]) -> Result<Self, transit_version::Refusal> {
        crate::Feed::from_zip_bytes(bytes).map_err(|e| format!("parse: {e}").into())
    }

    fn canonical(&self) -> Option<Result<String, String>> {
        None
    }

    fn answer(&self, hash: String) -> FeedLoaded {
        FeedLoaded {
            feed_hash: hash,
            summary: self.summary(),
        }
    }
}
