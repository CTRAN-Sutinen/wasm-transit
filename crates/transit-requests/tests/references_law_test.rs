//! The stop requests document's references (D-232): each field that names another
//! record or thing declares its target in the schema
//! (`x-references`). The kind table carries them, and the checks of
//! the record read them. A new reference field fails this law until it
//! is declared and listed here.

use transit_version::{references, Reference};

#[test]
fn every_reference_field_is_declared() {
    let schema = serde_json::to_value(schemars::schema_for!(transit_requests::StopRequests)).unwrap();
    assert_eq!(
        references(&schema),
        vec![
            Reference::new("/requests/*/attachments/*/hash", "attachment"),
            Reference::new("/requests/*/routes/*", "network.route"),
            Reference::new("/requests/*/stop", "stop"),
            Reference::new("/requests/*/work_order/id", "external.work_order"),
        ]
    );
}
