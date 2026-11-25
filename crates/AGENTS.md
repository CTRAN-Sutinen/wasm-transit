# AGENTS.md: the crates

Three rules govern everything in this directory. If a design breaks one
of them, the design is wrong. Do not change the rule to fit the design.
The mechanics behind the rules (the kernel, the loads, the day entry,
the declarations) are in `docs/architecture/bus.md`.

## Rule 1: the network is the core data model

GTFS is a serialization for sharing data with other systems. Nothing
new goes into GTFS. Code on the model side does not read a
serialization.

- GTFS lives only at the boundary: transit-codec (the whole-spec feed
  layer, import, and export), cap-gtfs and cap-network (the boundary
  capabilities), and cap-rt (rt trip ids resolve against the network
  there, with the import's stop numbering).
- cap-gtfs parses, exports, and answers primitive boundary queries.
  Add nothing else to it.
- Capabilities on the model side compose over the network document,
  named by its content hash, and its day views, never over the feed.
  This is settled. Do not offer the feed as an option.
- Operator edits, rt joins, analysis, and dispatch views work in the
  model's vocabulary: network, service type, pattern trips, walking
  group, and materialized trip. Route ids and stop ids cross the
  boundary. GTFS types and row shapes do not.
- A street layer (an agency GIS export, TIGER, or OSM) is a
  serialization too. Only `transit_streets::geojson` reads it, into the
  streets document. A network never refers to a streets document or a
  graph position.
- TODS (the operations plan), TIDES (the history of operations, read
  and never written), and GTFS-rt (the live picture, and the
  serialization of the edit document and the service alerts document)
  follow the same rule. Their row shapes stay at the boundary (D-124,
  D-126, D-127, D-128, D-206).

## Rule 2: do not rebuild the monolith inside a module

A monolith is one artifact that answers every op. A capability that
builds one with hash-keyed registries has the same defect. Reject
these:

- A whole GTFS feed written where a day is enough. For example,
  `transit_codec::export` inside a capability. That function is the
  codec's publish path and has no other use.
- Caches of a derived or effective feed, keyed by date. The derived
  thing is the model's day: a library's pure function, held by the
  kernel.
- GTFS-shaped registries, or GTFS row types in responses, such as
  `transit_codec::feed::TripRow`. Responses use the model's own types.

The day view is `transit_network::materialize(net, date)`. Patch the
day, never the calendar. The kernel holds the effective day once for
each (network, edits, date), so every capability that reads the day
finds the same day (`docs/architecture/bus.md`).

## Rule 3: one need for each module

A capability module is one need, as one unit. Examples: "see the
departures at a stop", "watch one vehicle's connections", and "plan a
trip".

A persona's set of ops comes together in a generated UI (the ops its
`ui.json` lists), never in a module's design. Do not design a module
around who will use it. Do not merge ops because the same users wire
them together.

Linking assembles the bytes of the capabilities that a UI lists into
one binary, at build time. Linking merges no surface. Each capability
keeps its crate, its `Request` enum, and its manifest entry, and can
still be retired alone (`docs/architecture/bus.md`, "Bus rules"; D-77,
D-176).

Reject these:

- Ops that serve different needs in one module, because one persona
  downloads them together. An example is a rider's board behind an
  operator's contract.
- An op duplicated across modules. Two answers to one need on the bus
  are the monolith built again in a different place.
- Shared day machinery as a reason to merge module surfaces. Shared
  day machinery is a library's job (transit-dispatch).

Registration belongs to the kernel. Each document kind's load is a
kernel op, never in a capability's `Request`. An authoring op creates
a document and registers it. A reader has no authoring op.

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
6. **No two answers to one question** in the whole manifest. Delete a
   duplicate answer in the change that makes it obsolete. Never keep it
   for compatibility.
7. **Registration is plumbing.** The kernel's loads never justify a
   capability, and they are not among its ops.
8. **Retirable alone.** The module can be removed alone.

### The split: when a module stops growing

Judge the cut again at every change to a module. A module that
conforms when it is made can become a bundle one op at a time. Any one
of these triggers makes a split the answer to "where does the next op
go". Split in the change that trips the trigger.

1. **The second cluster.** The module's ops fall into groups that share
   neither documents nor response vocabulary. The next op added to
   either group is the first op of the new module.
2. **The second document.** An op takes a document that none of the
   module's other ops take. One such op is a facet. The second one
   composes a question this module does not otherwise answer. Those
   ops go to the module of the need that composes them.
3. **The near neighbor.** An op answers inside another module's
   question. When the next op moves either module toward the other's
   answer, delete one of the pair in that change (criterion 6).

## Where new things go

1. **Library first.** Put the domain logic in a library crate, with its
   property tests.
2. **Capability second.** Write a thin module over the library: its
   `OPS.md` (the purpose, and one `##` section for each op), its
   `capability!` declaration, and a worked example for each op in
   `src/examples.rs`. `docs/architecture/bus.md` gives the exact shape,
   and the generator refuses any other. `docs/guide/first-change.md`
   walks through one.

A new domain op is never host code.

A capability is a documented namespace for one need, not a wall in the
domain. Its crate may link any library. A question that spans kinds is
one op of the capability whose need it answers, and that op links each
kind's library. A host never composes it from two answers, and applies
no domain rule (`docs/architecture/hosts.md`; D-226, D-228).

A capability declares no state. The documents it reads are the
kernel's kinds. A new kind is a line in the kind list
(`cap_abi::kinds`) and a `transit_version::Load` in its library, and
the kernel does not change (D-239). What a capability derives from
documents is a library's pure function, held by the kernel as a cache,
never a dependency.

Capabilities never call capabilities, and compose only through
documents. A capability crate depends only on cap-abi and libraries.
Only a generated assembly (`assemblies/`) links capabilities.

## Enforcement

`just test` checks rules 1 and 2 and the linking rule, with the arch
gates, the `compile_fail` doctests on the registry, and the assembly
independence check (`docs/architecture/bus.md`, "Bus rules"). Rule 3
is a review gate, and the criteria above are its checklist.

If a gate fails your change, read this file again. Do not widen the
gate.
