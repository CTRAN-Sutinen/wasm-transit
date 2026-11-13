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
transit-version): every kind in the inventory but the watchlist
carries its kind and version inside it, and every kernel load and
every request field taking one refuses another version naming both
(`cap_abi::parse_versioned`, `cap_abi::Current`); the network's,
the edit document's and the watchlist's chains live in the owning
libraries and cap-migrate wraps them; the Go server brings an older
edit document forward at receive and commits an older `edits` head
forward at startup; service design brings a stored plan forward at
restore; cap-divert takes and answers the tagged edit document; a
stop display keeps a URL parameter it does not know. What is left:

## Scope

1. **The watchlist's refusal.** The kernel still reads both
   watchlist versions (v1 and v2), and the ride-now host reads a v1
   by hand (`restoreSaved`). It refuses v1 at the load once the
   browser brings a stored v1 forward (item 3); the watchlist gains
   its `kind` then. A diversion draft gains its tags when something
   first persists one ([ui-disruptions.md](ui-disruptions.md)).
2. **The documents in use, at the server's startup (D-79).** Only
   what is in use comes forward: beside `edits`, the published plan
   set, the policy, the work rules, and the networks of the pick in
   force and the picks still to come. A pick's network committed
   forward moves the plans document's `network_hash` with it (a
   republish through cap-plans, never a host rewriting hashes). A
   retired or expired pick stays in its version, and comes forward
   when it is republished.
3. **The browser's document store** (`web/src/documents.svelte.ts`,
   `web/src/plans.svelte.ts`): a stored plan or watchlist brought
   forward keeps its old record (`produced_by` naming the
   migration), and every device record ride-now keeps (recent
   places, live-feed URLs) moves out of `localStorage` into that
   store, tagged (`localStorage` keeps only presentation
   preferences: the map's last frame and fix).
4. **The migration module cut by use (D-79).** cap-migrate carries
   every chain, so linking it into service design costs about
   0.8 MB of assembly for chains that page never meets, and ride-now
   needs the watchlist's (item 3). Operational pieces, each linked
   only where its documents arrive (the server's receive, a page's
   restore), and one archive piece holding every chain (D-66), which
   reads an archive in today's shape on request.
5. **The schema lock and the published spec.** The registry's
   schemas become the agency's published spec for its data: a frozen
   schema and example for every version of every kind, readable by a
   person or another tool, served by every deployment, and the build
   fails when the code drifts from it. Rust stays the one source; the
   locks are what it has promised. Nothing is deployed yet, so there
   are no older versions to recover: the baseline is each kind's
   version at the first deployment, and the shapes are fixed in place
   before it is frozen.
   - **The shapes, before the baseline** (no version bumps): the
     browser's `plan_set` becomes a tagged object
     (`{"kind":"plan_set","schema_version":N,"plans":[…]}`, not a bare
     array); `work_rules` names its version `schema_version`, as every
     other kind does; the watchlist joins `registry/documents.json`;
     the Go host's `manifest` and `build` get hand-written schemas,
     locked in `server/`, checked by a Go test against what the server
     emits, and indexed as host-owned. The storage model's shape
     changes land in the same window
     ([storage-model.md](storage-model.md), item 1; D-166, D-167), as
     does the canonical form (D-169).
   - **The lock.** Per kind, per version, checked into the owning
     crate: a frozen example, its generated JSON Schema, and the hash
     of its canonical bytes. `just test` fails when the current
     version's serde type changes without a version bump (the
     generated schema differs), when any older example no longer
     migrates to the current one, or when the current example's
     canonical hash moves (a byte change is an identity change for
     every document of the kind, so it is a version change too). The
     generator is the one the registry's documents use (gen-manifest's
     schemas) — one schema emitter, which now compares rather than
     writes; `just caps` copies the locks into `registry/`, so the
     registry lists every locked version, each marked written or read
     only through a migration.
   - **Reachable.** The index is keyed by the tag a document carries
     inside it (`edit_records`, beside the bus kind `operator_edits`
     and the envelope's `store_kind`), and the files are named by it,
     so a reader holding only a file finds its schema. Every schema
     carries a stable `$id` that names no host. The server serves
     `registry/documents.json` and the schemas under `/schemas/`,
     open with auth on (reads are open, D-152); `just export` writes
     the same paths into the bundle; a blob's response links its
     schema. A Go test reads a blob through `/schemas/` alone, from a
     running server and from an export directory.
   - **The schemas describe the real bytes.** A gate runs every
     capability's real output on the e2e fixture feed against its
     locked schema, not only the worked examples — the places
     `schemars` and serde disagree (`skip_serializing_if`, `untagged`,
     `flatten`) — and the real-data soak checks the C-TRAN store's
     blobs the same way, a mismatch a QA finding.
6. **Rolling a version back (D-79).** A format change ships in two
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
     piece). Rolling the CODE back is
     [release-artifacts.md](release-artifacts.md)'s; both follow this
     policy.

## Done when

- The watchlist's load refuses another version naming both (every
  other kind's does: `cap-abi` `kernel_test.rs`,
  `every_versioned_kind_reads_untagged_as_v1_and_refuses_a_newer_copy`).
- The schema-lock gate runs in `just test` and fails on each of its
  three conditions (each proven by a deliberately broken fixture in
  the gate's own test), and every persisted kind in `docs/FORMATS.md`,
  "Versions" has an index entry (a gate compares the two lists).
- `server/reader_test.go` reads a blob through `/schemas/` alone, from
  a running server and from a `just export` directory.
- The real-output gate runs in `just test`, and one real-data soak
  reports no blob that fails its schema.
- Go: a server started on a store holding an older pick commits it
  forward and republishes the plan set (the edit document's receive
  and startup are pinned: `server/version_test.go`).
- e2e: a browser seeded with older-version IndexedDB and
  `localStorage` records boots into the migrated documents with the
  old records kept; a page served a newer edit document than it
  reads does not write it.
- Go and e2e: a store written by release B (vN+1) is read by release
  A (reads vN and vN+1) with no document lost or misread, and a page
  refused for its version reloads its manifest.
