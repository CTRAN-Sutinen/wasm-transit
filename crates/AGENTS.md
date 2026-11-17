# AGENTS.md: the crates

Three laws govern everything in this directory. If a design breaks one
of them, the design is wrong. Do not change the law to fit the design.

## Law 1: the network is the core data model

GTFS is a serialization for sharing data with other systems. Nothing
new goes into GTFS. Code on the model side does not read a
serialization.

- GTFS lives only at the boundary. Its homes are:
  - transit-codec: the whole-spec feed layer, and import and export;
  - cap-gtfs and cap-network: the boundary capabilities;
  - cap-rt: rt trip ids resolve against the network here, with the
    import's stop numbering.
- The charter of cap-gtfs is closed. It parses, exports, and answers
  primitive boundary queries. Add nothing else.
- **"Network or feed" is never a design choice.** Capabilities on the
  model side compose over the network document and its day views. Do
  not present composition over the feed as an option, a trade-off, or
  an open question. That breaks this law. Write no new code that
  composes over the feed. No plan may open the question again.
- Capabilities compose over the network document, named by its content
  hash. The module is the hash authority.
- Operator edits, rt joins, analysis, and dispatch views all work in
  the model's vocabulary: network, service type, pattern trips, walking
  group, and materialized trip. Route ids and stop ids cross the
  boundary. GTFS types and row shapes do not.
- Streets follow the same rule. A street layer (an agency GIS export,
  TIGER, or OSM) is a serialization. Only `transit_streets::geojson`
  reads it, into the streets document. The graph and every op read the
  document. A network never refers to a streets document or a graph
  position. A trace commits geometry.
- The formats beside GTFS follow the same rule.
  - TODS serializes the operations plan: the runs layer, deadheads, and
    internal locations. It lives in transit-codec, beside GTFS (D-126).
  - TIDES is the history of operations. It is read as a source into a
    document (D-127) and is never written (D-128).
  - GTFS-rt is read for the live picture. It is also the serialization
    of the authored cancellations and detours, which the edit document
    models (D-124).

  Their row shapes stay at the boundary.

## Law 2: do not rebuild the monolith inside a module

The monolith was one artifact that answered every op. It is gone. A
capability that builds it again with hash-keyed registries has the
same defect.

Reject these on sight:

- A whole GTFS feed written where a day is enough. For example,
  `transit_codec::export` inside a capability. That function is the
  codec's publish path and has no other use.
- Caches of a derived or effective feed, keyed by date. This was the
  monolith's `FeedState` and `with_effective` shape. The derived thing
  is the model's day: a library's pure function, held by the kernel.
- GTFS-shaped registries, or GTFS row types in responses, such as
  `transit_codec::feed::TripRow`. Responses use the model's own types.

The day view is `transit_network::materialize(net, date)`: the trips of
the day. Patch the day, never the calendar. Operator edits patch the
materialized stop times. `transit_dispatch::day` is the effective day
and what the edits took away, in one materialization. The kernel holds
it for each (network, edits, date) as a derived entry
(`cap_abi::kernel::day`). The entry is bounded, and it goes when its
documents go. So every capability that reads the day finds the same
day.

## Law 3: one need for each module

A capability module is one need, as one unit. Examples: "see the
departures at a stop", "watch one vehicle's connections", and "plan a
trip".

The persona or bundle comes together in a generated UI: the ops its
`ui.json` lists. It never comes together in a module's design. Do not
design a module around who will use it. Do not merge ops because the
same users wire them together.

Linking assembles the bytes of the capabilities that a UI lists into
one binary, at build time (D-77, D-176; `docs/architecture/bus.md`,
"Bus rules"). Linking merges no surface.
Each capability keeps its crate, its `Request` enum, and its manifest
entry, and can still be retired alone.

Reject these on sight:

- Ops that serve different needs in one module, because one persona
  downloads them together. An example is a rider's board behind an
  operator's contract.
- An op duplicated across modules. Two answers to one need on the bus
  are the monolith built again in a different place.
- Shared day machinery as a reason to merge module surfaces. Shared
  day machinery is a library's job (transit-dispatch).

Registration belongs to the kernel. Loads such as `load_network`,
`load_edits`, and `load_feed` connect documents to capabilities. They
are not authoring. Each document kind's load is a kernel op
(`cap_abi::kernel`) and is never in a capability's `Request`. The host
owns every document and replaces it whole. Authoring ops mint
documents, and register them where they mint them. A reader has no
authoring op, by construction.

