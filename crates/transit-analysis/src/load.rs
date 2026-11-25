#[derive(Debug, Clone, serde::Serialize)]
pub struct LayerLoaded {
    pub layer_hash: String,
    pub features: u64,
    pub points: u64,
    pub polygons: u64,
    pub fields: Vec<crate::LayerField>,
}

/// The hash is of the bytes, because the coverage op chooses the weight
/// property later.
impl transit_version::Load for crate::WeightedLayer {
    const ID: &'static str = "layer";
    const NAME: &'static str = "layer";
    const LOAD_OP: &'static str = "load_layer";
    const CODE: u32 = 265;
    const FORM: transit_version::Form = transit_version::Form::Bytes;
    type Answer = LayerLoaded;

    fn parse(bytes: &[u8]) -> Result<Self, transit_version::Refusal> {
        Ok(crate::parse_layer(bytes)?)
    }

    fn canonical(&self) -> Option<Result<String, String>> {
        None
    }

    fn answer(&self, hash: String) -> LayerLoaded {
        LayerLoaded {
            layer_hash: hash,
            features: self.features() as u64,
            points: self.points() as u64,
            polygons: self.polygons() as u64,
            fields: self.fields().to_vec(),
        }
    }
}
