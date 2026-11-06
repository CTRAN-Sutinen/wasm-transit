# Document versioning — every persisted document migrates forward

There is no central database to run a schema change against. A
document lives in the server store, in every browser's IndexedDB,
in files `caps-run` loads, in a request body sent by a client on
last month's build, and in the URL a stop display was configured
with. So the version travels WITH the document, and the code that
reads it carries the migrations: a document is migrated forward,
by the crate that owns its type, at the boundary where it is
received, into a NEW document (its own hash) that names the one it
came from. The old bytes stay addressable; rollback is a pointer
moved back, never an inverse migration.

What already exists, and is the shape the rest follows: the
manifest's `kernel` version (the bus worker refuses a mismatched
manifest); each capability's `speaks` and `migrates`, with
migration-path reachability proven at compose time (`cap-abi`
`validate.rs`); the watchlist (a version tag in the document, a
pure `migrate_watchlist` op that reports every loss as a
`MigrationNote`, the browser document store keeping both versions
chained by `produced_by`); the store envelope's `schema_version`
(`docs/FORMATS.md`). What does not: the network document carries
no version (`transit-network`'s `read_network` sniffs the shape),
`edit_records` carries its version only in the store's sidecar
(the copy in IndexedDB has none), every capability declares
`network` v1 that nothing checks, and the rider apps' device data
(`ride-now:recent`, `ride-now:live-urls`, `ride-now:last-fix`, `ride-now:network-view`) is untagged
`localStorage`.

## Scope

1. **The inventory.** Every persisted kind, its owning crate, and
   its class:
   - *authored* — written by a person, not re-derivable, migrated
     forever (D-66): `operator_edits`/`edit_records`,
     `diversion_draft`, `plans`, an authored or forked `network`,
     `watchlist`, `policy`, the rider's device records (recent
     places, live-feed URLs), layouts and the UI manifest when
     [wire-time-assembly.md](wire-time-assembly.md) lands them;
   - *derived* — a pure function of other documents: an imported
     `network`, `boundary`, `walks`, an imported `streets`. A
     version bump re-derives (the server holds the inputs); the
     tag exists so a reader refuses a mismatch instead of
     misreading it;
   - *ephemeral* — `live`, `gtfs_rt` snapshots, `feed` bytes:
     never persisted past a session, tagged for the same refusal;
   - *host-owned* — the `manifest`: the server's, versioned in its
     envelope.
   The network is both: an import is derived, a planner's fork is
   authored, so the network keeps a migration chain.
2. **The tag is inside the document (D-62)**: a top-level version
   field, so every copy anywhere says what it is and the hash covers
   it. A kind without one today is v1 when untagged; the bare
   `edit_records` array becomes a tagged object (its v1 → v2 step).
3. **Migrations are kept forever, as their own capability modules
   (D-66)** — the SQL-migrations-folder model. The steps live in
   the owning library crate: one pure, deterministic function per
   step (vN → vN+1), total over valid vN, preserving every record
   id, reporting every loss as a `MigrationNote` (the import-notes
   convention). A migration capability per authored kind wraps the
   chain as one op — "bring this document to version N" — and
   declares its steps in `migrates`; it is small, hash-pinned in the
   registry forever, and assembled only when an older document needs
   it. Consumer modules speak the current version only and never
   carry old chains (cap-board's `migrate_watchlist` moves out into
   the watchlist's migration capability). `read_network`'s shape
   sniffing becomes the network chain's first step. No host migrates
   anything; it asks the module (logic in Rust, the TS layer only
   carries bytes).
4. **Where they run — the receive boundaries**, and only there:
   - the server store: a `POST` in an older version is migrated
     before the merge; the head is always the current version; a
     server upgrade commits each pointer forward at startup as an
     ordinary logged CAS (writer `migrate:<module hash>`), the way
     D-57's compaction commits;
   - the browser: at restore, the document store moves each kind's
     `current` forward and keeps the old record; every device
     record ride-now and the stop display keep moves into that
     store (`localStorage` keeps only presentation preferences);
   - a module's load (a document in a version it does not speak is
     refused at the seam, naming both versions — never three
     layers deep in an op);
   - `caps-run --load` (the same module op).
5. **A newer document is never rewritten by an older reader.** A
   module refuses a version it does not speak; it never parses and
   re-emits it (serde would drop the fields it does not know, and
   under LWW the lossy write wins). A client that receives one goes
   read-only for that kind and follows the deployment (D-63).
6. **The schema lock — the gate that bakes this in.** Per kind,
   per version, checked into the owning crate: a frozen fixture,
   its generated JSON Schema, and the hash of its canonical bytes.
   `just test` fails when the current version's serde type changes
   without a version bump (the generated schema differs), when any
   older fixture no longer migrates to the current one, or when the
   current fixture's canonical hash moves (a byte change is an
   identity change for every document of the kind, so it is a
   version change too). The generator is the one the registry's
   documents use (gen-manifest's schemas, `registry/documents.json`,
   `docs/FORMATS.md`) — one schema emitter, not two.
7. **Rolling a version out.** The server's modules learn the new
   version first (accept old and new at receive, store new); the
   manifest then pins them; clients follow the manifest (D-63).
   Because every client gets its modules from the server's
   manifest, no client writes a version the server does not speak.
8. **The kernel version** bumps only when an existing kernel op
   changes meaning. A new kernel op (the eviction op,
   [module-runtime.md](module-runtime.md)) is listed in each
   capability's `ops`, so a host checks for it rather than the
   kernel number.
9. **Rolling a version back** (Blocked: D-79). Rollback is a
   pointer moved back, never an inverse migration, so the bytes
   before an upgrade stay addressable. But once a newer build has
   written vN+1 documents, a build rolled back to vN cannot read them
   (rule 5: it refuses them). The rollback rule says what the previous
   build can read, and when a migration may start writing. The
   candidates are in D-79. The server's startup commit-forward (rule
   4) and the browser's restore are where the rule applies. Rolling
   the CODE back is [release-artifacts.md](release-artifacts.md)'s;
   both follow one compatibility policy.
10. **URLs are persisted data.** The stop display's `stop` and
   `layout` parameters are configured into screens and printed
   into QR codes: they only gain optional parameters, and an
   unknown parameter is ignored.

The first migrations through the machinery: the untagged network
→ tagged, and the bare `edit_records` array → a tagged object.

## Done when

- Every kind in the inventory carries its version inside the
  document, and each authored kind has its chain from its first
  version in its owning crate, wrapped by its migration capability,
  with law tests: chain composition equals stepwise, ids preserved,
  losses reported, migrating the current version is refused.
- The schema-lock gate runs in `just test` and fails on each of its
  three conditions (each proven by a deliberately broken fixture in
  the gate's own test).
- Go: an older-version `POST` migrates at receive; a server started
  on a store holding older heads commits them forward as logged
  CAS records; the old blobs stay readable.
- caps-smoke + the runner: every module refuses a newer document at
  load, naming both versions.
- e2e: a browser seeded with older-version IndexedDB and
  `localStorage` records boots into the migrated documents with the
  old records kept; a page served a newer edit document than it
  speaks does not write it.
- Go and e2e: an upgrade that writes vN+1 is rolled back to the vN
  build under D-79's rule, and no document is lost or misread (the
  older build reads, or the newer writes wait, as decided).
- `docs/FORMATS.md` (the tag, the receive boundaries) and
  `docs/ARCHITECTURE.md` (the schema lock, the inventory) describe
  it.
