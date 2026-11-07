//! The network document's version: its tag, and its migration chain.
//!
//! Every network document says what it is inside it (D-62, D-115):
//! `"kind": "network"` and `"schema_version"`, first, so a copy
//! anywhere — an export, a backup, a file another tool reads — names
//! its kind and its version, and the hash covers both. A document
//! with no `schema_version` is version 1: every shape written before
//! the tag (pre-trip, pre-alignment, and the untagged current shape).
//!
//! The chain is kept forever (D-66): one pure step per version
//! (`vN → vN+1`), total over valid vN, preserving every record id,
//! reporting every loss as a [`MigrationNote`]. The migration
//! capability wraps it; a consumer reads the current version only.
//!
//! - v1 → v2: the shapes v1 covers read into the current model
//!   (`migrate.rs`), and the tag is added. Nothing is lost.

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;
use ts_rs::TS;

use crate::model::Network;

/// The kind a network document names itself.
pub const KIND: &str = "network";

/// The network version this build reads and writes.
pub const SCHEMA_VERSION: u32 = 2;

/// The network document's kind tag: always `"network"`. A document
/// that names another kind is refused at read; one that names none
/// reads as a network (the tag is written on every write).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct NetworkKind;

impl Serialize for NetworkKind {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(KIND)
    }
}

impl<'de> Deserialize<'de> for NetworkKind {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let kind = String::deserialize(d)?;
        if kind == KIND {
            Ok(NetworkKind)
        } else {
            Err(serde::de::Error::custom(format!("a {kind:?} document is not a network")))
        }
    }
}

impl schemars::JsonSchema for NetworkKind {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "NetworkKind".into()
    }
    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({ "const": KIND })
    }
}

/// The network document's version. A network built in code is the
/// current version; a document read without one is version 1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(transparent)]
pub struct NetworkVersion(pub u32);

impl Default for NetworkVersion {
    fn default() -> Self {
        NetworkVersion(SCHEMA_VERSION)
    }
}

impl NetworkVersion {
    /// What a document without the tag is.
    pub fn untagged() -> Self {
        NetworkVersion(1)
    }
}

/// One loss a migration step applied to document data — reported,
/// never silent (the import-notes convention, restated for
/// migrations).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "transit_network/")]
pub struct MigrationNote {
    /// The version the step started from.
    pub from: u32,
    /// What happened ("dropped_field", …).
    pub kind: String,
    /// The field it happened to.
    pub field: String,
    /// How many records it touched.
    pub count: u32,
}

/// The version a network document is in: its `schema_version`, or 1
/// when it carries none. Not an object (or a non-numeric tag) is an
/// error: no version reads it.
pub fn schema_version_of(doc: &Value) -> Result<u32, String> {
    let root = doc.as_object().ok_or("a network document is a JSON object")?;
    if let Some(kind) = root.get("kind") {
        if kind.as_str() != Some(KIND) {
            return Err(format!("a {kind} document is not a network"));
        }
    }
    match root.get("schema_version") {
        None => Ok(1),
        Some(v) => v
            .as_u64()
            .and_then(|n| u32::try_from(n).ok())
            .filter(|&n| n >= 1)
            .ok_or_else(|| format!("network schema_version {v} is not a version")),
    }
}

/// One step: a document in version `from` to version `from + 1`, and
/// what it lost. Refuses a document that is not in `from`.
pub fn migrate_step(mut doc: Value, from: u32) -> Result<(Value, Vec<MigrationNote>), String> {
    let found = schema_version_of(&doc)?;
    if found != from {
        return Err(format!("the network v{from} step was given a v{found} document"));
    }
    match from {
        1 => {
            crate::migrate::migrate_v1(&mut doc)?;
            let root = doc.as_object_mut().expect("checked above");
            // The tag leads the document, as the type writes it.
            let mut tagged = serde_json::Map::new();
            tagged.insert("kind".into(), KIND.into());
            tagged.insert("schema_version".into(), 2.into());
            for (k, v) in std::mem::take(root) {
                if k != "kind" && k != "schema_version" {
                    tagged.insert(k, v);
                }
            }
            Ok((Value::Object(tagged), Vec::new()))
        }
        n => Err(format!("no network step from v{n} (this build reads up to v{SCHEMA_VERSION})")),
    }
}

/// A document brought to version `to` through every step between,
/// with every step's losses. Refuses a document already at or past
/// `to`: migrating never rewrites a version (a newer document is
/// never re-emitted by an older reader).
pub fn migrate_to(doc: Value, to: u32) -> Result<(Value, Vec<MigrationNote>), String> {
    let from = schema_version_of(&doc)?;
    if to > SCHEMA_VERSION {
        return Err(format!("network v{to} is newer than this build (it reads up to v{SCHEMA_VERSION})"));
    }
    if from >= to {
        return Err(format!("network v{from} is not older than v{to}: nothing to migrate"));
    }
    let mut doc = doc;
    let mut notes = Vec::new();
    for step in from..to {
        let (next, lost) = migrate_step(doc, step)?;
        doc = next;
        notes.extend(lost);
    }
    Ok((doc, notes))
}

/// An older network document read as the current one: the chain to
/// [`SCHEMA_VERSION`], then the type.
pub fn migrate_network(doc: Value) -> Result<(Network, Vec<MigrationNote>), String> {
    let (doc, notes) = migrate_to(doc, SCHEMA_VERSION)?;
    let network = serde_json::from_value(doc).map_err(|e| format!("network v{SCHEMA_VERSION}: {e}"))?;
    Ok((network, notes))
}
