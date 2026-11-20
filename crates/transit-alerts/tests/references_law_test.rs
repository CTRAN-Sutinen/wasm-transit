//! The service alerts document's references (D-232): each field that names another
//! record or thing declares its target in the schema
//! (`x-references`). The kind table carries them, and the checks of
//! the record read them. A new reference field fails this law until it
//! is declared and listed here.

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
