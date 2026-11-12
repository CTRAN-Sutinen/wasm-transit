# sync

What is the converged state of this edit document? sync_edits: LWW merge per record id plus tombstones (the terminal authority is the agency scheduling system — propose, review, accept), the receive path clamping client clocks to the host-passed now_ms (D-56), then drafts transition to submitted; the answer carries the canonical document bytes and their hash, so a host stores exact bytes and never mints identity. read_edits reads a received document at receive (an older version is cap-migrate's to bring forward); compact_edits is the D-57 tombstone-retention commit. transit-sync is the domain library; the module is stateless — document storage is the host's (the -store, docs/FORMATS.md), and what a record amounts to on a network is the divert capability's reports, never this one's.

## sync_edits

Merge the client's edit document with the server's (LWW per record id — the newer updated_at wins, tombstones propagate), then transition drafts to submitted (rev bumped, now_ms stamped). The receive path clamps the client's updated_at to now_ms first (D-56). The answer is the converged document — records, the canonical document bytes, and document_hash; the client adopts it wholesale. The host owns both documents and the clock.

## read_edits

Read a received or stored edit document at the receive boundary, in the version this module reads ({"kind": "edit_records", "schema_version": 2, "records"}) — an older one (a bare array is v1) is refused naming the migration capability, a newer one naming both versions: serde defaults fill what age removed, malformed records are dropped and counted (never wedging the document). The answer carries the records, the dropped count, the schema_version, and the canonical document + hash for seeding a store pointer.

## compact_edits

The D-57 tombstone-retention commit: drop records that are retired AND older than now_ms - horizon_ms; in-force records are never dropped. Answer: the compacted records, the canonical document + hash, and the dropped count.
