# cap-log

The log capability: what does this write add to the log, and what does the log hold? transit-log is the library. It is stateless, and it never reads a record.
- `compose` gives one write's entry: each record that differs from the fold, with its hash as stored before. It gives no entry when nothing changed, and the entry that holds the key when the write's idempotency key landed before. Given the entries after the fold its writer read (`unread`), it gives no entry and the `overlap` when one of them changed a record the write changes: a write is checked against the records it changes, not the whole log.
- `fold` folds the entries after a checkpoint into the records by kind and id, and gives the checkpoint. It quarantines an entry it cannot read or apply.
- `revert` gives the entry that writes back the states before an entry. A record the entry created leaves the fold (D-198). A record a later entry changed refuses it. Its answer gives each record's state now and before (`states`). A kind's capability may answer other records to write back (`written`), and the host sends them with the revert again.
- `close_gap` gives the empty entry that closes a gap, with its cause.

The server keeps the disruptions log, the stops log, and the plans log through it, and folds and reverts every log with it. The alerts log and the notes log are written by their capabilities' receive ops ([server.md](server.md), "One path for a log").
