# sync

What is the converged state of this edit document?

- `sync_edits` merges by LWW for each record id, plus tombstones. The agency scheduling system is the terminal authority (propose, review, accept). The receive path clamps client clocks to the host-passed `now_ms`. Then drafts transition to submitted. The answer carries the canonical document bytes and their hash, so a host stores exact bytes and never mints identity.
- `read_edits` reads a received document at receive. An older version is cap-migrate's to bring forward.
- `compact_edits` is the tombstone-retention commit.
- `log_records`, `log_document`, and `log_revert` are the edit document's half of the disruptions log (D-190). The log capability (cap-log) keeps the log: each entry is one write, and holds each record it changed. These ops turn the document into the log's records and back, and answer what a revert writes back.

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

## log_records

Answer the edit document as the disruptions log's records, as the log capability's `compose` takes them (`next`):

- one record of kind `edit_record` for each record, whole, by its id;
- the document's own record, of kind `edit_records` and id `document`: the document with its `records` empty. It keeps every field the document carries outside its records.

A record that the fold holds and the document does not leaves the fold, as compaction drops one.

## log_document

Assemble the edit document from the fold's records: the `edit_record` records, in id order, under the document's own record (absent: the empty document). The answer is the document's canonical bytes and its hash, which a host serves at `GET /edits`.

A record in another version is refused, and the error names both versions.

## receive

A write to the disruptions log, whole: the server's one call for `POST /edits`, `POST /pointers/edits`, the tombstone compaction, and the log's first entry. This capability holds no registry, so the host gives the edit document at the head inline (`documents.edit_records`; absent: the empty document). It takes the write (`write`: the fold's checkpoint, the entry's header, and the entries after the fold that the writer did not read) and the ops, each `{cap: "sync", op, args}`:

- `sync` (`{client, from}`): the sync protocol's write. The client's document is brought forward when it is older (a bare array is v1) and read as `read_edits` reads it. Its clock is clamped to the entry's (`at_ms`, D-56), then it is merged with the head and its drafts are submitted, as `sync_edits` does. With `from`, the hash of the head the writer read, a head that moved answers `moved`.
- `compact` (`{horizon_ms}`): the retired records older than the entry's clock minus the horizon leave the document (D-57). A retired record that a held record of another log names stays, so compaction alone never breaks a reference. The host gives those logs as the reference checks read them (`write.references`, `write.now`).
- `migrate`: the head read as `read_edits` reads it and recorded. The host sends it with the old `edits` pointer's head, or the dev store's file, to give an empty log its first entry.

The answer is the entry's canonical bytes, which the host stores at the header's seq, `heads.edit_records`, the document's hash after the write, and the fields the client's records carry that this build does not know (`unknown_fields`). No entry comes back when the write changes nothing, when its key landed before (`landed`), when a record it changes changed in an unread entry (`overlap`), when an op does not run (`failed`), or when the head moved (`moved`).

## log_revert

Answer the records that a revert of the disruptions log writes back, over the `states` that the log capability's `revert` answers. The host sends them back to `revert` as `written`.

- A record goes back to its state before the reverted write.
- A record that the reverted write created is retired, because the LWW merge would bring a dropped record back (D-193).
- Each is stamped with the next rev and `now_ms`, so it wins the merge over a copy a page still holds.

The document's own record is not answered: it goes back to its state before.
