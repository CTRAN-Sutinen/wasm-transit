//! The network document's version laws (D-62, D-66, D-115;
//! docs/units/document-versioning.md).
//!
//! * Every network written names its kind and its version, first:
//!   `{"kind":"network","schema_version":2,…}`.
//! * A document without a `schema_version` is version 1; one naming
//!   another kind is no network.
//! * `migrate_to(doc, N)` is the steps from the document's version to
//!   N applied in order (composition equals stepwise); every record id
//!   survives; every loss is a `MigrationNote`.
//! * Migrating a document already at (or past) the target is refused;
//!   a version newer than this build is refused naming both versions.
//!
//! The v1 shapes themselves (pre-trip, pre-alignment) are pinned in
//! trip_law_test.rs and alignment_law_test.rs, through
//! `migrate_network`.

use serde_json::{json, Value};
use transit_network::{migrate_network, migrate_step, migrate_to, schema_version_of, Network, SCHEMA_VERSION};

/// A small v1 document: the untagged current shape, with ids in every
/// keyed map the migration must keep.
fn v1() -> Value {
    json!({
        "name": "Sample",
        "stops": {
            "s1": {"name": "First", "lat": 45.0, "lon": -122.0},
            "s2": {"name": "Second", "lat": 45.01, "lon": -122.01}
        },
        "routes": {"r1": {"name": "1", "mode": "bus"}},
        "patterns": {},
        "service_types": {},
        "pattern_trips": {},
        "blocks": {}
    })
}

fn untag(doc: &mut Value) {
    let root = doc.as_object_mut().unwrap();
    root.remove("kind");
    root.remove("schema_version");
}

#[test]
fn a_written_network_names_its_kind_and_version_first() {
    let text = serde_json::to_string(&Network::default()).unwrap();
    assert!(
        text.starts_with(&format!("{{\"kind\":\"network\",\"schema_version\":{SCHEMA_VERSION},")),
        "{}",
        &text[..80.min(text.len())]
    );
}

#[test]
fn an_untagged_document_is_version_1() {
    assert_eq!(schema_version_of(&v1()), Ok(1));
    let read: Network = serde_json::from_value(v1()).unwrap();
    assert_eq!(read.schema_version.0, 1, "the type reads an untagged document as v1, so the kernel can refuse it");
}

#[test]
fn a_document_naming_another_kind_is_no_network() {
    let mut doc = v1();
    doc["kind"] = json!("edit_records");
    assert!(schema_version_of(&doc).unwrap_err().contains("not a network"));
    assert!(serde_json::from_value::<Network>(doc).unwrap_err().to_string().contains("not a network"));
}

#[test]
fn migration_is_the_steps_in_order() {
    let mut stepwise = v1();
    for from in 1..SCHEMA_VERSION {
        stepwise = migrate_step(stepwise, from).unwrap().0;
    }
    let (chained, _) = migrate_to(v1(), SCHEMA_VERSION).unwrap();
    assert_eq!(chained, stepwise);
    assert_eq!(schema_version_of(&chained), Ok(SCHEMA_VERSION));
}

#[test]
fn migration_keeps_every_id_and_reports_no_loss_it_did_not_make() {
    let (network, notes) = migrate_network(v1()).unwrap();
    assert_eq!(network.schema_version.0, SCHEMA_VERSION);
    assert_eq!(network.stops.keys().map(|k| k.as_str()).collect::<Vec<_>>(), ["s1", "s2"]);
    assert_eq!(network.routes.keys().map(|k| k.as_str()).collect::<Vec<_>>(), ["r1"]);
    assert!(notes.is_empty(), "v1 → v2 loses nothing: {notes:?}");
}

#[test]
fn a_current_network_untagged_migrates_to_itself() {
    let mut net = Network::default();
    net.name = "n".into();
    let mut doc = serde_json::to_value(&net).unwrap();
    untag(&mut doc);
    let (migrated, _) = migrate_network(doc).unwrap();
    assert_eq!(migrated, net);
}

#[test]
fn migrating_the_current_version_is_refused() {
    let doc = serde_json::to_value(Network::default()).unwrap();
    let err = migrate_to(doc, SCHEMA_VERSION).unwrap_err();
    assert!(err.contains("nothing to migrate"), "{err}");
}

#[test]
fn a_newer_version_is_refused_naming_both() {
    let mut doc = serde_json::to_value(Network::default()).unwrap();
    doc["schema_version"] = json!(SCHEMA_VERSION + 1);
    let err = migrate_to(doc, SCHEMA_VERSION + 1).unwrap_err();
    assert!(err.contains(&format!("v{}", SCHEMA_VERSION + 1)) && err.contains(&format!("v{SCHEMA_VERSION}")), "{err}");
}
