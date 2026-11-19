//! The kind table (docs/FORMATS.md, "The kind table"): one table for
//! each build, beside the server's modules (`web/pkg/kinds.json`). It
//! gives each kind the store keeps how it is written, the module that
//! checks a write, the merge rule, the read class, and the log records
//! of its collection. The server's receive path reads it, and refuses a
//! kind with no entry. `store verify` and `store gc` read the copy the
//! store keeps, to find a log record's schema (D-202).

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

/// How a kind is written.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Write {
    /// `POST /pointers/<name>`, one of `pointers`, checked by `module`'s
    /// `validate` op, and stored as the module answers it.
    Pointer,
    /// Its own route (`route`), which runs its module.
    Route,
    /// The server alone: an import, a release, or the manifest.
    Server,
}

/// Who reads a kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Read {
    /// Anyone, by hash (`/blobs/<hash>`; D-152).
    Open,
    /// Never served by hash. A client reads only its own projection.
    Restricted,
    /// A caller with the audit rule (a log's entries name their writers).
    Audit,
}

/// How two writes of a kind meet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Merge {
    /// The whole document, by CAS on the head the writer read.
    Replace,
    /// Last writer wins for each record id (the edit document; D-56).
    Lww,
    /// Ops over the log's head; different records never conflict.
    Log,
    /// No writer but the server.
    None,
}

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
}

/// The governance of each kind: everything but its versions, which come
/// from the published schemas.
fn entry(store_kind: &str, tag: &str, write: Write, route: &str, pointers: &[&str], merge: Merge, read: Read) -> KindEntry {
    KindEntry {
        store_kind: store_kind.into(),
        tag: tag.into(),
        versions: vec![],
        write,
        route: route.into(),
        pointers: pointers.iter().map(|p| p.to_string()).collect(),
        log: None,
        records: vec![],
        module: None,
        validate: None,
        merge,
        read,
    }
}

impl KindEntry {
    fn checked(mut self, module: &str, op: &str) -> Self {
        self.module = Some(module.into());
        self.validate = Some(op.into());
        self
    }

    fn logged(mut self, log: &str, records: &[(&str, &str)]) -> Self {
        self.log = Some(log.into());
        self.records = records.iter().map(|(k, c)| RecordEntry { kind: k.to_string(), collection: c.to_string() }).collect();
        self
    }
}

/// Every kind the store keeps. A test (`every_store_kind_is_governed`)
/// holds it to the kinds the published schemas name, and the server's
/// tests hold it to the envelopes the server writes.
pub fn kinds() -> Vec<KindEntry> {
    use Merge::*;
    use Read::*;
    use Write::*;
    vec![
        entry("edit_records", "edit_records", Route, "POST /edits", &["edits"], Lww, Open)
            .checked("sync", "sync_edits")
            .logged("disruptions", &[("edit_record", "records")]),
        entry("stops", "stops", Route, "POST /stops, POST /logs/stops", &["stops"], Log, Open)
            .checked("stops", "validate")
            .logged("stops", &[("stop", "stops")]),
        entry("stop_requests", "stop_requests", Route, "POST /stop-requests, POST /logs/stops", &["stop_requests"], Log, Open)
            .checked("requests", "validate")
            .logged("stops", &[("stop_request", "requests")]),
        entry("service_alerts", "service_alerts", Route, "POST /service-alerts", &[], Log, Open)
            .checked("alerts", "validate")
            .logged("alerts", &[("service_alert", "alerts")]),
        entry("alert_templates", "alert_templates", Route, "POST /alert-templates", &[], Log, Open)
            .checked("alerts", "validate_templates")
            .logged("alerts", &[("alert_template", "templates")]),
        entry("plans", "plans", Route, "POST /plans, POST /plans/adopt, POST /plans/retire", &["plans"], Log, Open)
            .checked("plans", "adopt")
            .logged("plans", &[("pick", "plans")]),
        entry("frozen_versions", "frozen_versions", Route, "POST /plans/freeze, POST /plans", &[], Log, Open)
            .checked("plans", "freeze")
            .logged("plans", &[("frozen_version", "versions")]),
        entry("network_document", "network", Route, "POST /plans/freeze, POST /plans; the server's import", &["plan.*", "network"], Replace, Open)
            .checked("network", "publishable"),
        entry("boundary", "boundary", Server, "the server's import of the GTFS zip", &["boundary"], None, Open),
        entry("streets", "streets", Server, "the server's import of the street layer", &["streets"], None, Open),
        entry("walks", "walks", Server, "the server's walk table", &["walks"], None, Open),
        entry("derivation", "derivation", Server, "the server's imports", &["derived.*"], None, Open),
        entry("policy", "policy", Pointer, "POST /pointers/policy", &["policy"], Replace, Restricted).checked("authz", "validate"),
        entry("manifest", "", Server, "the server's publishing", &["manifest"], None, Open),
        entry("build", "build", Route, "the server's -dist, and POST /builds", &["build", "build.*"], Replace, Open),
        entry("build_file", "", Route, "the server's -dist", &[], None, Open),
        entry("gtfs_feed", "", Server, "the server's -feed", &["source.feed"], None, Open),
        entry("street_layer", "", Server, "the server's -streets", &["source.streets"], None, Open),
        entry("log_entry", "log_entry", Route, "the log's own writes", &[], None, Audit),
        entry("log_checkpoint", "log_checkpoint", Server, "the server's fold", &[], None, Audit),
        entry("kind_table", "kind_table", Server, "the server's setup of the store", &[], None, Open),
        entry("store_layout", "store_layout", Server, "the server's setup of the store", &[], None, Open),
    ]
}
