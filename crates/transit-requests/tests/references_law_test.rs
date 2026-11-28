//! Every reference field of the stop requests document declares its target
//! in the schema (D-232). A new reference field fails this test until it is
//! listed.

use transit_version::{references, Reference};

#[test]
fn every_reference_field_is_declared() {
    let schema =
        serde_json::to_value(schemars::schema_for!(transit_requests::StopRequests)).unwrap();
    assert_eq!(
        references(&schema),
        vec![
            Reference::new("/requests/*/attachments/*/hash", "attachment"),
            Reference::new("/requests/*/stop", "stop"),
            Reference::new("/requests/*/work_order/id", "external.work_order"),
        ]
    );
}
