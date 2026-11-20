//! The kind table (docs/FORMATS.md, "The kind table"): one table for
//! each build, generated from the kind list (`cap_abi::kinds::KINDS`)
//! and the locked schemas. It is checked in as `crates/cap-abi/kinds.json`
//! and written beside the server's modules as `web/pkg/kinds.json`. It
//! gives each kind the store keeps how it is written, the module that
//! checks a write, the merge rule, the read class, the log records of
//! its collection, its kernel load, and its references. The server's
//! receive path reads it, and refuses a kind with no entry. `store
//! verify` and `store gc` read the copy the store keeps, to find a log
//! record's schema (D-202).

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// The kind table's version.
pub const SCHEMA_VERSION: u32 = 1;

transit_version::kind_tag!(
    /// The kind table's kind tag: always `"kind_table"`.
    KindTableKind = "kind_table"
);

/// The kind table of one build.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct KindTable {
    /// The kind tag (D-115): `"kind_table"`.
    #[serde(default)]
    pub kind: KindTableKind,
    pub schema_version: u32,
    /// Each kind the store keeps, by its store kind.
    pub kinds: Vec<KindEntry>,
    /// Fields this build does not know, kept as written (D-167).
    #[serde(flatten)]
    pub extra: transit_version::Extra,
}

impl transit_version::Versioned for KindTable {
    const KIND: &'static str = "kind_table";
    const READS: u32 = SCHEMA_VERSION;
    fn schema_version(&self) -> u32 {
        self.schema_version
    }
}

pub use cap_abi::kinds::{Merge, Read, Write};

/// One record kind of a log, and where it lives in its document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RecordEntry {
    /// The record's kind in a log entry's changes.
    pub kind: String,
    /// The document field that holds the collection: an array of records
    /// with an `id`, or an object by id.
    pub collection: String,
}

/// One version of a kind, and its schema.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct VersionEntry {
    pub version: u32,
    /// The schema's content hash (the store keeps it as
    /// `schemas/<tag>.v<N>/<hex>.json`).
    pub schema: String,
}

/// One kind the store keeps.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct KindEntry {
    /// The envelope's kind, or a log's document kind.
    pub store_kind: String,
    /// The tag the document carries inside it, which names its schema.
    /// Empty for a kind with no schema of ours (a GTFS zip, a file of a
    /// build).
    pub tag: String,
    /// The versions with a schema in this build.
    pub versions: Vec<VersionEntry>,
    pub write: Write,
    /// Where it is written, in words a person reads in a refusal.
    pub route: String,
    /// The pointers it heads (`*` ends a prefix).
    pub pointers: Vec<String>,
    /// The log that holds its records.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub log: Option<String>,
    /// The log's record kinds of its collection.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub records: Vec<RecordEntry>,
    /// The capability that checks a write.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub module: Option<String>,
    /// Its op that checks a write.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub validate: Option<String>,
    pub merge: Merge,
    pub read: Read,
    /// The kernel's load of the kind, when a module reads it from the bus.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub load: Option<LoadEntry>,
    /// The fields that name other records or things, each with its
    /// target, from the current version's schema (`x-references`, D-232).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub references: Vec<transit_version::Reference>,
}

/// A kind's kernel load: its JSON op and its bulk op's code.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct LoadEntry {
    pub op: String,
    pub code: u32,
}

/// The table's entries: one for each line of the kind list that the
/// store keeps (`cap_abi::kinds::KINDS`), with no versions and no
/// references. gen-manifest adds those from the locked schemas.
pub fn kinds() -> Vec<KindEntry> {
    cap_abi::kinds::KINDS
        .iter()
        .filter_map(|k| {
            let s = k.store?;
            Some(KindEntry {
                store_kind: s.kind.into(),
                tag: k.tag.into(),
                versions: vec![],
                write: s.write,
                route: s.route.into(),
                pointers: s.pointers.iter().map(|p| p.to_string()).collect(),
                log: s.log.map(|l| l.name.into()),
                records: s.log.map(|l| l.records.iter().map(|(k, c)| RecordEntry { kind: k.to_string(), collection: c.to_string() }).collect()).unwrap_or_default(),
                module: s.check.map(|(m, _)| m.into()),
                validate: s.check.map(|(_, op)| op.into()),
                merge: s.merge,
                read: s.read,
                load: k.load.map(|l| LoadEntry { op: l.op.into(), code: l.code }),
                references: vec![],
            })
        })
        .collect()
}
