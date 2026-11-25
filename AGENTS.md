# AGENTS.md

wasm-transit is local-first transit software. It reads GTFS static and
GTFS-rt with Rust, and ships as **capability modules**. Every module has
the same flat WebAssembly interface with no imports. Every host runs
the same documents and the same contracts: the browser, Node, and Go
with wazero. Each generated UI links the capabilities it needs into one
module over one kernel registry at build time (D-77, D-176).

These rules shape every change. `docs/architecture/README.md` holds
their detail:

- **Two deployment shapes, both complete:** with the sync server
  (`just serve`), or as static files with no server (`just export
  <ui>`). A change for one shape adds to that shape and takes nothing
  from the other ("Deployment shapes").
- **Built for the outage.** For each failure, the design states what
  still works, what each person does instead, and how it recovers. The
  bucket alone recovers the agency. A page never shows an old answer as
  current ("Built for the outage").
- **Each generated UI is its own product.** It is deployed, updated,
  and rolled back on its own ("Each UI is its own product").

## Keeping the documents correct

This file holds rules and pointers, not lists. It does not describe a
crate, list the capabilities or the document kinds, or repeat a
recipe's options. A list that grows when the code grows lives in its
home, and is generated where it can be. So most changes do not edit
this file.

- **Docs describe the repo as it is now.** Git holds the history. Do
  not write "was", "moved from", unit numbers for finished work, or
  session narratives.
- **One home for each topic.** Architecture detail lives in
  `docs/architecture/`, one file for each topic, with no overlap. Open
  work lives only in `docs/units/`. Never state status in any other
  place. The guide (`docs/guide/`) explains the topics to people and
  links to each home. Where the guide and a home disagree, the home is
  right.
- **Update the docs in the same change.** When your work changes the
  architecture, the ops surface, naming, conventions, commands, or
  testing expectations, update this file and every doc whose topic the
  change touches. When content moves, fix
  every reference to it in the same change. A pointer to a missing
  section breaks the build for the next agent.
- **Deleting a unit file is the claim that the unit is done**
  (`docs/units/README.md`). The change that deletes it names its pin
  (the gate, the e2e spec, or the dogfood session) and what landed in
  the architecture docs. On the bus, read by a UI, and dogfooded are
  three different claims. A unit that is missing a grade stays open.
  Delete a fixed QA finding the same way.
- **The code wins.** If a doc contradicts the code, fix the doc
  immediately.

**Finishing work.** Work is not done, and you do not report it as
done, until its documentation is done. Before you report, run the
documentation sweep (the `doc-sweep` skill,
`.claude/skills/doc-sweep/SKILL.md`), and say in your report what you
updated.

## Where the detail lives

