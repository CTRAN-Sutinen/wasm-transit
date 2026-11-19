//! cap-sync — the sync capability.
//!
//! One question: **what is the converged state of this edit
//! document?** The sync protocol is deliberately LWW per record id
//! plus tombstones (the terminal authority is the agency scheduling
//! system: propose → review → accept). The ops are that document's
//! lifecycle facets: `sync_edits` merges a client document with the
//! server's and transitions drafts to submitted; `read_edits`
//! reads a received/stored document at the receive boundary (an
//! older version is the migration capability's to bring forward);
//! `compact_edits` is the D-57 tombstone-retention commit. The
//! disruptions log (D-190, D-197) is the record of that document, kept
//! by the log capability (cap-log): `log_records` answers the document
//! as the log's records (one `edit_record` for each record, and the
//! document's own record), `log_document` assembles the document from
//! the fold's records, and `log_revert` answers the records a revert
//! writes back, stamped to win the merge (D-193). The host
//! owns both documents (wholesale-replaced, JS/store-owned) and the
//! clock (`now_ms` is the only route; hosts own time), and clamps the
//! incoming clock at receive (D-56). transit-sync is the domain
//! library — the semilattice merge is property-tested there; this
//! module is the seam. Answers carry the canonical `document` bytes
//! and their `document_hash` (the module is the hash authority), so a
//! host stores exact bytes and never mints identity.
//!
//! Validation is NOT here: edit records validate against the
//! NETWORK at load_edits — every model-side module's registration
//! answer (reported, never rejected — the sync contract). A second
//! answer here would be two answers to one question on the bus.
//! The module is stateless: no registries, no persistence —
//! document storage is the host's, and the sync store's design
//! (the log's objects, their create-if-absent — docs/FORMATS.md) is
//! the host discipline's, never a module's.

#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// One operation call. Tagged like every capability's seam.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "cap_sync/")]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Request {
    /// Merge the client's edit document with the server's (LWW per
    /// record id — `transit_sync::merge`, the property-tested
    /// semilattice), then transition drafts to submitted (the
    /// submit bumps rev and stamps `now_ms`). The receive path clamps
    /// the client's `updated_at` to `now_ms` first (D-56), so the
    /// server clock bounds the LWW order. The answer is the converged
    /// document — as records, and as the canonical `document` bytes
    /// with their `document_hash` — and the client adopts it
    /// wholesale.
    SyncEdits {
        client: Vec<transit_sync::EditRecord>,
        server: Vec<transit_sync::EditRecord>,
        #[ts(type = "number")]
        now_ms: u64,
    },
    /// Read a received or stored edit document in the version this
    /// module reads — an older one is refused naming the migration
    /// capability, a newer one naming both versions — with serde
    /// defaults filling what age removed and malformed records dropped
    /// and counted, never wedging the document
    /// (`transit_sync::read_document`).
    ReadEdits {
        #[ts(type = "any")]
        document: serde_json::Value,
    },
    /// The D-57 compaction commit: drop tombstones older than
    /// `now_ms - horizon_ms`, as an ordinary (host-CAS'd) commit.
    CompactEdits {
        records: Vec<transit_sync::EditRecord>,
        #[ts(type = "number")]
        now_ms: u64,
        #[ts(type = "number")]
        horizon_ms: u64,
    },
    /// The edit document as the disruptions log's records (D-197): one
    /// record of kind `edit_record` for each record, whole, and the
    /// document's own record, as the log capability's `compose` takes
    /// them.
    LogRecords { document: transit_sync::EditDocument },
    /// The edit document assembled from the disruptions log's
    /// `edit_record` records, in id order, answered as its canonical
    /// bytes and hash. A record in another version is refused, naming
    /// both.
    LogDocument {
        #[ts(type = "Record<string, { version: number, value: unknown }>")]
        records: std::collections::BTreeMap<String, transit_log::Held>,
        /// The document's own record (absent: the empty document's).
        #[serde(default)]
        #[ts(optional, type = "{ version: number, value: unknown }")]
        document: Option<transit_log::Held>,
    },
    /// The records a revert of the disruptions log writes back (D-193),
    /// over the log capability's `states`: each edit record as it was
    /// before the reverted write, or retired when that write created it,
    /// stamped with the next rev and `now_ms` so it wins the merge.
    LogRevert {
        #[ts(type = "Array<{ kind: string, id: string, now?: { version: number, value: unknown }, prior?: { version: number, value: unknown } }>")]
        states: Vec<transit_log::RevertState>,
        #[ts(type = "number")]
        now_ms: u64,
    },
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "cap_sync/")]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Response {
    SyncEdits {
        records: Vec<transit_sync::EditRecord>,
        /// The edit document over `records`, canonically serialized
        /// (`{"kind":"edit_records","schema_version":…,"records":[…]}`)
        /// — the exact bytes a host stores.
        document: String,
        /// The document's canonical hash — its identity.
        document_hash: String,
        /// The fields the client's records carry that this build does
        /// not know (D-167): kept in them, and flagged for a person,
        /// never refused. Absent when every field is known.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        #[ts(optional, as = "Option<Vec<String>>")]
        unknown_fields: Vec<String>,
    },
    ReadEdits {
        records: Vec<transit_sync::EditRecord>,
        dropped: usize,
        schema_version: u32,
        /// The edit document over the read `records`, canonically
        /// serialized — the exact bytes a host stores when seeding a
        /// pointer.
        document: String,
        document_hash: String,
    },
    CompactEdits {
        records: Vec<transit_sync::EditRecord>,
        document: String,
        document_hash: String,
        dropped: usize,
    },
    LogRecords {
        /// The records of each kind: the edit records, and the
        /// document's own record (`document`), which keeps the fields
        /// outside the collection.
        #[ts(type = "Array<{ kind: string, version: number, records: Record<string, unknown> }>")]
        next: Vec<transit_log::KindRecords>,
    },
    LogDocument {
        /// The document's canonical bytes, which a host serves under
        /// `document_hash`.
        document: String,
        document_hash: String,
    },
    LogRevert {
        /// The records to write back, as the log capability's `revert`
        /// takes them (`written`).
        #[ts(type = "Array<{ kind: string, id: string, version: number, after?: unknown }>")]
        written: Vec<transit_log::Written>,
    },
}

