# AGENTS.md

wasm-transit is local-first transit software. It reads GTFS static and
GTFS-rt with Rust, and ships as **capability modules**: WebAssembly
modules with the same four exports and no imports. The browser, Node,
and Go (wazero) run the same documents under the same contracts. Each
generated UI links the capabilities that it needs into one module at
build time.

Three rules shape every change (`docs/architecture/README.md`):

- **Two deployment shapes, both complete:** with the sync server
  (`just serve`), or as static files (`just export <ui>`). A change to
  one shape takes nothing from the other.
- **Built for the outage.** For each failure, the design states what
  still works, what each person does instead, and how it recovers. A
  page never shows an old answer as current.
- **Each generated UI is its own product.** It is deployed, updated,
  and rolled back on its own.

## Keeping the documents correct

This file holds rules and pointers. A list that grows with the code
lives in its home, and is generated where it can be.

- **Docs describe the repo as it is now.** Git holds the history.
- **One home for each topic** (the table below). Open work and its
  status live only in `docs/units/`. The guide (`docs/guide/`)
  explains the topics to people and links to each home. Where they
  disagree, the home is right.
- **Update the docs in the same change**, with every pointer to
  content that moved.
- **Deleting a unit file claims that the unit is done**
  (`docs/units/README.md`). Delete a fixed QA finding the same way.
- **The code wins.** If a doc contradicts the code, fix the doc.

**Finishing work.** Work is not done until its documentation is done.
Before you report, run the `doc-sweep` skill
(`.claude/skills/doc-sweep/SKILL.md`), and say what you updated.

## Where the detail lives

