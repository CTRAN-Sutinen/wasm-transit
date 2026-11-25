# The capability bus

The bus is the architecture. Each capability answers one need, and no
capability belongs to one user. Each generated UI links the
capabilities it needs. So the capability bytes differ for each UI, and
the documents and the contracts are the same everywhere.

A **module** is an assembly over the kernel (`cap-abi`). There are two
kinds:

- `module-<cap>` holds one capability alone. caps-run, agents, the
  registry's blobs, and the Go server run this kind.
- `module-ui-<id>` links the capabilities of one generated UI into one
  module (D-77). That UI's page loads it. A document registers once
  for each page, and the shared code ships once.

Linking joins bytes and merges no interface. Each capability keeps its
crate, its `Request` enum, and its manifest entry. Linking happens at
build time only. A user does not choose capabilities (D-176).

The assembly crates are in `assemblies/`. `gen-assemblies` generates
them from the capability crates and from the `ops` list in each
`web/src/uis/<id>/ui.json`. `just caps` writes them, and `just test`
checks that they are current. gen-assemblies also writes
`assemblies/gen-manifest/`, the generator's binary ("gen-manifest",
below). It is no module.

## The kernel: cap-abi

`cap-abi` holds the kernel and the interface between a module and its
host.

**The module interface.** The `assembly!` macro writes the four
exports that every module has, once in each assembly crate. A
capability crate registers itself with `capability!` and forbids
`unsafe`.

**The declaration.** `capability!` is a capability's whole
declaration: its id, its two entry points (`transit_invoke` for JSON,
`transit_invoke_bytes` for bytes), the kinds it reads (`documents`, by
type), its `Request` enum, and the kinds it consumes, speaks, and
migrates. The macro reads `OPS.md` from the crate's root. It writes
`CAPABILITY` (what an assembly links) and `declaration()` (what the
generator reads). No module calls `declaration()`, so no module
carries the docs' text. The ops are the `Request` enum's tags. Serde's
refusal of an unknown tag names every tag, so the list cannot differ
from what the dispatch decodes.

**The manifest contract and the validator.** `cap-abi` defines the
capability manifest contract: what a capability speaks, migrates, and
consumes, and its agent context. It also holds the assembly validator,
which proves an assembly when it is composed: closure, version
coherence, and that a migration path can be reached. The validator is
total and has property tests. gen-manifest validates its own output
before it writes it.

**gen-manifest.** The generator (`crates/generator`) is a library that
takes the capabilities as data (`Linked`: each one's declaration and
worked examples) and names none. Its binary, gen-manifest, is
generated: gen-assemblies writes `assemblies/gen-manifest/`, which
links every capability with its `examples` feature, in id order, and
calls the library. A new capability is a new crate directory, and
nothing hand-written names it. A fresh clone builds, because nothing
hand-written depends on the generated crate.

- The generator reads each capability's agent context from its
  `OPS.md`. It refuses a file whose `##` sections are not the
  `Request` enum's ops, and the manifest lists the ops in the file's
  order. A reviewer reads the manifest's diff to see a change to an
  assembly.
- Each capability gives its worked examples in its own crate
  (`src/examples.rs`, behind the `examples` feature): the documents to
  load and the request. gen-manifest runs each through the
  capability's own dispatch and records the answer, so an example
  cannot differ from what the module answers.
- gen-manifest's tests run two checks over every capability and the
  locked schemas (`generator::checks`): the real-output gate and the
  real-data soak.
- cap-author's example of `apply` also gives one value of each edit
  kind (`kinds`), applied to the fixture in order. The build fails if
  an edit kind has no example, or if an example stops applying.

**The kind list.** `kinds.rs` lists every kind of document on one
line: its name and tag, its library type and the crate that keeps its
lock, its kernel load, and how the store keeps it. A kind exists only
if it has a line. No module reads the list, so a kind costs a module
nothing until a capability names it. gen-manifest generates the kind
table from it (`docs/FORMATS.md`, "The kind table"), and gives each
load in the registry's kernel ops its kind. caps-run's `--load
KIND:FILE` and the pages' engine read that kind. The generator holds
each load to its line: the line names the library type, and the type's
id, op, and code are the line's.

