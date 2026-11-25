# The agency's data

The agency and the people who work with its data own it. No
application owns it, this repo's applications included. Every
application is an interface onto that data: the generated UIs, the
server, the runner, a GIS, and the agency's other software. Each one
reads the data, presents it, and writes changes back. The record is the
agency's documents in its store, with their history. What an
application holds (a page's IndexedDB, the bus worker's registries, a
cache keyed by hash) is a working copy.

Reads are open and writes are governed.

## Reads are open

A reader that misreads a document harms only its own view, so any
application reads any document with no coordination. Documents are
plain JSON or an external open format, and each says its kind and
version inside it. Every kind's schema and a worked example are
published per version (`docs/architecture/bus.md`, "Every document says
what it is"; `docs/FORMATS.md`). Where a standard fits (GTFS, GTFS-rt,
TODS, GeoJSON), the document is that standard or leans on it, so another
application needs no adapter. Import and export (GTFS in and out, a
street layer in) translate between a standard and a model document. The
two formats define the translation, never the caller.

Reporting, analytics, and ad hoc queries belong to the agency's data
lake, which reads the open documents and logs like any other
application. The store and the server need no read model, projection,
or query language for them.

## Writes are governed

A write changes the agency's record for every reader at once, so it
meets one contract, whichever application makes it:

- **Shape.** The document is well formed and the current version. An
  older one is migrated on write (cap-migrate). A writer never saves an
  older shape over a newer one.
- **Invariants.** The owning module's validation passes. A document that
  breaks the model's rules is refused.
- **Concurrency.** The write names the version it read (`from`). The
  store refuses the write with `409` when another writer moved the
  document first. The store relies only on a conditional create
  (`docs/FORMATS.md`).
- **Attribution.** The pointer log records who wrote it and when
  (`writer`: the verified JWT subject when auth is on, D-58).
- **Fields this build does not know.** Every authored kind keeps them
  through a read and a write (D-167), so a writer on an older build does
  not drop what a newer one wrote. The write path never refuses one.
  The module that checks the write names each such field
  (`unknown_fields`), and the server names them on the response
  (`X-Unknown-Fields`) and logs them with the writer.

Every write goes through a write path that holds that contract, and
nothing writes the bucket directly (`server/openapi.json`; each route
is in `server.md`). The table shows where each check holds:

| Write | Shape | Invariants | Concurrency | Attribution |
|---|---|---|---|---|
| `POST /edits`, and a commit to the `edits` pointer | migrated on write (cap-migrate) | cap-sync's write check (D-56) and the LWW merge | merged per record; one entry of the disruptions log | the entry's `writer` |
| `POST /plans`, `/plans/freeze`, `/plans/adopt`, `/plans/retire` | read and canonicalized by cap-network | cap-network's validation; cap-plans' chain, and only a frozen version adopted (`422`) | one entry of the plans log | the entry's `writer` |
| `POST /stops` | read by cap-stops | cap-stops' `validate` (`422`); a survey's surveyor named for the writer (D-163) | `from` / `409`; one entry of the stops log | the entry's `writer` |
| `POST /stop-requests` | read by cap-requests | cap-requests' `validate` (`422`) | `from` / `409`; one entry of the stops log | the entry's `writer` |
| `POST /service-alerts`, `/alert-templates` | read by cap-alerts | cap-alerts' `validate`, `validate_templates` (`422`); `flags` tells a person, never refuses | `from` / `409`; one entry of the alerts log | the entry's `writer` |
| `POST /notes` | one note: id, target, text | cap-notes' `write` over the head (`422`): a blank text, a target that names no id, or another target for a held note | none needed: each note runs over the head; one entry of the notes log | the entry's `writer`, and the note's own `written` and `edited`, which the server stamps |
| `POST /logs/stops`, `/logs/alerts`, `/logs/notes` (the intent write, D-189) | each op run by its module over the head | the op's own checks and `validate` (`422`); the surveyor named for the writer; a note's `by` and `at` given by the server | ops over the head: different records never conflict; a record an entry after `read_seq` changed is a `409` naming it; an event changed since is a `409` naming it; the idempotency key lands once | the entry's `writer`, and the device's `claimed_at_ms` as a claim beside its time (D-222) |
| `POST /pointers/<name>`, the policy | read by cap-authz | cap-authz's `validate` (`422`); a policy nobody can change again is accepted and named (`warnings`) | `from` / `409` | the log's `writer` |
| `POST /pointers/<name>`, any other kind | refused (`400`): a kind the kind table does not name, or one with its own route, named in the answer | | | |

The kind table (D-202; `docs/FORMATS.md`, "The kind table") governs the
generic pointer write. Each kind the store keeps has one entry: how it
is written, the module and op that check it, its merge rule, and its
read class. A kind with no entry is refused, so no write reaches the
bucket unchecked.

**The gate refuses only what makes the data wrong:** a broken shape or
a broken invariant. A legitimate change with consequences is accepted
and flagged for a person, such as a pick that visits a retired stop or
drops a route. The plan issue that gates export and publish is never a
refused edit. Stewardship stays with people. Code shows them what a
change does and decides nothing for them.

The rules a write meets are the agency's, and they are published. The
validation is a capability module with no imports, so the same rules run
in the server's write path, the runner, a page, or another application's
own tooling. The protocol is open to any writer the store's permissions
admit.

## Why

### The agency owns the data; applications are interfaces (this file)
- **Holds:** no application is the record for any document. Every
  application reads and writes through the open formats and the write
  path. What it keeps locally is a working copy.
- **Because:** an agency's data outlives its software. A stop inventory
  lasts decades. Scheduling software turns over in five to ten years.
  Data held in one vendor's private store makes every change of software
  a migration project, which is how agencies get locked in. Open
  documents with one history in the agency's store keep the data
  readable and auditable after every application here is gone. They also
  make a new view cheap: a new reader, not an integration. (Each
  generated UI is its own product for this reason.)
- **Rejected:** an application as the system of record for its documents
  (the usual product shape; the lock-in is described above).
- **Decided:** D-115, D-146, D-152.

### Reads open, writes governed (the write path)
- **Holds:** any application reads any document. Every write meets the
  shape, invariants, concurrency, and attribution contract at a write
  path. The write path refuses only what makes the data wrong. It
  leaves the rest to be flagged.
- **Because:** a misread harms one view. A bad write harms every reader.
  Without an owning application, no natural place enforces integrity. So
  the contract has to sit at the write, run the agency's published rules,
  and be open to any writer. Otherwise ownership returns to whoever
  controls the write path.
- **Costs:** the formats cannot say what an edit means. A stop's point
  dragged is a survey correction or a move; only the person knows which.
  So the flags must cover edits made outside this repo's applications.
  Many writers make versioning harder, and migrating on write guards
  readers, not writers. So every authored record keeps the fields it does
  not know (D-167), and a misspelled field is kept too, flagged rather
  than refused. Permissions work per object, while stewardship is per
  field and role, so documents are split where the agency's governance
  splits. The agency also does the work of keeping its data clean. The
  applications make that cheap with flags, readable history, and a
  revert, and they do not do the work.
- **Rejected:** open writes with validation only at read. Every reader
  copes with every bad document, and the record carries them.
- **Decided:** D-152.
