# sync

What is the converged state of this edit document?

- `sync_edits` merges by LWW for each record id, plus tombstones. The agency scheduling system is the terminal authority (propose, review, accept). The receive path clamps client clocks to the host-passed `now_ms`. Then drafts transition to submitted. The answer carries the canonical document bytes and their hash, so a host stores exact bytes and never mints identity.
- `read_edits` reads a received document at receive. An older version is cap-migrate's to bring forward.
- `compact_edits` is the tombstone-retention commit.
- `entry_disruptions`, `fold_disruptions`, `revert_disruptions`, and `close_disruptions_gap` are the disruptions log (D-190). The log is the record of the edit document. Each entry is one write, and holds each record it changed: the hash before and the new state. The document is the fold of the entries.

transit-sync is the domain library. The module is stateless. Document storage is the host's (the `-store`, docs/FORMATS.md). What a record amounts to on a network is the divert capability's `reports`, never this capability's answer.

## sync_edits

Merge the client's edit document with the server's. The merge is LWW for each record id: the newer `updated_at` wins, and tombstones propagate. Then drafts transition to submitted, with the rev bumped and `now_ms` stamped.

The receive path first clamps the client's `updated_at` to `now_ms`.

The answer is the converged document: the records, the canonical document bytes, and `document_hash`. The client adopts it wholesale.

The host owns both documents and the clock.

## read_edits

Read a received or stored edit document at the receive boundary, in the version this module reads: `{"kind": "edit_records", "schema_version": 2, "records"}`.

- An older document is refused, and the error names the migration capability. A bare array is v1.
- A newer document is refused, and the error names both versions.
- Serde defaults fill what age removed.
- Malformed records are dropped and counted. They never wedge the document.

The answer carries the records, the `dropped` count, the `schema_version`, and the canonical document and hash, for seeding a store pointer.

## compact_edits

The tombstone-retention commit: drop the records that are retired and older than `now_ms - horizon_ms`. In-force records are never dropped.

The answer holds the compacted records, the canonical document and hash, and the dropped count.

## entry_disruptions

Answer the disruptions log's entry for one write: each record that differs between `head` (the edit document the write read: the fold) and `next` (the edit document after the write), in id order, with its canonical hash before and its new state, whole.

The host gives the `header`: the `seq` it will create, the writer, its clock, the module build, and the intent (the endpoint, and the hash of the arguments). The answer is the entry's canonical bytes, which the host stores at `logs/disruptions/entries/<seq>.json`.

A write that changes nothing answers no `entry`, and the host writes nothing. So a resend of the same document is idempotent.

## fold_disruptions

Fold the disruptions log: the entries after a `checkpoint` (or from the empty document), in `seq` order, as the host read them (`{seq, text}`).

An entry that does not read, that names another `seq` than its key, or whose `before` hashes do not match the fold, is quarantined. The answer names it in `quarantined`, and the fold continues. The host raises an alarm for each one.

The answer is the edit document (its canonical bytes and hash) and the checkpoint at the last `seq`. A host serves the document, and may store the checkpoint at `logs/disruptions/checkpoints/<seq>.json`.

## revert_disruptions

Answer the entry that reverts entry `target`. Each record that entry changed goes back to its state before the entry, with the next rev and the header's clock, so it wins the merge over a copy a page still holds. A record the entry created is retired.

The revert is refused, naming the record, when a later entry changed one of those records. A person decides what it becomes.

`checkpoint` is at or before `target - 1`. `entries` runs from after the checkpoint to the head, with no gap.

## close_disruptions_gap

Answer the entry that closes a gap in the disruptions log: no changes, and the cause in `header.intent.note`, which is required. The host creates it at the missing `seq`, and writes to the log go on.