| Topic | Home |
|---|---|
| The three architecture rules | `crates/AGENTS.md` (arch gates in `just test` enforce them) |
| Architecture, one file for each topic | `docs/architecture/` (its README indexes them) |
| A library | its `//!` crate comment |
| A capability | `crates/cap-<id>/OPS.md` (`node tools/caps-run.mjs tools <id>` lists its ops, and `tools <id> <op>` prints one), and its `//!` crate comment |
| cap-abi, the modules, the assemblies | `docs/architecture/bus.md`, `docs/architecture/registry.md` |
| The document kinds and formats at rest | `crates/cap-abi/src/kinds.rs`, `docs/FORMATS.md` |
| The server and the capabilities it hosts | `docs/architecture/server.md` (`node tools/caps-run.mjs kind <tag>` prints a kind's path through it) |
| The HTTP contract | `server/openapi.json`, held to the server by Go tests |
| The web and the generated UIs | `docs/architecture/web.md`, `docs/architecture/generated-uis.md`, and one file for each UI in `generated-uis/` |
| Writing, traceability, gotchas | `docs/CONVENTIONS.md` |
| The documentation sweep | `.claude/skills/doc-sweep/SKILL.md` |
| Testing, dogfood QA, UI screenshots | `docs/TESTING.md` |
| Each recipe | the comment above it in the justfile |
| A gate | its script, named in `_host-tests` (`tools/gates/`, `web/gates/`, or beside the code it guards) |
| The agent runner | the header of `tools/caps-run.mjs`, and `docs/architecture/registry.md` |
| Open work | `docs/units/` (read its README before you touch anything) |
| Open questions for the user, and answers not built yet | `docs/DECISIONS.md` (the rules and the summary), one file for each entry in `docs/decisions/` |
| Open real-data findings | `docs/QA-FINDINGS.md` |
| The sequenced feature plan | `docs/ROADMAP.md` |
| The standing end-to-end QA tests | `qa/`, one folder each, with a README |
| Long range plan dogfood material | `LRP/` (`LRP/SESSIONS.md`) |
| Operator guides | `DEPLOYMENTS.md` (static), `DEPLOYMENTS-SERVER.md` (server) |
| The guide for people | `docs/guide/` |

`just docs` draws the repo map, the capabilities and the documents they
read, and what each UI links, from the code.

## Commands

`just` runs every task. `just --list` names the recipes, and the
comment above each recipe says what it does. Read it before you run a
recipe for the first time. Build and test through the recipes.

- `just test` is the host-side gate. Run it before you say any change
  is done. `just e2e` adds the browser gate, for a change that the UI
  shows. `just check` runs both.
- The gates build the wasm-fast modules, which behave as the release
  modules do. The `-release` recipes take about an hour. Run them when
  a unit is done, not for each change.
- `node tools/caps-run.mjs` is the agent runner. `node tools/ui-shot.mjs
  <ui>` takes a screenshot of a UI with its status and page errors.

just, cargo, go, node, and pmtiles are on the PATH, and
`rust-toolchain.toml` pins Rust. Installs are the user's job. cargo,
npm, and go module fetches are fine.

## Architecture

Three rules govern every crate (`crates/AGENTS.md`):

1. The network is the core data model. GTFS (and TODS and TIDES) is a
   serialization for sharing data with other systems.
2. No module rebuilds the monolith inside it.
3. Each module answers one need. Personas assemble in a UI, never in a
   module.

```
crates/transit-*   the domain libraries. transit-network is the core model;
                   transit-codec reads and writes GTFS at the boundary.
crates/cap-abi     the kernel: the kind list, the registries, the module macros,
                   the manifest's types, gen-assemblies, and the arch gates
crates/cap-*       the capabilities: each one registers ops over its library
crates/generator   writes the manifest, the registry, the schema locks, and the
                   kind table. No module links it.
crates/fixtures    the worked examples' fixtures. No module links it.
assemblies/        generated: the module crates that `just caps` builds
server/            the Go and wazero host
web/               the Svelte 5 and Vite frontend: one page for each generated UI
tools/             the agent runner, the UI tools, and the docs site
qa/, LRP/          the standing QA tests and the dogfood material
```

**Ownership rules.** Breaking these caused bugs.

- **Hosts own I/O, files, and the clock**, and read the clock in the
  agency's time zone (`web/src/clock.ts`). Modules own the domain
  logic. Dates cross every boundary as "YYYY-MM-DD". Times cross as u32
  seconds after service-day midnight, and a value past 86400 is a trip
  after midnight.
- **The base feed is immutable.** A diversion makes a patched copy for
  each service date.
- **JS owns the documents** as `$state.raw`, and replaces each one
  whole (`docs/CONVENTIONS.md`). wasm holds only materialized views. A
  service change is a fork. The chain
  of picks is derived, never stored.
- **The sync store is object storage.** The engine assumes only
  whole-object writes and a conditional create
  (`docs/architecture/storage.md`).
- **Documents are open formats** that name their kind and version
  (`docs/FORMATS.md`).
- **The agency owns the data.** Every write goes through a write path
  that checks shape, invariants, concurrency, and attribution
  (`docs/architecture/agency-data.md`).
- **Nothing is deleted from the bucket.** A log keeps every state of
  every record. Where a kind is still a document, "delete"
  means `status: "retired"`.
- **Schedules change by replacing the artifact.** The server imports a
  published zip once and publishes the network. A browser never parses
  GTFS that the server has (`docs/architecture/server.md`).
- **Sync is LWW for each record id, with tombstones.** `POST /edits` is
  the whole client protocol (`docs/architecture/storage.md`).

## Conventions

- **Follow the writing rules** in code comments and developer
  documents (`docs/CONVENTIONS.md`, "Writing"). The guide keeps ISO
  24495-1.
- **Write no comment by default.** Add one only for a reason, a unit,
  an invariant, an outside constraint, or a surprise that the code
  cannot show. Never restate the code or a fact that has a home: an
  op's contract is in its `OPS.md`, and a route's is in
  `server/openapi.json` (`docs/CONVENTIONS.md`, "What to write").
- **Clarify, don't assume.** Ask before you build on an answer that
  you picked, or when a directive is ambiguous or conflicts with the
  GTFS spec or an architecture rule. Record the question
  (`docs/DECISIONS.md`, "The rules") and name the work it blocks.
- **Git is the user's.** Never commit, push, stash, or move refs.
- **The TS bindings are generated** (`just types`). Never edit them.
- **Parity.** The browser, caps-run, and the Go server run the same ops
  and get the same bytes. `web/` never computes answers
  (`docs/architecture/web.md`, "Parity").
- **One op caller.** A host asks through `ask(capability, op, args)` in
  `web/src/engine.ts`. No other code calls a module
  (`docs/architecture/web.md`).
- **External services are sources, presentation, or oracles**
  (`docs/architecture/web.md`, "External services").
- **Adjacent systems are out of scope**
  (`docs/architecture/adjacent-systems.md`).
- **A new op starts in the library**, and the capability wraps it. A
  new op is never host code.

## Testing

Every behavior change starts with the property tests that define it.
Derive them from the user's intent and the GTFS spec, never from the
current code. When the intent is ambiguous, ask
(`docs/TESTING.md`, "Laws first"). Every change lands with tests at
the layer that owns the logic.

## Deliberate non-goals (do not "fix" these)

- No CRDT or automerge for sync. LWW with tombstones is the design,
  because the agency's scheduling system is the final authority, through
  propose, review, and accept.
- GTFS stays out of transit-network. The codecs live in transit-codec.
- No wasm-bindgen and no JS glue.
- The geocoder is an oracle, not a document on the bus.
- The Go server stays scaffolding. Domain logic that it needs becomes a
  capability.
- Capabilities never call capabilities. Shared data crosses only as
  kernel documents named by hash.
