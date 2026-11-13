# AGENTS.md

Local-first transit software: GTFS static + GTFS-rt, parsed by
Rust, shipped as **capability modules** — one flat import-free
wasm ABI everywhere; a generated UI's capabilities link into ONE
module over one kernel registry (D-77). The same DOCUMENT and the same contracts on
every host (browser, Node, Go + wazero) is the point of the
project; the bytes are assembled per user from the manifest. The
bus is the architecture: every host instantiates the capability
modules it needs.

**Two deployment shapes, both first-class.** With the sync server
(`just serve`), or with no server at all (`just export <ui>`: a
rider UI as static files). The operator chooses per deployment.
Neither is the other's subset: a UI that runs without a server keeps
running without one, the server's features stay the server's, and a
change made for one shape adds to it and takes no capability from the
other. **Each generated UI is its own product**, deployed, updated, and
rolled back on its own, not a part of one app; which UIs need the
server is in `docs/architecture/README.md` ("Deployment shapes", "Each
UI is its own product").

**This file is load-bearing, and so are its pointers.** The detail
lives in the docs below; the rules that keep them from drifting:

- **Docs describe the repo as it is now.** History is git's: no
  "was", no "moved from", no unit numbers for finished work, no
  session narratives. Architecture docs are written in the present
  tense.
- One doc per topic. Architecture detail lives in
  `docs/architecture/`, one file per topic (and `crates/AGENTS.md` for
  the three laws);
  open work lives only in `docs/units/`, one file per unit — never
  restate status here or anywhere else. Architecture docs may be
  many, but they must not overlap: each topic has exactly one home.
  The guide (`docs/guide/`) explains the same topics to people in
  plain language and links to each topic's home; it holds no detail
  of its own, and where it and a home disagree, the home is right.
- When your work changes the architecture, the ops surface, naming,
  conventions, commands, or testing expectations — update this file
  AND every doc whose topic the change touches, in the same change,
  before you declare the work done.
- When content moves between docs, fix every reference to it in the
  same change. A pointer to a section that no longer exists is a
  broken build for the next agent.
- **Deleting a unit file is the DONE claim** (`docs/units/README.md`).
  Do not delete one whose work is not done. The change that deletes
  it NAMES ITS PIN — the gate, e2e spec, or dogfood session that
  proves it — and describes what landed in the architecture docs.
  On-the-bus, UI-reads-the-bus, and dogfooded are three different
  claims; a unit missing a grade stays open. A fixed QA finding is
  deleted the same way. Stale instructions are how the next agent
  breaks the build.
- If a section contradicts the code, the code wins for the moment
  and the section must be fixed immediately.

**Finishing work: the documentation sweep.** Work is not done, and
is not reported as done, until its documentation is. Before you
tell the user a piece of work is finished, sweep every place that
describes what you changed, and say in your report what you
updated:

1. **The docs.** `AGENTS.md` (the crate tree, commands, rules),
   `README.md` (what the project is and how to start it),
   `crates/AGENTS.md`, and every file under `docs/` whose topic the
   change touches — in the present tense, one home per topic. That
   includes `docs/guide/` when the change alters something it
   describes: what an app does (a new UI gets a short entry in
   `docs/guide/apps.md`), a step in its walkthrough, a design
   choice or its cost, or a term in its glossary. The guide is
   written for people to ISO 24495-1 (plain language): short
   sentences, everyday words, every remaining term in its glossary,
   no decision or finding numbers and no code names in its prose
   unless the reader needs them to act.
2. **The open-work records.** Edit a unit down to what is left, or
   delete it with its pin; add every decision you had to encode to
   `docs/DECISIONS.md` (provisional, with its pins) and mark the
   ones the user settled Decided, with their answer — once built, a
   decided entry's reasoning moves into its topic's Why
   (`docs/architecture/<topic>.md`) and the entry keeps the record — a decision is
   a durable record, never deleted; delete fixed findings in
   `docs/QA-FINDINGS.md`, reword ones the change answers in part,
   and update the reference hashes a dogfood session re-measured.
3. **The contract an agent reads.** The capability's purpose and
   op descriptions in its `crates/cap-<id>/OPS.md` (the generator
   refuses one whose `##` sections are not exactly the capability's
   ops), the worked examples in `crates/cap-abi/src/bin/gen_manifest.rs`
   (computed; regenerate the manifest), the justfile's gate comments,
   and the TS bindings (`just types`).
4. **The comments.** Module and crate doc comments, the API notes
   in law-test headers, and the web glue's comments — a comment
   describing a retired type, op, field, or address misleads the
   next reader exactly as a stale doc does.
5. **The retired names.** Search the whole tree (`*.md`, `*.rs`,
   `*.ts`, `*.svelte`, `*.mjs`, the justfile) for every name the
   change removed or renamed — crates, types, ops, fields, decision
   ids, unit files — and for phrases that describe the old design.
   The search must come back empty, except for code that
   deliberately reads old documents.

Where the detail lives:

- `docs/guide/` — the plain-language guide for people (agency staff,
  developers new to the project): what the software does, a
  walkthrough of one stop display, the design choices and their
  costs, and the glossary
- `crates/AGENTS.md` — the three architecture laws for everything
  under `crates/` (enforced by arch gates in `just test`)
- `docs/architecture/` — the architecture reference, one file per
  topic (its README indexes them): per-crate detail, the bus and its
  rules, the server, static deployment, the web, the generated UIs —
  each topic's What, and its Why where the reason is not obvious
- `docs/units/` — the open work, one file per unit (read its
  README before touching anything); a unit's file is deleted when
  it lands
- `docs/CONVENTIONS.md` — gotchas, Rust/wasm/ops and frontend
- `docs/TESTING.md` — the testing expectations and the dogfood-QA
  rules
- `docs/FORMATS.md` — the sync store's at-rest formats (blobs,
  envelopes, pointer logs) and the invariants JSON Schema cannot say
