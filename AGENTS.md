# AGENTS.md

wasm-transit is local-first transit software. It reads GTFS static and
GTFS-rt with Rust, and ships as **capability modules**. Every module has
the same flat WebAssembly interface with no imports. A generated UI's
capabilities link into one module over one kernel registry (D-77).

The point of the project is that every host runs the same document and
the same contracts: the browser, Node, and Go with wazero. Each
generated UI links the capabilities it needs at build time (D-176). The bus is the
architecture: every host instantiates the capability modules it needs.

**Two deployment shapes, both complete.** A deployment runs with the
sync server (`just serve`), or with no server at all (`just export
<ui>`, which exports a rider UI as static files). The operator chooses
for each deployment. Neither shape is a subset of the other:

- a UI that runs without a server keeps running without one;
- the server's features stay with the server;
- a change made for one shape adds to that shape and takes no
  capability from the other.

**Each generated UI is its own product.** It is deployed, updated, and
rolled back on its own. It is not a part of one app. Which UIs need the
server is in `docs/architecture/README.md` ("Deployment shapes", "Each
UI is its own product").

**The rules that keep the documents correct.** Other files hold the
detail. These rules keep them from drifting, and later agents depend on
them:

- **Docs describe the repo as it is now.** Git holds the history. Do
  not write "was", "moved from", unit numbers for finished work, or
  session narratives. Write architecture docs in the present tense.
- **One doc for each topic.**
  - Architecture detail lives in `docs/architecture/`, one file for
    each topic. The three laws live in `crates/AGENTS.md`.
  - Open work lives only in `docs/units/`, one file for each unit.
    Never state status here or in any other place.
  - Architecture docs can be many, but they must not overlap. Each
    topic has exactly one home.
  - The guide (`docs/guide/`) explains the same topics to people in
    plain language, and links to the home of each topic. It holds no
    detail of its own. Where the guide and a home disagree, the home is
    right.
- **Update the docs in the same change.** When your work changes the
  architecture, the ops surface, naming, conventions, commands, or
  testing expectations, update this file and every doc whose topic the
  change touches. Do this in the same change, before you say the work
  is done.
- **Fix references when content moves.** When content moves between
  docs, fix every reference to it in the same change. A pointer to a
  section that no longer exists breaks the build for the next agent.
- **Deleting a unit file is the claim that the unit is done**
  (`docs/units/README.md`). Do not delete the file of a unit whose work
  is not done. The change that deletes it names its pin: the gate, the
  e2e spec, or the dogfood session that proves it. The change also
  describes what landed in the architecture docs. On the bus, read by a
  UI, and dogfooded are three different claims. A unit that is missing
  a grade stays open. Delete a fixed QA finding the same way. Stale
  instructions make the next agent break the build.
- **The code wins.** If a section contradicts the code, the code is
  right for the moment. Fix the section immediately.

**Finishing work: the documentation sweep.** Work is not done, and you
do not report it as done, until its documentation is done. Before you
tell the user that work is finished, sweep every place that describes
what you changed. Say in your report what you updated.

1. **The docs.**
   - `AGENTS.md`: the crate tree, the commands, and the rules.
   - `README.md`: what the project is and how to start it.
   - `crates/AGENTS.md`.
   - Every file under `docs/` whose topic the change touches. Write in
     the present tense, with one home for each topic.
   - `docs/guide/`, when the change alters something it describes:
     what an app does (a new UI gets a short entry in
     `docs/guide/apps.md`), a step in its walkthrough, a design choice
     or its cost, or a term in its glossary. The guide is written for
     people to ISO 24495-1 (plain language). Use short sentences and
     everyday words, and put every remaining term in its glossary. Use
     no decision numbers, finding numbers, or code names in its prose,
     unless the reader needs them to act.
2. **The open-work records.**
   - Edit a unit down to what is left, or delete it with its pin.
   - Add every decision you had to encode to `docs/DECISIONS.md`, as
     provisional, with its pins. Mark the ones the user settled as
     Decided, with their answer.
   - When a decided entry is built, its reasoning moves into the Why of
     its topic (`docs/architecture/<topic>.md`), and the entry keeps
     the record. A decision is a durable record. Never delete one.
   - Delete fixed findings in `docs/QA-FINDINGS.md`. Reword the ones
     the change answers in part. Update the reference hashes that a
     dogfood session measured again.
3. **The contract an agent reads.**
   - The capability's purpose and op descriptions, in its
     `crates/cap-<id>/OPS.md`. The generator refuses a file whose `##`
     sections are not exactly the capability's ops.
   - The worked examples in `crates/cap-abi/src/bin/gen_manifest.rs`.
     They are computed. Generate the manifest again.
   - The justfile's gate comments.
   - The TS bindings (`just types`).
4. **The comments.** Module and crate doc comments, the API notes in
   law-test headers, and the web glue's comments. A comment that
   describes a retired type, op, field, or address misleads the next
   reader as much as a stale doc does.
5. **The retired names.** Search the whole tree (`*.md`, `*.rs`, `*.ts`,
   `*.svelte`, `*.mjs`, and the justfile) for every name the change
   removed or renamed: crates, types, ops, fields, decision ids, and
   unit files. Also search for phrases that describe the old design.
   The search must find nothing, except code that reads old documents
   on purpose.

Where the detail lives:

- `docs/guide/`: the plain-language guide for people (agency staff, and
  developers new to the project). It covers what the software does, a
  walkthrough of one stop display, a developer's first change, the
  design choices and their costs, and the glossary.
- `crates/AGENTS.md`: the three architecture laws for everything under
  `crates/`. Arch gates in `just test` enforce them.
- `docs/architecture/`: the architecture reference, one file for each
  topic. Its README indexes them. It covers the detail of each crate,
  the bus and its rules, the server, storage (the agency's record),
  static deployment, the web, and the generated UIs. Each topic gives
  its What, and its Why where the reason is not obvious.
- `docs/units/`: the open work, one file for each unit. Read its README
  before you touch anything. A unit's file is deleted when the unit
  lands.
- `docs/CONVENTIONS.md`: how to write comments and documents, and the
  gotchas for Rust, wasm, ops, and the frontend.
- `docs/TESTING.md`: the testing expectations and the dogfood-QA rules.
- `docs/FORMATS.md`: the sync store's formats at rest (blobs,
  envelopes, and pointer logs), and the invariants that JSON Schema
  cannot state.
- `qa/`: the standing end-to-end QA tests, one folder each. Each
  folder has a README (the steps, the last run, and its findings) and
  its walk scripts. `qa/route-99/` goes from the stops app to service
  change, ride now, and the stop display.
- `LRP/`: the C-TRAN 2045 long range plan, as dogfood material. It holds
  the excerpt, its extracted tables (`scenarios.csv`,
  `capital-projects.csv`, and today's feed in the same shape), and the
  service-change sessions built on it, each one step of the plan.
  `LRP/SESSIONS.md` has the backlog, how to run a session, the log, and
  its findings. `LRP/run.sh` runs the sessions that have walks.
- `docs/QA-FINDINGS.md`: the open real-data findings (defects, and gaps
  in ops, UIs, and data), each pinned. A finding is deleted when it is
  fixed.
- `docs/DECISIONS.md`: the questions for the user and their answers.
  It is the one durable record of them, and nothing is deleted from it.
  An open decision names the work it blocks (`Blocked: D-N`). Work that
  names no decision is free to start.
- `docs/ROADMAP.md`: the sequenced feature plan, for changes of
  direction.
- `DEPLOYMENTS.md`: the operator's guide to the static export deployed
  to Cloudflare Workers. It is written for a sysadmin who never reads
  the code. How it works inside is in
  `docs/architecture/static-deployment.md`.
- `DEPLOYMENTS-SERVER.md`: the operator's guide to the server, for the
  same reader. It covers the image, the bucket, sign-in, the load
  balancer's health checks, updates and rollback, and the store's
  upkeep. How it works inside is in `docs/architecture/server.md`.

## Commands

`just` is the task runner. There are two runners:

- `just test` is the host-side gate, with no browser. Run it before you
  say any change is done.
- `just e2e` adds the browser runner. Run it for changes the UI shows.
- `just check` runs both. Run it for changes that touch both sides.

While you iterate, `just test-fast` runs the same host-side suites over
the wasm-fast modules. `just e2e-fast` runs the browser specs: all of
them, or one spec or one test by name. Neither is the gate. Build and
test through these recipes, not by hand.

```
just build                 # the frontend (it type-checks first: the TS layer is a gate)
just caps                  # assemblies/ (generated, gitignored), then the modules: each
                           # capability alone, and each UI's linked assembly and manifest
                           # (and cap_sync, cap_network, cap_streets, cap_authz, cap_plans,
                           # cap_migrate, cap_stops, cap_requests, and cap_log .wasm for
                           # the server, with the kind table web/pkg/kinds.json and its
                           # schemas)
just caps-fast             # the same, under the wasm-fast profile (no whole-program LTO,
                           # incremental). A library change rebuilds in ~10 s, not ~10 min.
                           # This is the loop's build. The local registry's evidence says
                           # profile wasm-fast, and test-fast runs over it. test, e2e, and
                           # registry build release.
just registry              # publish the capability registry (gates, then blobs and
                           # evidence). Every `just caps` publishes it again, ungated.
                           # It refuses a compiler other than rust-toolchain.toml's.
just docs [serve [PORT]]   # the docs site: static HTML from registry/ into docs-site/
                           # (gitignored). `serve` then serves it on
                           # http://127.0.0.1:8085/. Run `just caps` first
                           # (docs/architecture/registry.md, "The docs site").
just reproducible          # two clean builds of the tree, compared byte for byte (a gate
                           # of `just registry`; docs/architecture/release-artifacts.md)
node tools/caps-run.mjs    # the agent runner: list | tools <cap> [<op>] | session | <cap> --op ...
                           # Steps run in command-line order. --cap ID changes the
                           # capability during a session (documents cross by $ref).
                           # --as NAME names a result. $refs: $name.x, $load.x, $op.x,
                           # $N.path, and $TODAY / $NOW_SECS (a number).
                           # --load [N:|KIND:]FILE for bulk ops (feed:x.zip uses the
                           # kernel's loads). --op @FILE for big JSON.
                           # --ask URL asks an oracle ($ask.x). --post URL REF writes a
                           # document to the deployment ($post.x). For example,
                           # POST /plans '$apply.network' publishes a pick.
                           # --session NAME keeps the --as results across commands
                           # (.caps-sessions/NAME/), so a later command reuses an
                           # import; a stored document registers itself when an op
                           # names its hash. --brief prints a whole document as its
                           # kind, version, and size. $TODAY and $NOW_SECS read the
                           # agency's zone: --tz ZONE, else the session's network
                           # (D-175). `caps-run session [NAME]` lists what a session
                           # holds. A session journals every module call as hashes;
                           # `caps-run replay NAME|FILE.json [--single]
                           # [--module CAP=FILE]` runs a session's or a page's
                           # journal (`?journal`) again and compares every output hash. `tools <cap> --brief` gives each op's first
                           # sentence and example request; `tools <cap> <op>` gives
                           # one op in full (docs/architecture/registry.md).
just test                  # the host-side runner (Rust, smokes, runner gates, web parity
                           # ratchets, Go -race), with no browser
just test-fast             # the same suites over `caps-fast` modules. The edit loop's
                           # runner, never the gate. WASM_TEST_CACHE=off: the Go tests
                           # compile the modules again, with no disk cache.
just e2e                   # the browser runner (Playwright; it builds first), for UI changes
just e2e-fast [args]       # the same over `caps-fast` modules. Playwright's args pass
                           # through (a spec file, -g "test name"). The edit loop's
                           # browser runner, never the gate.
just check                 # both runners, for changes that touch both sides
just types                 # generate the TS bindings again (gitignored; build runs it)
just schema-lock           # the documents' schema lock: freeze a kind's new version, or
                           # record an added optional field in the same version (D-167),
                           # into the owning crate's locks/. `just caps` fails while a
                           # kind differs from its lock (docs/FORMATS.md, "The schema
                           # lock"). It needs no module build.
just http-lock             # the HTTP contract's lock: server/openapi.json copied to
                           # openapi.lock.json once the compatibility gate passes. Run it
                           # when a release ships (docs/architecture/release-artifacts.md).
just serve [feed.zip [streets.geojson ["Publisher" [addr]]]]   # the sync server on :8080.
                           # It reads its settings files: server/dev.toml (the fixture
                           # feed and streets, a mem:// store and attachments, the stub
                           # work order system, and -rt-proxy; D-67), then
                           # server/local.toml (gitignored; copy local.example.toml for
                           # your feed, store, sign-in, and geocoder). Each key is a
                           # flag's name. The arguments beat both files
                           # (docs/architecture/server.md, "Settings").
                           # feed and streets: a path, an http(s) URL, or bucket:<key>
                           # (D-118). store: s3://bucket/prefix?endpoint=…, azblob://…,
                           # gs://…, or mem:// (docs/FORMATS.md). POST /plans publishes
                           # a pick.
just server-image [REPO [TAG]]   # the server as one container image (ko):
                           # target/server-image.tar, or pushed to REPO for amd64 and
                           # arm64 (DEPLOYMENTS-SERVER.md)
just build-ui UI            # one UI built alone into web/dist-ui/UI/ (its page, its
                           # assembly, and the map only if it draws one)
just ui-deploy SERVER UI OUT [BUILD]   # a UI at its own URL (D-171): the server keeps
                           # each UI's builds on build.<id> (-ui-dist); this writes the
                           # head, or a logged BUILD, into OUT for the UI's host, which
                           # routes the data paths to the server. Rollback: POST
                           # /builds?ui=<id>, then this again (DEPLOYMENTS-SERVER.md)
just export UI OUT feed.zip [streets.geojson ["Publisher"]]   # one rider UI (ride-now or
                           # stop-display) as a static deployment of its own, with no
                           # server (D-131): its page, `manifest`, and each published
                           # document as blobs/<hash>[.gz]. The server answers the same
                           # paths. LIVE_FEEDS= names the deployment's live-feeds.json
                           # (no proxy).
just export-feed-url UI OUT URL [streets.geojson ["Publisher"]]   # the same, but the pages
                           # fetch and import the agency's GTFS URL themselves and check
                           # it daily. There is no redeploy for each feed (D-132).
just store verify|gc|dump|copy|revert … # the store's offline tools (server/cmd/store):
                           # verify a bucket, collect orphans (D-117), write a bucket
                           # to a folder to read it (dump; read only), carry an older
                           # build's directory store into a bucket, or move a pointer
                           # back to an older head and list the commits it dropped
                           # (the emergency procedure; docs/FORMATS.md, "Rolling a
                           # version back")
just store-dev             # local object stores in podman: SeaweedFS (:8333, the one to
                           # serve over), Azurite, and fake-gcs-server (store-dev-stop)
just store-contract [URL]  # the store's laws against each of them, or against URL alone
                           # (an operator's own bucket) (opt-in, never in
                           # `just test`)
just dev                   # vite hot reload, with the server beside it
just qa [feed.zip]         # the UI QA loop: modules under wasm-fast, a server on the
                           # C-TRAN feed (else the e2e fixture) in a memory store, and
                           # vite hot reload on :5174. The server is on :8091, its own
                           # port beside serve and dev, on server/dev.toml and
                           # local.toml, with its store in memory. Drive it with:
node tools/ui-shot.mjs UI  # [--hash H] [--steps walk.mjs] [--full] [--size WxH] [--base URL] [--profile NAME]:
                           # a screenshot, the status, and page errors. It keeps one
                           # browser profile in .qa/ (forked plans persist).
                           # docs/TESTING.md
just geocoder-build        # once: a local Pelias (portland-metro) in .pelias/
just geocoder              # start it on :4000 (geocoder = "http://127.0.0.1:4000" in
                           # server/local.toml; serve and dev proxy it at /geocode);
                           # geocoder-stop
just basemap-assets        # once: the basemap's glyphs and sprites into web/public/
                           # (gitignored; labels read them locally, else from the network)
just fixtures              # generate the e2e GTFS fixture again
just soak                  # the real-data soak (COUNTY_GEOJSON=… county roads)
just spec                  # optional: the TLC model check of the sync store engine
                           # (server/store/store.tla). Run it only for changes that touch
                           # the sync engine. It needs Java 11+ and tla2tools.jar, and is
                           # never part of `just test`.
just perf                  # browser perf baselines on the real feed. It reports, and
                           # never gates (docs/TESTING.md, "Performance baselines").
```

**Toolchains.**

- The Rust toolchain is pinned in `rust-toolchain.toml`. rustup runs
  that release in this tree. Upgrading it is a change of its own.
- The toolchains (just, cargo and rustc, go, node, and pmtiles) are on
  the shell's PATH already. Run the recipes as they are.
- Installs (toolchains, release binaries) are the user's job. cargo,
  npm, and go module fetches are fine.
- Java 11+ and `~/bin/tla2tools.jar` are also the user's to install, and
  they are optional. Only `just spec` uses them, and it is never in
  `just test`'s path.
- On an immutable desktop (Fedora Atomic), keep them in the home
  directory, or use a toolbox:
  - In the home directory: a Temurin tarball under `~/.local`,
    symlinked into `~/.local/bin`.
  - In a toolbox: `sudo dnf install java-21-openjdk` inside the
    container. The jar needs no copy, because toolbox shares $HOME, so
    `~/bin/tla2tools.jar` is already there. Run `just spec` through
    `toolbx enter`, or `toolbx run -c <name> bash -lc 'just spec'`.
  - Do not layer them onto the base with rpm-ostree.

## Architecture

**Three laws govern every crate.** `crates/AGENTS.md` states and
enforces them:

1. The network is the core data model. GTFS (and its siblings TODS and
   TIDES) is a serialization for sharing data with other systems.
2. No module rebuilds the monolith inside it.
3. Each module answers one need. Personas assemble in a UI, never in a
   module.

```
transit-network  the authored network model. Trips are stored records (their own id,
                 departure, and times). materialize answers a day's instances.
                 Authoring edits answer the next document (apply_edits). It also
                 answers where things are (pattern alignments, entity focus), and what
                 exists as the planner lists it: rows, route and stop composition, a
                 direction's rider variants (a station's bays are one place), the
                 timetable (one route, one direction, one service combination), and the
                 service combinations (the types that run together, and when).
transit-codec    the network codec: the whole-spec GTFS feed (every file and column,
                 deterministic zip bytes) to and from the network document, losslessly
transit-router   RAPTOR planning over the network model's materialized day: leave-at and
                 arrive-by, live predictions, the diversion notice, transfers over a walk
                 table, and trips from and to any place
transit-analysis the planner's analysis library (stats, service types, coverage)
transit-dispatch the operator's day library: the patched materialized day (diversions
                 patched, cancelled trips out), boards, authored block chains with the
                 layover ripple, diversion drafts, and trip cancels; and a rider's own
                 documents (the watchlist and its chain, the device records)
