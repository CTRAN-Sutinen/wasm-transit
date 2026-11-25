#[derive(Debug, Clone, serde::Serialize)]
pub struct StopRequestsLoaded {
    pub requests_hash: String,
    pub requests: u32,
}

impl transit_version::Load for crate::StopRequests {
    const ID: &'static str = "stop_requests";
    const NAME: &'static str = "stop requests document";
    const LOAD_OP: &'static str = "load_stop_requests";
    const CODE: u32 = 271;
    const FORM: transit_version::Form = transit_version::Form::Json { field: "requests" };
    type Answer = StopRequestsLoaded;

    fn parse(bytes: &[u8]) -> Result<Self, transit_version::Refusal> {
        let document: crate::StopRequests = transit_version::parse_versioned(bytes).map_err(|e| e.context("stop requests document"))?;
        document.checked().map_err(|refusals| {
            let reasons: Vec<String> = refusals.iter().map(|r| r.to_string()).collect();
            format!("stop requests document: {}", reasons.join("; ")).into()
        })
    }

    fn canonical(&self) -> Option<Result<String, String>> {
        Some(transit_version::canonical_hash(self))
    }

    fn answer(&self, hash: String) -> StopRequestsLoaded {
        StopRequestsLoaded { requests_hash: hash, requests: self.requests.len() as u32 }
    }
}
