# The capability bus

The bus is the architecture. Each capability answers one need. The
capabilities are horizontal: no capability belongs to one user. Each
generated UI links the capabilities it needs. So the capability bytes
differ for each UI, and the documents and the contracts are the same
everywhere.

A **module** is an assembly over the kernel (`cap-abi`). There are two
kinds:

- `module-<cap>` holds one capability alone. caps-run, agents, the
  registry's blobs, and the Go server run this kind.
- `module-ui-<id>` links the capabilities of one generated UI into one
  module (D-77). That UI's page loads it. A document registers once
  for each page, and the shared code ships once.

Linking joins bytes and merges no surface. Each capability keeps its
crate, its `Request` enum, and its manifest entry. Linking happens at
build time only. A user does not choose capabilities (D-176).

The assembly crates are in `assemblies/`. `gen-assemblies` generates
them from the capability crates and from the `ops` list in each
`web/src/uis/<id>/ui.json`. `just caps` writes them, and `just test`
checks that they are current.

## The kernel and the seam: cap-abi

`cap-abi` holds the kernel and the seam between a module and its host.

**The module interface.** The `assembly!` macro writes the module
interface. Every module has the same four exports, written once in each
assembly crate. A capability crate only registers itself with
`capability!`, and it forbids `unsafe`.

**The manifest contract and the validator.** `cap-abi` defines the
capability manifest contract: what a capability speaks, migrates, and
consumes, and its agent context. It also holds the assembly validator.
The validator proves an assembly at compose time: closure, version
coherence, and that a migration path can be reached. The validator is
total and has property tests. gen-manifest validates its own output
before it writes it.

**gen-manifest.** gen-manifest writes the capability set as code. It
reads each capability's agent context from that capability's `OPS.md`.
A change to the manifest is the change a reviewer reads for an
assembly. gen-manifest computes each worked example. The examples for
cap-author `apply` include one for each edit kind, applied to the
fixture in order. The build fails if an edit kind has no example, or if
an example stops applying.

**The registries.** The kernel (`kernel.rs`) owns one registry for each
document kind: network, edits, live, boundary, walks, streets,
watchlist, layer, feed, rt, work_rules, and stops. It also owns one
derived registry. Each registry is a `Registry` (`registry.rs`). Its
constructor is private to cap-abi, so a capability cannot declare
shared state.

**Loads.** Each kind's load is a kernel op. The load comes in two forms:

- A bulk op over the document's bytes: 256 network, 259 edits, 260 live,
  261 boundary, 262 walks, 263 streets, 264 watchlist, 265 layer,
  266 feed, 267 rt, 269 work_rules, 270 stops, and 271 stop_requests.
- The same op in JSON, for example
  `{"op": "load_network", "network": …}`. The kernel removes it from the
  request before any capability decodes the request. The document stays
  raw. The kernel does not buffer it into a generic tree.

A load answers the document's identity. For most kinds that is the
canonical hash of the parsed document (`canonical_hash`, streamed into
the digest). For a feed, a layer, and an rt payload, it is the content
hash of the bytes. A load also answers what the host shows: a feed's
summary, a layer's headcount, a streets document's size, or a
watchlist's version.

**Reading and minting documents.** A capability declares the kinds it
reads (`capability! { documents: [...] }`). It reads them through the
kind, for example `NETWORK.require`. An op that mints a document
registers it where it mints it, for example `NETWORK.register`. These
ops mint documents:

- an import, which mints a network and its boundary;
- an `apply`, which mints the next network;
- cap-rt, which mints the live document and a decoded snapshot.

**Derived entries.** A derived entry is a library's pure function and
its key, held under a `Derivation`. Each derivation has its own bound
(`derive`). The derived entries are:

- the day (`kernel::day`): transit-dispatch's `day`, which is the
  effective trips and what the edits took away, as one materialization;
- the router's graph;
- the street graph and the walking graph.

A keep-set that drops any input of an entry drops the entry.

**Registry ops.** Every assembly answers 257 (evict) and 258 (held).
These ops cover the kinds its capabilities read and the derived entries.
Every assembly also answers 268 (about): the capabilities it links, and
the kinds each one reads.

**Requests in a linked module.** A JSON request to an assembly names
its capability (`"cap"`) when the assembly links more than one. A
capability's own bulk op (0 to 255) carries the capability's index in
the high 16 bits.

