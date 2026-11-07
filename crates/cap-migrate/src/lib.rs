//! cap-migrate — the migration capability.
//!
//! One question: **what is this older document in the version this
//! build reads?** Every authored kind's chain is kept forever (D-66),
//! in the library crate that owns the kind — one pure step per version
//! (`vN → vN+1`), total over valid vN, preserving every record id,
//! reporting every loss. This module wraps those chains as one op, so
//! a consumer reads the current version only and never carries an old
//! chain; a host assembles it only when an older document needs it
//! (the receive boundaries: the server store, a browser's restore,
//! `caps-run --load` — docs/units/document-versioning.md).
//!
//! The chains:
//!
//! * `network` (transit-network): v1 — every untagged shape, pre-trip
//!   and pre-alignment included — → v2, tagged.
//! * `edit_records` (transit-sync): v1, the bare array → v2, the
//!   tagged document.
//! * `watchlist` (transit-dispatch): v1 notes and colors → v2 labels,
//!   the colors dropped and reported.
//!
//! Migration never destroys: the answer is a NEW document (its own
//! hash); the caller keeps the old bytes. A document at or past the
//! target is refused — a newer document is never re-emitted by an
//! older reader. The module is stateless.

#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// One operation call. Tagged like every capability's seam.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "cap_migrate/")]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Request {
    /// Bring `document`, of `kind`, to version `to` (omitted: the
    /// version this build reads) through every step between.
    Migrate {
        /// "network", "edit_records" or "watchlist".
        kind: String,
        #[ts(type = "unknown")]
        document: serde_json::Value,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        to: Option<u32>,
    },
    /// The version a document is in, and the version this build reads
    /// — whether it needs `migrate`, without migrating it.
    Version {
        kind: String,
        #[ts(type = "unknown")]
        document: serde_json::Value,
    },
}

/// One loss a step applied — reported, never silent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "cap_migrate/")]
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

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "cap_migrate/")]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Response {
    Migrate {
        /// The migrated document, as JSON.
        #[ts(type = "unknown")]
        document: serde_json::Value,
        /// Its canonical bytes and their hash — the identity a host
        /// stores it under (the module is the hash authority).
        canonical: String,
        document_hash: String,
        from: u32,
        to: u32,
        notes: Vec<MigrationNote>,
    },
    Version {
        version: u32,
        /// The version this build reads (and `migrate` targets).
        reads: u32,
    },
}

/// The kinds this module migrates, and the version each reads.
pub const KINDS: &[(&str, u32)] = &[
    (transit_network::KIND, transit_network::SCHEMA_VERSION),
    (transit_sync::KIND, transit_sync::SCHEMA_VERSION),
    ("watchlist", 2),
];

fn current(kind: &str) -> Result<u32, String> {
    KINDS.iter().find(|(k, _)| *k == kind).map(|(_, v)| *v).ok_or_else(|| {
        let known: Vec<&str> = KINDS.iter().map(|(k, _)| *k).collect();
        format!("no migration chain for kind {kind:?} (known: {})", known.join(", "))
    })
}

/// The watchlist's version: its `"version": "vN"` tag.
fn watchlist_version(doc: &serde_json::Value) -> Result<u32, String> {
    doc.get("version")
        .and_then(serde_json::Value::as_str)
        .and_then(|v| v.strip_prefix('v'))
        .and_then(|n| n.parse().ok())
        .ok_or_else(|| "a watchlist names its version (\"version\": \"v1\")".into())
}

fn version_of(kind: &str, doc: &serde_json::Value) -> Result<u32, String> {
    match kind {
        transit_network::KIND => transit_network::schema_version_of(doc),
        transit_sync::KIND => transit_sync::version::schema_version_of(doc),
        "watchlist" => watchlist_version(doc),
        _ => current(kind),
    }
}

