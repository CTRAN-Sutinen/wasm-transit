//! The network's entry in the kind list
//! (`crates/cap-abi/src/kinds.rs`) names this impl (D-239).

/// `timezone` is the time zone of the network's operator. A host reads its
/// clock in that zone to get the date of the network (the "today" of a pick,
/// D-237).
#[derive(Debug, Clone, serde::Serialize)]
pub struct NetworkLoaded {
    pub network_hash: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timezone: Option<String>,
}

impl transit_version::Load for crate::Network {
    const ID: &'static str = "network";
    const NAME: &'static str = "network";
    const LOAD_OP: &'static str = "load_network";
    const CODE: u32 = 256;
    const FORM: transit_version::Form = transit_version::Form::Json { field: "network" };
    type Answer = NetworkLoaded;

    fn parse(bytes: &[u8]) -> Result<Self, transit_version::Refusal> {
        transit_version::parse_versioned(bytes)
    }

    fn canonical(&self) -> Option<Result<String, String>> {
        Some(transit_version::canonical_hash(self))
    }

    fn answer(&self, hash: String) -> NetworkLoaded {
        NetworkLoaded { network_hash: hash, timezone: self.timezone().map(str::to_string) }
    }
}
