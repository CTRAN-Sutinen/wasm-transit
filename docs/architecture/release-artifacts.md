# Release artifacts

A module's hash is its identity everywhere. The manifest pins it, and
the HTTP cache and the service worker keep it forever. A client
downloads a module again only when its hash moves. The bytes are a
function of the tree alone, and a change moves only the modules it
touches.

- **The toolchain is pinned.** `rust-toolchain.toml` names the Rust
  release and the wasm target. `Cargo.lock` pins every dependency, and
  the modules build `--locked`. Go is pinned by `server/go.mod`. npm is
  pinned by `web/package-lock.json`. The registry's evidence records the
  toolchain that ran. A gated publish (`just registry`) refuses a
  compiler other than the pin. Upgrading the toolchain is a change of
  its own, and so is upgrading a serialization dependency (serde,
  serde_json). Either can move every module's hash. A serde change can
  also move every document's canonical bytes, which is a version change
  for every document of the kind.
- **The build names no machine.** `just _modules` (run by `just caps`)
  remaps the cargo home to `/cargo` and the tree to `/src`, so a panic
  location carries no home directory.
- **Reproducible, gated.** `just reproducible` is a gate of `just
  registry`. It builds a copy of the tree at another path, with another
  cargo home and its own target directory. It fails unless every module
  matches byte for byte.
- **One build per assembly.** Cargo unifies a dependency's features
  across every package that one build selects. So the assemblies build
  one at a time, and a feature that one capability asks for moves only
  the modules that carry that capability.
- **The kernel's closure.** Every module links the kernel. So every
  library the kernel requires is in every module's closure. A code
  change moves only the modules whose linked code it changes. A
  dependency or feature change in a crate moves every module whose
  closure holds that crate. The kernel requires one library,
  transit-version: each kind's library loads its own kind (D-239), so a
  change to a kind's library moves only the modules whose capabilities
  read that kind. The generator is a crate of its own,
  `crates/generator`, that only its generated binary (gen-manifest)
  depends on, and so are its fixtures (`crates/fixtures`), so what they
  need is in no module's closure. `arch-gates` enforces both (gates 1
  and 4, `crates/cap-abi/src/bin/common/gates.rs`).
- **Every build is kept (D-80).** In store mode the server commits the
  release in `-dist` to the store's `build` pointer. Every file is a
  blob. A build document names the files and the modules the server
  hosts (docs/FORMATS.md, "Builds"). The server serves the pages from
  the head, and the manifest pins it (`build_hash`). The server re-checks
  `-dist`'s listing at most every 2 s, and commits a changed tree when
  its build is new to the log (D-118). The basemap (`basemap.pmtiles`,
  `basemap-assets/`) is map data and stays with `-dist`. The log holds
  every build the store ever served, whole, and GC keeps a logged
  build's files.
- **Rolling code back is a pointer move.** `POST /builds` moves the
  pointer to a build the log holds, and the manifest moves with it. On
  their next poll, the pages find the old build's service worker and
  apply it as they apply any build
  ([updates-offline.md](updates-offline.md)).
  `e2e/deploy-rollback.spec.mjs` pins it. The server never re-imports a
  build the log holds, so a restart on the release that a rollback left
  in `-dist` keeps the rollback. Moving forward is the same `POST`.
  Each build retains the server's own modules (`server`), and a running
  server keeps the modules it started on. Rolling the server back means
  deploying its previous image, so its Go code and its modules roll back
  together (D-81). Newer Go never runs on older modules, because an op's
  shape is private to the image that ships it. Either rollback works
  only when the build rolled back to reads what the newer build wrote
  (D-79).

## The HTTP contract

The parts of a deployment ship at different times. A display's cached
build meets a newer server, and a server rolled back meets newer pages.
So every contract between two deployables changes by expand, then
contract (D-81).

- **Ops are private to their deployable.** A page's host and its
  assembly ship in one bundle. The server's Go code and its modules ship
  in one image. An agent's runner pins each module by hash. So an op's
  request never meets a module from another build, and an op's shape
  changes freely. No gate diffs op shapes.
- **HTTP and documents are public.** They are the only things that
  cross between deployables. A document's version rules are in
  [FORMATS.md](../FORMATS.md), "Versions". The HTTP surface's contract
  is `server/openapi.json`: OpenAPI 3.1, every route the mux serves,
  each method, its parameters, and its request and response bodies. A
  body that is a whole document names its kind (`x-document`) and does
  not repeat the document's schema. The schemas use a subset of JSON
  Schema that the gate can compare (`server/httpcontract`). The gate
  refuses a keyword outside it.
- **The contract describes the real server.** Three Go tests hold it
  (`server/contract_test.go`):
  - `TestEveryRouteIsInTheContract`: the mux's routes and the
    contract's paths (`x-mux`) name the same patterns.
  - The contract transport: in the test binary, every request a test
    sends and every response it reads is checked against the contract.
    A whole run must also reach every operation the contract names.
  - `TestTheContractIsCompatibleWithTheLock`: the gate below.
- **The gate.** `server/openapi.lock.json` is the contract as last
  released. The gate fails on a change an older part cannot take:
  - a path, a method, a parameter, or a field removed;
  - a request parameter or field made required, or a new required one;
  - unknown request fields newly refused, or a request value narrowed;
  - a response field no longer always present, a response value
    widened, or a new response media type;
  - a document kind changed.

  An additive change passes. A new status passes too, because a client
  reads an unknown status by its class. A removal passes only when the
  lock already marks the item `deprecated`. So a removal takes two
  releases: the first marks it, and the second removes it once no
  deployed part sends or reads it. `just http-lock` writes the lock
  again after the gate passes. Run it when a release ships.

