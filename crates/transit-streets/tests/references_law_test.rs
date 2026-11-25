//! Pins the references that the walk table's schema declares (`x-references`,
//! D-232). A new reference field fails until it is listed here.

use transit_version::{references, Reference};

#[test]
fn every_reference_field_is_declared() {
    let schema = serde_json::to_value(schemars::schema_for!(transit_streets::walking::WalkTable)).unwrap();
    assert_eq!(
        references(&schema),
        vec![
            Reference::new("/network_hash", "network"),
            Reference::new("/streets_hash", "streets"),
        ]
    );
}
