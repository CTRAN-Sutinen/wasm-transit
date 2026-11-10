# Release artifacts

A module's hash is its identity everywhere: the manifest pins it, the
HTTP cache and the service worker keep it forever, and a client
downloads a module again only when its hash moves. So the bytes are a
function of the tree alone, and a change moves only the modules it
touches.

- **The toolchain is pinned.** `rust-toolchain.toml` names the Rust
  release (and the wasm target); `Cargo.lock` pins every dependency,
  and the modules build `--locked`. Go is pinned by `server/go.mod`,
  npm by `web/package-lock.json`. The registry's evidence records the
  toolchain that ran (`toolchain`: the pin, rustc's release, commit and
  LLVM, cargo), and a gated publish (`just registry`) refuses a
  compiler other than the pin. Upgrading the toolchain or a
  serialization dependency (serde, serde_json) is therefore a change
  of its own, made in the pin or the lockfile: it can move every
  module's hash, and a serde change can move every document's
  canonical bytes, which is a version change for every document of the
  kind.
- **The build names no machine.** `just _modules` (run by `just caps`)
  remaps the cargo home to `/cargo` and the tree to `/src`, so a
  panic location carries no home directory.
- **Reproducible, gated.** `just reproducible` (a gate of `just
  registry`) builds a copy of the tree at another path, with another
  cargo home, into its own target directory, and fails unless every
  module matches byte for byte.
- **One build per assembly.** Cargo unifies a dependency's features
  across every package one build selects, so the assemblies build one
  at a time: a feature one capability asks for moves only the modules
  that carry that capability.
- **The kernel's closure.** Every module links the kernel, so every
  library the kernel requires is in every module's closure. A code
  change moves only the modules whose linked code it changes, but a
  dependency or feature change in a crate moves every module whose
  closure holds that crate. The kernel requires only the libraries its
  own code names (its registries' kinds: the network, codec, rt, live,
  dispatch, runs, streets, sync and analysis libraries, and
  transit-version); what
  gen-manifest needs besides (transit-router, transit-plans,
  transit-authz) is optional, behind its `gen-manifest` feature.
  `arch-gates` enforces it (gate 4, `crates/cap-abi/src/bin/common/gates.rs`).
- **Every build is kept (D-80).** In store mode the server commits the
  release in `-dist` to the store's `build` pointer — every file a
  blob, a build document naming them and the modules the server hosts
  (docs/FORMATS.md, "Builds") — and serves the pages from the head;
  the manifest pins it (`build_hash`). `-dist` is read like a source
  (D-118): its listing is re-checked at most every 2 s, and a changed
  tree is committed when its build is new to the log. The basemap
  (`basemap.pmtiles`, `basemap-assets/`) is map data, not the release,
  and stays `-dist`'s. A store's log therefore holds every build it
  ever served, whole: GC keeps a logged build's files.
- **Rolling code back is a pointer move.** `POST /builds` moves the
  pointer to a build the log holds; the manifest moves with it, the
  pages' next poll finds the old build's service worker and applies it
  as it applies any build ([updates-offline.md](updates-offline.md)), and
  `e2e/deploy-rollback.spec.mjs` pins it. A build the log already holds
  is never re-imported, so a restart on the release a rollback left in
  `-dist` keeps the rollback; moving forward is the same `POST`. The
  server's own modules are retained in each build (`server`), but a
  running server keeps the modules it started on: rolling the server
  back is restarting it on the retained modules (`/blobs/<hash>`).
  Either rollback holds only within D-79: the build rolled back to
  must read what the newer one wrote.

## Why

One entry per load-bearing choice, headed by the code it governs.
**Holds** is what must stay true; change it only with the user, after
reading **Because**.

### Every build is kept in the sync store (`build` pointer, `POST /builds`, `builds.go`)
- **Holds:** module, assembly, and app-shell bytes are blobs in the sync store, named by a `build` document the log keeps; a rollback is a pointer move to a build the log holds; GC keeps a logged build's files.
- **Because:** rolling code back and expand-then-contract need every deployed build's exact bytes, and `just caps` rewrites `web/public/caps/` each time. One store means one backup and one verification for data and code alike.
- **Rejected:** a separate artifact store beside the registry (a second thing to back up and verify); release tarballs kept wherever the agency keeps releases.
- **Decided:** D-80. **Pinned:** `server/store/build_test.go`, `server/builds_test.go`, `web/e2e/deploy-rollback.spec.mjs`.
