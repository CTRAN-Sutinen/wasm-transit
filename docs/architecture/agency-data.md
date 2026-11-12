# The agency's data

The agency and the people who work with its data own it. No
application does, this repo's included: every application — the
generated UIs, the server, the runner, a GIS, the agency's other
software — is an interface onto that data. It reads it, presents it,
and writes changes back. The record is the agency's documents in its
store, with their history; what an application holds (a page's
IndexedDB, the bus worker's registries, a cache keyed by hash) is a
working copy.

The two directions are not symmetric. **Reads are open; writes are
governed.**

## Reads are open

A reader that misreads a document harms only its own view; the data
is unchanged. So any application reads any document with no
coordination: documents are plain JSON or an external open format,
each says its kind and version inside it, and every kind's schema and a
worked example are published per version (`docs/architecture/bus.md`,
"Every document says what it is"; `docs/FORMATS.md`). Where a standard
fits — GTFS, GTFS-rt, TODS, GeoJSON — the document is that standard or
leans on it, so another application needs no adapter. Import and export
(GTFS in and out, a street layer in) are interfaces like any other: a
translation between a standard and a model document, defined by the
two formats and never by who calls it.

## Writes are governed

A write changes the agency's record for every reader at once, so it
meets one contract whichever application makes it:

- **Shape** — the document is well formed and the current version; an
  older one is migrated at receive (cap-migrate), and a writer never
  saves an older shape over a newer one.
- **Invariants** — the owning module's validation passes; a document
  that breaks the model's rules is refused.
- **Concurrency** — the write names the version it read (`from`) and is
  refused when another writer moved the document first (`409`); the
  store relies only on a conditional create (`docs/FORMATS.md`).
- **Attribution** — the pointer log records who wrote it and when
  (`writer`: the verified JWT subject when auth is on, D-58).

Every write goes through a receive path that holds that contract;
nothing writes the bucket directly (`docs/FORMATS.md`, "The HTTP
surface"). Where it holds today:

| Write | Shape | Invariants | Concurrency | Attribution |
|---|---|---|---|---|
| `POST /edits`, and an `edit_records` pointer commit | migrated at receive (cap-migrate) | cap-sync's receive (D-56) and the LWW merge | merged per record | the log's `writer` |
| `POST /plans`, `/plans/retire` | read and canonicalized by cap-network | cap-network's validation, cap-plans' chain (`422`) | the server commits by CAS | the log's `writer` |
| any other kind's pointer commit (the plan set and picks are refused there) | stored verbatim | **not checked** | `from` / `409` | the log's `writer` |

The last row is the gap: a pointer commit for any kind but the edits
stores the document as given (`docs/units/README.md`, queued, "Governed
writes").

**The gate refuses only what makes the data wrong** — a broken shape
or a broken invariant. A legitimate change with consequences (a pattern
left ending at a closed stop, a pick that drops a route) is accepted and
flagged for a person: the plan issue that gates export and publish is
never a refused edit. Stewardship stays with people; code shows them
what a change does and does not decide for them.

**Strictness is not ownership.** The rules a write meets are the
agency's, published: the validation is a capability module with no
imports, so the same rules run in the server's receive path, the
runner, a page, or another application's own tooling, and the protocol
is open to any writer the store's permissions admit.

## Why

### The agency owns the data; applications are interfaces (this file)
- **Holds:** no application is the record for any document; every one
  reads and writes through the open formats and the receive path, and
  what it keeps locally is a working copy.
- **Because:** an agency's data outlives its software. A stop inventory
  lasts decades and scheduling software turns over in five to ten
  years; data held in one vendor's private store makes every change of
  software a migration project, which is how agencies get locked in.
  Open documents with one history in the agency's store keep the data
  readable and auditable after every application here is gone, and
  make a new view cheap — a new reader, not an integration (each
  generated UI is its own product for this reason).
- **Rejected:** an application as the system of record for its
  documents (the usual product shape — the lock-in above).
- **Decided:** D-115, D-146, D-152.

### Reads open, writes governed (the receive path)
- **Holds:** any application reads any document; every write meets the
  shape, invariants, concurrency, and attribution contract at a receive
  path, which refuses only what makes the data wrong and leaves the rest
  to be flagged.
- **Because:** a misread harms one view, a bad write harms every reader.
  Without an owning application there is no natural place where
  integrity is enforced, so the contract has to sit at the write, run
  the agency's published rules, and be open to any writer — or
  ownership returns as "whoever holds the door".
- **Costs:** the formats cannot say what an edit means (a stop's point
  dragged is a survey correction or a move; only the person knows), so
  the flags must cover edits made outside this repo's applications. Many
  writers make versioning harder: a writer that does not know a newer
  field can drop it, and migrating at receive guards readers, not that.
  Permissions work per object, while stewardship is per field and role,
  so documents are split where the agency's governance splits. And
  owning the data gives the agency the work of keeping it clean; the
  applications make that cheap (flags, readable history, an undo by
  tombstone) rather than doing it for them.
- **Rejected:** open writes with validation only at read (every reader
  copes with every bad document, and the record carries them).
- **Decided:** D-152.
