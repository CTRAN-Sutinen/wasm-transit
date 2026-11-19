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
//! disruptions log (D-190) is the record of that document: `entry_disruptions` answers
//! the entry for one write (each changed record's hash before and its
//! new state), `fold_disruptions` folds the entries into the document and a
//! checkpoint (quarantining an entry it cannot apply), `revert_disruptions`
//! answers the entry that writes back the states before an entry, and
//! `close_disruptions_gap` answers the empty entry that closes a gap. The host
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
    /// The disruptions log's entry for one write (D-190): the records that
    /// differ between `head` (the edit document the write read) and
    /// `next` (the document after it), each with its hash before and its
    /// new state. The host gives the entry's place and provenance
    /// (`header`). A write that changes nothing answers no entry.
    EntryDisruptions {
        head: transit_sync::EditDocument,
        next: transit_sync::EditDocument,
        header: transit_sync::log::Header,
    },
    /// The fold of the disruptions log: the entries after a checkpoint (or
    /// from the empty document), in `seq` order. An entry that does not
    /// read or apply is quarantined and named, and the fold continues.
    /// The answer is the edit document and the checkpoint at the last
    /// `seq`.
    FoldDisruptions {
        #[serde(default)]
        #[ts(optional, type = "any")]
        checkpoint: Option<serde_json::Value>,
        entries: Vec<transit_sync::log::Stored>,
    },
    /// The entry that reverts entry `target`: each record it changed
    /// goes back to its state before, stamped to win the merge, and a
    /// record it created is retired. Refused, naming the record, when a
    /// later entry changed one of them. `checkpoint` is at or before
    /// `target - 1`, and `entries` runs from after it to the head.
    RevertDisruptions {
        #[serde(default)]
        #[ts(optional, type = "any")]
        checkpoint: Option<serde_json::Value>,
        entries: Vec<transit_sync::log::Stored>,
        #[ts(type = "number")]
        target: u64,
        header: transit_sync::log::Header,
    },
    /// The entry that closes a gap in the disruptions log (D-190, rule 4): no
    /// changes, and the cause in `header.intent.note`.
    CloseDisruptionsGap { header: transit_sync::log::Header },
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
    EntryDisruptions {
        /// The entry's canonical bytes, which the host stores at its
        /// `seq`. Absent when the write changes nothing.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        entry: Option<String>,
        /// How many records the entry changes.
        changes: usize,
    },
    FoldDisruptions {
        /// The last entry the fold read.
        #[ts(type = "number")]
        seq: u64,
        /// The edit document, canonically serialized.
        document: String,
        document_hash: String,
        /// The checkpoint at `seq`, canonically serialized.
        checkpoint: String,
        /// The entries the fold did not apply. The host raises an alarm
        /// for each.
        quarantined: Vec<transit_sync::log::Quarantined>,
    },
    RevertDisruptions {
        entry: String,
        changes: usize,
    },
    CloseDisruptionsGap {
        entry: String,
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
        Request::EntryDisruptions { head, next, header } => {
            let changes = transit_sync::log::changes(&head.records, &next.records, &record_hash);
            let n = changes.len();
            let entry = (n > 0).then(|| cap_abi::canonical_bytes(&transit_sync::log::Entry::new(header, changes, None))).transpose()?;
            Ok(Response::EntryDisruptions { entry, changes: n })
        }
        Request::FoldDisruptions { checkpoint, entries } => {
            let start = checkpoint.map(cap_abi::versioned::<transit_sync::log::Checkpoint>).transpose()?;
            let folded = transit_sync::log::fold(start.as_ref(), &entries, &record_hash)?;
            let (document, document_hash) = canonical(&folded.records)?;
            let checkpoint = cap_abi::canonical_bytes(&transit_sync::log::Checkpoint::new(folded.seq, folded.records))?;
            Ok(Response::FoldDisruptions { seq: folded.seq, document, document_hash, checkpoint, quarantined: folded.quarantined })
        }
        Request::RevertDisruptions { checkpoint, entries, target, header } => {
            use transit_sync::log;
            let start = checkpoint.map(cap_abi::versioned::<log::Checkpoint>).transpose()?;
            let from = start.as_ref().map_or(0, |c| c.seq);
            if target <= from {
                return Err(format!("entry {target} is not after the checkpoint at {from}"));
            }
            let at = entries.iter().position(|e| e.seq == target).ok_or_else(|| format!("entry {target} is not in the entries given"))?;
            let before = log::fold(start.as_ref(), &entries[..at], &record_hash)?;
            if before.seq != target - 1 {
                return Err(format!("the entries before {target} end at {}: the log has a gap", before.seq));
            }
            let head = log::fold(start.as_ref(), &entries, &record_hash)?;
            if let Some(q) = head.quarantined.iter().find(|q| q.seq == target) {
                return Err(format!("entry {target} is quarantined ({}): it changed nothing to revert", q.reason));
            }
            let entry = log::read_entry(&entries[at])?;
            let changes = log::revert(&before.records, &head.records, &entry, header.at_ms, &record_hash)?;
            if changes.is_empty() {
                return Err(format!("entry {target} changed no record"));
            }
            let n = changes.len();
            Ok(Response::RevertDisruptions { entry: cap_abi::canonical_bytes(&log::Entry::new(header, changes, Some(target)))?, changes: n })
        }
        Request::CloseDisruptionsGap { header } => {
            if header.intent.note.as_deref().is_none_or(|n| n.trim().is_empty()) {
                return Err("an entry that closes a gap names its cause (header.intent.note)".into());
            }
            Ok(Response::CloseDisruptionsGap { entry: cap_abi::canonical_bytes(&transit_sync::log::Entry::new(header, Vec::new(), None))? })
        }
    }
}

/// A record's identity in an entry: the content hash of its canonical
/// bytes. A record that does not serialize (a non-finite coordinate,
/// which the receive path drops first) gets a name that never equals a
/// hash.
fn record_hash(record: &transit_sync::EditRecord) -> String {
    cap_abi::canonical_hash(record).unwrap_or_else(|e| format!("unhashable: {e}"))
}

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