transit-rt       GTFS-rt prost types and the joins over the network. An rt trip id is a
                 model trip id. A trip instance is (trip, service date, start). It
                 also writes the service alerts document as an Alerts feed.
transit-live     the live document: rt in the model's vocabulary, each instance's service
                 date, the on-time window, and what does not run (a cancelled trip, a
                 skipped stop). Pure.
transit-version  a document's kind tag and version (D-115): the tag every persisted
                 document writes, and what a reader checks to refuse another version;
                 the one version ahead a release reads, lossless or read-only
                 (`Versioned::AHEAD`, D-186); and a record's fields this build does
                 not know, kept and named (`Extra`, D-167)
transit-sync     the sync document (EditRecord: a Diversion or a TripCancel), merge, and
                 the record envelope; and the record a revert of the disruptions log
                 writes back, stamped to win the merge (D-193). Pure.
transit-plans    the service-plan chain (resolve, periods, validity; pure and derived),
                 the published plan set (publish, the pick in force), and the
                 frozen versions: a freeze writes a version with what it is, and
                 only a frozen version is adopted (D-191)
transit-stops    the stops document, beside the plans. Each stop is a list of dated
                 events (planned, permitted, built, in_service, suspended, moved,
                 changed, retired), each with an id unique in its stop (D-166). It
                 answers the stop on a date, the stops near a point, served, what a
                 write must pass, its edits (an event placed by date, a correction
                 named by event id), and what a stop may become. Pure. A stop's life
                 is its own, and a pick only reads it (D-157).
