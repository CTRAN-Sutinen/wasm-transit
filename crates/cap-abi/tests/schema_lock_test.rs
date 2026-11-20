//! The schema lock, each of its conditions broken on a deliberate
//! fixture: a gate that cannot fail proves nothing. The gate itself runs
//! over every real document kind in `just caps` (gen-manifest), and
//! `just schema-lock` writes the locks (docs/FORMATS.md, "The schema
//! lock").
//!
//! The fixture kind is `thing`. Each module below is one build's serde
//! type for it, with the same Rust name, so a schema differs only where
//! the shape differs.

#[path = "../src/bin/common/schema_lock.rs"]
mod schema_lock;

use schema_lock::{freeze, kind, read_lock, write_lock, Drift, Kind, Lock};
use serde_json::{json, Value};

/// The build that freezes v1: a name and a list of stops.
mod v1 {
    #[derive(serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
    pub struct Thing {
        pub kind: String,
        pub schema_version: u32,
        pub name: String,
        pub stops: Vec<String>,
        #[serde(flatten)]
        pub extra: transit_version::Extra,
    }
}

/// v1 with an optional field that writes nothing when it is absent.
mod added_optional {
    #[derive(serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
    pub struct Thing {
        pub kind: String,
        pub schema_version: u32,
        pub name: String,
        pub stops: Vec<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub note: Option<String>,
        #[serde(flatten)]
        pub extra: transit_version::Extra,
    }
}

/// v1 with a field that always writes, so every document's bytes move.
mod added_written {
    #[derive(serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
    pub struct Thing {
        pub kind: String,
        pub schema_version: u32,
        pub name: String,
        pub stops: Vec<String>,
        #[serde(default)]
        pub count: u32,
        #[serde(flatten)]
        pub extra: transit_version::Extra,
    }
}

/// v1 with `name` renamed to `title`: the frozen example's `name` is
/// then a field the type does not know.
mod renamed {
    #[derive(serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
    pub struct Thing {
        pub kind: String,
        pub schema_version: u32,
        #[serde(default)]
        pub title: String,
        pub stops: Vec<String>,
        #[serde(flatten)]
        pub extra: transit_version::Extra,
    }
}

/// v1 with `stops` made a count: the frozen example does not read.
mod retyped {
    #[derive(serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
    pub struct Thing {
        pub kind: String,
        pub schema_version: u32,
        pub name: String,
        pub stops: u32,
        #[serde(flatten)]
        pub extra: transit_version::Extra,
    }
}

/// v1 with `stops` declared to name stops (D-232): no byte changes.
mod declared {
    #[derive(serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
    pub struct Thing {
        pub kind: String,
        pub schema_version: u32,
        pub name: String,
        #[schemars(extend("x-references" = "stop"))]
        pub stops: Vec<String>,
        #[serde(flatten)]
        pub extra: transit_version::Extra,
    }
}

/// `declared` with the target changed to the network's stops.
mod retargeted {
    #[derive(serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
    pub struct Thing {
        pub kind: String,
        pub schema_version: u32,
        pub name: String,
        #[schemars(extend("x-references" = "network.stop"))]
        pub stops: Vec<String>,
        #[serde(flatten)]
        pub extra: transit_version::Extra,
    }
}

fn example(version: u32) -> Value {
    json!({ "kind": "thing", "schema_version": version, "name": "Main St", "stops": ["A", "B"] })
}

fn v1_kind() -> Kind {
    kind::<v1::Thing>("thing", 1, Some(example(1)), None)
}

/// The v1 lock, frozen from the v1 build.
fn v1_lock() -> Lock {
    freeze(&v1_kind(), &Lock::empty("thing")).expect("v1 freezes")
}

/// The v2 build: v1's `name` is `title`, a required field.
mod v2 {
    #[derive(serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
    pub struct Thing {
        pub kind: String,
        pub schema_version: u32,
        pub title: String,
        pub stops: Vec<String>,
        #[serde(flatten)]
        pub extra: transit_version::Extra,
    }
}

fn v1_to_v2(mut doc: Value, from: u32) -> Result<Value, String> {
    if from != 1 {
        return Err(format!("no step from v{from}"));
    }
    let name = doc.as_object_mut().and_then(|o| o.remove("name")).ok_or("v1 has a name")?;
    doc["title"] = name;
    doc["schema_version"] = json!(2);
    Ok(doc)
}

/// A step that forgets to carry `name` over, so v2 does not read.
fn broken_step(mut doc: Value, _from: u32) -> Result<Value, String> {
    doc["schema_version"] = json!(2);
    Ok(doc)
}