/// The typed dispatch (native hosts call this directly). Failures
/// are `{"error": ...}`, never panics — a total seam.
pub fn dispatch(request: Request) -> Result<Response, String> {
    match request {
        Request::SyncEdits { client, server, now_ms } => {
            let unknown_fields = transit_sync::unknown_fields(&client);
            let client = transit_sync::clamp_timestamps(&client, now_ms);
            let records = transit_sync::submit(transit_sync::merge(&client, &server), now_ms);
            let (document, document_hash) = canonical(&records)?;
            Ok(Response::SyncEdits { records, document, document_hash, unknown_fields })
        }
        Request::ReadEdits { document } => {
            let (read, dropped) = transit_sync::read_document(&document)?;
            let (canonical, hash) = canonical(&read.records)?;
            Ok(Response::ReadEdits {
                records: read.records,
                dropped,
                schema_version: transit_sync::SCHEMA_VERSION,
                document: canonical,
                document_hash: hash,
            })
        }
        Request::CompactEdits { records, now_ms, horizon_ms } => {
            let (records, dropped) = transit_sync::compact(&records, now_ms, horizon_ms);
            let (document, document_hash) = canonical(&records)?;
            Ok(Response::CompactEdits { records, document, document_hash, dropped })
        }
        Request::LogRecords { document } => {
            let doc = serde_json::to_value(&document).map_err(|e| e.to_string())?;
            Ok(Response::LogRecords { next: transit_log::document_records(&doc, "records", RECORD_KIND, transit_sync::KIND, transit_sync::SCHEMA_VERSION)? })
        }
        Request::LogDocument { records, document } => {
            let template = serde_json::json!({"kind": transit_sync::KIND, "schema_version": transit_sync::SCHEMA_VERSION});
            let doc: transit_sync::EditDocument = transit_log::assemble(template, document.as_ref(), "records", &records, true)?;
            Ok(Response::LogDocument { document: cap_abi::canonical_bytes(&doc)?, document_hash: cap_abi::canonical_hash(&doc)? })
        }
        Request::LogRevert { states, now_ms } => {
            let record = |held: &Option<transit_log::Held>, id: &str| -> Result<Option<transit_sync::EditRecord>, String> {
                match held {
                    None => Ok(None),
                    Some(h) if h.version != transit_sync::SCHEMA_VERSION => {
                        Err(format!("record {id}: {}", transit_version::refused(transit_sync::KIND, h.version, transit_sync::SCHEMA_VERSION)))
                    }
                    Some(h) => serde_json::from_value(h.value.clone()).map(Some).map_err(|e| format!("record {id}: {e}")),
                }
            };
            let mut written = Vec::new();
            for s in states.iter().filter(|s| s.kind == RECORD_KIND) {
                let back = transit_sync::revert_record(record(&s.prior, &s.id)?.as_ref(), record(&s.now, &s.id)?.as_ref(), now_ms);
                let after = back.map(|r| serde_json::to_value(r).map_err(|e| e.to_string())).transpose()?;
                written.push(transit_log::Written { kind: RECORD_KIND.into(), id: s.id.clone(), version: transit_sync::SCHEMA_VERSION, after });
            }
            Ok(Response::LogRevert { written })
        }
    }
}

/// The kind of an edit record in the disruptions log.
pub const RECORD_KIND: &str = "edit_record";

/// The edit document over `records` (current version, tagged): its
/// canonical bytes and their hash.
fn canonical(records: &[transit_sync::EditRecord]) -> Result<(String, String), String> {
    let document = transit_sync::EditDocument::new(records.to_vec());
    Ok((cap_abi::canonical_bytes(&document)?, cap_abi::canonical_hash(&document)?))
}

/// The JSON seam: `{"op": ...}` in, `{"ok": ...}` / `{"error": ...}`
/// out — byte-compatible with every other capability module.
pub fn dispatch_json(text: &str) -> String {
    cap_abi::answer(text, dispatch)
}

/// The edit document is a JSON document — no bulk-bytes op exists;
/// the seam answers honestly instead of silently ignoring one.
pub fn dispatch_bytes(op: u32, _bytes: &[u8]) -> String {
    serde_json::json!({ "error": format!("unknown bulk op {op}: the edit document is JSON (sync_edits)") })
        .to_string()
}

cap_abi::capability! {
    id: "sync",
    json: dispatch_json,
    bytes: dispatch_bytes,
    documents: [],
}