transit-requests the stop requests document: the planning team's requests for work at a
                 stop (new, reinstall, relocate/remove, repair/improve, temporary). Each
                 tracks the work order that the agency's work order system opens for it.
                 It answers what a write must pass, a request placed by id, and the work
                 across every stop. Pure (D-159).
transit-log      the append-only log of the agency's record, kind-agnostic (D-190,
                 D-197): an entry of changed records (each record's hash as stored
                 before, and its new state), the fold, checkpoints, the revert (a
                 record the entry created leaves the fold, D-198), the gap and
                 quarantine rules, and the idempotency keys. A document becomes
                 records and back by its collection. Pure.
transit-alerts   the service alerts document and the alert templates document (D-206,
                 D-210): the agency's alerts in the network's ids (cause, effect,
                 severity, texts in each language, periods, informed entities, and
                 the disruptions each explains). It answers what a write must pass,
                 what a person is told about one that passes, ending an alert, the
                 agency's list, the gap list (what is in force that no alert
                 explains), and a template's blanks filled. Pure. transit-rt writes
                 it as a GTFS-rt Alerts feed.
transit-runs     the runs layer beside the network: the work-rule document (recovery,
                 interlining for each route pair, run rules, pay; every rule optional),
                 the recovery a wait requires and its slack, and the suggested blocking.
                 Pure.