## Each UI at its own URL

Each generated UI is its own product (README, "Each UI is its own
product"). A deployment puts each UI at a URL of its own, and deploys,
updates, and rolls back each one alone (D-171). The server's one build
at `/` with the picker is the development setup.

- **The UI's host routes the data paths to the server.** The pages
  address the deployment by relative paths (`manifest`, `blobs/<hash>`,
  `edits`, `stops`, and the rest). The UI's host (a load balancer, a
  reverse proxy, or an edge worker) serves the UI's bundle and forwards
  every path of the HTTP contract but the static files, and the basemap
  (`basemap.pmtiles`, `basemap-assets/`). So the page is same-origin
  with its data, the pages do not change, and the server answers no
  cross-origin request. `DEPLOYMENTS-SERVER.md` gives the rules.
- **Each UI's builds are in the store.** `-ui-dist <id>=<dir>` names a
  UI's bundle (`just build-ui <id>`). The server commits it to the
  pointer `build.<id>` the same way it commits `-dist` to `build`: a
  build document, every file a blob, and a new record only when the
  log does not hold the build. A UI's build holds no server modules and
  no basemap. GC and verify keep every logged build's files, on any
  pointer.
- **The deploy tool.** `just ui-deploy <server> <id> <out> [build]`
  (`tools/ui-deploy.mjs`) writes the head of `build.<id>`, or a build its
  log holds, into an empty directory. It reads only the open paths
  (`/pointers/build.<id>`, `/blobs/<hash>`) and checks every file's
  hash. The operator uploads the directory to the UI's host.
- **Rolling a UI back.** `POST /builds?ui=<id>` moves `build.<id>` to a
  build its log holds, and nothing else moves. Then the deploy tool
  writes that build for the UI's host. The write check is on the
  pointer `build.<id>`.
- **Sign-in at each URL.** A page signs in with its own address as the
  redirect (`web/src/signin.svelte.ts`) and keeps its token in its own
  origin's session storage. So each UI's address is registered with the
  identity provider.
- **A static rider export** has no store. Its rollback is the previous
  bundle uploaded again ([static-deployment.md](static-deployment.md)).

`web/e2e/ui-origins.spec.mjs` pins it: two UIs at two origins over one
server. Each signs in at its own address and writes through its host.
Both ship a new build, and one rolls back while the other stays on its
newer build.

## Why

Each entry covers one choice that the design depends on. Its heading
names the code the choice governs. **Holds** is what must stay true.
Change it only with the user, and only after you read **Because**.

### Every build is kept in the sync store (`build` pointer, `POST /builds`, `builds.go`)
- **Holds:** module, assembly, and app-shell bytes are blobs in the sync store. A `build` document names them, and the log keeps it. A rollback is a pointer move to a build the log holds. GC keeps a logged build's files.
- **Because:** rolling code back and expand-then-contract need every deployed build's exact bytes, and `just caps` rewrites `web/public/caps/` each time. One store means one backup and one verification for data and code alike.
- **Rejected:** a separate artifact store beside the registry (a second thing to back up and verify); release tarballs kept wherever the agency keeps releases.
- **Decided:** D-80. **Pinned:** `server/store/build_test.go`, `server/builds_test.go`, `web/e2e/deploy-rollback.spec.mjs`.

### Ops are private; HTTP and documents expand, then contract (`server/openapi.json`, `server/httpcontract`)
- **Holds:** an op's shape changes freely inside the deployable that ships it. The HTTP surface and the documents' schemas change by expand, then contract, under a gate. A breaking change passes only as the contract step, after the lock marks the item deprecated.
- **Because:** only HTTP and documents cross between deployables. A page's host and its assembly ship together, the server's Go and its modules ship together, and an agent pins modules by hash. A gate on op shapes would slow every op change to guard a boundary no op crosses. A version on the wire is not needed when a gate keeps every change compatible.
- **Rejected:** additive-only ops, diffed against the published registry (the registry is regenerated by every build, so it holds no released baseline). A version for each op and endpoint, declared in the manifest (every module answers each version it keeps). A `/v1/` prefix on the HTTP surface.
- **Decided:** D-81. **Pinned:** `server/httpcontract/diff_test.go`, `server/contract_test.go`.

### A UI's host routes the data paths; each UI's builds are in the store (`-ui-dist`, `build.<id>`, `POST /builds?ui=`, `tools/ui-deploy.mjs`)
- **Holds:** a UI's host serves its bundle and forwards the data paths to the server, so the page is same-origin with its data. The server sends no CORS headers. Each UI's bundle is a build on `build.<id>`, and a UI rolls back by moving that pointer and deploying the build again.
- **Because:** the pages already use relative paths that a static bundle and the server share, so the pages need no change, and no page carries a server's address. Same-origin keeps the bearer token away from cross-origin requests, keeps the rider pages' policy at `connect-src 'self'`, and needs no allow-list that drifts from the deployment. Keeping each UI's builds in the store keeps one record for code and data, with no coupling to one hosting provider's version history.
- **Rejected:** the bundle names the server's URL, and the server answers cross-origin from an allow-list (every fetch takes a base, every write sends a preflight, and each page's policy names the server). The UI host's own versions as the record of its builds (the builds then live with each provider, outside the agency's bucket).
- **Decided:** D-171. **Pinned:** `web/e2e/ui-origins.spec.mjs`, `server/builds_test.go` (`TestEachUIsBundleIsABuildOfItsOwnAndRollsBackAlone`), `server/store/build_test.go` (`TestAUIsLoggedBuildsKeepTheirFilesThroughGC`).