fn v2_kind(step: fn(Value, u32) -> Result<Value, String>) -> Kind {
    kind::<v2::Thing>(
        "thing",
        2,
        Some(json!({ "kind": "thing", "schema_version": 2, "title": "Main St", "stops": ["A"] })),
        Some(step),
    )
}

fn has(drifts: &[Drift], matches: impl Fn(&Drift) -> bool) -> bool {
    drifts.iter().any(matches)
}

#[test]
fn a_build_that_matches_its_lock_passes() {
    assert_eq!(schema_lock::check(&v1_kind(), &v1_lock()), vec![]);
}

#[test]
fn the_frozen_example_is_its_canonical_bytes_and_hash() {
    let lock = v1_lock();
    let frozen = &lock.versions[&1];
    assert_eq!(frozen.example, r#"{"kind":"thing","name":"Main St","schema_version":1,"stops":["A","B"]}"#);
    assert_eq!(frozen.example_hash, cap_abi::content_hash(frozen.example.as_bytes()));
}

#[test]
fn a_version_this_build_writes_with_no_lock_fails() {
    let drifts = schema_lock::check(&v1_kind(), &Lock::empty("thing"));
    assert!(has(&drifts, |d| matches!(d, Drift::Unlocked { version: 1, .. })), "{drifts:?}");
}

/// Condition 1: the serde type changed without a version bump.
#[test]
fn a_changed_type_in_the_same_version_fails() {
    for (what, changed) in [
        ("renamed", kind::<renamed::Thing>("thing", 1, Some(example(1)), None)),
        ("retyped", kind::<retyped::Thing>("thing", 1, Some(example(1)), None)),
    ] {
        let drifts = schema_lock::check(&changed, &v1_lock());
        assert!(has(&drifts, |d| matches!(d, Drift::SchemaChanged { additive: false, .. })), "{what}: {drifts:?}");
        assert!(freeze(&changed, &v1_lock()).is_err(), "{what}: the lock command refuses it too");
    }
}

#[test]
fn a_renamed_field_is_named_as_unknown() {
    let drifts = schema_lock::check(&kind::<renamed::Thing>("thing", 1, Some(example(1)), None), &v1_lock());
    assert!(has(&drifts, |d| matches!(d, Drift::UnknownFields { names, .. } if names == &["name"])), "{drifts:?}");
}

#[test]
fn a_retyped_field_is_unreadable() {
    let drifts = schema_lock::check(&kind::<retyped::Thing>("thing", 1, Some(example(1)), None), &v1_lock());
    assert!(has(&drifts, |d| matches!(d, Drift::Unreadable { .. })), "{drifts:?}");
}

/// Condition 3: the current example's canonical hash moves.
#[test]
fn a_field_that_changes_the_bytes_fails() {
    let changed = kind::<added_written::Thing>("thing", 1, Some(example(1)), None);
    let drifts = schema_lock::check(&changed, &v1_lock());
    assert!(has(&drifts, |d| matches!(d, Drift::HashMoved { .. })), "{drifts:?}");
    assert!(has(&drifts, |d| matches!(d, Drift::SchemaChanged { additive: false, .. })), "{drifts:?}");
    assert!(freeze(&changed, &v1_lock()).is_err());
}

/// The exception (D-167): an added optional field needs no bump.
#[test]
fn an_added_optional_field_is_recorded_in_the_same_version() {
    let added = kind::<added_optional::Thing>("thing", 1, Some(example(1)), None);
    let drifts = schema_lock::check(&added, &v1_lock());
    assert_eq!(drifts.len(), 1, "{drifts:?}");
    assert!(matches!(drifts[0], Drift::SchemaChanged { additive: true, .. }), "{drifts:?}");
    let recorded = freeze(&added, &v1_lock()).expect("the lock command records an added optional field");
    assert_eq!(recorded.versions[&1].example, v1_lock().versions[&1].example, "the frozen example stays");
    assert_ne!(recorded.versions[&1].schema, v1_lock().versions[&1].schema, "the schema is the new one");
    assert_eq!(schema_lock::check(&added, &recorded), vec![]);
}

/// Condition 2: an older version's example no longer migrates.
#[test]
fn an_older_example_that_does_not_migrate_fails() {
    let bumped = v2_kind(broken_step);
    let drifts = schema_lock::check(&bumped, &v1_lock());
    assert!(has(&drifts, |d| matches!(d, Drift::MigrationBroken { from: 1, .. })), "{drifts:?}");
    assert!(freeze(&bumped, &v1_lock()).is_err(), "v2 is not frozen over a v1 it cannot read");
}

#[test]
fn an_older_version_with_no_migration_fails() {
    let mut bumped = v2_kind(v1_to_v2);
    bumped.migrate = None;
    let drifts = schema_lock::check(&bumped, &v1_lock());
    assert!(has(&drifts, |d| matches!(d, Drift::MigrationBroken { from: 1, .. })), "{drifts:?}");
}

#[test]
fn a_bump_freezes_the_new_version_and_keeps_the_old_one() {
    let bumped = v2_kind(v1_to_v2);
    let drifts = schema_lock::check(&bumped, &v1_lock());
    assert_eq!(drifts, vec![Drift::Unlocked { tag: "thing".into(), version: 2 }]);
    let both = freeze(&bumped, &v1_lock()).expect("v2 freezes");
    assert_eq!(both.versions.keys().copied().collect::<Vec<_>>(), vec![1, 2]);
    assert_eq!(both.versions[&1], v1_lock().versions[&1], "v1 stays as frozen");
    assert_eq!(schema_lock::check(&bumped, &both), vec![]);
}

#[test]
fn a_lock_newer_than_the_build_fails() {
    let both = freeze(&v2_kind(v1_to_v2), &v1_lock()).unwrap();
    let drifts = schema_lock::check(&v1_kind(), &both);
    assert!(has(&drifts, |d| matches!(d, Drift::LockAhead { version: 2, current: 1, .. })), "{drifts:?}");
}

fn scratch(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("schema-lock-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

#[test]
fn a_lock_reads_back_as_written() {
    let dir = scratch("roundtrip");
    let lock = freeze(&v2_kind(v1_to_v2), &v1_lock()).unwrap();
    write_lock(&dir, &lock).unwrap();
    for file in ["thing.lock.json", "thing.v1.schema.json", "thing.v1.example.json", "thing.v2.schema.json", "thing.v2.example.json"] {
        assert!(dir.join(file).exists(), "{file}");
    }
    assert_eq!(read_lock(&dir, "thing").unwrap(), lock);
    assert_eq!(read_lock(&dir.join("absent"), "thing").unwrap(), Lock::empty("thing"), "no lock yet is an empty lock");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_lock_edited_by_hand_fails() {
    let dir = scratch("edited");
    write_lock(&dir, &v1_lock()).unwrap();
    let path = dir.join("thing.v1.example.json");
    let edited = std::fs::read_to_string(&path).unwrap().replace("Main St", "Elm St");
    std::fs::write(&path, edited).unwrap();
    let drifts = schema_lock::check(&v1_kind(), &read_lock(&dir, "thing").unwrap());
    assert!(has(&drifts, |d| matches!(d, Drift::Edited { version: 1, .. })), "{drifts:?}");
    let _ = std::fs::remove_dir_all(&dir);
}

/// D-232: a declaration added to a field that had none states what was
/// already true. The bytes do not change, so it is recorded in the same
/// version.
#[test]
fn an_added_reference_declaration_is_recorded_in_the_same_version() {
    let declared = kind::<declared::Thing>("thing", 1, Some(example(1)), None);
    let drifts = schema_lock::check(&declared, &v1_lock());
    assert_eq!(drifts.len(), 1, "{drifts:?}");
    assert!(matches!(drifts[0], Drift::SchemaChanged { additive: true, .. }), "{drifts:?}");
    let recorded = freeze(&declared, &v1_lock()).expect("the lock command records an added declaration");
    assert_eq!(schema_lock::check(&declared, &recorded), vec![]);
}

/// D-232: a changed target changes what the field means, though no byte
/// changes. It needs a version bump, and so does a dropped declaration.
#[test]
fn a_changed_or_dropped_reference_target_needs_a_bump() {
    let declared = kind::<declared::Thing>("thing", 1, Some(example(1)), None);
    let lock = freeze(&declared, &v1_lock()).expect("the declaration is recorded");
    let retargeted = kind::<retargeted::Thing>("thing", 1, Some(example(1)), None);
    let drifts = freeze(&retargeted, &lock).expect_err("a changed target is refused");
    assert!(
        has(&drifts, |d| matches!(d, Drift::ReferenceChanged { path, was, now: Some(now), .. }
            if path == "/stops/*" && was == "stop" && now == "network.stop")),
        "{drifts:?}"
    );
    let dropped = freeze(&v1_kind(), &lock).expect_err("a dropped declaration is refused");
    assert!(has(&dropped, |d| matches!(d, Drift::ReferenceChanged { now: None, .. })), "{dropped:?}");
}