transit-streets  the streets document (canonical: its hash is the streets' hash, not a
                 file's), its import from street layers (GeoJSON: agency exports, TIGER,
                 OSM), the graph over it (snap, route), and walking (the walk table
                 between stops, and a point's walks to the stops near it)
cap-abi          the kernel and the seam: the registries (one for each document kind,
                 with a private constructor), each kind's load, derived entries (the
                 held day, graphs), the routing that an assembly's exports run
                 (`assembly!`; a capability only registers, with `capability!`), the
                 kernel ops (loads, evict, held, about), the canonical form (RFC 8785,
                 `jcs`; D-169), the manifest, the validator, gen-manifest (with
                 the schema lock: each kind's frozen schema and example,
                 kept in the owning crate's locks/; and the kind table, how
                 each kind the store keeps is written, checked, and read,
                 web/pkg/kinds.json; D-202), gen-assemblies, and arch-gates
cap-gtfs cap-router cap-analysis cap-network cap-rt cap-board cap-runcut cap-calendar cap-directory cap-streets cap-sync cap-authz cap-plans cap-author cap-divert cap-migrate cap-stops cap-requests cap-alerts cap-log
                 the capabilities: rlibs that register, forbid unsafe, declare no state,
                 and never depend on each other (docs/architecture/bus.md)
