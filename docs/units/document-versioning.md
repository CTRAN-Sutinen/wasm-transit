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

The machinery stands (`docs/FORMATS.md`, "Versions";
`docs/ARCHITECTURE.md`, cap-migrate): the network and the edit
document lead with `kind` and `schema_version` (both v2; untagged is
v1); their chains and the watchlist's live in the owning libraries
and cap-migrate wraps them; every module refuses another version of
those two at its load, naming both (`cap_abi::speaks`); the Go server
brings an older edit document forward at receive and commits an
older `edits` head forward at startup; service design brings a
stored plan forward at restore. What is left:

## Scope

1. **The inventory**, written into `docs/ARCHITECTURE.md`: every
   persisted kind, its owning crate, and its class —
   - *authored* — written by a person, not re-derivable, migrated
     forever (D-66): `edit_records`, `diversion_draft`, `plans`, an
     authored or forked `network`, `watchlist`, `policy`, the rider's
     device records (recent places, live-feed URLs), layouts and the
     UI manifest when [wire-time-assembly.md](wire-time-assembly.md)
     lands them;
   - *derived* — a pure function of other documents: an imported
     `network`, `boundary`, `walks`, an imported `streets`. A
     version bump re-derives (the server holds the inputs); the
     tag exists so a reader refuses a mismatch instead of
     misreading it;
   - *ephemeral* — `live`: never persisted past a session, tagged for
     the same refusal;
   - *host-owned* — the `manifest`: the server's, versioned in its
     envelope.
2. **The rest of the tags (D-62, D-115).** `kind` inside `plans`,
   `policy`, `work_rules`, and the watchlist; `kind` and
   `schema_version` inside `boundary`, `walks`, `streets`, `live`,
   and `diversion_draft`; each kind's load refusing another version
   the way the network's and the edit document's do. A kind that
   gains only its `kind` reads an absent one as itself.
3. **Picks' networks at the server's startup.** A `plan.<id>` head
   in an older network version is committed forward, and the plans
   document's `network_hash` for that pick follows it (a republish
   through cap-plans, never a host rewriting hashes).
4. **The browser's document store** (`web/src/documents.svelte.ts`,
   `web/src/plans.svelte.ts`): a stored plan brought forward keeps
   its old record (`produced_by` naming the migration), and every
   device record ride-now and the stop display keep moves out of
   `localStorage` into that store, tagged (`localStorage` keeps only
   presentation preferences).
5. **The authoring ops answer the document.** cap-divert's ops take
   and answer bare record lists; a client that persists or posts
   their answer writes edit document v1, which the server brings
   forward. They answer the tagged document instead.
6. **Migration modules sized to their page.** cap-migrate carries
   every chain, so linking it into service design costs about
   0.8 MB of assembly for chains that page never meets. Split it per
   kind (one migration capability per authored kind), or measure the
   cost first in [runtime-cost-gate.md](runtime-cost-gate.md).
7. **The schema lock — the gate that bakes this in.** Per kind,
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
8. **Rolling a version out.** The server's modules learn the new
   version first (accept old and new at receive, store new); the
   manifest then pins them; clients follow the manifest (D-63).
   Because every client gets its modules from the server's
   manifest, no client writes a version the server does not speak.
9. **The kernel version** bumps only when an existing kernel op
   changes meaning. A new kernel op (the eviction op,
   `docs/ARCHITECTURE.md`, "The bus worker") is listed in each
   capability's `ops`, so a host checks for it rather than the
   kernel number.
10. **Rolling a version back** (Blocked: D-79). Rollback is a
   pointer moved back, never an inverse migration, so the bytes
   before an upgrade stay addressable. But once a newer build has
   written vN+1 documents, a build rolled back to vN cannot read them
   (a module refuses a version it does not read). The rollback rule
   says what the previous build can read, and when a migration may
   start writing. The candidates are in D-79. The server's startup
   commit-forward and the browser's restore are where the rule
   applies. Rolling the CODE back is
   [release-artifacts.md](release-artifacts.md)'s; both follow one
   compatibility policy.
11. **URLs are persisted data.** The stop display's `stop` and
   `layout` parameters are configured into screens and printed
   into QR codes: they only gain optional parameters, and an
   unknown parameter is ignored.

## Done when

- Every kind in the inventory carries its kind and version inside
  the document and its load refuses another version naming both
  (the network and the edit document do: `cap-abi`
  `kernel_test.rs`, caps-smoke).
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
- Go and e2e: an upgrade that writes vN+1 is rolled back to the vN
  build under D-79's rule, and no document is lost or misread (the
  older build reads, or the newer writes wait, as decided).
- e2e: a stop display URL with an unknown parameter loads.
- `docs/ARCHITECTURE.md` holds the inventory and the schema lock.
