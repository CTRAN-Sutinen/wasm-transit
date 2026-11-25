//! Every reference field of the edit document is declared in the schema
//! (`x-references`) and listed here (D-232).

use transit_version::{references, Reference};

#[test]
fn every_reference_field_is_declared() {
    let schema = serde_json::to_value(schemars::schema_for!(transit_sync::EditDocument)).unwrap();
    assert_eq!(
        references(&schema),
        vec![
            Reference::new("/records/*/cancel/trip_id", "network.trip"),
            Reference::new("/records/*/diversion/first_kept_stop", "network.stop"),
            Reference::new("/records/*/diversion/last_kept_stop", "network.stop"),
            Reference::new("/records/*/diversion/route_id", "network.route"),
            Reference::new("/records/*/diversion/skipped_stops/*", "network.stop"),
        ]
    );
}
