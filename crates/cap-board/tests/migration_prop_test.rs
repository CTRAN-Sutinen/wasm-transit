//! The watchlist migration's property laws — the personal document
//! is the migration mechanism's real payload, so its laws are
//! property-tested here (the capability owns the document):
//!
//! * **Identity** — the stop-id sequence is invariant across
//!   versions. This is the law that keeps sync sound: LWW-by-record-id
//!   merges stay well-defined across clients at different document
//!   versions only if migrations never touch the key space.
//! * **No silent loss** — every dropped color is counted in a
//!   MigrationNote. Never silent, always reported.
//! * **Determinism** — the same input produces the same output bytes,
//!   on every host (the sync thesis needs this).
//! * **Totality of the seam** — a v2 input is an error, never a panic.

use proptest::prelude::*;
use proptest::strategy::BoxedStrategy;

use cap_board::{migrate_watchlist, WatchStopV1, Watchlist};

fn gen_stop_v1() -> BoxedStrategy<WatchStopV1> {
    (
        0u8..6,               // id pool small enough for duplicates
        proptest::collection::vec(any::<char>().prop_filter("printable", |c| c.is_ascii_alphanumeric()), 0..=8),
        proptest::option::of(proptest::collection::vec(any::<char>().prop_filter("printable", |c| c.is_ascii_alphabetic()), 3..=6)),
    )
        .prop_map(|(id, note, color)| WatchStopV1 {
            id: format!("s{id}"),
            note: note.into_iter().collect(),
            color: color.map(|c| c.into_iter().collect()),
        })
        .boxed()
}

fn v1_doc(stops: Vec<WatchStopV1>) -> Watchlist {
    Watchlist::V1 { stops }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// Totality, identity, loss reporting, and determinism — the four
    /// laws that make a migration publishable evidence in a registry.
    #[test]
    fn migration_is_total_deterministic_and_reports_loss(
        stops in proptest::collection::vec(gen_stop_v1(), 0..=12),
    ) {
        let doc = v1_doc(stops);
        let (migrated, notes) = migrate_watchlist(&doc).expect("every valid v1 document migrates");

        // Totality: the result is a valid v2 document (JSON round-trip).
        prop_assert_eq!(migrated.version(), 2);
        let json = serde_json::to_string(&migrated).unwrap();
        let back: Watchlist = serde_json::from_str(&json).unwrap();
        prop_assert_eq!(back.version(), 2);

        // Identity: the stop-id sequence is invariant.
        prop_assert_eq!(migrated.stop_ids(), doc.stop_ids());

        // Semantics: every label is the authored note.
        let (Watchlist::V2 { stops: v2 }, Watchlist::V1 { stops: v1 }) = (&migrated, &doc) else {
            panic!("v2 over v1")
        };
        for (a, b) in v1.iter().zip(v2.iter()) {
            prop_assert_eq!(&a.id, &b.id);
            prop_assert_eq!(&a.note, &b.label);
        }

        // No silent loss: dropped colors are counted exactly.
        let dropped = v1.iter().filter(|s| s.color.is_some()).count() as u32;
        if dropped > 0 {
            prop_assert_eq!(notes.len(), 1, "one note per dropped field kind");
            prop_assert_eq!(&notes[0].kind, "dropped_field");
            prop_assert_eq!(&notes[0].field, "color");
            prop_assert_eq!(notes[0].count, dropped);
        } else {
            prop_assert!(notes.is_empty());
        }

        // Determinism: migrating again yields identical bytes.
        let (again, notes_again) = migrate_watchlist(&doc).expect("migrates twice");
        prop_assert_eq!(serde_json::to_string(&again).unwrap(), json);
        prop_assert_eq!(notes_again.len(), notes.len());
    }

    /// A v2 input is an error, never a panic — the seam rejects what
    /// it cannot handle.
    #[test]
    fn migration_rejects_v2_as_an_error(
        stops in proptest::collection::vec(gen_stop_v1(), 1..=4),
    ) {
        let (migrated, _) = migrate_watchlist(&v1_doc(stops)).expect("v1 migrates");
        let err = migrate_watchlist(&migrated).expect_err("v2 must error, not re-migrate");
        prop_assert!(err.contains("already version 2"), "got: {err}");
    }
}
