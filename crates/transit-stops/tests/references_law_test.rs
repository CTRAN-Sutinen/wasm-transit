//! Every reference field of the stops document declares its target in the
//! schema (D-232). A new reference field fails this test until it is listed.

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