- `docs/QA-FINDINGS.md` — the open real-data findings (defects,
  op/UI/data gaps), each pinned; deleted when fixed
- `docs/DECISIONS.md` — the questions for the user and their
  answers, the one durable record of them (never deleted); an open
  one names the work it blocks (`Blocked: D-N`), and work that names
  none is free to start
- `docs/ROADMAP.md` — the sequenced feature plan (direction changes)
- `DEPLOYMENTS.md` — the operator's guide: the static export deployed
  to Cloudflare Workers, written for a sysadmin who never reads the code
  (how it works inside is `docs/architecture/static-deployment.md`)
- `DEPLOYMENTS-SERVER.md` — the operator's guide to the server: its
  image, the bucket, sign-in, the load balancer's health checks,
  updates and rollback, and the store's upkeep, for the same reader
  (how it works inside is `docs/architecture/server.md`)

## Commands

`just` is the task runner. Two runners: `just test` is the
host-side gate (no browser) — run it before declaring any change
done; `just e2e` adds the browser runner, for UI-visible changes;
`just check` runs both, for changes that touch both sides. While
iterating, `just test-fast` runs the same host-side suites over the
wasm-fast modules; it is not the gate.

```
just build                 # frontend (type-checks first — the TS layer is a gate)
just caps                  # assemblies/ (generated, gitignored) → the modules: each
                           # capability alone and each UI's linked assembly + manifest
                           # (+ cap_sync, cap_network, cap_streets, cap_authz,
                           # cap_plans, cap_migrate and cap_stops .wasm for the server)
just caps-fast             # the same, under the wasm-fast profile (no whole-program
                           # LTO, incremental): a library change rebuilds in ~10 s, not
                           # ~10 min — the loop's build; the local registry's evidence
                           # says profile wasm-fast; test-fast runs over it;
                           # test/e2e/registry build release
just registry              # publish the capability registry (gates → blobs + evidence;
                           # every `just caps` republishes it ungated); refuses a
                           # compiler other than rust-toolchain.toml's
just reproducible          # two clean builds of the tree, byte for byte (a gate of
                           # `just registry`; docs/architecture/release-artifacts.md)
node tools/caps-run.mjs    # the agent runner: list | tools <cap> | <cap> --op ...
                           # steps run in command-line order; --cap ID switches
                           # capability mid-session (documents cross by $ref);
                           # --as NAME names a result; $refs ($name.x, $load.x,
                           # $op.x, $N.path) + $TODAY/$NOW_SECS (a number);
                           # --load [N:|KIND:]FILE for bulk ops (feed:x.zip — the
                           # kernel's loads); --op @FILE for big JSON;
                           # --ask URL asks an oracle ($ask.x); --post URL REF
                           # writes a document to the deployment ($post.x —
                           # e.g. POST /plans '$apply.network' publishes a pick)
just test                  # the host-side runner (Rust + smokes + runner gates + web
                           # parity ratchets + Go -race) — no browser
just test-fast             # the same suites over `caps-fast` modules — the edit
                           # loop's runner, never the gate (WASM_TEST_CACHE=off:
                           # the Go tests compile modules afresh, no disk cache)
just e2e                   # the browser runner (Playwright, builds first) — UI changes
just check                 # both runners — changes touching both sides
just types                 # regenerate TS bindings (gitignored; build runs it)
just serve [feed.zip [streets.geojson ["Publisher" [addr]]]]   # sync server on :8080
                           # (with -rt-proxy, the dev feed relay — D-67)
                           # feed/streets: a path, an http(s) URL, or bucket:<key>
                           # (D-118). STORE=url adds the durable sync store
                           # (-store: s3://bucket/prefix?endpoint=…, azblob://…,
                           # gs://…, or mem://; docs/FORMATS.md; POST /plans
                           # publishes a pick)
                           # No STORE = the dev store: edits in memory, no
                           # history or publishing (logged DEV STORE)
just server-image [REPO [TAG]]   # the server as one container image (ko):
                           # target/server-image.tar, or pushed to REPO for
                           # amd64 + arm64 (DEPLOYMENTS-SERVER.md)
just build-ui UI            # one UI built alone → web/dist-ui/UI/ (its page, its
                           # assembly, the map only if it draws one)
just export UI OUT feed.zip [streets.geojson ["Publisher"]]   # one rider UI (ride-now,
                           # stop-display) as a static deployment of its own, no
                           # server (D-131): its page, `manifest`, and each
                           # published document as blobs/<hash>[.gz] — the paths
                           # the server answers too; LIVE_FEEDS= names the
                           # deployment's live-feeds.json (no proxy)
just export-feed-url UI OUT URL [streets.geojson ["Publisher"]]   # the same, but the
                           # pages fetch and import the agency's GTFS URL
                           # themselves, checked daily — no redeploy per feed (D-132)
just store verify|gc|copy … # the store's offline tools (server/cmd/store): verify a
                           # bucket, collect orphans (D-117), carry an older
                           # build's directory store into a bucket
just store-dev             # local object stores in podman: SeaweedFS (:8333, the one
                           # to serve over), Azurite, fake-gcs-server (store-dev-stop)
just store-contract        # the store's laws against each of them (opt-in, never
                           # in `just test`)
                           # OIDC_ISSUER/OIDC_JWKS_URL/OIDC_AUDIENCE/OIDC_KEY/POLICY=…
                           # turn on JWT auth + policy (D-58..D-61;
                           # docs/architecture/server.md)
just dev                   # vite hot reload + server alongside
just qa [feed.zip]         # the UI QA loop: modules under wasm-fast, a server on the
                           # C-TRAN feed (else the e2e fixture) in a memory store,
                           # vite hot reload on :5174 (server :8091, its own
                           # ports beside serve/dev); drive it with
node tools/ui-shot.mjs UI  # [--hash H] [--steps walk.mjs] [--full] [--size WxH]:
                           # a screenshot + status + page errors, one browser
                           # profile in .qa/ (forked plans persist) — docs/TESTING.md
just geocoder-build        # once: a local Pelias (portland-metro) in .pelias/
just geocoder              # start it on :4000 (GEOCODER=http://127.0.0.1:4000
                           # just serve|dev proxies it at /geocode); geocoder-stop
just basemap-assets        # once: the basemap's glyphs + sprites into web/public/
                           # (gitignored; labels read them locally, else the network)
just fixtures              # regenerate the e2e GTFS fixture
just soak                  # real-data soak (COUNTY_GEOJSON=… county roads)
just spec                  # OPTIONAL: TLC model-check of the sync store engine
                           # (server/store/store.tla) — only for changes touching the sync
                           # engine; needs Java 11+ + tla2tools.jar, never part of `just test`
just perf                  # browser perf baselines on the real feed — reports, never
                           # gates (docs/TESTING.md, "Performance baselines")
```

