# AGENTS.md — the crates

Two laws govern everything under this directory. If a design
violates either one, the design is wrong — not the law.

## Law 1: the NETWORK is the core data model

GTFS is a serialization spec for external data sharing. Nothing
new goes "in GTFS," and model-side code does not read
serialization.

- GTFS lives only at the boundary: transit-codec (the whole-spec
  feed layer and import/export), cap-gtfs and cap-network (the
  boundary capabilities), and cap-rt (rt trip ids resolve against
  the network there, with the import's stop numbering). cap-gtfs's charter is
  CLOSED: parse, export, primitive boundary queries — no more.
- **"Network vs feed" is never a decision.** Model-side
  capabilities compose over the NETWORK document (+ its day
  views) — presenting composition over the feed as an option,
  a trade-off, or an open question is the law broken in prose.
  No new feed-composing code, and no plan may reopen the
  question.
- Capabilities compose over the network document, addressed by
  content hash. The module is the hash authority.
- Operator edits, rt joins, analysis, dispatch views: all work in
  MODEL vocabulary (network, service type, pattern trips,
  walking group, materialized trip). Route/stop ids cross over;
  GTFS types and row shapes do not.
- Streets are the same kind of thing: a street layer (an agency GIS
  export, TIGER, OSM) is a serialization read into the streets
  document by `transit_streets::geojson` and nowhere else; the graph
  and every op read the document. A network never references a
  streets document or a graph position — a trace commits geometry.
- GTFS's siblings are the same kind of thing. TODS serializes the
  operations plan (the runs layer, deadheads, internal locations)
  beside GTFS, and lives in transit-codec with it (D-126); TIDES is
  operations history, read as a source into a document (D-127) and
  never written (D-128); GTFS-rt is read for the live picture and
  is the serialization of the authored cancels and detours, which the
  edit document models (D-124). Their row shapes stay at the
  boundary.

## Law 2: do not reinvent the monolith inside a module

The monolith (one artifact answering every op) is gone. A
capability that re-erects it with hash-keyed registries is the
same bug with new clothes.
Rejected on sight:

- a whole GTFS feed written where a day would do
  (`transit_codec::export` inside a capability — it is the codec's
  publish path, nothing else);
- derived/effective FEED caches keyed by date (the monolith's
  `FeedState`/`with_effective` shape) — what is derived is the
  model's day, a library's pure function held by the kernel;
- GTFS-shaped registries or GTFS row types in responses
  (`transit_codec::feed::TripRow`, ...) — responses are
  model-native types.

The day view is `transit_network::materialize(net, date)` — the
day's trips. Patch the DAY (operator edits over materialized
stop_times), never the calendar: `transit_dispatch::day` is the
effective day and what the edits took away, one materialization,
held by the kernel per (network, edits, date) (`cap_abi::kernel::day`
— a derived entry, bounded, dropped with its documents) so every
capability that reads it finds the same day.

## Law 3: one NEED per module — personas assemble at wire time