**Each library loads its own kind (D-239).** A kind's library
implements `transit_version::Load` for its type, in the library's
`src/load.rs`. So cap-abi depends on no domain library, and an arch
gate holds it to that.

A document's identity and its versioned read are in transit-version
(`transit_version::document`, and `jcs` for RFC 8785). They give the
canonical hash, and the read of JSON or gzipped JSON in this build's
version, or one version ahead. The kernel re-exports them
(`cap_abi::canonical_hash` and the others).

The read is in the library for this reason. A reader and a canonical
writer are generic code, compiled where they are called. If the kernel
called them for each kind, every capability crate that names a kind
would compile its own copy of that kind's reader. A UI assembly that
links seven capabilities over the network would carry seven. A
library's `parse` and `canonical` are plain functions, so each kind's
reader is compiled once, in its library.

**The registries.** The kernel (`kernel.rs`) keeps one registry for
each kind type, created the first time a capability or the routing
asks (`kind::<T>()`). It knows no kind. It also owns one derived
registry. Each registry is a `Registry` (`registry.rs`). Its
constructor is private to cap-abi, so a capability cannot declare
shared state.

**Loads.** Each kind's load is a kernel op in two forms:

- A bulk op over the document's bytes, with its library's code (256
  for the network), which its line in the list repeats. A code is
  never reused. The pages register every kind this way.
- The same op in JSON, for example
  `{"op": "load_network", "network": …}`. The kernel removes the
  document from the request before any capability decodes the request.
  The document stays raw, and the kernel does not buffer it into a
  generic tree.

A load answers the document's identity. For most kinds that is the
canonical hash of the parsed document (`canonical_hash`: the sha256 of
its RFC 8785 bytes, streamed into the digest). For a feed, a layer,
and an rt payload, it is the content hash of the bytes. A load also
answers what the host shows, such as a feed's summary or a layer's
headcount. It names the fields that the document carries and this
build does not know (`unknown_fields`, D-167), and answers nothing
more when there are none.

**Reading and creating documents.** A capability declares the kinds it
reads by their types (`capability! { documents: [Network, ...] }`). It
reads them through the kind's registry, for example
`kind::<Network>().require`. An op that creates a document registers
it where it creates it, for example `kind::<Network>().register`.
These ops create documents:

- an import, which creates a network and its boundary;
- an `apply`, which creates the next network;
- cap-rt, which creates the live document and a decoded snapshot.

**Derived entries.** A derived entry is a library's pure function and
its key, held under a `Derivation` with its own bound (`derive`):

- the day (`DAY`): transit-dispatch's `day_of` under its `day_key`,
  which is the effective trips and what the edits took away, as one
  materialization. Each capability that reads the day holds it through
  the same key, so one assembly holds one copy;
- the router's graph;
- the street graph and the walking graph.

An evict whose keep list drops any input of an entry drops the entry.

**Registry ops.** Every assembly answers 257 (evict), 258 (held), and
268 (about). Evict and held cover the kinds its capabilities read and
the derived entries. About answers the capabilities the assembly
links, and the kinds each one reads.

**Requests in a linked module.** A JSON request to an assembly names
its capability (`"cap"`) when the assembly links more than one. A
capability's own bulk op (0 to 255) carries the capability's index in
the high 16 bits.

**The manifest.** The manifest's `kernel_ops` documents every kernel
op, with an example computed through the same kernel. Each
capability's entry names its `documents`. The `assemblies` entry
records each UI's linked module: its capabilities in index order, and
its pinned bytes.

**Answers.** Every capability's JSON entry point answers through
`answer` or `outcome`. The answer is serialized directly from the
typed response, with no generic tree. An answer that carries a
document a host stores, such as an import's network, carries the
document's canonical bytes exactly (RFC 8785, `cap_abi::Canonical`;
`docs/FORMATS.md`).

**Generators and gates.** gen-assemblies writes the assembly crates.
arch-gates checks the dependency graph and the source of every
capability crate ("Bus rules", below). Two Node checks also run:

- `caps-smoke.mjs` is the Node smoke test of the module interface. It
  checks three properties:
  - every advertised op is documented, so the agent context cannot
    drift (an undocumented op fails the build);
  - every module's `about` matches its manifest entry;
  - each linked assembly registers a document once for every
    capability.
- `independence.mjs` checks assembly independence ("Bus rules",
  below).

## The capabilities

Each capability is described by its `OPS.md`, and by its `//!` crate
comment for the rules its ops share ([crates.md](crates.md)).

## Bus rules

The bus depends on these rules.

**Composition is through documents.** Capabilities compose through
documents named by content hash. A capability never calls a
capability.

- In one linked module, the capabilities share the kernel's
  registries.
- In separate modules, the host loads the same bytes into each module
  and checks that the returned hashes agree.
- In the browser this is true by construction. A document is one
  engine `Doc`. Every instance that registers it must answer the
  identity that the first one answered. A served document must answer
  the hash its server pins.

**Capabilities never call capabilities.** In one binary, a crate can
still name only the code of the crates it depends on. Four gates in
`just test` enforce the rule. Each gate has a test that breaks it on a
fixture.

1. The dependency graph. `arch-gates` reads `cargo metadata` and
   denies by default. A capability depends only on cap-abi and
   libraries. Only the generated assemblies depend on capabilities.
   The generator (`crates/generator`) reads the kernel, the libraries,
   and the fixtures (`crates/fixtures`, D-235), and nothing depends on
   either, so no module's closure holds them. Only the codec's homes
   read transit-codec. The test is
   `crates/cap-abi/tests/arch_gates_test.rs`.
2. The kernel's private registry constructor, checked by the
   `compile_fail` doctests on `cap_abi::registry::Registry`. A
   capability crate declares no state: no static and no
   `thread_local`.
3. `#![forbid(unsafe_code)]` in every capability crate. The unsafe
   exports are written by `assembly!`.
4. Assembly independence (`crates/cap-abi/independence.mjs`, with its
   broken fixture in `independence.test.mjs`). Every worked example
   gives the same answer from the capability alone and inside each
   UI's assembly and the server's, in either order of its
   capabilities. A shared derived entry is only a cache, never a
   dependency.

**Each name is declared once.** A new op changes only its capability's
crate, and a new capability is a new crate directory: the generator,
the server's assembly, and the engine's op map find it there. Two more
arch gates hold this, each with its broken fixture in
`arch_gates_test.rs`:

- The generator, cap-abi, and the root `Cargo.toml` name no
  capability's crate (`cap_<id>`). The gates' own lists are the
  exception.
- The kernel, the server, the runner, and the engine name no kind that
  the kind table routes generically: a kind with a `post` route or a
  `work_order`. Its name in a string literal, its load op, and its hash
  fields count, and comments do not. They read such a kind from the
  table: each way it differs from another kind is a named field in its
  line (`docs/FORMATS.md`, "The kind table"). A kind the server imports,
  publishes, or serves on a route of its own (the network, the streets,
  the plans, the edits, the policy) is a server feature, and the server
  may name it.

`crates/generator/tests/new_names_test.rs` is the proof. A test
capability and a test kind, declared only in `fixtures::new_names`, pass
the kind list's checks (`cap_abi::kinds::problems`), take their place in
the kind table and the server's assembly, pass the gate, and answer
through the kernel's dispatch.

**The module computes a document's hash.** A host does not compute it.
The canonical form is RFC 8785, so another tool can check an identity.
`JSON.stringify` is not that form, because it keeps keys in the order
they were inserted. Query with the hash the module returned.

**Bulk op numbers.** A capability's own bulk ops number from 0, for
example streets' `import` and rt's `decode`. In a module that links
several capabilities, the capability's index is in the high 16 bits.
Kernel ops number from 256, and each means the same thing in every
module that answers it. The manifest's `kernel_ops` lists them.

- A module answers the load of each kind its capabilities read. The
  browser host registers every document this way: the engine has a
  loader for each kind, and it encodes a document once however many
  instances register it.
