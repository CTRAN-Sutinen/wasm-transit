//! The edit document's references (D-232): each field that names another
//! record or thing declares its target in the schema
//! (`x-references`). The kind table carries them, and the checks of
//! the record read them. A new reference field fails this law until it
//! is declared and listed here.

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