| Topic | Home |
|---|---|
| The three architecture laws | `crates/AGENTS.md` (arch gates in `just test` enforce them) |
| Architecture, one file for each topic | `docs/architecture/` (its README indexes them) |
| A library | its `//!` crate comment |
| A capability | `crates/cap-<id>/OPS.md` (`node tools/caps-run.mjs tools <id>` prints it), and its `//!` crate comment |
| cap-abi, the modules, the assemblies | `docs/architecture/bus.md`, `docs/architecture/registry.md` |
| The document kinds and formats at rest | `crates/cap-abi/src/kinds.rs`, `docs/FORMATS.md` |
| The server and the capabilities it hosts | `docs/architecture/server.md` |
| The HTTP contract (each route's bodies and answers) | `server/openapi.json`, held to the server by Go tests |
| The web and the generated UIs | `docs/architecture/web.md`, `docs/architecture/generated-uis.md` (one file for each UI in `generated-uis/`: its purpose, its rules, and its failure table) |
| Writing, traceability, gotchas | `docs/CONVENTIONS.md` |
| The documentation sweep | `.claude/skills/doc-sweep/SKILL.md` |
| Testing and dogfood QA | `docs/TESTING.md` |
| Each recipe | the comment above it in the justfile |
| The agent runner | the header of `tools/caps-run.mjs`, and `docs/architecture/registry.md` |
| UI screenshots (`tools/ui-shot.mjs`) | `docs/TESTING.md` |
| Open work | `docs/units/` (read its README before you touch anything) |
| Questions for the user, and their answers | `docs/DECISIONS.md` (the rules and the summary), one file for each entry in `docs/decisions/` (never delete one) |
| Open real-data findings | `docs/QA-FINDINGS.md` |
| The sequenced feature plan | `docs/ROADMAP.md` |
| The standing end-to-end QA tests | `qa/`, one folder each, with a README |
| Long range plan dogfood material | `LRP/` (`LRP/SESSIONS.md`) |
| Operator guides | `DEPLOYMENTS.md` (static), `DEPLOYMENTS-SERVER.md` (server) |
| The guide for people | `docs/guide/` |

`just docs` draws the repo map, the capabilities and the documents they
read, and what each UI links, from the code (`architecture.html`).

## Commands

`just` is the task runner. `just test` is the host-side gate, with no
browser. Run it before you say any change is done. `just e2e` adds the
browser runner, for changes the UI shows. `just check` runs both.

These build the wasm-fast modules (no whole-program LTO), which behave
as the release modules do; only their size differs. The `-release`
recipes take about an hour.
Run them when a unit is done, not for each change (D-236). Build and
test through the recipes, not by hand.

```
just test                  # the host-side gate (Rust, smokes, runner gates, web parity
                           # ratchets, Go -race), with no browser
just e2e [args]            # the browser gate (Playwright; it builds first)
just check                 # both gates
just test-release          # the same over the release modules: when a unit is done
just e2e-release
just check-release
just build                 # the frontend (it type-checks first: the TS layer is a gate)
just caps                  # the release modules, the manifest, and the local registry
just caps-fast             # the same under the wasm-fast profile: what the gates build
just types                 # the TS bindings again (gitignored; build runs it)
just schema-lock           # freeze a document kind's new version (docs/FORMATS.md)
just serve                 # the sync server on :8080
just dev                   # vite hot reload, with the server beside it
just qa                    # the UI QA loop on real data (docs/TESTING.md)
node tools/caps-run.mjs    # the agent runner: list | tools <cap> [<op>] | session | <cap> --op ...
node tools/ui-shot.mjs UI  # a screenshot of a UI, its status, and its page errors
```

`just --list` names every recipe. Read a recipe's comment before you
run it for the first time.

**Toolchains.** just, cargo and rustc, go, node, and pmtiles are on the
PATH. `rust-toolchain.toml` pins Rust, and upgrading it is a change of
its own. Installs are the user's job. cargo, npm, and go module fetches
are fine. Java is optional and only `just spec` uses it (see its
recipe comment).

## Architecture

**Three laws govern every crate** (`crates/AGENTS.md`):

1. The network is the core data model. GTFS (and its siblings TODS and
   TIDES) is a serialization for sharing data with other systems.
2. No module rebuilds the monolith inside it.
3. Each module answers one need. Personas assemble in a UI, never in a
   module.

```
crates/transit-*   the domain libraries, in plain Rust. transit-network is the core
                   model. transit-codec reads and writes GTFS at the boundary.
crates/cap-abi     the kernel and the seam: the kind list, the registries, the macros
                   that make a module, the manifest's types, gen-assemblies, and the
                   arch gates
crates/cap-*       the capabilities: each one registers ops over its library
crates/generator   the generator: writes the manifest, the registry, the schema lock,
                   and the kind table. It names no capability; its binary,
                   gen-manifest, is generated. No module links it.
crates/fixtures    the worked examples' fixtures (D-235). No module links it.
assemblies/        generated and gitignored: the module crates that `just caps` builds,
                   and gen-manifest
server/            the Go and wazero host. It is scaffolding and makes no domain
                   decision.
web/               the Svelte 5 and Vite frontend: one page for each generated UI
tools/             the agent runner, the UI tools (screenshots, deploys), and the docs
                   site
qa/, LRP/          the standing QA tests and the dogfood material
```

**Ownership rules.** Breaking these rules caused real bugs. Keep them.

- **Hosts own I/O, files, and the clock.** They read the clock in the
  agency's time zone (the manifest's `timezone`; `web/src/clock.ts`),
  never the device's. The wasm module owns the domain logic. Dates
  cross every boundary as "YYYY-MM-DD" strings. Times cross as u32
  seconds since service-day midnight. Values past 86400 are valid:
  they are trips after midnight.
- **The base feed is immutable.** Diversions produce patched copies for
  each service date. Patching never changes its input.
- **JS owns the documents** (the plan set, the watchlist) as
  `$state.raw` (`src/plans.svelte.ts`, `src/documents.svelte.ts`). JS
  replaces each document whole and never changes one in place. This
  makes IndexedDB cloning and sync work. wasm holds only materialized
  views. A service change is a fork: the next pick copies the whole
  world it changes. The chain (the pick in force, and the governing
  periods) is always derived, never stored.
- **The sync store's production home is object storage** (S3 and
  compatible stores, Azure Blob, GCS). The engine assumes only
  whole-object writes and a conditional create of an absent key. It
  never assumes an append, a rename, an fsync, or a file lock. There is
  no directory store. The bucket is the agency's record. Every other copy is a working copy
  (`docs/architecture/storage.md`; D-113).
- **Documents are open formats.** A document is plain JSON (gzip
  allowed) or an external open format (GTFS, GTFS-rt, GeoJSON), never
  a private encoding. Each document says its kind and version inside
  it. Each version's schema and worked example are published
  (`docs/FORMATS.md`; D-115).
