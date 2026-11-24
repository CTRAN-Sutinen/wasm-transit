//! The kernel's load of a network (D-239): its line in the kind list
//! (`crates/cap-abi/src/kinds.rs`) names this impl.

/// What `load_network` answers: the network's hash, and its zone (its
/// operator's time zone) when it names one. A host reads its clock there
/// to give a date in the network's own day (a pick's "today", D-237).
#[derive(Debug, Clone, serde::Serialize)]
pub struct NetworkLoaded {
    pub network_hash: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timezone: Option<String>,
}

/// Networks: the document every model-side capability reads.
impl transit_version::Load for crate::Network {
    const ID: &'static str = "network";
    const NAME: &'static str = "network";
    const LOAD_OP: &'static str = "load_network";
    const CODE: u32 = 256;
    const FORM: transit_version::Form = transit_version::Form::Json { field: "network" };
    type Answer = NetworkLoaded;

    fn parse(bytes: &[u8]) -> Result<Self, String> {
        transit_version::parse_versioned(bytes)
    }

    fn canonical(&self) -> Option<Result<String, String>> {
        Some(transit_version::canonical_hash(self))
    }

    fn answer(&self, hash: String) -> NetworkLoaded {
        NetworkLoaded { network_hash: hash, timezone: self.timezone().map(str::to_string) }
    }
}