assemblies/      generated (gen-assemblies, gitignored): module-<cap> (one capability
                 alone) and module-ui-<id> (a generated UI's capabilities, linked)
server/          the Go and wazero host. It runs cap-sync (the edit document and its
                 records in the disruptions log), cap-network,
                 cap-plans, and cap-streets (the network, picks, streets, and walk
                 table it publishes; in store mode, each stored when it is made, so
                 a boot imports nothing it already holds (D-199), and every build
                 it serves, kept and rolled back by pointer; D-80). It also runs cap-migrate (an older
                 document, at receive), cap-log (the disruptions log, the stops log, the
                 plans log, and the alerts log: logs/<log>/ in the bucket), cap-stops
                 (the stops document's writes and records, and what a published pick
                 reads of it), cap-requests (the stop requests document's writes and
                 records), cap-alerts (the service alerts and alert templates
                 documents' writes and records), and cap-authz when sign-in is on. Its HTTP surface is server/openapi.json,
                 which changes by expand, then contract (D-81). It has two proxies:
                 /geocode to a Pelias geocoder, and /rt, a dev feed relay that is off
                 unless -rt-proxy is set. It is scaffolding and never makes a domain
                 decision.
web/             the Svelte 5 and Vite frontend. / is a picker over the generated UIs
                 (src/uis/<id>/, each its own page and host layer; every page is one).
                 docs/architecture/generated-uis.md is the recipe a new agent follows.
                 Each page runs its linked assembly in the page's one bus worker,
                 behind the engine.
```

The detail for each crate is in `docs/architecture/crates.md`.

**Ownership rules.** Breaking these rules caused real bugs. Keep them.

- **Hosts own I/O, files, and the clock.** They read the clock in the
  agency's time zone (the manifest's `timezone`; `web/src/clock.ts`),
  never the device's. The wasm module owns the domain logic.
  - Dates cross every boundary as "YYYY-MM-DD" strings.
  - Times cross as u32 seconds since service-day midnight. Values past
    86400 are valid: they are trips after midnight.
- **The base feed is immutable.** Diversions produce patched copies for
  each service date. Patching never changes its input.
- **JS owns the documents** (the plan set, the watchlist). They are
  `$state.raw` in the host layer (`src/plans.svelte.ts`,
  `src/documents.svelte.ts`). JS replaces each document whole and never
  changes one in place. This is what makes IndexedDB cloning and sync
  work. wasm holds only materialized views.
  - A service change is a fork: the next pick copies the whole world it
    changes.
  - The chain (which pick is in force, and the governing periods) is
    always derived by the artifact, and is never stored.
- **The sync store's production home is object storage** (S3 and
  S3-compatible stores, Azure Blob, GCS; D-113). The engine may assume
  only whole-object writes and a conditional create of an absent key.
  It never assumes an append, a rename, an fsync, or a file lock. There
  is no directory store. The bucket is the agency's record. Every other
  copy is a working copy (`docs/architecture/storage.md`).
- **Documents are open formats (D-115).** A document is plain JSON
  (gzip allowed) or an external open format (GTFS, GTFS-rt, GeoJSON),
  never a private encoding. Each document says its kind and version
  inside it. Every kind's schema and a worked example are published for
  each version (`docs/FORMATS.md`). So a document can be read without
  the app.
- **The agency owns the data, and applications are interfaces (D-146,
  D-152).** No application, including this repo's, is the record for a
  document.
  - Reads are open to any application.
  - Every write goes through a receive path. The receive path checks
    shape, invariants, concurrency, and attribution. It refuses only
    what makes the data wrong. It accepts a change that has
    consequences, and flags it for a person.
  - Import and export are interfaces defined by their formats, never by
    their callers (`docs/architecture/agency-data.md`).
- **The history is the log.** Nothing is deleted from the bucket. Where a
  kind is kept in a log, the log keeps every state of every record, and a
  record leaves the current document only through an entry: the revert of
  the entry that created it (D-198). Where a kind is still a document, or
  where a merge would bring a dropped record back (the edit document's
  LWW, D-193), "delete" means `status: "retired"`, with the rev
  increased.
- **Schedules change by replacing the artifact, not by changing it.**
  - The planner publishes a new zip. The server imports it once
    (cap-network) and publishes the network.
  - Browsers never parse GTFS that the server has. A serverless
    deployment may instead name the agency's GTFS URL, and each page
    imports it (D-132).
  - The manifest pins `network_hash` (with the zip's `feed_hash` as its
    source), the published picks (`plans_hash`; a page loads the pick in
    force), and the sync module's `wasm_hash`.
  - Every client pins again and validates again.
- **The sync protocol is LWW for each record id, with tombstones.** It
  is not a CRDT, on purpose: the final authority is the agency's
  scheduling system, through propose, review, and accept.
  `transit_sync::merge` is a property-tested semilattice. `POST /edits`
  is the whole client protocol. The Go host treats documents as opaque
  JSON.

## Conventions

- **Write at about 80% of ASD-STE100.** This applies to code comments
  and to the developer documents. Use short sentences and one
  instruction in each sentence. Use the active voice and one name for
  each concept. Do not use dash asides, capitals for emphasis, or
  aphorisms. The rules and the reasons are in `docs/CONVENTIONS.md`,
  "Writing". The guide keeps ISO 24495-1.
- **Clarify, don't assume.** Ask before you build on an answer you
  picked, when a directive is ambiguous, conflicts with the GTFS spec
  or an architecture law, or leaves a design decision open. Record the
  question in `docs/DECISIONS.md`, and name the work it blocks. The rest
  of the work continues while the question is decided.
- **Git is the user's.** Never commit, push, stash, or move refs in any
  other way. Do the work, leave the tree, and report what changed.
- **The TS bindings are generated and gitignored** (`just types`, which
  `just build` runs). Never edit them by hand, and never commit them.
  Rust is the source of truth. The manifest's computed examples are the
  readable wire contract.
- **Parity.** The browser, caps-run, and the Go server run the same ops
  and get the same bytes. `web/` is host glue and render components. It
  never computes answers (`docs/architecture/web.md`, "Parity"; a
  ratchet in `just test` holds it).
- **One op caller.** A host asks through the engine
  (`web/src/engine.ts`) with `ask(capability, op, args)`. It holds
  documents (`Doc`), never modules or hashes. No other code imports the
  bus or calls a module. The modules live in the page's one bus worker
  (`web/src/bus.worker.ts`), off the render thread. Web gates in
  `just test` enforce this. A generated UI's host names its page
  (`assemble("<id>")`), so its capabilities answer in its linked
  assembly.
- **External services are sources, presentation, or oracles.** An
  oracle (the geocoder) answers an input the user could have given by
  hand. A host sends it only the user's input, shows its answer as
  given, and finds it in the manifest's `services`
  (`docs/architecture/web.md`, "External services").
- **Adjacent systems** (CAD/AVL, workforce management, the scheduling
  system) run the dispatcher's day, which is out of scope (D-122). They
  read documents by hash. They write only records, through the receive
  path, as effects in this repo's vocabulary. Each fact has one home
  (`docs/architecture/adjacent-systems.md`).
- **New ops follow the extraction pattern:** the crate boundary first,
  the capability wrapper second. The monolith that the ops were
  extracted from is gone. So a new op is a library and a capability
  module, never host code.

The other gotchas are in `docs/CONVENTIONS.md`.

## Testing

**Laws first.** Every behavior change starts with the property tests
that define it. Derive them from the user's stated intent and the GTFS
spec, never from the current code. Write them before the
implementation. When the intent leaves a behavior ambiguous, ask the
user. Do not pick an answer and encode it. The rules are in
`docs/TESTING.md`, "Laws first".

Every change lands with tests at the layer that owns the logic.
`docs/TESTING.md` states the expectations: unit and property tests in
the owning crate, a module dispatch test, caps-smoke and the runner
gates, a Go round trip, Playwright e2e, and, for work a user workflow
reads, a real-data dogfood of that workflow (D-196).

## Deliberate non-goals (do not "fix" these)

- **No CRDT or automerge for sync.** LWW with tombstones is the design.
- **GTFS stays out of transit-network.** transit-network is the
  authored model. The import and export codecs live at the boundary,
  in transit-codec.
- **No wasm-bindgen and no JS glue, on purpose.** Every module is the
  raw rustc output of an assembly crate, with the same four exports.
  There are no custom calling conventions.
- **The geocoder is an oracle, not a document on the bus.** There is no
  gazetteer capability ("External services").
- **The Go server stays scaffolding.** Domain logic that the server
  needs becomes a capability module. The server hosts:
  - cap-sync;
  - cap-network, for the import and the picks it publishes;
  - cap-plans, for the published plan set and the frozen versions;
  - cap-streets, for the streets and walk table it publishes;
  - cap-migrate, for an older document at receive;
  - cap-stops, for the stops document's writes;
  - cap-requests, for the stop requests document's writes;
  - cap-alerts, for the service alerts and alert templates documents'
    writes;
  - cap-log, for the logs that record the edit document, the stops,
    the stop requests, the published plan set, the frozen versions, the
    service alerts, and the alert templates.
- **The bus is the architecture.**
  - No capability grows a private document format. Shared data crosses
    capabilities only as kernel documents named by content hash.
  - Capabilities never call capabilities. Linking assembles bytes, not
    surfaces. The arch gates and assembly independence enforce this.
