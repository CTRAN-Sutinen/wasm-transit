//! Version 1 is the bare array of records, which carries no tag. Each
//! migration step stays forever, keeps every record, and reports every
//! loss as a [`MigrationNote`].

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;
use ts_rs::TS;

use crate::EditRecord;

pub const KIND: &str = "edit_records";

pub const SCHEMA_VERSION: u32 = 2;

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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export)]
pub struct EditDocument {
    #[ts(type = "\"edit_records\"")]
    pub kind: EditsKind,
    pub schema_version: u32,
    pub records: Vec<EditRecord>,
    #[serde(flatten)]
    #[ts(skip)]
    pub extra: transit_version::Extra,
}

impl transit_version::Versioned for EditDocument {
    const KIND: &'static str = KIND;
    const READS: u32 = SCHEMA_VERSION;
    fn schema_version(&self) -> u32 {
        self.schema_version
    }
}

impl EditDocument {
    pub fn new(records: Vec<EditRecord>) -> Self {
        EditDocument { kind: EditsKind, schema_version: SCHEMA_VERSION, records, extra: Default::default() }
    }
}

impl Default for EditDocument {
    fn default() -> Self {
        EditDocument::new(Vec::new())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "transit_sync/")]
pub struct MigrationNote {
    pub from: u32,
    pub kind: String,
    pub field: String,
    pub count: u32,
}

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