The Rust toolchain is pinned in `rust-toolchain.toml` (rustup runs
that release in this tree; upgrading it is a change of its own).
Toolchains live in `~/.cargo/bin` and `~/.local/bin` (rustc, pmtiles,
just, go at `~/.local/go/bin`) — export PATH if a shell lacks them.
Installs (toolchains, release binaries) are the user's job;
cargo/npm/go module fetches are fine. Java 11+ and
`~/bin/tla2tools.jar` are the same and OPTIONAL — only `just spec`
uses them, never in `just test`'s path. On an immutable desktop
(Fedora Atomic) either keep them in the home directory (a Temurin
tarball under `~/.local`, symlinked into `~/.local/bin`) or use a
toolbox: `sudo dnf install java-21-openjdk` inside the container —
the jar needs no copy, toolbox shares $HOME, so `~/bin/tla2tools.jar`
is already there; run `just spec` via `toolbx enter` or
`toolbx run -c <name> bash -lc 'just spec'`. Don't rpm-ostree-layer
them onto the base.

## Architecture

**Three laws govern every crate** (spelled out and enforced — see
`crates/AGENTS.md`): the NETWORK is the core data model, GTFS (and
its siblings TODS and TIDES) is a serialization spec for external
data sharing; nobody reinvents
the monolith inside a module; and one NEED per module — personas
assemble at wire time.

