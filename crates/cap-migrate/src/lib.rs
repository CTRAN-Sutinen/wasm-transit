//! The chains live in the libraries that own each kind (D-66).
//! Migration never destroys, and an older reader never writes a newer
//! document (docs/FORMATS.md, "Versions").

#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "cap_migrate/")]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Request {
    Migrate {
        kind: String,
        #[ts(type = "unknown")]
        document: serde_json::Value,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        to: Option<u32>,
    },
    Version {
        kind: String,
        #[ts(type = "unknown")]
        document: serde_json::Value,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "cap_migrate/")]
pub struct MigrationNote {
    pub from: u32,
    pub kind: String,
    pub field: String,
    pub count: u32,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "cap_migrate/")]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Response {
    Migrate {
        #[ts(type = "unknown")]
        document: serde_json::Value,
        canonical: String,
        document_hash: String,
        from: u32,
        to: u32,
        notes: Vec<MigrationNote>,
    },
    Version {
        version: u32,
        reads: u32,
    },
}

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

/// Hashes through the type (RFC 8785, D-169), as every module that loads the
/// document does.
fn canonical(kind: &str, doc: &serde_json::Value) -> Result<(String, String), String> {
    fn typed<T: serde::de::DeserializeOwned + Serialize>(doc: &serde_json::Value) -> Result<(String, String), String> {
        // The kernel's parse is the one deserializer for the type in a linked
        // assembly.
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

/// An intermediate version has no type in this build, so it hashes as a JSON
/// value (RFC 8785, D-169).
fn loose(doc: &serde_json::Value) -> Result<(String, String), String> {
    let text = cap_abi::canonical_bytes(doc)?;
    let hash = cap_abi::content_hash(text.as_bytes());
    Ok((text, hash))
}

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

pub fn dispatch_json(text: &str) -> String {
    cap_abi::answer(text, dispatch)
}

pub fn dispatch_bytes(op: u32, _bytes: &[u8]) -> String {
    cap_abi::outcome::<()>(Err(format!("unknown bulk op {op}: migrate takes its document inline")))
}

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
