//! The real-output gate's check (`generator::doc_check`), broken on
//! fixtures: a gate that cannot fail proves nothing.

use generator::doc_check::Locks;
use serde_json::json;

#[test]
fn a_document_that_does_not_match_its_lock_is_named() {
    let locks = Locks::read();
    let good = json!({"ok": {"rules": {"kind": "work_rules", "schema_version": 1}}});
    assert_eq!(
        locks.check(&good),
        vec![("work_rules v1".to_string(), vec![])]
    );
    let bad =
        json!({"ok": {"rules": {"kind": "work_rules", "schema_version": 1, "recovery": "lots"}}});
    let found = locks.check(&bad);
    assert_eq!(found.len(), 1);
    assert!(
        !found[0].1.is_empty(),
        "a wrong field type is a problem: {found:?}"
    );
}

#[test]
fn a_version_with_no_lock_is_named_and_a_lookalike_is_no_document() {
    let locks = Locks::read();
    let found = locks.check(&json!({"ok": {"doc": {"kind": "work_rules", "schema_version": 9}}}));
    assert_eq!(
        found[0].1,
        vec!["the locks have no schema for that version".to_string()]
    );
    assert!(locks
        .check(&json!({"notes": [{"kind": "dropped_trips", "count": 1}]}))
        .is_empty());
}