**The manifest.** The manifest's `kernel_ops` documents every kernel
op, with an example computed through the same kernel. Each
capability's entry names its `documents`. The `assemblies` entry
records each UI's linked module: its capabilities in index order, and
its pinned bytes.

**Answers.** Every capability's JSON lane answers through `answer` or
`outcome`. The answer is serialized directly from the typed response,
with no generic tree. So an answer that carries a document, such as an
import's network, carries the document's canonical bytes in struct
field order (`docs/FORMATS.md`).

**Generators and gates.** gen-assemblies writes the assembly crates.
arch-gates checks the dependency graph and the source of every
capability crate ("Bus rules", below). Two Node checks also run:

- `caps-smoke.mjs` is the Node smoke test of the module interface. It
  also checks these laws:
  - every advertised op is documented, so the agent context cannot
    drift (an undocumented op fails the build);
  - every module's `about` matches its manifest entry;
  - each linked assembly registers a document once for every
    capability.
- `independence.mjs` checks the assembly-independence law ("Bus
  rules", below).

## The capabilities

### cap-gtfs

The feed-document capability. It answers the feed's own question over a
GTFS zip the kernel registered. `load_feed` answers what the zip
contains at a glance: its summary. cap-gtfs answers the feed's identity
(`feed_info`).

The day views, the watchlist, the pickers, and the names join are not
here. They answer over the network document, in cap-board, cap-runcut,
cap-calendar, and cap-directory. The bus has one answer for each
question.

cap-gtfs only parses and gives the identity, over transit-codec's
whole-spec `Feed`. Its charter is closed: parse, export, and primitive
boundary queries. Nothing more goes into GTFS.

### cap-router

The router capability answers one question: how do I get from here to
there? It reads the network document and the effective day. The
effective day is the model's materialized trips, patched with the edits
in force. Every day view starts from the same transit-dispatch
composition.

- When `live_hash` names a live document (cap-rt's live answer), the
  router reads the day at that document's predictions.
- When `walks_hash` names a walk table (cap-streets' `walks` answer),
  transfers walk the streets. The router refuses a walk table derived
  for another network. With no walk table, transfers are straight
  lines.
- The RAPTOR graph is a derived entry for each (network, edits, live,
  date, walks). Only the graphs for the newest live document stay.

A plan's `from` and `to` are stop ids or places (`{id, name, walks}`).
A place passes cap-streets' `access` walks as they are. `direct` is the
walk between the two ends (from `access`).

- `plan` leaves at a time.
- `plan_arrive_by` answers the latest departure that arrives by a
  deadline. With no deadline, it answers the last bus home.
- `itinerary_paths` takes the plan's ends and `direct`. It draws a
  place's walks along their street paths (`transit_router::walk_path`).
  Without them, a place draws nothing.

The compare facet is the diversion delta: the unpatched day read at the
same predictions, one materialization apart, with its notice. The
`itinerary_paths` facet draws a plan's legs over the same day: a
transit leg's ridden segment, and a walk through its via stops. The
browser plans only through `plan_compare`, with no edits hash, so there
is no baseline. The browser shows the notice's text.

cap-router is a pure-function capability. It writes nothing.

### cap-analysis

The analysis capability gives the planner's numbers: stats, service
types, the frequency grid, hub connections, and coverage. It reads the
kernel's networks and weighted layers. `load_layer` answers the
features of a layer, and the numeric fields it can be read by. A
coverage op names the field and the label (D-97).

The domain code is in the shared transit-analysis library. cap-analysis
consumes network documents from cap-network. The manifest carries this
registry edge.

### cap-network

The network codec capability. Its ops:

- `import`: the planner's start, over a feed the kernel registered. The
  document and its boundary residue register themselves. The response
  has both hashes and the network's time zone. The time zone is the one
  zone of the network's operators, and a host counts the agency's date
  in it.
- `validate`: the authoring feedback. Issues stop an export. Warnings
  never stop it.
- `export`: the publish path. It gives deterministic GTFS zip bytes
  under their content hash, in base64 across the JSON seam. When
  `boundary_hash` names the import's boundary, the export uses that
  numbering. The zip registers as a feed.
- `publishable`: what refuses a network as a published pick. That is its
  issues, and a pick with no service in its period (D-76).
- `document`: a registered network's canonical bytes, hash, plan frame,
  and time zone. The server stores this when it publishes a pick it got
  in any serialization.

The domain code is in the shared transit-codec library.

### cap-rt

The realtime capability.

**Decode.** `decode` (bulk-bytes op 0) reads a GTFS-rt protobuf
payload. An agency serves these bytes every few seconds. The answer is
the overview view: delays for each trip and stop, live vehicles, and
service alerts, all in the feed's ids. Each snapshot registers under
its content hash. The kernel's `load_rt` registers a snapshot and
answers only its hash.

**The schedule join.** cap-rt joins the feed to the network. It uses
the network's boundary, when the network was imported, for the
published `stop_sequence` numbering. The boundary is optional.

- `trip_progress` answers one trip's board: scheduled, delay, and
  predicted. A stop that the update skips carries the last stated
  delay. This is the GTFS-rt propagation rule.
- `late_trips` ranks late trips. It flags a trip that is delayed but
  dark. It takes `vp_hash` for agencies that serve the two feeds
  separately. C-TRAN does. Darkness is judged for each block. When a
  delay is propagated to a block's next trip while its bus runs the
  trip before, the vehicle is located. `position_trip_id` names the
  trip the vehicle is on. `vehicle_id` is beside the label. A vehicle
  with no label is named by its id.

**The live document.** `live` mints the live document (transit-live)
from the trip-updates, vehicle-positions, and service-alerts snapshots.
Any of the three may be present (`alerts_hash` names the alerts). Each
snapshot's alerts count once for each entity id. The answer also has
the summary a status panel reads: vehicles by status, and the late
list.

The live document registers where cap-rt mints it. When cap-rt is
linked beside the board and the router, they read it by `live_hash`.
It never crosses to the host and back. With `with_document: false`, the
answer leaves the document out too. The stop display uses this
(QAF-49). The browser's live views render the live document. No
capability joins rt again.

### cap-board

The board capability answers one question: when does service leave
this stop? It answers at a stop, at a station's pooled bays, or for a
personal saved set. The saved set is the watchlist document, version 1
or 2, read in either form (cap-migrate migrates it). A personal board
of many stops is the same question with a personal document as its
parameter.

cap-board reads the kernel's network, edit document, live document
(cap-rt's, by hash), and watchlist. Its ops:

- `stop_board` is the board. The module answers all of these, never the
  host: pooling, realtime-first order, the row status, the route's long
  name, the bay's name, the sign's destination, the visits that the
  in-force edits skip and the diversion that skips each, and the live
  alerts about the board.
- `grouped_board` gives the same rows by route and destination. By
  default it gives the next three of each. This is the stop display's
  answer. A group that has stopped says when it runs next within a
  week. It materializes the later days itself and does not hold them.
  The kernel holds only a few days, and a week of days would push them
  out.
- `watchlist_board` gives each watched stop's `stop_board`, with the same
  edits and live document.

Each op takes `previous`: the previous date at the same instant, from
the host's clock. With it, the op also answers that date's departures
after midnight that have not left yet. Each such row names its date
(`stop_board_at`, `grouped_board_at`). A trip's final visit is an
arrival and is never a row.

All three ops read one materialized day: `materialize(net, date)`,
patched with the in-force edits. This is the kernel's held day: one
materialization for each (network, edits, date), however many boards
ask. There is no GTFS expansion. The domain code is in the shared
transit-dispatch library. The arch gate forbids a GTFS layer here. The
UI's departure board renders `stop_board`'s answer. Each row carries the
address of its trip instance (trip and start, and its date when the
board is asked at a moment). So the click-through needs no join.

### cap-runcut

The runcut capability answers one question: what does this vehicle do
today? Its ops:

- `blocks`: the day's blocks. This is the index facet: the vehicles out,
  with the count of trips each runs that day. A block whose every trip
  cannot be represented or is skipped is not offered. The hole is shown
  as a hole.
- `fleet_summary`: the day's fleet. It gives the peak of concurrent
  vehicles and when it happens, the platform, revenue, and layover
  hours, and an outline of each running block (span, trips, and the
  longest internal gap). One call replaces a `block_trips` call for each
  block.
- `block_trips`: one block's chain. This is the detail facet: the
  authored runcut with the layover ripple. The ripple is the time the
  in-force edits add. With a live document, the ripple is a trip's
  predicted delay at its last stop, and that wins (D-18).
  `layover_remaining_secs` goes negative when a connection is eaten.
  `layover_health` classifies the layover as overlap, tight, or ok,
  under the caller's `tight_layover_secs`. Each row names its trip by
  id. A frequency trip appears once for each instance.
- `trip_stops`: one trip instance's effective stops on the date. This
  is the trip detail facet.
- `trip_path`: one trip instance's line, which the map's click-through
  draws. It is the authored alignment. An in-force diversion's traced
  detour is spliced in between the stops the diversion keeps.

Blocking is where cutting the work begins (D-46). So the same question
also answers under the work rules (the kernel's `work_rules` kind):

- `recovery`: what each wait requires. It gives the required time and
  the slack, for each wait and each pattern.
- `suggest_blocks`: which vehicles the trips that no block carries need.
  It gives the suggested blocks, each wait's slack, the adopted peak
  against the authored one, and the edits that author `apply` adopts.

cap-runcut reads the same documents and the same held day as cap-board.
The domain code is in the shared transit-dispatch and transit-runs
libraries. The arch gate forbids a GTFS layer here. The UI's blocks
panel and trip lines answer through this module.

### cap-calendar

The calendar capability gives two reads of one relation:

- `active_services`: which service keys run today.
- `service_dates`: the reverse, the dates one service type runs, over
  an inclusive window.

Both read the effective day. A key whose every trip is skipped or
cannot be represented is not running, whatever its calendar says. This
is the same rule as the runcut view's hole. cap-calendar reads the same
documents and the same held day as cap-board. The domain code is in the
shared transit-dispatch library. The arch gate forbids a GTFS layer
here.

### cap-directory

The directory capability answers one question: what exists in the
network? Its ops:

- `about`: what the network is, its name and plan. This is the heading
  of a page that holds the network only as bytes.
- `stops`: the stop list, sorted by name, with the walking-group
  structure the station board keys on. An optional query narrows the
  list the way typing narrows a picker. The match is fuzzy: exact
  matches first, then a sign's abbreviations, then typos (D-48).
- `stops_by_ids`: the names join.
- `stops_near`: the places nearest a point. Distance is in great-circle
  meters, nearest first, with ties broken by id. It answers at most
  `limit` places, and only within `radius_m` when it is given. A
  station's bays answer once, as the station, at the distance of its
  nearest member. This is the walking group that the board pools. When
  the point's `walks` are given (cap-streets `access`), the places a
  walk reaches come first, by walk (the shortest to any member). The
  rest follow by distance (D-34). Names are matched by the sign's own
  words, never by an alias.
- `routes`: the route list, in the model's closed set of modes. Every
  row that names a route carries its long name. This is true for
  `routes`, `entities`, `stop_detail`'s routes, and the timetable
  header.
- `route_stop_sequence`: one route's patterns, with their ordered
  stops. The diversion editor
  anchors against these. New edit records carry `direction_id` null,
  which is the documented case for both directions.
- `focus`: where an entity is on the map. The planner's drill-down
  frames it.
- `entities`, `route_detail`, and `stop_detail`: the planner's lists.
  `entities` gives filtered rows for each kind. The forms' pickers and
  the document counts read them too. `route_detail` gives a route's
  composition with trip counts, and its patterns and rider variants by
  direction. `stop_detail` gives a stop's serving routes: its own, or a
  station pin's bays' (the board's rule). It also gives the stop's own
  record, and every pattern position that visits it, with the trips
  each runs under the members asked.
- `timetable`: the planner's timetable, for a route, a direction, and a
  service type, across the direction's patterns.

The pickers are model questions, never feed reads. The UI's comboboxes
and the map's name lookups read this module. The feed line reads the
kernel's `load_feed` summary and cap-gtfs's `feed_info`. The planner
computes no number.

### cap-streets

The street-graph capability answers where along the streets. Its ops:

- `import` (bulk-bytes op 0) reads a street layer into the streets
  document and registers it. It answers the document, which the host
  keeps, with its hash and size. The kernel's `load_streets` registers a
  document the host holds, under the same hash. It refuses a document
  that is not canonical.
- `snap` answers the nearest street position. This is the graph address
  that a trace's waypoint commits to.
- `route` answers the street geometry between two positions. It is the
  shortest path, starting after `from`, so trace legs chain. It also
  gives the length along the graph and the streets it follows. On
  parts of the graph that do not connect, it answers empty, and the
  caller falls back to a straight line. For the bus profile, the
  streets to confirm come back named. `confirm` is the op's parameter.
- `walks` answers the walk table between a registered network's stops,
  with its hash. The kernel's `load_walks` answers the same.
- `access` answers a point's walks to those stops, nearest first, each
  with its path. It also answers the direct walk to a second point. This
  is the router's place.

The walking graph is a derived entry, built on the first `walks`. The
street graph is built on the first `snap`. A host that only walks never
builds the street graph. The domain code is in the shared
transit-streets library. The module writes nothing.

Today the server uses cap-streets for the streets and the walk table it
publishes, and ride-now uses it for walks from a point to the stops
near it. Drawing a detour or a pattern on the streets is next
(`docs/units/streets-model.md`, `docs/units/ui-disruptions.md`).

## Bus rules

The bus depends on these rules.

**Composition is through documents.** Capabilities compose through
documents named by content hash. A capability never calls a capability.

- In one linked module, the capabilities share the kernel's
  registries.
- In separate modules, the host loads the same bytes into each module
  and checks that the returned hashes agree.
- In the browser this is true by construction. A document is one engine
  `Doc`. Every instance that registers it must answer the identity that
  the first one answered. A served document must answer the hash its
  server pins.

**Capabilities never call capabilities.** In one binary, a crate can
still name only the code of the crates it depends on. Four gates in
`just test` enforce the rule. Each gate has a test that breaks it on a
fixture.

1. The dependency graph. `arch-gates` reads `cargo metadata` and denies
   by default. A capability depends only on cap-abi and libraries. Only
   the generated assemblies depend on capabilities. Only the codec's
   homes read transit-codec. The test is
   `crates/cap-abi/tests/arch_gates_test.rs`.
2. The kernel's private registry constructor, checked by the
   `compile_fail` doctests on `cap_abi::registry::Registry`. A
   capability crate declares no state: no static and no
   `thread_local`.
3. `#![forbid(unsafe_code)]` in every capability crate. The unsafe
   exports are written by `assembly!`.
4. Assembly independence (`crates/cap-abi/independence.mjs`, with its
   broken fixture in `independence.test.mjs`). Every worked example
   gives the same answer from the capability alone and inside each UI's
   assembly, in either order of its capabilities. A shared derived
   entry is only a cache, never a dependency.

**The module is the hash authority.** Hosts never mint a document's
identity. JavaScript's `JSON.stringify` formats floats differently from
the module's canonical serde bytes. Query with the hash the module
returned.

**Bulk op numbers.** A capability's own bulk ops number from 0, for
example streets' `import` and rt's `decode`. In a module that links
several capabilities, the capability's index is in the high 16 bits.
Kernel ops number from 256, and each means the same thing in every
module that answers it.

- Each kind's load: 256 network, 259 edits, 260 live, 261 boundary,
  262 walks, 263 streets, 264 watchlist, 265 layer, 266 feed, 267 rt,
  269 work_rules, 270 stops, and 271 stop_requests. A module answers
  the load of each kind its capabilities read. The browser host
  registers every document this way: the engine has a loader for each
  kind, and it encodes a document once however many instances register
  it.
- Every module answers the registry ops:
  - 257 (evict): `{"keep": [hash, …]}` drops every document not kept,
    and every derived entry whose inputs are not all kept. It answers
    what it dropped and what it holds.
  - 258 (held): what the module holds, for each registry, oldest first.
  - 268 (about): the capabilities the module links, each with its kinds.

**The kernel keeps every document.** The documents are in
`cap_abi::registry::Registry` statics that only the kernel constructs.
The module owns what it holds, because it is the hash authority. The
host names what survives, because it owns the documents.

- Short-lived kinds bound themselves. rt snapshots keep their last 8,
  and live documents keep their last 4.
- Each derivation also has a bound. Held days keep 4. Router graphs keep
  8, and only the newest live document's graphs. Street and walking
  graphs keep 4.
- So a poll loop levels off with no steering from the host.
- Long-lived kinds stay until a keep-set drops them: networks, feeds,
  edits, boundaries, layers, walk tables, streets, and watchlists.

When a host asks for an evicted document, every module gives the same
error: `unknown <kind> <hash>: <load op> first`. It never gives a wrong
answer.

**One Request enum for each capability is its whole surface.** No load
is in it, because registration is the kernel's. Only cap-author's
`apply` and cap-network's `import` mint networks. Readers have no
authoring ops, by construction.

**A capability crate is an rlib that registers.** `capability!`
declares `CAPABILITY` and its native seam: `invoke`, `invoke_bytes`,
and `registries`. The native seam answers as the capability's
one-capability module answers, and the tests use it. Only an assembly
crate emits the `#[no_mangle]` exports (`assembly!`), so linking never
gives a name collision.

**No private document formats.** A capability does not grow a private
document format. Shared data crosses modules only as kernel documents
named by content hash.

## Why

Each entry covers one choice that the design depends on. Its heading
names the code the choice governs. **Holds** is what must stay true.
Change it only with the user, and only after you read **Because**. The
rules above are part of the design too. The entries below explain the
rules whose reasons are not clear from the rules alone.

### One linked module per page, one registry (`assembly!`, `cap_abi::registry::Registry`, `independence.mjs`)
- **Holds:** a page's capabilities link into one module over one
  registry that the kernel owns. So each document is parsed and held
  once for each page. Four things enforce "capabilities never call
  capabilities": the dependency graph, the private registry
  constructor, `forbid(unsafe_code)`, and assembly independence.
  Linking happens for each generated UI at build time. The server stays
  on single-capability modules (D-78).
- **Because:** separate instances each parsed their own copy of the
  network. On C-TRAN each copy took about 9 to 12 MB and 300 ms, and the
  stop display held three. Each instance also shipped the same base.
  Linking took the stop display from 41.5 MB to 21.0 MB.
- **Cost accepted:** an assembly's capabilities share one model version.
  When one instance fails or resets, all its capabilities do. The
  compiler and the gates give the isolation that separate memories gave
  before.
- **Rejected:**
  - one module for each capability;
  - wasm threads with a shared imported memory, which cannot share a
    parsed heap across separately compiled modules, breaks the zero
    imports, and needs COOP/COEP and nightly Rust;
  - a compact format that needs no parse, which makes smaller copies
    but still one copy for each module;
  - a user's own selection, linked on demand at wire time. A static
    deployment has no server to link it, so it needs a prebuilt set of
    selections. It also makes a UI's bytes differ for each user, so a UI
    is no longer one product that is deployed and rolled back alone. The
    UIs already link only what each one needs (D-176).
- **Decided:** D-77, D-176.

### The module verifies every pin by hashing (`canonical_hash`, kernel loads)
- **Holds:** a module registers a document only under the hash it
  computes over the canonical re-serialization. The module checks a
  host's pin and never trusts it.
- **Because:** only re-serializing proves that the bytes are canonical.
  If the module trusted a pin, a host could mint an identity that no
  module derived. With one registration for each page (D-77), the cost
  is one canonical hash for each document on each page.
- **Rejected:**
  - trusting a pin for bytes that hash to it;
  - hashing once for each page and passing the identity on;
  - a strict canonical parser kept in step with serde.
- **Decided:** D-71.

### Every document says what it is (`kind`, `schema_version`, `transit-version`, `registry/schemas/`)
- **Holds:** every persisted document is plain JSON (gzip allowed) or an
  external open format. It carries its kind and version inside it, so
  the hash covers them. Its schema and a worked example are published
  for each version.
- **Because:** documents leave the app as files: exports, `caps-run`
  loads, store blobs, and backups. A copy anywhere must say what it is.
  A person or another tool must be able to read it with no module and
  no deployment. Two versions of the same content never share a hash.
- **Rejected:** the version only in the store's envelope, because every
  other copy would then need a wrapper or a guess from its shape.
- **Decided:** D-62, D-115. Who owns the data, and how a write is
  governed, is in `docs/architecture/agency-data.md`.

### Migrations are kept forever, as a capability (`cap-migrate`, each owning library's chain)
- **Holds:** the migration chain of every authored kind is kept forever,
  in the library that owns the type. cap-migrate wraps it, and it is
  assembled only where an older document arrives. Consumers speak only
  the current version. Derived documents derive again and need no
  migration.
- **Because:** authored documents (edits, plans, a fork, a watchlist)
  can sit on an unopened device or in a file for years. A migration is a
  pure function with a frozen fixture, so keeping one costs one test.
  This is like keeping an application's SQL migrations.
- **Rejected:** a support floor (a year, or N versions) that refuses
  older documents.
- **Decided:** D-66.

### The assembly crates are generated, never committed (`assemblies/`, `gen-assemblies`)
- **Holds:** git ignores `assemblies/*`. A `.gitkeep` keeps the
  directory. `just caps` generates the crates again before every build.
- **Because:** they are generated from each `ui.json`, as the TS
  bindings are. A committed copy would need a check that it is current.
  The workspace glob only needs the directory to exist.
- **Rejected:** committing them with a `--check` gate.
- **Decided:** D-85.
