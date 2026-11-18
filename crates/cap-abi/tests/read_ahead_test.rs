//! Reading one version ahead (D-79, D-186; docs/FORMATS.md, "Rolling a
//! version back"). A format change ships in two releases: release A
//! reads vN and vN+1 and writes vN, and release B writes vN+1. So a
//! rollback from B to A reads what B stored. Each change declares how A
//! reads vN+1:
//!
//! - lossless: A reads it down, and vN+1 read down and brought forward
//!   again is the same document;
//! - read-only: A reads it down, and refuses to write over it, naming
//!   both versions.
//!
//! A version past the next, or an older one, is refused as before. The
//! laws run over fixture kinds: no kind has a next version today.

use cap_abi::{read_versioned, versioned, Base, Current, ReadAhead};
use proptest::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use transit_version::{reads, Ahead, ReadAs, Versioned};

transit_version::kind_tag!(ThingKind = "thing");
transit_version::kind_tag!(RatedKind = "rated");
transit_version::kind_tag!(PlainKind = "plain");

/// v1 of a kind whose v2 renames `name` to `title`: lossless.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Thing {
    #[serde(default)]
    kind: ThingKind,
    schema_version: u32,
    name: String,
    /// Fields this build does not know, kept as written (D-167).
    #[serde(flatten)]
    extra: transit_version::Extra,
}

impl Versioned for Thing {
    const KIND: &'static str = "thing";
    const READS: u32 = 1;
    const AHEAD: Option<(u32, Ahead)> = Some((2, Ahead::Lossless));
    fn schema_version(&self) -> u32 {
        self.schema_version
    }
    fn step_down(mut next: Value) -> Result<Value, String> {
        let object = next.as_object_mut().ok_or("a thing is an object")?;
        let title = object.remove("title").ok_or("a thing v2 has its title")?;
        object.insert("name".into(), title);
        object.insert("schema_version".into(), json!(1));
        Ok(next)
    }
}

/// The migration step the step down reverses: v1 to v2.
fn thing_forward(mut v1: Value) -> Value {
    let object = v1.as_object_mut().unwrap();
    let name = object.remove("name").unwrap();
    object.insert("title".into(), name);
    object.insert("schema_version".into(), json!(2));
    v1
}

/// v1 of a kind whose v2 adds stars, a meaning v1 cannot hold:
/// read-only.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Rated {
    #[serde(default)]
    kind: RatedKind,
    schema_version: u32,
    name: String,
}

impl Versioned for Rated {
    const KIND: &'static str = "rated";
    const READS: u32 = 1;
    const AHEAD: Option<(u32, Ahead)> = Some((2, Ahead::ReadOnly));
    fn schema_version(&self) -> u32 {
        self.schema_version
    }
    fn step_down(mut next: Value) -> Result<Value, String> {
        let object = next.as_object_mut().ok_or("a rated is an object")?;
        object.remove("stars");
        object.insert("schema_version".into(), json!(1));
        Ok(next)
    }
}

/// A kind that reads only its own version.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Plain {
    #[serde(default)]
    kind: PlainKind,
    schema_version: u32,
    name: String,
}

impl Versioned for Plain {
    const KIND: &'static str = "plain";
    const READS: u32 = 1;
    fn schema_version(&self) -> u32 {
        self.schema_version
    }
}

fn bytes(value: &Value) -> Vec<u8> {
    serde_json::to_vec(value).unwrap()
}

#[test]
fn a_build_reads_its_version_and_the_one_it_declares_ahead() {
    assert_eq!(reads::<Thing>(1), Ok(ReadAs::Current));
    assert_eq!(reads::<Thing>(2), Ok(ReadAs::Ahead(Ahead::Lossless)));
    assert_eq!(reads::<Rated>(2), Ok(ReadAs::Ahead(Ahead::ReadOnly)));
    assert!(reads::<Plain>(2).is_err(), "a kind that declares nothing ahead reads only its version");
}

#[test]
fn a_version_past_the_next_or_an_older_one_is_refused_as_before() {
    let newer = read_versioned::<Thing>(&bytes(&json!({"kind": "thing", "schema_version": 3, "title": "a"}))).unwrap_err();
    assert!(newer.contains("thing v3 is newer") && newer.contains("reads v1"), "{newer}");
    let plain = read_versioned::<Plain>(&bytes(&json!({"kind": "plain", "schema_version": 2, "name": "a"}))).unwrap_err();
    assert!(plain.contains("plain v2 is newer") && plain.contains("reads v1"), "{plain}");
    let older = cap_abi::speaks("thing", 0, 1).unwrap_err();
    assert!(older.contains("migrate.migrate"), "{older}");
}

#[test]
fn a_copy_in_this_version_reads_as_itself() {
    let v1 = json!({"kind": "thing", "schema_version": 1, "name": "a"});
    let (thing, ahead) = read_versioned::<Thing>(&bytes(&v1)).unwrap();
    assert_eq!((thing.name.as_str(), ahead), ("a", None));
}

proptest! {
    /// Lossless: vN+1 reads down, keeps the identity it is stored under,
    /// and brought forward again is the same document. A write over it
    /// is allowed.
    #[test]
    fn a_lossless_step_reads_down_and_round_trips(name in "\\PC{0,24}", extra in any::<bool>()) {
        let mut v2 = json!({"kind": "thing", "schema_version": 2, "title": name});
        if extra {
            v2["note"] = json!("a field this build does not know");
        }
        let (thing, ahead) = read_versioned::<Thing>(&bytes(&v2)).unwrap();
        let hash = cap_abi::canonical_hash(&v2).unwrap();
        prop_assert_eq!(ahead, Some(ReadAhead { how: Ahead::Lossless, found: 2, hash }));
        prop_assert_eq!(&thing.name, &name);
        prop_assert_eq!(thing_forward(serde_json::to_value(&thing).unwrap()), v2.clone());
        prop_assert!(serde_json::from_value::<Base<Thing>>(v2).is_ok(), "a lossless copy is a base of a write");
    }
}

#[test]
fn a_read_only_step_reads_and_refuses_a_write_naming_both() {
    let v2 = json!({"kind": "rated", "schema_version": 2, "name": "a", "stars": 4});
    let (rated, ahead) = read_versioned::<Rated>(&bytes(&v2)).unwrap();
    assert_eq!(rated.name, "a");
    assert_eq!(ahead.map(|read| (read.how, read.found)), Some((Ahead::ReadOnly, 2)));
    let read: Current<Rated> = serde_json::from_value(v2.clone()).unwrap();
    assert_eq!(read.name, "a", "an op that only reads it reads it");
    let refused = serde_json::from_value::<Base<Rated>>(v2).unwrap_err().to_string();
    assert!(refused.contains("rated v2") && refused.contains("v1") && refused.contains("does not write over it"), "{refused}");
    let current: Base<Rated> = serde_json::from_value(json!({"kind": "rated", "schema_version": 1, "name": "b"})).unwrap();
    assert_eq!(current.name, "b", "a copy in this version is a base as usual");
}

#[test]
fn a_request_field_reads_ahead_like_bulk_bytes() {
    let v2 = json!({"kind": "thing", "schema_version": 2, "title": "a"});
    assert_eq!(versioned::<Thing>(v2.clone()).unwrap(), read_versioned::<Thing>(&bytes(&v2)).unwrap().0);
}
