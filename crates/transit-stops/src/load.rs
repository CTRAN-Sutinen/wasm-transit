//! The kernel's load of a stops document (D-239): its line in the kind list
//! (`crates/cap-abi/src/kinds.rs`) names this impl.

/// What `load_stops` answers: the hash, and how many stops the document holds.
#[derive(Debug, Clone, serde::Serialize)]
pub struct StopsLoaded {
    pub stops_hash: String,
    pub stops: u32,
}

/// Stops documents (every stop a list of dated events, D-153), refused
/// unless a write could store them, each refusal named, so a registered
/// document is always one the model reads.
impl transit_version::Load for crate::Stops {
    const ID: &'static str = "stops";
    const NAME: &'static str = "stops document";
    const LOAD_OP: &'static str = "load_stops";
    const CODE: u32 = 270;
    const FORM: transit_version::Form = transit_version::Form::Json { field: "stops" };
    type Answer = StopsLoaded;

    fn parse(bytes: &[u8]) -> Result<Self, String> {
        let document: crate::Stops = transit_version::parse_versioned(bytes).map_err(|e| format!("stops document: {e}"))?;
        document.checked().map_err(|refusals| {
            let reasons: Vec<String> = refusals.iter().map(|r| r.to_string()).collect();
            format!("stops document: {}", reasons.join("; "))
        })
    }

    fn canonical(&self) -> Option<Result<String, String>> {
        Some(transit_version::canonical_hash(self))
    }

    fn answer(&self, hash: String) -> StopsLoaded {
        StopsLoaded { stops_hash: hash, stops: self.stops.len() as u32 }
    }
}