- **The agency owns the data, and applications are interfaces.** No
  application, including this repo's, is the record for a document. Reads are open to any
  application. Every write goes through a receive path that checks
  shape, invariants, concurrency, and attribution. It refuses only
  what makes the data wrong, and flags a change with consequences for
  a person. Import and export are defined by their formats, never by
  their callers (`docs/architecture/agency-data.md`; D-146, D-152).
- **The history is the log.** Nothing is deleted from the bucket. A log
  keeps every state of every record, and a record leaves the current document only through the revert of
  the entry that created it (D-198). Where a kind is still a document,
  or a merge would bring a dropped record back (the edit document's
  LWW, D-193), "delete" means `status: "retired"`, with the rev
  increased.
- **Schedules change by replacing the artifact, not by changing it.**
  The planner publishes a new zip. The server imports it once
  (cap-network) and publishes the network. Browsers never parse GTFS
  that the server has. A serverless deployment may name the agency's
  GTFS URL, and each page imports it (D-132). The manifest pins
  `network_hash` (sourced from `feed_hash`), `plans_hash`, and the sync
  module's `wasm_hash`. Every client pins and validates the network and
  the picks again. No client reads `wasm_hash`.
- **The sync protocol is LWW for each record id, with tombstones.** It
  is not a CRDT, on purpose: the final authority is the agency's
  scheduling system, through propose, review, and accept.
  `transit_sync::merge` is a property-tested semilattice. `POST /edits`
  is the whole client protocol. The Go host treats documents as opaque
  JSON.

## Conventions

- **Follow the writing rules** in code comments and developer
  documents (`docs/CONVENTIONS.md`, "Writing"). The guide keeps ISO
  24495-1.
- **Clarify, don't assume.** Ask before you build on an answer you
  picked, when a directive is ambiguous, conflicts with the GTFS spec
  or an architecture law, or leaves a design decision open. Record the
  question as an entry (`docs/DECISIONS.md`, "The rules"), and name
  the work it blocks. The rest
  of the work continues while the question is decided.
- **Git is the user's.** Never commit, push, stash, or move refs in any
  other way. Do the work, leave the tree, and report what changed.
  The deny list in `.claude/settings.json` enforces this for Claude
  Code, and also blocks hand edits to the generated TS bindings.
- **The TS bindings are generated and gitignored** (`just types`).
  Never edit or commit them. Rust is the source of truth. The
  manifest's computed examples are the readable wire contract.
- **Parity.** The browser, caps-run, and the Go server run the same ops
  and get the same bytes. `web/` is host glue and render components,
  and never computes answers (`docs/architecture/web.md`, "Parity"; a
  ratchet in `just test` holds it).
- **One op caller.** A host asks through the engine
  (`web/src/engine.ts`) with `ask(capability, op, args)`. It holds
  documents (`Doc`), never modules or hashes. No other code imports the
  bus or calls a module. The modules live in the page's one bus worker
  (`web/src/bus.worker.ts`). A generated UI's host names its page
  (`assemble("<id>")`). Web gates in `just test` enforce this.
- **External services are sources, presentation, or oracles.** A host
  sends an oracle (the geocoder) only the user's input, shows its
  answer as given, and finds it in the manifest's `services`
  (`docs/architecture/web.md`, "External services").
- **Adjacent systems** (CAD/AVL, workforce management, the scheduling
  system) are out of scope. They read documents by hash, and write only
  records through the receive path
  (`docs/architecture/adjacent-systems.md`; D-122).
- **New ops follow the extraction pattern:** the library first, the
  capability wrapper second. A new op is never host code.

## Testing

**Laws first.** Every behavior change starts with the property tests
that define it. Derive them from the user's stated intent and the GTFS
spec, never from the current code. Write them before the
implementation. When the intent leaves a behavior ambiguous, ask the
user. Do not pick an answer and encode it (`docs/TESTING.md`, "Laws
first").

Every change lands with tests at the layer that owns the logic.
`docs/TESTING.md` states the expectations for each layer, and when a
real-data dogfood of a user workflow is due (D-196).

## Deliberate non-goals (do not "fix" these)

- **No CRDT or automerge for sync.** LWW with tombstones is the design.
- **GTFS stays out of transit-network.** The codecs live at the
  boundary, in transit-codec.
- **No wasm-bindgen and no JS glue.** Every module is the raw rustc
  output of an assembly crate, with the same four exports.
- **The geocoder is an oracle, not a document on the bus.** There is no
  gazetteer capability.
- **The Go server stays scaffolding.** Domain logic that the server
  needs becomes a capability module, and the server hosts it.
- **The bus is the architecture.** No capability grows a private
  document format. Shared data crosses capabilities only as kernel
  documents named by content hash. Capabilities never call
  capabilities: linking assembles bytes, not surfaces. The arch gates and assembly independence enforce this.
