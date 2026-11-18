# Document versioning — every persisted document migrates forward

There is no central database to run a schema change against. A
document lives in the server store, in every browser's IndexedDB,
in files `caps-run` loads, in a request body sent by a client on
last month's build, and in the URL a stop display was configured
with. So the version travels WITH the document, and the code that
reads it carries the migrations: a document is migrated forward,
by the crate that owns its type, at the boundary where it is
received, into a NEW document (its own hash). The old bytes stay
addressable; rollback is a pointer moved back, never an inverse
migration.

The machinery stands (`docs/FORMATS.md`, "Versions" — the inventory
of every persisted kind, the rollout order, the kernel-version rule,
and URLs as persisted data; `docs/architecture/crates.md`, cap-migrate and
transit-version): every kind in the inventory carries its kind and
version inside it, and each one's schema and example are frozen in the
schema lock and served at `/schemas/`; every kernel load and every
request field taking one refuses another version naming both
(`cap_abi::parse_versioned`, `cap_abi::Current`); the network's, the
edit document's and the watchlist's chains live in the owning
libraries and cap-migrate wraps them; the Go server brings an older
edit document forward at receive, and at startup commits forward an
older `edits` head and the older networks of the picks in use
(cap-plans `in_use`), republishing the plan set through cap-plans; the planners' pages and ride-now bring a stored
plan or watchlist forward at restore, keeping the old record, and
ride-now's device records are tagged documents in the browser's store;
cap-divert takes and answers the tagged edit document; a stop display
keeps a URL parameter it does not know. What is left:

## Scope

1. **The migration module cut by use (D-79).** cap-migrate carries
   every chain, so linking it into service design costs about
   0.8 MB of assembly for chains that page never meets. Ride-now
   fetches all of it, alone beside its assembly, when a stored
   watchlist is older than the board reads (a static export of
   ride-now does not carry it, so the old record waits there). Operational pieces, each linked
   only where its documents arrive (the server's receive, a page's
   restore), and one archive piece holding every chain (D-66), which
   reads an archive in today's shape on request.
2. **The Go host's own documents (D-185).** The schema lock, its
   gates, `/schemas/`, and the soak stand (`docs/FORMATS.md`, "The
   schema lock"). The `build` document keeps its hand-written schema in
   `server/schemas/`, and the manifest its HTTP contract, until D-185 is
   answered.
3. **Rolling a version back (D-79).** A format change ships in two
   releases: A reads vN and vN+1 and writes vN, B writes vN+1, so
   rollback one release back always reads what the store holds.
   - The reader of the next version: a kind's load accepts its
     version and, for one release, the next (`Versioned` names both;
     `cap_abi::parse_versioned` reads either), and the store keeps
     writing the current one.
   - A client refused for its version reloads its manifest.
   - The emergency procedure, for a bad release A: the pointers back
     to their pre-upgrade heads, and the dropped commits listed from
     the pointer log, to re-enter by hand.
   - Written in `docs/FORMATS.md`, "Versions", beside the rollout
     order, with the backup rule (a backup restores into a build at
     least as new as it) and the archive's two readings (as it was,
     the default for an audit; as it is today, through the archive
     piece). Rolling the CODE back is in
     `docs/architecture/release-artifacts.md`; both follow this
     policy.

## Done when

- e2e: a page served a newer edit document than it reads does not
  write it (a newer plan set is pinned:
  `ui-service-design.spec.mjs`).
- Go and e2e: a store written by release B (vN+1) is read by release
  A (reads vN and vN+1) with no document lost or misread, and a page
  refused for its version reloads its manifest.
