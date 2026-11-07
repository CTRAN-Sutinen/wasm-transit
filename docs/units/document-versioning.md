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
and URLs as persisted data; `docs/ARCHITECTURE.md`, cap-migrate and
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
5. **The schema lock — the gate that bakes this in.** Per kind,
   per version, checked into the owning crate: a frozen fixture,
   its generated JSON Schema, and the hash of its canonical bytes.
   `just test` fails when the current version's serde type changes
   without a version bump (the generated schema differs), when any
   older fixture no longer migrates to the current one, or when the
   current fixture's canonical hash moves (a byte change is an
   identity change for every document of the kind, so it is a
   version change too). The generator is the one the registry's
   documents use (gen-manifest's schemas, `registry/documents.json`,
   `docs/FORMATS.md`) — one schema emitter, not two. The locked
   schemas and fixtures are the published spec (D-115): every kind's
   schema and a worked example, per version, readable as plain files
   by a person or another tool.
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
  the gate's own test).
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
