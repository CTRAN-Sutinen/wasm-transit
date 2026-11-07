//! The edit document's version laws (D-62, D-66, D-115;
//! docs/units/document-versioning.md).
//!
//! * The document names its kind and version, first:
//!   `{"kind":"edit_records","schema_version":2,"records":[…]}`.
//! * A bare array is version 1.
//! * `migrate_to` is the steps in order; every record survives, in
//!   order, byte for byte; every loss is a `MigrationNote`.
//! * Migrating the current version is refused; `read_document` reads
//!   the current version only and refuses an older or newer one naming
//!   both versions.

use serde_json::{json, Value};
use transit_sync::version::{migrate_step, migrate_to, schema_version_of};
use transit_sync::{read_document, EditDocument, SCHEMA_VERSION};

fn v1() -> Value {
    json!([
        {"id": "e1", "status": "accepted", "rev": 1, "updated_at": 1000,
         "cancel": {"id": "c1", "trip_id": "t1", "start_secs": 28800, "date": "2026-10-23"}},
        {"id": "e2", "status": "retired", "rev": 3, "updated_at": 2000,
         "cancel": {"id": "c2", "trip_id": "t2", "start_secs": 30000, "date": "2026-10-24"}}
    ])
}

#[test]
fn a_written_edit_document_names_its_kind_and_version_first() {
    let text = serde_json::to_string(&EditDocument::new(vec![])).unwrap();
    assert_eq!(text, format!("{{\"kind\":\"edit_records\",\"schema_version\":{SCHEMA_VERSION},\"records\":[]}}"));
}

#[test]
fn a_bare_array_is_version_1() {
    assert_eq!(schema_version_of(&v1()), Ok(1));
    assert!(schema_version_of(&json!({"kind": "network", "schema_version": 2})).unwrap_err().contains("not an edit document"));
}

#[test]
fn migration_is_the_steps_in_order_and_keeps_every_record() {
    let mut stepwise = v1();
    for from in 1..SCHEMA_VERSION {
        stepwise = migrate_step(stepwise, from).unwrap().0;
    }
    let (chained, notes) = migrate_to(v1(), SCHEMA_VERSION).unwrap();
    assert_eq!(chained, stepwise);
    assert_eq!(chained["records"], v1(), "every record, in order, unchanged");
    assert!(notes.is_empty(), "v1 → v2 loses nothing: {notes:?}");
    let (read, dropped) = read_document(&chained).unwrap();
    assert_eq!((read.records.len(), dropped), (2, 0));
}

#[test]
fn migrating_the_current_version_is_refused() {
    let doc = serde_json::to_value(EditDocument::new(vec![])).unwrap();
    assert!(migrate_to(doc, SCHEMA_VERSION).unwrap_err().contains("nothing to migrate"));
}

#[test]
fn reading_refuses_other_versions_naming_both() {
    let older = read_document(&v1()).unwrap_err();
    assert!(older.contains("edit_records v1") && older.contains(&format!("reads v{SCHEMA_VERSION}")), "{older}");
    let newer = read_document(&json!({"kind": "edit_records", "schema_version": SCHEMA_VERSION + 1, "records": []})).unwrap_err();
    assert!(newer.contains(&format!("v{}", SCHEMA_VERSION + 1)) && newer.contains(&format!("reads v{SCHEMA_VERSION}")), "{newer}");
}
