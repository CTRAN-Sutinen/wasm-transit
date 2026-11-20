//! The stops document's references (D-232): each field that names another
//! record or thing declares its target in the schema
//! (`x-references`). The kind table carries them, and the checks of
//! the record read them. A new reference field fails this law until it
//! is declared and listed here.

use transit_version::{references, Reference};

#[test]
fn every_reference_field_is_declared() {
    let schema = serde_json::to_value(schemars::schema_for!(transit_stops::Stops)).unwrap();
    assert_eq!(
        references(&schema),
        vec![
            Reference::new("/stops/*/*/attachments/*/hash", "attachment"),
            Reference::new("/stops/*/*/from_feed", "feed"),
            Reference::new("/stops/*/*/parent", "stop"),
            Reference::new("/stops/*/*/replaces/*", "stop"),
            Reference::new("/stops/*/*/request", "stop_request"),
        ]
    );
}
