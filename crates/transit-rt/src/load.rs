//! The kernel's load of an rt snapshot (D-239): its line in the kind list
//! (`crates/cap-abi/src/kinds.rs`) names this impl.

/// What `load_rt` answers.
#[derive(Debug, Clone, serde::Serialize)]
pub struct RtLoaded {
    pub rt_hash: String,
}

/// Decoded GTFS-rt snapshots, by the hash of the payload. They are
/// short-lived: an agency makes them again every poll, and a live answer
/// reads at most three (trip updates, positions, alerts), so the last
/// eight stay.
impl transit_version::Load for crate::RtOverview {
    const ID: &'static str = "rt";
    const NAME: &'static str = "rt snapshot";
    const LOAD_OP: &'static str = "load_rt";
    const CODE: u32 = 267;
    const FORM: transit_version::Form = transit_version::Form::Bytes;
    const RETAIN: transit_version::Retain = transit_version::Retain::Last(8);
    type Answer = RtLoaded;

    fn parse(bytes: &[u8]) -> Result<Self, String> {
        crate::decode_overview(bytes)
    }

    fn canonical(&self) -> Option<Result<String, String>> {
        None
    }

    fn answer(&self, hash: String) -> RtLoaded {
        RtLoaded { rt_hash: hash }
    }
}