- Every module answers the registry ops:
  - 257 (evict): `{"keep": [hash, …]}` drops every document not kept,
    and every derived entry whose inputs are not all kept. It answers
    what it dropped and what it holds.
  - 258 (held): what the module holds, for each registry, oldest
    first.
  - 268 (about): the capabilities the module links, each with its
    kinds.

**The kernel keeps every document.** The documents are in
`cap_abi::registry::Registry` statics that only the kernel constructs.
The host owns the documents, so it decides which survive. The kernel
drops the rest.

- Short-lived kinds bound themselves. rt snapshots keep their last 8,
  and live documents keep their last 4.
- Each derivation also has a bound. Cached days keep 4. Router graphs
  keep 8, and only the newest live document's graphs. Street and
  walking graphs keep 4. So a poll loop levels off with no steering
  from the host.
- Long-lived kinds stay until an evict drops them: networks, feeds,
  edits, boundaries, layers, walk tables, streets, and watchlists.

When a host asks for an evicted document, every module gives the same
error: `unknown <kind> <hash>: <load op> first`. It never gives a
wrong answer.

**One Request enum for each capability is its whole surface.** It
holds no load, because registration is the kernel's. Only cap-author's
`apply` and cap-network's `import` create networks. Readers have no
authoring ops, by construction.

**A capability crate is an rlib that registers.** `capability!`
declares `CAPABILITY` and its native interface: `invoke`,
`invoke_bytes`, and `registries`. The native interface answers as the
capability's one-capability module answers, and the tests use it. Only
an assembly crate emits the `#[no_mangle]` exports (`assembly!`), so
linking never gives a name collision.

**No private document formats.** A capability does not grow a private
document format. Shared data crosses modules only as kernel documents
named by content hash.

## Why

Each entry covers one choice that the design depends on. Its heading
names the code the choice governs. **Holds** is what must stay true.
Change it only with the user, and only after you read **Because**.
The rules above are part of the design too. The entries below explain
the rules whose reasons are not clear from the rules alone.

### A capability is a documented namespace for one need (`capability!`, `OPS.md`; D-226)
- **Holds:** a capability is a set of ops, each answering a whole question over documents and a person's choices. It ships with its `OPS.md`, schemas, worked examples, and evidence. Its crate is thin and may link any library. The libraries own the domain and the kinds.
- **Because:** parity needs a whole question answered in shared code. A wall around each capability's libraries pushed questions that span kinds out to the hosts, where each host wrote its own answer. Rule 3 and the extraction pattern already put the domain in libraries.
- **Rejected:** a capability that a third party implements in place of ours. It is not designed for until a concrete need arises.
- **Decided:** D-226.

### One linked module per page, one registry (`assembly!`, `cap_abi::registry::Registry`, `independence.mjs`)
- **Holds:** a page's capabilities link into one module over one
  registry that the kernel owns. So each document is parsed and held
  once for each page. The server links one assembly the same way
  (D-227).
- **Because:** separate instances each parsed their own copy of the
  network. On C-TRAN each copy took about 9 to 12 MB and 300 ms, and
  the stop display held three. Linking took the stop display from
  41.5 MB to 21.0 MB.
- **Cost accepted:** an assembly's capabilities share one model
  version. When one instance fails or resets, all its capabilities do.
  The compiler and the gates give the isolation that separate memories
  gave.
- **Rejected:**
  - one module for each capability;
  - wasm threads with a shared imported memory, which cannot share a
    parsed heap across separately compiled modules, breaks the zero
    imports, and needs COOP/COEP and nightly Rust;
  - a compact format that needs no parse, which makes smaller copies
    but still one copy for each module;
  - a user's own selection, linked on demand at wire time. A static
    deployment has no server to link it, so it needs a prebuilt set of
    selections. It also makes a UI's bytes differ for each user, so a
    UI is no longer one product that is deployed and rolled back
    alone.
- **Decided:** D-77, D-176.

### The module verifies every pin by hashing (`canonical_hash`, kernel loads)
- **Holds:** a module registers a document only under the hash it
  computes over the canonical re-serialization. The module checks a
  host's pin and never trusts it.
- **Because:** only re-serializing proves that the bytes are canonical.
  If the module trusted a pin, a host could create an identity that no
  module derived. With one registration for each page (D-77), the cost
  is one canonical hash for each document on each page.
