# Release artifacts

A module's hash is its identity everywhere. The manifest pins it. The
HTTP cache and the service worker keep it forever. A client downloads a
module again only when its hash moves. So the bytes are a function of
the tree alone. A change moves only the modules it touches.

- **The toolchain is pinned.** `rust-toolchain.toml` names the Rust
  release and the wasm target. `Cargo.lock` pins every dependency, and
  the modules build `--locked`. Go is pinned by `server/go.mod`. npm is
  pinned by `web/package-lock.json`. The registry's evidence records the
  toolchain that ran: `toolchain` holds the pin, rustc's release, commit
  and LLVM, and cargo. A gated publish (`just registry`) refuses a
  compiler other than the pin. Upgrading the toolchain is a change of
  its own. So is upgrading a serialization dependency (serde,
  serde_json). Make either change in the pin or the lockfile. It can
  move every module's hash. A serde change can also move every
  document's canonical bytes. That is a version change for every
  document of the kind.
- **The build names no machine.** `just _modules` (run by `just caps`)
  remaps the cargo home to `/cargo`. It remaps the tree to `/src`. So a
  panic location carries no home directory.
- **Reproducible, gated.** `just reproducible` is a gate of `just
  registry`. It builds a copy of the tree at another path. It uses
  another cargo home and its own target directory. It fails unless every
  module matches byte for byte.
- **One build per assembly.** Cargo unifies a dependency's features
  across every package that one build selects. So the assemblies build
  one at a time. A feature that one capability asks for moves only the
  modules that carry that capability.
- **The kernel's closure.** Every module links the kernel. So every
  library the kernel requires is in every module's closure. A code
  change moves only the modules whose linked code it changes. A
  dependency or feature change in a crate moves every module whose
  closure holds that crate. The kernel requires only the libraries its
  own code names. These are its registries' kinds: the network, codec,
  rt, live, dispatch, runs, streets, sync and analysis libraries, and
  transit-version. What gen-manifest needs besides (transit-router,
  transit-plans, transit-authz) is optional. It sits behind its
  `gen-manifest` feature. `arch-gates` enforces it (gate 4,
  `crates/cap-abi/src/bin/common/gates.rs`).
- **Every build is kept (D-80).** In store mode the server commits the
  release in `-dist` to the store's `build` pointer. Every file is a
  blob. A build document names the files and the modules the server
  hosts (docs/FORMATS.md, "Builds"). The server serves the pages from
  the head. The manifest pins it (`build_hash`). `-dist` is read like a
  source. Its listing is re-checked at most every 2 s, and a changed tree
  is committed when its build is new to the log (D-118). The basemap
  (`basemap.pmtiles`, `basemap-assets/`) is map data, not the release. It
  stays with `-dist`. A store's log holds every build it ever served,
  whole. GC keeps a logged build's files.
- **Rolling code back is a pointer move.** `POST /builds` moves the
  pointer to a build the log holds. The manifest moves with it. On the
  pages' next poll, they find the old build's service worker. They apply
  it as they apply any build ([updates-offline.md](updates-offline.md)).
  `e2e/deploy-rollback.spec.mjs` pins it. A build the log already holds
  is never re-imported. So a restart on the release that a rollback left
  in `-dist` keeps the rollback. Moving forward is the same `POST`. The
  server's own modules are retained in each build (`server`). A running
  server keeps the modules it started on. Rolling the server back means
  restarting it on the retained modules (`/blobs/<hash>`). Either
  rollback works only when the build rolled back to reads what the newer
  build wrote (D-79).

## Why

Each entry covers one choice that the design depends on. Its heading
names the code the choice governs. **Holds** is what must stay true.
Change it only with the user, and only after you read **Because**.

### Every build is kept in the sync store (`build` pointer, `POST /builds`, `builds.go`)
- **Holds:** module, assembly, and app-shell bytes are blobs in the sync store. A `build` document names them, and the log keeps it. A rollback is a pointer move to a build the log holds. GC keeps a logged build's files.
- **Because:** rolling code back and expand-then-contract need every deployed build's exact bytes, and `just caps` rewrites `web/public/caps/` each time. One store means one backup and one verification for data and code alike.
- **Rejected:** a separate artifact store beside the registry (a second thing to back up and verify); release tarballs kept wherever the agency keeps releases.
- **Decided:** D-80. **Pinned:** `server/store/build_test.go`, `server/builds_test.go`, `web/e2e/deploy-rollback.spec.mjs`.
