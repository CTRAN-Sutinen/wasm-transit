//! Each field that names another record declares its target in the schema
//! (`x-references`, D-232). A new reference field fails this test until it is
//! declared and listed here.

use transit_version::{references, Reference};

#[test]
fn every_reference_field_is_declared() {
    let schema = serde_json::to_value(schemars::schema_for!(transit_alerts::ServiceAlerts)).unwrap();
    assert_eq!(
        references(&schema),
        vec![
            Reference::new("/alerts/*/explains/*[kind=edit]/id", "edit_record"),
            Reference::new("/alerts/*/explains/*[kind=stop]/stop", "stop"),
            Reference::new("/alerts/*/explains/*[kind=trip]", "trip_instance"),
            Reference::new("/alerts/*/informed/*/route", "network.route"),
            Reference::new("/alerts/*/informed/*/stop", "network.stop"),
            Reference::new("/alerts/*/informed/*/trip", "network.trip"),
        ]
    );
}

#[test]
fn an_alert_template_names_nothing() {
    let schema = serde_json::to_value(schemars::schema_for!(transit_alerts::AlertTemplates)).unwrap();
    assert!(references(&schema).is_empty());
}
