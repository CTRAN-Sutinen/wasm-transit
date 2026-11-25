#[derive(Debug, Clone, serde::Serialize)]
pub struct RtLoaded {
    pub rt_hash: String,
}

impl transit_version::Load for crate::RtOverview {
    const ID: &'static str = "rt";
    const NAME: &'static str = "rt snapshot";
    const LOAD_OP: &'static str = "load_rt";
    const CODE: u32 = 267;
    const FORM: transit_version::Form = transit_version::Form::Bytes;
    // A live answer reads at most three snapshots (trip updates, positions,
    // alerts), so the last eight stay.
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
