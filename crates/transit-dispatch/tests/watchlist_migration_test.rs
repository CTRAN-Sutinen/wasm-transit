//! The watchlist's laws. The personal document is the migration
//! mechanism's real payload, so its chain is property-tested here (the
//! library owns the document; cap-migrate wraps the chain):
//!
//! * **Identity** — the stop-id sequence is invariant across
//!   versions. This is the law that keeps sync sound: LWW-by-record-id
//!   merges stay well-defined across clients at different document
//!   versions only if migrations never touch the key space.
//! * **No silent loss** — every dropped color is counted in a
//!   MigrationNote. Never silent, always reported.
//! * **Determinism** — the same input produces the same output bytes,
//!   on every host (the sync thesis needs this).
//! * **Totality of the seam** — a current input is an error, never a
//!   panic.
//!
//! And the tags (D-115): the current version writes `kind` and
//! `schema_version`. A copy written before the tags named its version as
//! `"version": "vN"`. Such a copy of v2 reads as v2 and writes tagged,
//! and one of v1 does not read as the current type (it comes forward
//! through the chain).

use proptest::prelude::*;
use proptest::strategy::BoxedStrategy;
use serde_json::{json, Value};

use transit_dispatch::watchlist::{migrate_to, schema_version_of, Watchlist, SCHEMA_VERSION};
use transit_version::Versioned;

/// One v1 stop: a note and an optional color.
fn gen_stop_v1() -> BoxedStrategy<Value> {
    (
        0u8..6, // id pool small enough for duplicates
        proptest::collection::vec(any::<char>().prop_filter("printable", |c| c.is_ascii_alphanumeric()), 0..=8),
        proptest::option::of(proptest::collection::vec(any::<char>().prop_filter("printable", |c| c.is_ascii_alphabetic()), 3..=6)),
    )
        .prop_map(|(id, note, color)| {
            let mut stop = json!({"id": format!("s{id}"), "note": note.into_iter().collect::<String>()});
            if let Some(color) = color {
                stop["color"] = json!(color.into_iter().collect::<String>());
            }
            stop
        })
        .boxed()
}

fn v1_doc(stops: Vec<Value>) -> Value {
    json!({"version": "v1", "stops": stops})
}

fn ids(doc: &Value) -> Vec<String> {
    doc["stops"].as_array().unwrap().iter().map(|s| s["id"].as_str().unwrap().to_string()).collect()
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// Totality, identity, loss reporting, and determinism: the four
    /// laws that make a migration publishable evidence in a registry.
    #[test]
    fn migration_is_total_deterministic_and_reports_loss(
        stops in proptest::collection::vec(gen_stop_v1(), 0..=12),
    ) {
        let doc = v1_doc(stops);
        let (migrated, notes) = migrate_to(doc.clone(), SCHEMA_VERSION).expect("every valid v1 document migrates");

        // Totality: the result reads as the current type, tagged.
        let read: Watchlist = serde_json::from_value(migrated.clone()).unwrap();
        prop_assert_eq!(read.schema_version(), SCHEMA_VERSION);
        prop_assert_eq!(&migrated["kind"], "watchlist");
        prop_assert_eq!(&migrated["schema_version"], SCHEMA_VERSION);

        // Identity: the stop-id sequence is invariant.
        prop_assert_eq!(read.stop_ids(), ids(&doc));

        // Semantics: every label is the authored note.
        for (a, b) in doc["stops"].as_array().unwrap().iter().zip(read.stops.iter()) {
            prop_assert_eq!(a["note"].as_str().unwrap(), b.label.as_str());
        }

        // No silent loss: dropped colors are counted exactly.
        let dropped = doc["stops"].as_array().unwrap().iter().filter(|s| s.get("color").is_some()).count() as u32;
        if dropped > 0 {
            prop_assert_eq!(notes.len(), 1, "one note per dropped field kind");
            prop_assert_eq!(&notes[0].kind, "dropped_field");
            prop_assert_eq!(&notes[0].field, "color");
            prop_assert_eq!(notes[0].count, dropped);
            prop_assert_eq!(notes[0].from, 1);
        } else {
            prop_assert!(notes.is_empty());
        }

        // Determinism: migrating again yields identical bytes.
        let (again, notes_again) = migrate_to(doc, SCHEMA_VERSION).expect("migrates twice");
        prop_assert_eq!(serde_json::to_string(&again).unwrap(), serde_json::to_string(&migrated).unwrap());
        prop_assert_eq!(notes_again.len(), notes.len());
    }

    /// A current input is an error, never a panic: the seam refuses
    /// what it has nothing to do to.
    #[test]
    fn migrating_the_current_version_is_refused(
        stops in proptest::collection::vec(gen_stop_v1(), 1..=4),
    ) {
        let (migrated, _) = migrate_to(v1_doc(stops), SCHEMA_VERSION).expect("v1 migrates");
        let err = migrate_to(migrated, SCHEMA_VERSION).expect_err("the current version is not migrated again");
        prop_assert!(err.contains("not older"), "got: {err}");
    }
}

#[test]
fn a_written_watchlist_names_its_kind_and_version() {
    let doc: Watchlist = serde_json::from_value(json!({"kind": "watchlist", "schema_version": 2, "stops": [{"id": "A", "label": "Home"}]})).unwrap();
    assert_eq!(
        serde_json::to_value(&doc).unwrap(),
        json!({"kind": "watchlist", "schema_version": 2, "stops": [{"id": "A", "label": "Home"}]})
    );
}

#[test]
fn a_copy_of_v2_from_before_the_tags_reads_as_v2_and_writes_tagged() {
    let legacy = json!({"version": "v2", "stops": [{"id": "A", "label": "Home"}]});
    assert_eq!(schema_version_of(&legacy), Ok(2));
    let read: Watchlist = serde_json::from_value(legacy).unwrap();
    assert_eq!(read.schema_version(), 2);
    assert_eq!(
        serde_json::to_value(&read).unwrap(),
        json!({"kind": "watchlist", "schema_version": 2, "stops": [{"id": "A", "label": "Home"}]}),
        "the legacy marker is read, never written"
    );
}

#[test]
fn a_copy_of_v1_names_its_version_and_does_not_read_as_current() {
    let legacy = v1_doc(vec![json!({"id": "A", "note": "Home", "color": "red"})]);
    assert_eq!(schema_version_of(&legacy), Ok(1));
    assert_eq!(<Watchlist as Versioned>::version_in(&legacy), 1, "the kernel's check reads the legacy marker");
    assert!(serde_json::from_value::<Watchlist>(legacy).is_err());
}

#[test]
fn a_newer_version_is_refused_by_the_chain() {
    let newer = json!({"kind": "watchlist", "schema_version": 3, "stops": []});
    assert_eq!(schema_version_of(&newer), Ok(3));
    assert!(migrate_to(newer, SCHEMA_VERSION).is_err());
    assert!(migrate_to(v1_doc(vec![]), 3).unwrap_err().contains("newer than this build"));
}

#[test]
fn a_copy_with_no_version_is_the_current_one() {
    // Every copy from before the tags named its version, so a copy with
    // neither field is current, as the type reads it.
    assert_eq!(schema_version_of(&json!({"stops": []})), Ok(SCHEMA_VERSION));
    let read: Watchlist = serde_json::from_value(json!({"stops": []})).unwrap();
    assert_eq!(read.schema_version(), SCHEMA_VERSION);
}
