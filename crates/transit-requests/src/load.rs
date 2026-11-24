//! The kernel's load of a stop requests document (D-239). Its line in the
//! kind list (`crates/cap-abi/src/kinds.rs`) names this impl.

/// What `load_stop_requests` answers: the hash, and how many requests the
/// document holds.
#[derive(Debug, Clone, serde::Serialize)]
pub struct StopRequestsLoaded {
    pub requests_hash: String,
    pub requests: u32,
}

/// Stop requests documents (the planning team's requests for work at a
/// stop, each tracking its work order, D-159), refused unless a write
/// could store them, each refusal named.
impl transit_version::Load for crate::StopRequests {
    const ID: &'static str = "stop_requests";
    const NAME: &'static str = "stop requests document";
    const LOAD_OP: &'static str = "load_stop_requests";
    const CODE: u32 = 271;
    const FORM: transit_version::Form = transit_version::Form::Json { field: "requests" };
    type Answer = StopRequestsLoaded;

    fn parse(bytes: &[u8]) -> Result<Self, String> {
        let document: crate::StopRequests = transit_version::parse_versioned(bytes).map_err(|e| format!("stop requests document: {e}"))?;
        document.checked().map_err(|refusals| {
            let reasons: Vec<String> = refusals.iter().map(|r| r.to_string()).collect();
            format!("stop requests document: {}", reasons.join("; "))
        })
    }

    fn canonical(&self) -> Option<Result<String, String>> {
        Some(transit_version::canonical_hash(self))
    }

    fn answer(&self, hash: String) -> StopRequestsLoaded {
        StopRequestsLoaded { requests_hash: hash, requests: self.requests.len() as u32 }
    }
}