```
transit-network  THE authored network model — trips are stored records (own id,
                 departure, times); materialize answers a day's instances;
                 authoring edits answer the next document (apply_edits);
                 where things are (pattern alignments, entity focus);
                 what exists, as the planner lists it (rows, route and stop composition,
                 a direction's rider variants — a station's bays one place,
                the timetable: one route, one direction, one service combination;
                 the service combinations: the types that run together, and when)
transit-codec   the network codec: the whole-spec GTFS feed (every file and column,
                 deterministic zip bytes) ⇄ network document, losslessly
transit-router  RAPTOR planning over the network model's materialized day (leave-at and
                 arrive-by, live predictions, the diversion notice, transfers over a walk table,
                 trips from and to any place)
transit-analysis planner analysis library (stats, service types, coverage)
transit-dispatch the operator's day library (patched materialized day — diversions
                 patched, cancelled trips out — boards, authored block chains with
                 the layover ripple, diversion drafts and trip cancels)
transit-rt       GTFS-rt prost types + the joins over the network (an rt trip id is a
                 model trip id; a trip instance is (trip, service date, start))
transit-live     THE live document (rt in model vocabulary, each instance's service
                 date, the on-time window, what does not run — a cancelled trip, a
                 skipped stop — pure)
transit-version  a document's kind tag and version (D-115): the tag every persisted
                 document writes, and what a reader checks to refuse another version
transit-sync     THE sync document (EditRecord: a Diversion or a TripCancel, merge, the record envelope — pure)
transit-plans    THE service-plan chain (resolve, periods, validity — pure, derived)
                 and the published plan set (publish, the pick in force)
transit-stops    THE stops document, beside the plans: each stop a list of dated events
                 (planned, permitted, built, in_service, suspended, moved, changed, retired), the
                 stop on a date, served, what a write must pass, its edit (an
                 event placed by date) — pure; a stop's life is its own, and a
                 pick only reads it (D-157)
transit-runs     THE runs layer beside the network: the work-rule document (recovery,
                 interlining per route pair, run rules, pay — every rule optional), the
                 recovery a wait requires and its slack, the suggested blocking — pure
transit-streets  THE streets document (canonical: its hash is the streets', not a file's),
                 its import from street layers (GeoJSON: agency exports, TIGER, OSM),
                 the graph over it (snap, route), and walking (the walk table between stops,
                 a point's walks to the stops near it)
cap-abi          the kernel and the seam: the registries (one per document kind,
                 constructor private), each kind's load, derived entries (the held
                 day, graphs), the routing an assembly's exports run (`assembly!`;
                 a capability only registers with `capability!`), the kernel ops
                 (loads, evict, held, about), manifest, validator, gen-manifest,
                 gen-assemblies, arch-gates
cap-gtfs cap-router cap-analysis cap-network cap-rt cap-board cap-runcut cap-calendar cap-directory cap-streets cap-sync cap-authz cap-plans cap-author cap-divert cap-migrate cap-stops
                 the capabilities: rlibs that register, forbid unsafe, declare no
                 state, and never depend on each other (docs/architecture/bus.md)
assemblies/      GENERATED (gen-assemblies, gitignored): module-<cap> (one capability
                 alone) and module-ui-<id> (a generated UI's capabilities linked)
server/          Go + wazero host over cap-sync, cap-network, cap-plans and
                 cap-streets (the network, picks, streets and walk table it
                 publishes; in store mode every build it serves, kept and
                 rolled back by pointer — D-80), cap-migrate (an older document, at receive),
                 cap-stops (the stops document's writes),
                 cap-authz when auth is on, and the proxies
                 (/geocode to a Pelias geocoder; /rt, a dev feed relay off
                 unless -rt-proxy) — scaffolding, never a domain decision
web/             Svelte 5 + Vite frontend: / is a picker over the GENERATED UIs
                 (src/uis/<id>/, each its own page and host layer; every page
                 is one) (docs/architecture/generated-uis.md —
                 the recipe a fresh agent follows); each page runs its linked
                 assembly in the page's one bus worker, behind the engine
```

