# cap-sync

The sync capability: the converged state of an edit document, with its lifecycle facets. It is stateless.
- `sync_edits` does the LWW merge and the submit transition. The receive path clamps client clocks to the host's (D-56). The answer carries the canonical document bytes and their hash. The dev store's POST /edits answers through it.
- `read_edits` is the receive. It reads the current version only. An older version is cap-migrate's. It does a typed parse, defaults fill what age removed, and it drops and counts malformed records.
- `compact_edits` is the D-57 tombstone-retention commit.
- `receive` writes the disruptions log whole (D-228): `sync` (a client's document brought forward with transit-sync's own migration, read, clamped, merged, and submitted), `compact` (which skips a tombstone that a held record of another log names, D-231), and `migrate`, over the head the host gives inline. This is the op behind POST /edits in store mode.
- `log_records`, `log_document`, and `log_revert` are the edit document's half of the disruptions log, which cap-log keeps: the document as records (`edit_record`, and the document's own record), the document assembled from the fold's records, and the records a revert writes back, stamped to win the merge (D-193). The server stores the entries ([storage.md](storage.md)).

Validation is not here. What a record amounts to on a network is cap-divert's reports: one question, one answer.
