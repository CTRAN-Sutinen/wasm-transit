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
//! `caps-run --load` — docs/FORMATS.md, "Versions").
//!
//! The chains:
//!
//! * `network` (transit-network): v1 — every untagged shape, pre-trip
//!   and pre-alignment included — → v2, tagged.
//! * `edit_records` (transit-sync): v1, the bare array → v2, the
//!   tagged document.
//! * `watchlist` (transit-rider): v1 notes and colors → v2 labels,
//!   the colors dropped and reported.
//! * `service_alerts` (transit-alerts): v1 → v2, where `explains` gains
//!   a trip instance (D-216). Every v1 document is a v2 document.
//! * `stop_requests` (transit-requests): v1 → v2, where a request's
//!   `routes` stop being a reference into the network (D-234). Every
//!   v1 document is a v2 document.
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
    (transit_rider::watchlist::KIND, transit_rider::watchlist::SCHEMA_VERSION),
    (transit_alerts::version::KIND, transit_alerts::ALERTS_VERSION),
    (transit_requests::version::KIND, transit_requests::SCHEMA_VERSION),
];

fn current(kind: &str) -> Result<u32, String> {
    KINDS.iter().find(|(k, _)| *k == kind).map(|(_, v)| *v).ok_or_else(|| {
        let known: Vec<&str> = KINDS.iter().map(|(k, _)| *k).collect();
        format!("no migration chain for kind {kind:?} (known: {})", known.join(", "))
    })
}

fn version_of(kind: &str, doc: &serde_json::Value) -> Result<u32, String> {
    match kind {
        transit_network::KIND => transit_network::schema_version_of(doc),
        transit_sync::KIND => transit_sync::version::schema_version_of(doc),
        transit_rider::watchlist::KIND => transit_rider::watchlist::schema_version_of(doc),
        transit_alerts::version::KIND => transit_alerts::version::schema_version_of(doc),
        transit_requests::version::KIND => transit_requests::version::schema_version_of(doc),
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
        transit_rider::watchlist::KIND => {
            let (doc, notes) = transit_rider::watchlist::migrate_to(doc, to)?;
            let notes = notes.into_iter().map(|n| MigrationNote { from: n.from, kind: n.kind, field: n.field, count: n.count });
            Ok((doc, notes.collect()))
        }
        transit_alerts::version::KIND => {
            let (doc, notes) = transit_alerts::version::migrate_to(doc, to)?;
            let notes = notes.into_iter().map(|n| MigrationNote { from: n.from, kind: n.kind, field: n.field, count: n.count });
            Ok((doc, notes.collect()))
        }
        // No step of this chain loses anything.
        transit_requests::version::KIND => Ok((transit_requests::version::migrate_to(doc, to)?, Vec::new())),
        _ => Err(current(kind).err().unwrap_or_default()),
    }
}

/// A current-version document's canonical bytes and hash: through its
/// type (RFC 8785, D-169), as every module that loads it hashes it.
fn canonical(kind: &str, doc: &serde_json::Value) -> Result<(String, String), String> {
    fn typed<T: serde::de::DeserializeOwned + Serialize>(doc: &serde_json::Value) -> Result<(String, String), String> {
        // Read through the kernel's own parse (one deserializer for the
        // type in a linked assembly, not a second one over JSON values).
        let text = serde_json::to_string(doc).expect("documents serialize");
        let typed: T = cap_abi::parse_document(text.as_bytes()).map_err(|e| format!("the migrated document does not read: {e}"))?;
        Ok((cap_abi::canonical_bytes(&typed)?, cap_abi::canonical_hash(&typed)?))
    }
    match kind {
        transit_network::KIND => typed::<transit_network::Network>(doc),
        transit_sync::KIND => typed::<transit_sync::EditDocument>(doc),
        transit_rider::watchlist::KIND => typed::<transit_rider::watchlist::Watchlist>(doc),
        transit_alerts::version::KIND => typed::<transit_alerts::ServiceAlerts>(doc),
        transit_requests::version::KIND => typed::<transit_requests::StopRequests>(doc),
        _ => loose(doc),
    }
}

/// An intermediate version has no type in this build: its canonical
/// bytes as a JSON value (RFC 8785, D-169).
fn loose(doc: &serde_json::Value) -> Result<(String, String), String> {
    let text = cap_abi::canonical_bytes(doc)?;
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

/// The worked examples, which the generator runs (no module build turns
/// the feature on).
#[cfg(feature = "examples")]
pub mod examples;

cap_abi::capability! {
    id: "migrate",
    json: dispatch_json,
    bytes: dispatch_bytes,
    documents: [],
    request: Request,
    speaks: &[
        ("network", &[1, transit_network::SCHEMA_VERSION]),
        ("operator_edits", &[1, transit_sync::SCHEMA_VERSION]),
        ("watchlist", &[1, 2]),
        ("service_alerts", &[1, transit_alerts::ALERTS_VERSION]),
        ("stop_requests", &[1, transit_requests::SCHEMA_VERSION]),
    ],
    migrates: &[
        ("network", &[(1, 2)]),
        ("operator_edits", &[(1, 2)]),
        ("watchlist", &[(1, 2)]),
        ("service_alerts", &[(1, 2)]),
        ("stop_requests", &[(1, 2)]),
    ],
}
