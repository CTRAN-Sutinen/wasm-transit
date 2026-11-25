//! The migration chain has one pure step for each version (`vN` to `vN+1`),
//! total over valid vN, and it preserves every record id. A document
//! with no `schema_version` is version 1: that covers every shape written
//! before the tag. Each loss is a [`MigrationNote`]. The
//! migration capability wraps the chain.

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;
use ts_rs::TS;

use crate::model::Network;

pub const KIND: &str = "network";

pub const SCHEMA_VERSION: u32 = 2;

/// Read refuses a document that names another kind. A document that names
/// none reads as a network.
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

impl transit_version::Versioned for Network {
    const KIND: &'static str = KIND;
    const READS: u32 = SCHEMA_VERSION;
    fn schema_version(&self) -> u32 {
        self.schema_version.0
    }
}

/// A network built in code is the current version.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(transparent)]
pub struct NetworkVersion(pub u32);

impl Default for NetworkVersion {
    fn default() -> Self {
        NetworkVersion(SCHEMA_VERSION)
    }
}

impl NetworkVersion {
    pub fn untagged() -> Self {
        NetworkVersion(1)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "transit_network/")]
pub struct MigrationNote {
    pub from: u32,
    /// For example "dropped_field".
    pub kind: String,
    pub field: String,
    pub count: u32,
}

/// A document with no tag is version 1. A non-object or a non-numeric tag is
/// an error.
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

pub fn migrate_step(mut doc: Value, from: u32) -> Result<(Value, Vec<MigrationNote>), String> {
    let found = schema_version_of(&doc)?;
    if found != from {
        return Err(format!("the network v{from} step was given a v{found} document"));
    }
    match from {
        1 => {
            crate::migrate::migrate_v1(&mut doc)?;
            let root = doc.as_object_mut().expect("checked above");
            // The type writes the tag first.
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

/// Refuses a document already at or past `to`, so an older reader never
/// re-emits a newer document.
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

pub fn migrate_network(doc: Value) -> Result<(Network, Vec<MigrationNote>), String> {
    let (doc, notes) = migrate_to(doc, SCHEMA_VERSION)?;
    let network = serde_json::from_value(doc).map_err(|e| format!("network v{SCHEMA_VERSION}: {e}"))?;
    Ok((network, notes))
}