Per-crate detail: `docs/architecture/crates.md`.

**Ownership rules** (violations here caused real bugs; keep them):

- Hosts own I/O, files, and the clock — read in the agency's zone
  (the manifest's `timezone`; `web/src/clock.ts`), never the
  device's. The wasm module owns domain logic. Dates cross every boundary as "YYYY-MM-DD" strings, times
  as u32 seconds since service-day midnight (values past 86400 are
  legitimate — after-midnight trips).
- The **base feed is immutable**. Diversions produce patched copies
  per service date; patching never mutates its input.
- **JS owns the documents** (plan set, watchlist): `$state.raw` in
  the host layer (`src/plans.svelte.ts`, `src/documents.svelte.ts`),
  wholesale-replaced, never mutated in place — that is what makes IndexedDB cloning and sync work. wasm
  holds only materialized views. A service change is a FORK (the
  next pick copies the whole world it changes); the chain (who is
  in force, governing periods) is always derived by the artifact,
  never stored.
- **The sync store's production home is object storage** (S3 and
  S3-compatible, Azure Blob, GCS — D-113). The engine may assume only
  whole-object writes and a conditional create of an absent key —
  never an append, a rename, an fsync, or a file lock. There is no
  directory store.
- **Documents are open formats (D-115)**: plain JSON (gzip allowed)
  or an external open format (GTFS, GTFS-rt, GeoJSON), never a
  private encoding; each says its kind and version inside it, and
  every kind's schema and a worked example are published per version
  (`docs/FORMATS.md`) — a document reads without the app.
- **The agency owns the data; applications are interfaces (D-146,
  D-152)**: no application, this repo's included, is the record for a
  document. Reads are open to any application; every write goes
  through a receive path that checks shape, invariants, concurrency,
  and attribution, refuses only what makes the data wrong, and leaves
  a change with consequences to be flagged for a person. Import and
  export are interfaces defined by their formats, never by their
  callers (`docs/architecture/agency-data.md`).
- **Retirement is the tombstone**: "delete" means
  `status: "retired"`, rev bumped. No hard deletes.