- **Rejected:**
  - trusting a pin for bytes that hash to it;
  - hashing once for each page and passing the identity on;
  - a strict canonical parser kept in step with serde.
- **Decided:** D-71.

### The canonical form is RFC 8785 (`transit_version::jcs`, `canonical_hash`, `Canonical`)
- **Holds:** a document's canonical bytes are RFC 8785's: keys sorted
  by UTF-16 code units, no whitespace, numbers as ECMAScript writes
  them. Its identity is their sha256.
- **Because:** the agency owns the data, and any tool may read and
  write it (D-146, D-152). With a standard form, a tool in any language
  computes the same identity for the same content, without this repo.
- **Costs:** the writer sorts each object's keys, and wasm memory never
  shrinks back from a peak, so the writer holds no whole document. A
  map whose keys arrive in order streams. Below the root's fields, a
  value is staged in a small buffer, where an object out of order is
  taken back and written key by key. The records that a network holds
  by the hundred thousand (a trip, a stop's times, an alignment point)
  declare their fields in canonical order, so they stream. On C-TRAN's
  network (9.6 MB), natively and with link-time optimization, the hash
  takes 20.5 ms, against 12.4 ms for serde's field order.
- **Rejected:** serde's field order (only this repo's Rust reproduces
  it); buffering the whole document to sort it (megabytes of wasm
  memory, kept).
- **Decided:** D-169.

### Every document says what it is (`kind`, `schema_version`, `transit-version`, `registry/schemas/`)
- **Holds:** every persisted document is plain JSON (gzip allowed) or
  an external open format. It carries its kind and version inside it,
  so the hash covers them. Its schema and a worked example are
  published for each version.
- **Because:** documents leave the app as files: exports, `caps-run`
  loads, store blobs, and backups. A copy anywhere must say what it is,
  so that a person or another tool can read it with no module and no
  deployment. Two versions of the same content never share a hash.
- **Rejected:** the version only in the store's envelope, because every
  other copy would then need a wrapper or a guess from its shape.
- **Decided:** D-62, D-115. Who owns the data, and how a write is
  governed, is in `docs/architecture/agency-data.md`.

### Migrations are kept forever, as a capability (`cap-migrate`, each owning library's chain)
- **Holds:** the migration chain of every authored kind is kept
  forever, in the library that owns the type. cap-migrate wraps it, and
  it is assembled only where an older document arrives. Consumers speak
  only the current version. Derived documents derive again and need no
  migration.
- **Because:** authored documents (edits, plans, a fork, a watchlist)
  can sit on an unopened device or in a file for years. A migration is
  a pure function with a frozen fixture, so keeping one costs one test.
- **Rejected:** a support floor (a year, or N versions) that refuses
  older documents.
- **Decided:** D-66.

### A release reads one version ahead (`Versioned::AHEAD`, `cap_abi::Base`, `Kind::require_writable`)
- **Holds:** a format change ships in two releases. Release A reads vN
  and vN+1 and writes vN. Release B writes vN+1. Release A reads vN+1
  through a step down that the change declares lossless or read-only.
  It refuses to write over a read-only copy, naming both versions.
- **Because:** rollback is a pointer moved back, never an inverse
  migration. So the build one release back must read what the store
  holds. A step down that cannot keep everything must not store vN over
  vN+1, or a write would drop data with no word.
- **Rejected:** rolling the data back with the code, except as the
  emergency procedure (`store revert`), because it can lose edits; and
  one rule for every change, because most changes can step down
  without loss.
- **Decided:** D-79, D-186. The rules are in docs/FORMATS.md, "Rolling
  a version back".

### The assembly crates are generated, never committed (`assemblies/`, `gen-assemblies`)
- **Holds:** git ignores `assemblies/*`. A `.gitkeep` keeps the
  directory. `just caps` generates the crates again before every build.
- **Because:** they are generated from each `ui.json`, as the TS
  bindings are. A committed copy would need a check that it is current.
- **Rejected:** committing them with a `--check` gate.
- **Decided:** D-85.
