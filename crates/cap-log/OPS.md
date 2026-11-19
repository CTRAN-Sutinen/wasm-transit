# log

What does this write add to the log, and what does the log hold? A log is the record of one consistency domain (D-190): an entry for each write, and the current state the fold of the entries. A mistake is fixed by a new entry, so the truth before it stays in the record (library: transit-log).

The capability never reads a record. A record is a JSON value named by its kind (`stop`, `stop_request`) and its id, in its kind's version. The capability that owns a kind turns its document into records and records back into its document (`log_records`, `log_document`). So one entry may change the records of two capabilities, and no capability calls another (D-197).

A change's `before` is the canonical hash of the record as the entry that wrote it stored it, never of a migrated copy. The module is stateless: the host stores each entry at `logs/<log>/entries/<seq>.json` and each checkpoint at `logs/<log>/checkpoints/<seq>.json`, and gives them back in order.

## compose

Answer the entry for one write. `next` names, for each kind the write touched, every record of that kind's document after the write (`{kind, version, records}`, as `log_records` answers it). The entry holds each record that differs from the fold `checkpoint` (absent: the empty log), with its hash before and its new state, in kind and id order. A record of a named kind that `next` leaves out leaves the fold.

The host gives the `header`: the log, the `seq` it will create (the fold's `seq` plus one), the writer, its clock, the module build, and the intent (the endpoint, the hash of the arguments, and the writer's idempotency `key`).

The answer is the entry's canonical bytes, or no entry when nothing changed. When the fold holds the write's key, the write landed before: the answer is no entry and `landed`, the entry that holds it. So an outbox's resend lands once.

## fold

Fold a log's entries after a `checkpoint` (absent: the empty log), in `seq` order, as the host read them (`{seq, text}`).

An entry that does not read, that names another log or another `seq` than its key, or whose `before` hashes do not match the fold, is quarantined. The answer names it in `quarantined`, and the fold goes on. The host raises an alarm for each.

The answer is the last `seq`, the `records` by kind and id (each `{version, value}`, what each kind's `log_document` assembles), and the `checkpoint` at that `seq`, canonically serialized, which a host may store.

## revert

Answer the entry that reverts entry `target`. Each record that entry changed goes back to its state before the entry. A record the entry created leaves the fold, and the log keeps every state of it (D-198).

The revert is refused, naming the record, when a later entry changed one of those records: a person decides what it becomes.

The answer's `states` gives each record the revert writes back, with its state now and before the entry. A kind whose merge would bring the state before back over the revert asks its capability for the records to write instead, and sends them as `written`. The edit document's records are stamped to win the merge, and a record the entry created is retired (D-193).

`checkpoint` is at or before `target - 1`. `entries` runs from after the checkpoint to the head, with no gap. `header.seq` is the head plus one.

## close_gap

Answer the entry that closes a gap in a log: no changes, and the cause in `header.intent.note`, which is required. The host creates it at the missing `seq`, and writes to the log go on.