- Schedules change by **artifact replacement, not mutation**: the
  planner publishes a new zip; the server imports it once
  (cap-network) and publishes the NETWORK — browsers never parse
  GTFS the server has (a serverless deployment may instead name the
  agency's GTFS URL, and each page imports it — D-132); the manifest pins `network_hash` (with the
  zip's `feed_hash` as provenance) + the published picks
  (`plans_hash`; a page loads the pick in force) + the sync module's
  `wasm_hash`; everyone re-pins and revalidates.
- The sync protocol is **LWW per record id + tombstones**, not CRDT
  (deliberate — the terminal authority is the agency scheduling
  system; propose → review → accept). `transit_sync::merge` is a
  property-tested semilattice; `POST /edits` is the whole client
  protocol. The Go host treats documents as opaque JSON.

## Conventions

- **Clarify, don't assume.** When a directive is ambiguous, conflicts
  with the GTFS spec or an architecture law, or leaves a design
  decision open, ask before building on an answer you picked —
  and record the question in `docs/DECISIONS.md`, naming the work
  it blocks, so the rest proceeds while it is decided.
- **Git is the user's** — never commit, push, stash, or otherwise
  move refs. Do the work, leave the tree, report what changed.
- The TS bindings are GENERATED and gitignored (`just types`, run
  by `just build`; never hand-edit, never commit) — Rust is the
  source of truth, the manifest's computed examples the readable
  wire contract.
- **Parity**: the browser, caps-run, and the Go server run the
  same ops and get the same bytes — `web/` is host glue plus
  render components, never answers (`docs/architecture/web.md`, "Parity"; ratcheted in `just test`).
- **One op caller**: a host asks through the engine
  (`web/src/engine.ts`) — `ask(capability, op, args)`, holding
  documents (`Doc`), never modules or hashes; nothing else imports
  the bus or calls a module, and the modules live in the page's one
  bus worker (`web/src/bus.worker.ts`), off the render thread (web
  gates in `just test`). A generated UI's host names its page
  (`assemble("<id>")`) so its capabilities answer in its linked
  assembly.
- **External services** are sources, presentation, or oracles — an
  oracle (the geocoder) answers an input the user could have given
  by hand; a host sends it the user's input only, shows its answer
  as given, and finds it in the manifest's `services`
  (`docs/architecture/web.md`, "External services").
- **Adjacent systems** (CAD/AVL, workforce management, the
  scheduling system) run the dispatcher's day, which is out of scope
  (D-122): they read documents by hash and write only records, through
  the receive path, as effects in this repo's vocabulary — one home
  per fact (`docs/architecture/adjacent-systems.md`).
- New ops go through the extraction pattern (crate boundary
  first, capability wrapper second): the monolith that ops were
  extracted FROM is gone, so a new op is a library + a capability
  module, never a host.

The rest of the gotchas: `docs/CONVENTIONS.md`.

## Testing

**Laws first.** Every behavior change starts with the property
tests that define it, derived from the user's stated intent and the
GTFS spec — never from the current code — and written before the
implementation. When the intent leaves a behavior ambiguous, ask
the user; do not pick an answer and encode it. The rules are in
`docs/TESTING.md`, "Laws first".

Every change lands with tests at the layer that owns the logic —
the expectations (unit/property in the owning crate, module
dispatch test, caps-smoke + runner gates, Go round-trip, Playwright
e2e, real-data dogfood) are spelled out in `docs/TESTING.md`.

## Deliberate non-goals (do not "fix" these)

- No CRDT/automerge for sync — LWW + tombstones is the design.
- transit-network is the authored model; GTFS stays OUT of it — the
  import/export codecs live at the boundary in transit-codec.
- wasm-bindgen / JS glue is gone on purpose; every module is the
  raw rustc output of an assembly crate with the same four exports —
  no custom calling conventions.
- The geocoder is an oracle, not a document on the bus — no
  gazetteer capability ("External services").
- The Go server stays scaffolding; domain logic the server needs
  becomes a capability module (it hosts cap-sync, cap-network for
  the import and the picks it publishes, cap-plans for the published
  plan set, cap-streets for the streets and walk table it
  publishes, cap-migrate for an older document at receive, cap-stops
  for the stops document's writes).
- The bus is the architecture: no capability grows a private
  document format — shared data crosses capabilities as kernel
  documents addressed by content hash only, and capabilities never
  call capabilities (linking assembles bytes, not surfaces — the
  arch gates and assembly independence enforce it).
