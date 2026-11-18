# The agency's data

The agency and the people who work with its data own it. No
application owns it, this repo's applications included. Every
application is an interface onto that data. The generated UIs, the
server, the runner, a GIS, and the agency's other software all count.
Each one reads the data, presents it, and writes changes back. The
record is the agency's documents in its store, with their history. What
an application holds (a page's IndexedDB, the bus worker's registries, a
cache keyed by hash) is a working copy.

The two directions are not symmetric. **Reads are open; writes are
governed.**

## Reads are open

A reader that misreads a document harms only its own view. The data is
unchanged. So any application reads any document with no coordination.
Documents are plain JSON or an external open format. Each one says its
kind and version inside it. Every kind's schema and a worked example are
published per version (`docs/architecture/bus.md`, "Every document says
what it is"; `docs/FORMATS.md`). Where a standard fits (GTFS, GTFS-rt,
TODS, GeoJSON), the document is that standard or leans on it. So another
application needs no adapter. Import and export (GTFS in and out, a
street layer in) are interfaces like any other. Each one is a
translation between a standard and a model document. The two formats
define it, never the caller.

## Writes are governed

A write changes the agency's record for every reader at once. So it
meets one contract, whichever application makes it:

- **Shape.** The document is well formed and the current version. An
  older one is migrated at receive (cap-migrate). A writer never saves an
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
  not drop what a newer one wrote. The receive path never refuses one.
  The module that checks the write names each such field
  (`unknown_fields`), and the server names them on the response
  (`X-Unknown-Fields`) and logs them with the writer.

Every write goes through a receive path that holds that contract.
Nothing writes the bucket directly (`docs/FORMATS.md`, "The HTTP
surface"). Where it holds today:

| Write | Shape | Invariants | Concurrency | Attribution |
|---|---|---|---|---|
| `POST /edits`, and an `edit_records` pointer commit | migrated at receive (cap-migrate) | cap-sync's receive (D-56) and the LWW merge | merged per record | the log's `writer` |
| `POST /plans`, `/plans/retire` | read and canonicalized by cap-network | cap-network's validation, cap-plans' chain (`422`) | the server commits by CAS | the log's `writer` |
| `POST /stops` | read by cap-stops | cap-stops' `validate` (`422`); a survey's surveyor named for the writer (D-163) | `from` / `409` | the log's `writer` |
| `POST /stop-requests` | read by cap-requests | cap-requests' `validate` (`422`) | `from` / `409` | the log's `writer` |
| any other kind's pointer commit (the plan set, picks, stops, and stop requests are refused there) | stored verbatim | **not checked** | `from` / `409` | the log's `writer` |

The last row is the gap. A pointer commit for any kind other than the
edits stores the document as given (`docs/units/storage-model.md`, item
1, the kind table).

**The gate refuses only what makes the data wrong.** That means a broken
shape or a broken invariant. A legitimate change with consequences is
accepted and flagged for a person. Two examples: a pick that visits a
retired stop, and a pick that drops a route. The plan issue that gates
export and publish is never a refused edit. Stewardship stays with
people. Code shows them what a change does, and what it does not decide
for them.

**Strictness is not ownership.** The rules a write meets are the
agency's, and they are published. The validation is a capability module
with no imports. So the same rules run in the server's receive path, the
runner, a page, or another application's own tooling. The protocol is
open to any writer the store's permissions admit.

## Why

### The agency owns the data; applications are interfaces (this file)
- **Holds:** no application is the record for any document. Every
  application reads and writes through the open formats and the receive
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

### Reads open, writes governed (the receive path)
- **Holds:** any application reads any document. Every write meets the
  shape, invariants, concurrency, and attribution contract at a receive
  path. The receive path refuses only what makes the data wrong. It
  leaves the rest to be flagged.
- **Because:** a misread harms one view. A bad write harms every reader.
  Without an owning application, no natural place enforces integrity. So
  the contract has to sit at the write, run the agency's published rules,
  and be open to any writer. Otherwise ownership returns to whoever
  controls the write path.
- **Costs:** the formats cannot say what an edit means. A stop's point
  dragged is a survey correction or a move; only the person knows which.
  So the flags must cover edits made outside this repo's applications.
  Many writers make versioning harder. Migrating at receive guards
  readers, not writers. So every authored record keeps the fields it does
  not know (D-167), and a misspelled field is kept too, flagged rather
  than refused. Permissions work per object, while stewardship is per field and
  role. So documents are split where the agency's governance splits.
  Owning the data also gives the agency the work of keeping it clean. The
  applications make that cheap: flags, readable history, and an undo by
  tombstone. They do not do the work for the agency.
- **Rejected:** open writes with validation only at read. Every reader
  copes with every bad document, and the record carries them.
- **Decided:** D-152.
