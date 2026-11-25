//! Every reference field of the published plan set is declared in the schema
//! (`x-references`) and listed here (D-232).

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
    assert_eq!(
        references(&schema),
        vec![Reference::new("/versions/*/from_network_hash", "network"), Reference::new("/versions/*/network_hash", "network")]
    );
}