fn migrate(kind: &str, doc: serde_json::Value, to: u32) -> Result<(serde_json::Value, Vec<MigrationNote>), String> {
    match kind {
        transit_network::KIND => {
            let (doc, notes) = transit_network::migrate_to(doc, to)?;
            let notes = notes.into_iter().map(|n| MigrationNote { from: n.from, kind: n.kind, field: n.field, count: n.count });
            Ok((doc, notes.collect()))
        }
        transit_sync::KIND => {
            let (doc, notes) = transit_sync::version::migrate_to(doc, to)?;
            let notes = notes.into_iter().map(|n| MigrationNote { from: n.from, kind: n.kind, field: n.field, count: n.count });
            Ok((doc, notes.collect()))
        }
        "watchlist" => {
            let from = watchlist_version(&doc)?;
            if from >= to {
                return Err(format!("watchlist v{from} is not older than v{to}: nothing to migrate"));
            }
            let watchlist: transit_dispatch::watchlist::Watchlist =
                serde_json::from_value(doc).map_err(|e| format!("watchlist v{from}: {e}"))?;
            let (migrated, notes) = transit_dispatch::watchlist::migrate(&watchlist)?;
            let notes = notes.into_iter().map(|n| MigrationNote { from, kind: n.kind, field: n.field, count: n.count });
            Ok((serde_json::to_value(migrated).expect("watchlists serialize"), notes.collect()))
        }
        _ => Err(current(kind).err().unwrap_or_default()),
    }
}

/// A current-version document's canonical bytes and hash: through its
/// type (struct field order), as every module that loads it hashes it.
fn canonical(kind: &str, doc: &serde_json::Value) -> Result<(String, String), String> {
    fn typed<T: serde::de::DeserializeOwned + Serialize>(doc: &serde_json::Value) -> Result<(String, String), String> {
        // Read through the kernel's own parse (one deserializer for the
        // type in a linked assembly, not a second one over JSON values).
        let text = serde_json::to_string(doc).expect("documents serialize");
        let typed: T = cap_abi::parse_document(text.as_bytes()).map_err(|e| format!("the migrated document does not read: {e}"))?;
        Ok((serde_json::to_string(&typed).expect("documents serialize"), cap_abi::canonical_hash(&typed)?))
    }
    match kind {
        transit_network::KIND => typed::<transit_network::Network>(doc),
        transit_sync::KIND => typed::<transit_sync::EditDocument>(doc),
        "watchlist" => typed::<transit_dispatch::watchlist::Watchlist>(doc),
        _ => loose(doc),
    }
}

/// An intermediate version has no type in this build: its bytes as
/// JSON writes them (keys sorted).
fn loose(doc: &serde_json::Value) -> Result<(String, String), String> {
    let text = serde_json::to_string(doc).expect("documents serialize");
    let hash = cap_abi::content_hash(text.as_bytes());
    Ok((text, hash))
}

/// The typed dispatch (native hosts call this directly). Failures
/// are `{"error": ...}`, never panics — a total seam.
pub fn dispatch(request: Request) -> Result<Response, String> {
    match request {
        Request::Migrate { kind, document, to } => {
            let reads = current(&kind)?;
            let to = to.unwrap_or(reads);
            if to > reads {
                return Err(format!("{kind} v{to} is newer than this build (it reads up to v{reads})"));
            }
            let from = version_of(&kind, &document)?;
            if from > reads {
                return Err(format!(
                    "{kind} v{from} is newer than this build, which reads v{reads}: a newer build wrote it (refused, never rewritten)"
                ));
            }
            let (document, notes) = migrate(&kind, document, to)?;
            let (canonical, document_hash) = if to == reads { canonical(&kind, &document)? } else { loose(&document)? };
            Ok(Response::Migrate { document, canonical, document_hash, from, to, notes })
        }
        Request::Version { kind, document } => {
            let reads = current(&kind)?;
            Ok(Response::Version { version: version_of(&kind, &document)?, reads })
        }
    }
}

/// The JSON seam: `{"op": ...}` in, `{"ok": ...}` / `{"error": ...}`
/// out — byte-compatible with every other capability module.
pub fn dispatch_json(text: &str) -> String {
    cap_abi::answer(text, dispatch)
}

/// Documents travel inline — no bulk op of its own; the seam answers
/// honestly instead of silently ignoring one.
pub fn dispatch_bytes(op: u32, _bytes: &[u8]) -> String {
    cap_abi::outcome::<()>(Err(format!("unknown bulk op {op}: migrate takes its document inline")))
}

cap_abi::capability! {
    id: "migrate",
    json: dispatch_json,
    bytes: dispatch_bytes,
    documents: [],
}
