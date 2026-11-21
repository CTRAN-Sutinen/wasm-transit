# log

What does this write add to the log, and what does the log hold? A log is the record of one consistency domain (D-190): an entry for each write, and the current state the fold of the entries. A mistake is fixed by a new entry, so the truth before it stays in the record (library: transit-log).

The capability never reads a record. A record is a JSON value named by its kind (`stop`, `stop_request`) and its id, in its kind's version. The capability that owns a kind turns its document into records and records back into its document (`log_records`, `log_document`). So one entry may change the records of two capabilities, and no capability calls another (D-197).

A change's `before` is the canonical hash of the record as the entry that wrote it stored it, never of a migrated copy. The module is stateless: the host stores each entry at `logs/<log>/entries/<seq>.json` and each checkpoint at `logs/<log>/checkpoints/<seq>.json`, and gives them back in order.

## compose

Answer the entry for one write. `next` names, for each kind the write touched, every record of that kind's document after the write (`{kind, version, records}`, as `log_records` answers it). The entry holds each record that differs from the fold `checkpoint` (absent: the empty log), with its hash before and its new state, in kind and id order. A record of a named kind that `next` leaves out leaves the fold.

The host gives the `header`: the log, the `seq` it will create (the fold's `seq` plus one), the writer, its clock, the module build, and the intent (the endpoint, the hash of the arguments, and the writer's idempotency `key`).

The answer is the entry's canonical bytes, or no entry when nothing changed. When the fold holds the write's key, the write landed before: the answer is no entry and `landed`, the entry that holds it. So an outbox's resend lands once.

`unread` are the stored entries after the fold the writer read (the log's seq it names, `read_seq`), as the host read them. A record the write changes that one of them changed too answers no entry and `overlap`: the record's kind and id, and the latest such entry. A person settles it. A change to another record is no overlap, so a write is checked against the records it changes, never the whole log. A key that landed answers before an overlap. The intent's `claimed_at_ms` is the device's clock when the person made a write that waited: the entry keeps it as a claim beside the host's `at_ms` (D-222).

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

## references

Answer each broken reference of the folds (D-232, D-233): a reference a kind declares whose target record its log's fold does not hold. `kinds` are the kind table's kinds as `just caps` writes them; the op reads each one's `log`, `records`, and `references`. `folds` are the logs' records by log name, as `fold` answers them.

A reference path names a field of the kind's document (`/requests/*/stop`). Its first segment is the collection and its second the record's id, so the rest of the path is read in each record of that collection. A segment is a field, `*` (each element or value), or either with a filter (`target[kind=alert]`).

A target is judged when it is a record kind that a log keeps and that log's fold is given. A record the fold holds exists in any state: retired is a state, and the rules of each reference judge states. The network's things, a trip instance, a document or an attachment by hash, an entry by seq, and an external thing are not judged here. Each broken reference names the referring record (`log`, `kind`, `id`), the `field` in it (a JSON pointer inside the record), the `target`, and the id it `names`, in (log, kind, id, field) order.

A referring record in a version newer than the kind table reads (after a rollback, D-186) is not judged: the table's declarations may not be its fields. The answer lists it in `unjudged` (its log, kind, id, `version`, and the version the table `reads`), a warning, never a broken reference.

With `entries` (each log's stored entries, `{seq, text}`), each broken reference also names the `entry` that broke it: the later of the entry that last wrote the referring record and the entry that last changed the target. In one log the later is the higher seq; across two logs it is the later clock. A target never written names the referrer's entry. `store verify` asks it this way.

With `log` and `entry` (one entry of that log, `{seq, text}`), the answer is only the broken references that entry touches: those whose referring record it wrote, and those whose target it changed, each naming that entry. The server asks it this way after a revert, and answers them as the revert's warnings. A write's own warnings, and its refusal, come from its log's receive op (`transit_log::receive`, given the kind table's kinds and each log's records as of the seqs the writer read and now).

## naming

Answer each declared reference of the folds whose value is one of `hashes`: the referring record (`log`, `kind`, `id`), its `field`, the declared `target`, and the hash it `names`, in (log, kind, id, field) order. Every target is read, since a document by hash can be any kind's. `store revert` asks it for the commits it drops, so an operator sees which records still name a document that is no longer the head.
