//! Pins the references that the notes document's schema declares
//! (`x-references`, D-232). A new reference field fails until it is listed
//! here.

use transit_version::{references, Reference};

#[test]
fn every_reference_field_is_declared() {
    let schema = serde_json::to_value(schemars::schema_for!(transit_notes::Notes)).unwrap();
    assert_eq!(
        references(&schema),
        vec![
            Reference::new("/notes/*/target[kind=alert]/id", "service_alert"),
            Reference::new("/notes/*/target[kind=edit]/id", "edit_record"),
            Reference::new("/notes/*/target[kind=stop]/stop", "stop"),
            Reference::new("/notes/*/target[kind=trip]", "trip_instance"),
        ]
    );
}
