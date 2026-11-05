# Linked assembly — one module and one registry per page

Every capability module is its own wasm instance with its own linear
memory, so every module that reads the network parses and holds its
own copy: ~9–12 MB and ~300 ms of registration per copy on C-TRAN
([runtime-cost-gate.md](runtime-cost-gate.md)). The stop display
holds three copies (QAF-93), ride-now four. Every network consumer
also ships the same base — the `Network` decoder, serde_json,
chrono, sha2, flate2 — so a page downloads and compiles it once per
module. cap-calendar, ~200 lines of its own code, is 1.89 MB raw and
553 KB gzipped; each consumer's own code adds 20–400 KB on top.
Measured on `web/public/caps` (2026-11-05, gzip): the stop display
downloads 2.8 MB across five modules, ride-now 4.1 MB across seven,
and service-design 3.3 MB across six. Each Law 3 split of a network
consumer adds another copy and another base.

The goal (D-77): the capabilities a page uses link into ONE module
with ONE kernel-owned registry. A document registers once per page,
and every capability reads the same `Arc`. The shared code ships and
compiles once. Estimated, not measured (linked sizes do not add
exactly): the stop display ~1.1 MB gzipped, ride-now ~1.2 MB,
service-design ~1.3 MB, and every capability together ~1.7 MB.

Capabilities stay separate crates, `Request` enums, manifest
entries, and independently retirable units: linking assembles
bytes, and it merges no op surface. "Capabilities never call
capabilities" still holds. The compiler and the gates below enforce
it, instead of separate memories.

## Scope

- **The kernel owns the registries.** `cap-abi` declares one
  `Registry` per document kind (networks, edits, live documents,
  boundaries, walk tables, streets, watchlists, layers, feeds), and
  the registry constructor is private to `cap-abi`, so a capability
  cannot declare shared state. The ephemeral bounds (D-70) move with
  their kinds.
- **The kernel owns registration.** Each kind's load is a kernel op,
  answered once per instance. The per-capability load ops
  (`load_network`, `load_edits`, `load_live`, `load_walks`,
  `load_boundary`, `load_streets`, `load_watchlist`,
  `load_population_layer`) leave the capabilities' `Request` enums —
  registration is plumbing (Law 3, criterion 7). The version check at
  load ([document-versioning.md](document-versioning.md)) is the
  kernel's. `load_edits` returns per-record validation reports from
  the board, runcut, calendar, and router alike: one answer, four
  times. The reports become one op of the capability that answers
  the edit document's rows (cap-divert's `list` today).
- **A document minted in the assembly registers there.** An op that
  mints a document (cap-rt's `live`, cap-author's `apply`,
  cap-network's `import`) registers it in the kernel registry and
  answers its hash. The page holds a handle, and the document's
  bytes never cross to the page and back (QAF-49: the live document
  is 388 KB per poll, and today it makes that round trip into the
  board and the router).
- **Derived data is a library's; the kernel holds it.** A
  derivation — the day patched with the edits in force for
  (network, edits, date), the router's graph — is defined in its
  library as a pure function and its key, and held in a bounded
  kernel `derived` registry. Any capability that uses the library
  finds the entry or computes it, and the answer is the same either
  way. That takes the per-call day rebuild out of every board, and
  out of `day_edits`' second materialization.
- **`capability!` registers; the assembly exports.** A capability
  crate hands its dispatch to the kernel instead of emitting the
  four `#[no_mangle]` exports; an assembly emits them once. A JSON
  request names its capability (`{"cap": "board", "op":
  "stop_board", …}`); a bulk op carries the capability's index in
  its high bits (the kernel ops, 256 and up, name none). The ABI is
  unchanged: four exports, zero imports.
- **One assembly per generated UI.** `just caps` generates an
  assembly crate for each `web/src/uis/<id>/ui.json`, from the
  capabilities its `ops` name, and builds it beside the
  single-capability modules. The single-capability builds stay:
  caps-run, agents, the registry's blobs, and the Go server
  (Blocked: D-78) run them. Linking a user's own selection at wire
  time is [wire-time-assembly.md](wire-time-assembly.md)'s.
- **The host.** The engine (`web/src/engine.ts`) registers a `Doc`
  once per assembly instance. The identity check then holds between
  instances: a page's assembly against the server's pin. The bus
  worker runs one instance per page. The memory ceiling (D-70) is
  per assembly instance, and a reset drops every capability's state
  together and re-registers each document once. The engine's
  journal key names the assembly's bytes and the capability
  ([host-discipline.md](host-discipline.md)).
- **The manifest.** gen-manifest records each UI's assembly (its
  capabilities, its hash) beside the per-capability entries, and
  the assembly validator proves closure per assembly. The
  generated-UI answer-graph tables (`ui-*.md`) drop the
  per-capability load ops.
- **The rule, enforced.** In one binary, a crate can still name only
  the code of crates it depends on:
  1. A `cargo metadata` gate, deny by default over every crate: a
     `cap-*` crate depends only on `cap-abi` and library crates, and
     only the generated assembly crates depend on capabilities. The
     Law 1a codec gate moves onto the same check (it checks a
     hand-kept crate list today).
  2. The private registry constructor (above): it replaces the ONE
     REGISTRY grep.
  3. `#![forbid(unsafe_code)]` in every capability crate (the
     unsafe exports live in the kernel).
  4. Assembly independence: every manifest worked example answers
     identically from the capability built alone and from inside
     each UI's assembly. This proves a shared derived entry is only a
     cache, never a dependency.

## Order

1. Kernel registries and kernel loads, with one module per
   capability still (every test and the runner stay green).
2. The derived-entry API; the day moves onto it.
3. `capability!` as registration, the assembly generator, and the
   four gates.
4. The engine and the worker on one instance per page: the stop
   display first (QAF-93), then ride-now and service-design.
5. Measure on C-TRAN (`just perf`): wasm memory per page,
   registration time, and the bytes a cold boot transfers.

The Law 3 re-cuts of cap-analysis, cap-directory, and cap-streets
follow this unit (`docs/units/README.md`, queued).

## Done when

- Each generated UI loads one assembly, and its session holds each
  network version once (an e2e reads `window.__bus.held`).
- The four gates run in `just test`, each failing on a deliberately
  broken fixture in its own test.
- The assembly-independence law runs in `just test` over every
  worked example.
- No capability's `Request` carries a load op, and the edit reports
  are one capability's op.
- The live document reaches the board and the router without
  crossing to the page and back; QAF-49 is deleted.
- A C-TRAN `just perf` record lands in the same change: the stop
  display within D-68's 40 MB, and QAF-93 deleted (or reworded with
  the new numbers).
