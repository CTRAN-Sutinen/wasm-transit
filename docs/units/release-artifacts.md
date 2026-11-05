# Release artifacts — keep every build, and change contracts without breaking them

The parts of a deployment ship at different times. The server's
modules, a page's assembly, a display's cached app shell, and an agent
pulling from the registry can each run a different build at the same
moment. Documents get versions and migrations
([document-versioning.md](document-versioning.md)). This unit does
the same for code. Every artifact ever deployed stays addressable and
rebuildable. The contracts between separately deployed parts (op
shapes and the server's HTTP surface) change by expand-then-contract.
Rolling back code is moving a pointer back to bytes that still exist.
Retirement is the tombstone here too: a build that is no longer
deployed is retired, never deleted.

## Scope

- **Retention** (Blocked: D-80, where retired artifacts live). Today
  every `just caps` deletes `registry/` and `web/public/caps/` and
  writes them again. Git holds the source but not the bytes, and the
  bytes cannot be rebuilt exactly (below), so the only copies of a
  past build are browser caches. Every published module, assembly,
  app-shell build, and manifest is kept by its hash. The store's
  `manifest` pointer already logs every manifest it published; with
  the bytes those manifests name retained, any past deployment can be
  reconstructed.
- **Reproducible builds.** Pin the toolchain (`rust-toolchain.toml`;
  Go is pinned in `go.mod`, npm by its lockfile), record it in the
  registry's evidence (rustc is recorded today), and gate it: two
  clean builds of the same tree give the same module hashes.
- **Hash churn.** Adding one dependency on 2026-11-05 changed all 15
  module hashes, and a changed hash means every client downloads the
  module again: a Pi display on a cellular link pays that for every
  release. Measure the churn and cut it where it is cheap. A page
  runs one linked assembly (D-77), so any of its capabilities'
  changes rebuilds the whole assembly: measure there.
- **Canonical bytes across upgrades.** A document's identity is its
  serde_json bytes, so a serde, serde_json, or Rust upgrade can change
  every document hash. Upgrading the toolchain or a serialization
  dependency is a deliberate act, checked by document-versioning's
  schema lock (its canonical-hash condition); this unit owns the pin
  that makes the upgrade deliberate.
- **Op contracts** (Blocked: D-81, the scheme). An op's request and
  response have no version of their own; the kernel version is one
  number for the whole ABI. Example: cap-plans `publish` gained a
  required `today` (2026-11-05), and nothing would catch a host still
  sending the old shape. The browser runs linked assemblies while
  the server runs single-capability modules (D-78),
  so the two can run different builds. The rule is expand, then
  contract: a new field is optional, or the change is a new op; the
  old shape is accepted until no deployed host sends it; then it is
  removed, with the old artifact retained. The gate diffs each op's
  generated request and response schemas against the last published
  registry (the documents' schema emitter, extended to op shapes), and
  a breaking change fails unless it is the contract step of a
  migration.
- **HTTP contracts** (D-81). `/manifest`, `/edits`, `/pointers`,
  `/blobs`, `/plans`, `/plans/retire`, and `/feed` change under the
  same rule, because a display's cached build meets a newer server.
- **Rolling code back.** A rollback is the manifest pointer moved back
  to a retained build: the pages follow the manifest (D-63), and the
  server restarts on the retained modules. This works only within
  document-versioning's rollback rule (D-79): the build rolled back to
  must read what the newer build wrote.

## Order

Retention and the toolchain pin first (independent), then op and
HTTP contract versioning, over the artifacts as they are: one
assembly per page, single-capability modules on the server, and the
`{"cap": …}` request envelope.

## Done when

- Every published build's bytes are retained and addressable, and an
  e2e publishes build A, then build B, rolls back to A by moving the
  manifest pointer, and the page runs A's bytes.
- `just test` (or `just registry`) fails when two clean builds of the
  same tree differ.
- The contract gate fails on a deliberately breaking op change, and
  on a breaking HTTP change, each in its own test; an additive change
  passes.
- D-80 and D-81 are decided and encoded, and `docs/ARCHITECTURE.md`
  states the compatibility policy shared with document-versioning.
