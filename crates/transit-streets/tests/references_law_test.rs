//! The walk table's references (D-232). Each field that names another
//! record or thing declares its target in the schema (`x-references`).
//! The kind table carries them. The record checks read them. A new
//! reference field fails this law until it is declared and listed here.

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
