#[derive(Debug, Clone, serde::Serialize)]
pub struct StopsLoaded {
    pub stops_hash: String,
    pub stops: u32,
}

impl transit_version::Load for crate::Stops {
    const ID: &'static str = "stops";
    const NAME: &'static str = "stops document";
    const LOAD_OP: &'static str = "load_stops";
    const CODE: u32 = 270;
    const FORM: transit_version::Form = transit_version::Form::Json { field: "stops" };
    type Answer = StopsLoaded;

    fn parse(bytes: &[u8]) -> Result<Self, transit_version::Refusal> {
        let document: crate::Stops =
            transit_version::parse_versioned(bytes).map_err(|e| e.context("stops document"))?;
        document.checked().map_err(|refusals| {
            let reasons: Vec<String> = refusals.iter().map(|r| r.to_string()).collect();
            format!("stops document: {}", reasons.join("; ")).into()
        })
    }

    fn canonical(&self) -> Option<Result<String, String>> {
        Some(transit_version::canonical_hash(self))
    }

    fn answer(&self, hash: String) -> StopsLoaded {
        StopsLoaded {
            stops_hash: hash,
            stops: self.stops.len() as u32,
        }
    }
}