### The cut: the criteria every capability must pass

A capability is a named question over documents named by hash. A cut
that fails one criterion is a bundle or a duplicate.

1. **One question.** State the purpose as one question. If it needs
   "and", cut at the "and".
2. **Persona-blind.** The purpose has no role words: dispatcher, rider,
   planner, or operator. Two personas that use separate subsets of one
   module's ops, for different ends, make a bundle.
3. **One subject.** Every answer describes one subject, such as a
   street graph, a vehicle's chain, or a service calendar. Documents
   are inputs, not subjects.
4. **Facets, not surface.** Each op is the one question at another
   granularity or with other parameters: a list then a detail, or a
   stop then a station then a saved set. An op that is not a facet does
   not belong, however related it is.
5. **Valuable alone.** The capability is worth assembling by itself.
   If it only feeds another capability, it is an op of that capability.
   Capabilities never call capabilities. Composition is through
   documents only.
6. **No two answers to one question** in the whole manifest. Delete a
   duplicate answer in the change that makes it obsolete. Never keep it
   for compatibility.
7. **Registration is plumbing.** The kernel's loads never justify a
   capability, and they are not among its ops.
8. **Retirable alone.** The module can be removed alone.

### The split: when a module stops growing

Judge the cut again at every change to a module, not only when it is
made. A module that conforms when it is made can become a bundle one op
at a time. Any one of these triggers makes a split the answer to "where
does the next op go". Split in the change that trips the trigger.

1. **The second cluster.** The module's ops fall into groups that share
   neither documents nor response vocabulary. One cluster is a family
   of facets. Two clusters are one module that holds two questions. The
   next op added to either cluster is the first op of the new module.
2. **The second document.** An op takes a document that none of the
   module's other ops take. One such op is a facet. The second one
   composes a question this module does not otherwise answer. Those
   ops go to the module of the need that composes them.
3. **The near neighbor.** An op answers inside another module's
   question. Criterion 6 applies to answers that move toward each
   other, not only to duplicates that start that way. When the next op
   moves either module toward the other's answer, delete one of the
   pair in that change.

## Where new things go

Always use the extraction pattern, in this order:

1. **Library first.** Put the domain logic in a shared library crate,
   with its law tests. Examples: transit-analysis, transit-rt,
   transit-dispatch, transit-runs, and transit-plans.
2. **Capability second.** Write a thin module over the library. Put its
   purpose and each op's description in the crate's `OPS.md`. The file
   has a `#` heading that names the capability, then one `##` section
   for each op, in the order of its `ops`. gen-manifest refuses any
   other shape. Compute the op's worked example in gen-manifest's
   `example_for`.

A new domain op is a library and a capability, never host code.

A capability declares no state.

- The documents it reads are the kernel's kinds, named in its
  `capability! { documents: [...] }`. A new kind goes into
  `cap_abi::kernel` with its bound and its load.
- What it derives from documents is a library's pure function and its
  key. The kernel holds it through `cap_abi::kernel::derive`, under a
  `Derivation`. A derived entry is a cache, never a dependency.

Capabilities never call capabilities. A capability crate depends only
on cap-abi and libraries. Only a generated assembly (`assemblies/`)
links capabilities.

## Enforcement

The `test` recipe in the justfile checks laws 1 and 2 and the linking
rule:

- `arch-gates` reads the dependency graph (`cargo metadata`) and denies
  by default:
  - a capability depends only on cap-abi and libraries;
  - only assemblies depend on capabilities;
  - only the codec's homes read transit-codec;
  - the kernel requires only the libraries its own code names. Every
    module carries the kernel's closure, so a generator's extra library
    is an optional dependency behind its feature
    (`docs/architecture/release-artifacts.md`).
- `arch-gates` also reads the source of every capability crate. It
  requires `#![forbid(unsafe_code)]`, and no static or `thread_local`
  of the crate's own.
- The registry constructor is private to the kernel. Its
  `compile_fail` doctests check this.
- `transit_codec::export` may not appear outside the codec's homes.
- No library code reads a clock, randomness, or the environment. This
  is the determinism gate.
- Every worked example gives the same answer alone and inside each UI's
  assembly (`crates/cap-abi/independence.mjs`).

Each gate fails on a broken fixture in its own test:
`crates/cap-abi/tests/arch_gates_test.rs` and
`independence.test.mjs`.

Law 3 is a review gate. The criteria above are the checklist: each op
of every new module must name one need.

If a gate fails your change, read this file again. Do not widen the
gate.
