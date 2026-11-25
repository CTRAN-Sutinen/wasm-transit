#[derive(Debug, Clone, serde::Serialize)]
pub struct WatchlistLoaded {
    pub watchlist_hash: String,
    pub version: u32,
}

/// A copy from before the tags reads as the version it names.
impl transit_version::Load for crate::watchlist::Watchlist {
    const ID: &'static str = "watchlist";
    const NAME: &'static str = "watchlist";
    const LOAD_OP: &'static str = "load_watchlist";
    const CODE: u32 = 264;
    const FORM: transit_version::Form = transit_version::Form::Json { field: "document" };
    type Answer = WatchlistLoaded;

    fn parse(bytes: &[u8]) -> Result<Self, transit_version::Refusal> {
        transit_version::parse_versioned(bytes)
    }

    fn canonical(&self) -> Option<Result<String, String>> {
        Some(transit_version::canonical_hash(self))
    }

    fn answer(&self, hash: String) -> WatchlistLoaded {
        WatchlistLoaded { watchlist_hash: hash, version: transit_version::Versioned::schema_version(self) }
    }
}