A capability module is ONE need, atomically ("see the departures
at a stop", "watch one vehicle's connections", "plan a trip").
The persona/bundle comes together at WIRE time — the user's
selection over the manifest — never in a module's design. Do not
design a module around who will use it, and do not merge ops
because the same users end up wiring them together. Linking the
capabilities a selection assembles into one binary (a generated
UI's at build time, a user's at wire time — D-77,
`docs/ARCHITECTURE.md`, "Bus rules") assembles BYTES and merges no
surface: each capability keeps its crate, its Request enum, and its
manifest entry, and stays independently retirable.

Rejected on sight:

- ops serving different needs in one module because one persona
  downloads them together (a rider's board behind an operator's
  contract);
- DUPLICATING an op across modules (two answers to one need on
  the bus — the monolith rebuilt sideways);
- shared DAY MACHINERY is a LIBRARY's job (transit-dispatch), not
  a reason to merge module surfaces.

Registration (`load_network`, `load_edits`, `load_feed`, …) is
composition fabric, not authoring, and it is the KERNEL's: each
document kind's load is a kernel op (`cap_abi::kernel`), never in a
capability's Request. The host owns every document,
wholesale-replaced. Authoring ops MINT documents (and register them
where they are minted); readers have none by construction.

### The cut — criteria every capability must pass

A capability is a named question over hash-addressed documents.
A cut that fails one criterion is a bundle or a duplicate:

1. **One question.** The purpose states as a single question. If
   it needs "and", cut at the "and".
2. **Persona-blind.** No role words in the purpose (dispatcher,
   rider, planner, operator). Two personas using disjoint op
   subsets of one module for different ends is a bundle.
3. **One subject.** Every answer describes one subject (a street
   graph, a vehicle's chain, a service calendar). Documents are
   inputs, not subjects.
4. **Facets, not surface.** Ops are the one question at other
   granularities or parameterizations (list → detail; stop →
   station → saved set). An op that is not a facet does not
   belong, however related.
5. **Alone-valuable.** Worth assembling by itself; if it only
   feeds another capability, it is that capability's op.
   Capabilities never call capabilities — composition is
   documents only.
6. **No two answers to one question** across the whole manifest.
   A duplicate answer is deleted with the change that obsoletes
   it, never kept "for compatibility".
7. **Registration is plumbing.** The kernel's loads never justify
   a capability and are not among its ops.
8. **Independently retirable.** The module can die alone.

### The split — triggers that end a module's growth

The cut is re-judged at every change to a module, not only at
its birth: a module conforming at birth drifts into a bundle one
op at a time. Any one trigger makes SPLIT the answer to "where
does the next op go", in the change that trips it:

1. **The second cluster.** The module's ops partition into
   groups that share neither documents nor response vocabulary.
   One cluster is a facet family; two is one module holding two
   questions. The next op added to either cluster is the new
   module's first op.
2. **The second document.** An op takes a document none of the
   module's other ops take. One such op is a facet; the second
   composes a question this module does not otherwise answer —
   those ops go where the need that composes it lives.
3. **The near neighbor.** An op answers inside another module's
   question. Criterion 6 applies to converging answers, not only
   born duplicates: the next op that moves either module toward
   the other's answer deletes one of the pair in that change.

## Where new things go

The extraction pattern, always in this order:

1. crate boundary first — the domain logic lands in a shared
   library crate (transit-analysis, transit-rt, transit-dispatch,
   transit-runs,
   transit-plans) with the law tests;
2. capability wrapper second — a thin module over the library.

A new domain op is a library and a capability, never a host. A
capability declares no state: the documents it reads are the
kernel's kinds, named in its `capability! { documents: [...] }` (a
new kind lands in `cap_abi::kernel` with its bound and its load);
what it derives from documents is a library's pure function and its
key, held through `cap_abi::kernel::derive` under a `Derivation` —
a cache, never a dependency. Capabilities never call capabilities:
a capability crate depends on cap-abi and libraries only; only a
generated assembly (`assemblies/`) links capabilities.

## Enforcement

The justfile `test` recipe asserts laws 1 and 2 and the linking
rule: `arch-gates` reads the dependency graph (`cargo metadata`,
deny by default — a capability depends only on cap-abi and
libraries, only assemblies depend on capabilities, only the codec's
homes read transit-codec, and the kernel requires only the libraries
its own code names — every module carries the kernel's closure, so a
generator's extra library is an optional dependency behind its
feature; docs/ARCHITECTURE.md, "Release artifacts"), and every
capability crate's source
(`#![forbid(unsafe_code)]`, no static or thread_local of its own);
the registry constructor is private to the kernel (its
`compile_fail` doctests); `transit_codec::export` may not appear
outside the codec's homes; no library code reads a clock,
randomness, or the environment (DETERMINISM); and every worked
example answers identically alone and inside each UI's assembly
(`crates/cap-abi/independence.mjs`). Each gate fails on a broken
fixture in its own test (`crates/cap-abi/tests/arch_gates_test.rs`,
`independence.test.mjs`). Law 3
is a review gate (the criteria above are the checklist — every
new module's ops must each name one need). If a gate fails your
change, reread this file — do not widen the gate.
