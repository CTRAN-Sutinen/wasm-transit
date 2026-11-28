//! The kind table (docs/FORMATS.md, "The kind table") is generated from the
//! kind list (`cap_abi::kinds::KINDS`) and the locked schemas. It is checked
//! in as `crates/cap-abi/kinds.json` and written beside the server's modules
//! as `web/pkg/kinds.json`. The server's write path reads it and refuses a
//! kind with no entry. `store verify` and `store gc` read the copy that the
//! store keeps, to find a log record's schema (D-202).
//!
//! The doc comments of the types here are in the locked schema
//! (`schemars` writes them as descriptions), so a change to one changes
//! the lock.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub const SCHEMA_VERSION: u32 = 1;

transit_version::kind_tag!(
    /// The tag is always `"kind_table"`.
    KindTableKind = "kind_table"
);

/// The kind table of one build.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct KindTable {
    /// The kind tag: `"kind_table"`.
    #[serde(default)]
    pub kind: KindTableKind,
    pub schema_version: u32,
    /// Each kind the store keeps, by its store kind.
    pub kinds: Vec<KindEntry>,
    /// Fields this build does not know, kept as written.
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
    /// The document field that holds the collection, either an array of
    /// records with an `id` or an object by id.
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
    /// The log's record kinds.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub records: Vec<RecordEntry>,
    /// Its own write route, answered by its log's receive op.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub post: Option<PostEntry>,
    /// The route that sends its records to the work order system, and the
    /// checking capability's ops that answer what to send and record the
    /// answer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub work_order: Option<WorkOrderEntry>,
    /// What a pick does with the kind: on an adoption, the argument that
    /// names the seq of the kind's log it read; on an export, the checking
    /// capability's op over the network and the kind's head.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pick: Option<PickEntry>,
    /// The checking capability's op that records an imported feed's
    /// differences from the kind.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub feed_events: Option<String>,
    /// The capability whose `receive` op writes the kind's log.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub receive: Option<String>,
    /// False when a page never writes the kind's log by intent (absent:
    /// it does, or the kind is in no log).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub intent: Option<bool>,
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

/// The write route of a kind names the path, the op that takes the request
/// body as its arguments, the answer field that names the document after
/// the write, and the body fields that the answer repeats.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PostEntry {
    pub path: String,
    pub op: String,
    pub answer: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub echo: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PickEntry {
    pub seq: String,
    pub export: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct WorkOrderEntry {
    pub method: String,
    pub path: String,
    pub call: String,
    pub record: String,
}

/// The kernel load of a kind names its JSON op and the code of its bytes op.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct LoadEntry {
    pub op: String,
    pub code: u32,
}

/// The entries have no versions and no references. gen-manifest adds those
/// from the locked schemas.
pub fn kinds() -> Vec<KindEntry> {
    kinds_of(cap_abi::kinds::KINDS)
}

pub fn kinds_of(list: &[cap_abi::kinds::Declaration]) -> Vec<KindEntry> {
    list.iter()
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
                records: s
                    .log
                    .map(|l| {
                        l.records
                            .iter()
                            .map(|(k, c)| RecordEntry {
                                kind: k.to_string(),
                                collection: c.to_string(),
                            })
                            .collect()
                    })
                    .unwrap_or_default(),
                post: s.post.map(|p| PostEntry {
                    path: p.path.into(),
                    op: p.op.into(),
                    answer: p.answer.into(),
                    echo: p.echo.iter().map(|e| e.to_string()).collect(),
                }),
                work_order: s.work_order.map(|w| WorkOrderEntry {
                    method: w.method.into(),
                    path: w.path.into(),
                    call: w.call.into(),
                    record: w.record.into(),
                }),
                pick: s.pick.map(|p| PickEntry {
                    seq: p.seq.into(),
                    export: p.export.into(),
                }),
                feed_events: s.feed_events.map(Into::into),
                receive: s.log.and_then(|l| l.receive).map(Into::into),
                intent: s.log.filter(|l| !l.intent).map(|_| false),
                module: s.check.map(|(m, _)| m.into()),
                validate: s.check.map(|(_, op)| op.into()),
                merge: s.merge,
                read: s.read,
                load: k.load.map(|l| LoadEntry {
                    op: l.op.into(),
                    code: l.code,
                }),
                references: vec![],
            })
        })
        .collect()
}
