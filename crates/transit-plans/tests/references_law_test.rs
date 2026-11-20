//! The published plan set's references (D-232): each field that names another
//! record or thing declares its target in the schema
//! (`x-references`). The kind table carries them, and the checks of
//! the record read them. A new reference field fails this law until it
//! is declared and listed here.

use transit_version::{references, Reference};

#[test]
fn every_reference_field_is_declared() {
    let schema = serde_json::to_value(schemars::schema_for!(transit_plans::PublishedPlans)).unwrap();
    assert_eq!(
        references(&schema),
        vec![
            Reference::new("/plans/*/network_hash", "network"),
            Reference::new("/plans/*/stops_seq", "log.stops"),
        ]
    );
}

#[test]
fn every_frozen_version_reference_is_declared() {
    let schema = serde_json::to_value(schemars::schema_for!(transit_plans::FrozenVersions)).unwrap();
    assert_eq!(references(&schema), vec![Reference::new("/versions/*/network_hash", "network")]);
}
