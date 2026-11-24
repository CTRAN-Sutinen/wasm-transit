//! The kernel's load of a weighted layer (D-239): its line in the kind list
//! (`crates/cap-abi/src/kinds.rs`) names this impl.

/// What `load_layer` answers: what the layer holds and the weights it can be
/// read by (each numeric property, with its total). It is the whole answer
/// the layer's import shows; a coverage op names the field.
#[derive(Debug, Clone, serde::Serialize)]
pub struct LayerLoaded {
    pub layer_hash: String,
    pub features: u64,
    pub points: u64,
    pub polygons: u64,
    pub fields: Vec<crate::LayerField>,
}

/// Weighted layers (GeoJSON points and polygons: people, jobs, households;
/// D-47), by the hash of their bytes. Which property is the weight is the
/// coverage op's question, so the bytes are the whole document.
impl transit_version::Load for crate::WeightedLayer {
    const ID: &'static str = "layer";
    const NAME: &'static str = "layer";
    const LOAD_OP: &'static str = "load_layer";
    const CODE: u32 = 265;
    const FORM: transit_version::Form = transit_version::Form::Bytes;
    type Answer = LayerLoaded;

    fn parse(bytes: &[u8]) -> Result<Self, String> {
        crate::parse_layer(bytes)
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
