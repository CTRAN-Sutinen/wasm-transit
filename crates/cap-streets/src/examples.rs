//! The street capability's worked examples (its `examples` feature): the
//! documents to load and each request. The generator runs them through
//! this crate's `invoke` and records the answers.

use cap_abi::manifest::Example;
use serde_json::json;

pub fn examples() -> Vec<Example> {
    let net = fixtures::network();
    // The walking fixture: a residential street through the station's
    // bays, a motorway across it no rider walks.
    let bytes = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../../web/e2e/fixtures/walk-streets.geojson")).expect("the walking fixture");
    let walking = transit_streets::geojson::import(&bytes).expect("the walking fixture imports");
    let walking_hash = cap_abi::canonical_hash(&walking).expect("a hash");
    let walk_loads = || vec![fixtures::load("load_streets", "streets", &walking), fixtures::load_network(&net.derived)];
    let ctx = fixtures::streets_example_context().expect("the street fixture");
    let street_loads = || vec![fixtures::load("load_streets", "streets", &ctx.streets)];
    vec![
        Example { bytes: None, kinds: vec![], op: "walks", loads: walk_loads(), request: json!({ "op": "walks", "streets_hash": walking_hash, "network_hash": net.network_hash }) },
        // From a dropped pin ~22 m off Station Rd, west of the bays, with
        // a destination east of them.
        Example {
            bytes: None, kinds: vec![],
            op: "access",
            loads: walk_loads(),
            request: json!({ "op": "access", "streets_hash": walking_hash, "network_hash": net.network_hash,
                             "point": { "lat": 45.5152, "lon": -73.5165 }, "to": { "lat": 45.5148, "lon": -73.5135 } }),
        },
        Example { bytes: None, kinds: vec![], op: "snap", loads: street_loads(), request: json!({ "op": "snap", "streets_hash": ctx.streets_hash, "lat": 45.5040, "lon": -73.5030 }) },
        Example { bytes: None, kinds: vec![], op: "route", loads: street_loads(), request: json!({ "op": "route", "streets_hash": ctx.streets_hash, "from": ctx.pos_a, "to": ctx.pos_b }) },
    ]
}
