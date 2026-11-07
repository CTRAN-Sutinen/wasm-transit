//! The edit document's version: its tag, and its migration chain.
//!
//! The edit document names its kind and version inside it (D-62,
//! D-115): `{"kind":"edit_records","schema_version":2,"records":[…]}`.
//! Version 1 is the bare array of records every copy carried before
//! the tag (the store's sidecar said v1; no copy itself did).
//!
//! The chain is kept forever (D-66): one pure step per version,
//! preserving every record, reporting every loss as a
//! [`MigrationNote`]. The migration capability wraps it; a consumer
//! reads the current version only.
//!
//! - v1 → v2: the array becomes the document's `records`, unchanged.

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;
use ts_rs::TS;

use crate::EditRecord;

/// The kind an edit document names itself (the store envelope's kind).
pub const KIND: &str = "edit_records";

/// The edit document version this build reads and writes.
pub const SCHEMA_VERSION: u32 = 2;

/// The edit document's kind tag: always `"edit_records"`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct EditsKind;

impl Serialize for EditsKind {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(KIND)
    }
}

impl<'de> Deserialize<'de> for EditsKind {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let kind = String::deserialize(d)?;
        if kind == KIND {
            Ok(EditsKind)
        } else {
            Err(serde::de::Error::custom(format!("a {kind:?} document is not an edit document")))
        }
    }
}

impl schemars::JsonSchema for EditsKind {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "EditsKind".into()
    }
    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({ "const": KIND })
    }
}

/// The operator edit document: every edit record, whole, keyed by id
/// (LWW per id, retirement the tombstone — `merge`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
pub struct EditDocument {
    #[ts(type = "\"edit_records\"")]
    pub kind: EditsKind,
    pub schema_version: u32,
    pub records: Vec<EditRecord>,
}

impl EditDocument {
    /// A current-version document over these records.
    pub fn new(records: Vec<EditRecord>) -> Self {
        EditDocument { kind: EditsKind, schema_version: SCHEMA_VERSION, records }
    }
}

impl Default for EditDocument {
    fn default() -> Self {
        EditDocument::new(Vec::new())
    }
}

/// One loss a migration step applied — reported, never silent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "transit_sync/")]
pub struct MigrationNote {
    /// The version the step started from.
    pub from: u32,
    pub kind: String,
    pub field: String,
    pub count: u32,
}

/// The version an edit document is in: 1 for a bare array, else its
/// `schema_version`. Anything else is no edit document.
pub fn schema_version_of(doc: &Value) -> Result<u32, String> {
    if doc.is_array() {
        return Ok(1);
    }
    let root = doc.as_object().ok_or("an edit document is a JSON object (or, in v1, an array)")?;
    if let Some(kind) = root.get("kind") {
        if kind.as_str() != Some(KIND) {
            return Err(format!("a {kind} document is not an edit document"));
        }
    }
    root.get("schema_version")
        .and_then(Value::as_u64)
        .and_then(|n| u32::try_from(n).ok())
        .filter(|&n| n >= 2)
        .ok_or_else(|| "an edit document object names its schema_version (2 or later)".into())
}

/// One step: a document in version `from` to `from + 1`, and what it
/// lost.
pub fn migrate_step(doc: Value, from: u32) -> Result<(Value, Vec<MigrationNote>), String> {
    let found = schema_version_of(&doc)?;
    if found != from {
        return Err(format!("the edit document v{from} step was given a v{found} document"));
    }
    match from {
        1 => {
            let mut tagged = serde_json::Map::new();
            tagged.insert("kind".into(), KIND.into());
            tagged.insert("schema_version".into(), 2.into());
            tagged.insert("records".into(), doc);
            Ok((Value::Object(tagged), Vec::new()))
        }
        n => Err(format!("no edit document step from v{n} (this build reads up to v{SCHEMA_VERSION})")),
    }
}

/// A document brought to version `to` through every step between.
/// Refuses a document already at or past `to`.
pub fn migrate_to(doc: Value, to: u32) -> Result<(Value, Vec<MigrationNote>), String> {
    let from = schema_version_of(&doc)?;
    if to > SCHEMA_VERSION {
        return Err(format!("edit document v{to} is newer than this build (it reads up to v{SCHEMA_VERSION})"));
    }
    if from >= to {
        return Err(format!("edit document v{from} is not older than v{to}: nothing to migrate"));
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
