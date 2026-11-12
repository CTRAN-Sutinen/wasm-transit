# The web (`web/`)

Svelte 5 runes + Vite, TypeScript end to end, as a multi-page
build: `/` is the PICKER (`index.html` → `src/picker/`), one card
per generated UI ([generated-uis.md](generated-uis.md)). Every page lives at the bundle's root
because every fetch (the built assets — Vite's `base: "./"` —
`caps/manifest.json`, `basemap.pmtiles`, the deployment's `manifest`
and `blobs/`) is relative, so a bundle serves from any path. Every page is a generated UI; there is no other shell.

## Parity

An agent (caps-run), the Go server, and the browser a human uses
run the SAME ops and get the SAME bytes. Any answer the browser
computes in TypeScript is an answer the other two hosts cannot
give — so `web/` holds host glue (I/O, fetch and pagination,
IndexedDB, the clock, settings, polling, the sync POST, the
bus worker) and RENDER components (props in, events out,
formatting), and nothing else. A join, fallback, threshold, record
builder, or geometry rule in TypeScript is a parity break, however
small: it lands as an op instead.

Two rulings that follow from it:

- **The live document.** cap-rt answers "what is live" in MODEL
  vocabulary: an rt snapshot plus the network resolve into a
  kernel document of delays per trip instance and stop and of the
  service alerts in force, addressed by hash; cap-board (and the
  router) consume it by hash. Which alert touches which departure
  is the board's answer, never the host's. rt trip ids resolve
  INSIDE the op, never in a host. Thresholds (the on-time
  window) are op parameters.
- **Geometry is a model answer.** "What path does this trip take
  today" (diversions, the authored alignment, an itinerary leg's
  ridden segment) answers in an op; `web/` only draws the GeoJSON
  it is handed.

The WEB GATES in `just test` hold it: no component imports the
bus, no host code reads raw rt `stop_updates`, nothing clones a
document to mutate it (every change is a NetworkEdit through
cap-author), and the engine is the one op caller — no file but
`src/engine.ts` imports the bus (`src/bus.ts`), no file but the bus
spawns the bus worker, and no file but the worker loads or calls a
module. Each
gate's allowlist only shrinks, and all are empty. Beside them, the
DETERMINISM gate holds the modules pure: no clock, randomness, or
environment in any crate's library code (hosts pass time in), so an
answer is a function of its inputs.

File ownership:

- `src/mapview.ts` only shapes GeoJSON: every line the map draws
  is an answer from the bus (cap-router `itinerary_paths`,
  cap-directory `focus`); it turns `[{lat, lon}]` into
  `[lon, lat]` and computes no geometry.
- Components are RENDER-ONLY: props in, events out. No component
  imports the bus or a UI's host layer (the WEB GATES and the
  GENERATED UIs gates in `just test`).
- `src/engine.ts` is THE ENGINE — the one caller of a module's ops.
  A host asks `ask(capability, op, args)`, the generated request
  types with every `*_hash` argument taking a `Doc` (a document
  version: its kind, its value or bytes, the document it is read
  over, the hash its server pins). A generated UI's host names its
  page first (`assemble("<id>")`): every capability its linked
  assembly links answers in that ONE module instance (the bus worker,
  below), and a capability it does not link answers alone. The engine
  registers a `Doc` in a module instance on first use through the
  kernel's load of its kind (`load` answers the load's whole answer —
  a feed's summary, a layer's headcount), so a document registers
  once per page however many capabilities read it; it asserts every
  instance answers the same identity, and caches answers under the
  journal's key (the module's pinned hash — the assembly's — and the
  request's bytes, which name the capability in a linked module).
  `produced` adopts a document an op answered (an import, an `apply`,
  the live document) as registered where it was made — for every
  capability linked there; a document the page declined to receive
  (the stop display's live document, `with_document: false`) carries
  the ask that mints it again (`remint`), and `together` says whether
  capabilities share an instance (so a host may decline it). `available` says
  whether a capability assembled, and `ask` throws `Unavailable`
  when it did not. `servedNetwork`, `servedStreets`, and
  `servedWalks` read the deployment's published documents and refuse
  one that does not answer its pinned hash — `servedNetwork` in the
  first of the page's named readers that assembles, so no module
  holds a copy only to check it. `servedNetwork` takes the page's local
  date: when the manifest pins a published plan set (`plans_hash`) it
  asks the plans module which pick is in force (`in_force`) and loads
  that pick's network by hash (`blobs/<network_hash>.gz`) with the
  default boundary; before the first pick takes effect it loads the
  import by the manifest's pins (`blobs/<network_hash>.gz`,
  `blobs/<boundary_hash>` — one manifest read names both, so the
  boundary always belongs to the network). Every pin is the
  manifest's, so a static host serves the same ([static-deployment.md](static-deployment.md)). It throws
  Unavailable when picks are published and the plans module does not
  assemble — a page never guesses which schedule runs. Either way it
  holds the network as the gzipped bytes the server sends: the
  page never unzips, parses, or re-encodes it, and every module it
  registers in decodes the stream itself (a heading asks the
  directory's `about` for the network's name). `servedNetworkValue`
  is the planner's: the network as a value it forks; `publishedPlanSet`
  and `publishedPickValue` are its timeline (D-139) — every published
  pick, and one pick's network as a value, fetched gzipped and
  decompressed in the page (a value is parsed either way). A module may drop what
  it holds (see "The bus worker", below): asking for a dropped document is
  the module's unknown-hash error, and the engine registers the
  document again (or mints it again) and asks once more — a host
  never sees it. `keep` names what survives: every module instance
  drops every document not named (nor read over by one named); the
  rider pages call it when they adopt a network and when the edit
  document changes, and service-design after every change it adopts
  (the plan set's current networks and the layer — each `apply` mints
  a network, and undo registers a dropped version again). `window.__bus` is the e2e's inspection (the
  module instance a capability answers in, what it holds, its memory
  and resets, the ceiling, a replay of the last op asked of a
  capability past the answer cache).
- `src/bus.ts` is the page's side of the bus worker, imported only by
  the engine: the one Worker, spawned on first use, and its calls as
  promises — it carries messages and decides nothing.
- `src/documents.svelte.ts` is the device's document store (every
  version hash-chained by `produced_by`; `current` is a pointer;
  IndexedDB-persisted — ride-now's watchlist lives there).
- `src/plans.svelte.ts` owns the service plans (the authored
  network documents service-design keeps on the device: plain data,
  wholesale-replaced, IndexedDB-persisted). Stored plans restore
  through a module: `restorePlans(readStoredNetwork)` reads each
  through cap-author (an `apply` with no edits answers the canonical
  document), so a plan saved under older field names comes back in
  the current shape; a bare `restorePlans()` restores them as
  stored, and `readPlan(id, readStoredNetwork)` reads one on first
  use. The host never sniffs a shape.
- `src/basemap.ts` is the basemap choice every map view shares
  (local pmtiles, else OpenFreeMap) and the MapLibre worker wiring.
  The archive is probed by reading its header with the one `PMTiles`
  instance the protocol then serves every tile from (no separate
  probe request, the header fetched once per page); the local
  style's glyphs and sprites are `basemap-assets/` beside the tiles
  when `just basemap-assets` installed them (probed, never
  configured), else protomaps.github.io. `pickStyle(near)` returns
  the view the map opens on. Each map component owns its map
  (`window.__map` is the e2e hook).
- `src/viewport.ts` is where a map's camera starts and when it
  frames the whole network — pure presentation rules (`startView`:
  the rider's last fix where tiles cover it, else the view the
  network was last framed on, else the archive's view;
  `shouldFrameNetwork`: only when nothing better is known — no fix
  pending, none arrived, the camera not opened inside the network;
  `sameView`: a camera already on a view is not moved to it). The
  point is that no camera move precedes the one the rider ends on:
  each fetches tiles that are thrown away. The last fix and the last
  network frame are the rider app's own device data
  (`ride-now:last-fix`, `ride-now:network-view`, D-90), kept in
  `localStorage` by its host layer and passed to RideMap as `start`
  and `framed` (RideMap reports a frame it gave as `onframed`).

## The bus worker

`src/bus.worker.ts` is the ONLY wasm loader: every module a page
assembles lives in ONE module Worker, off the thread the page renders
on. A generated UI's page runs ONE module instance — its linked
assembly (the manifest's `assemblies`, D-77): every capability it
uses over one kernel registry, so a document registers once per page
and crosses between capabilities by hash without leaving the
instance. A capability the assembly does not link — or every one,
when the assembly does not load — runs alone in its own module
instance. The worker validates the manifest at the seam, fetches each
module demand-driven, verifies its bytes against the pinned hash,
compiles and instantiates it, and degrades per capability (a module
that fails to assemble is `Unavailable` for the capabilities it
would answer; the rest answer). Each `ensure` answers a capability's
ROUTE: the instance it answers in, that module's hash, its index
there, and whether the module links several. Document bytes cross to
it per registration (copied, never kept there: the page owns the
documents, and a reset re-registers from them).

Wasm linear memory never shrinks — eviction lets a module reuse what
it freed, but a peak stays allocated for the instance's life. So the
worker holds a MEMORY CEILING per instance (128 MiB, D-70: twice the
largest working set measured on C-TRAN — service-design holding five
plans' networks, 66 MB; a rider page holds one network, 16–18 MB):
before an op (never a registration), an
instance past it that has answered an op since it was made — and was
not reset in the last minute — is replaced by a fresh instance of the
same compiled module, dropping every linked capability's state
together. The reply says so; the engine voids that instance's
registrations (a generation per instance) and its recovery registers
what the op names again, each document once. Modules are pure, so the
fresh instance answers the same bytes; the guards keep a working set
above the ceiling from looping. The e2e pins both bounds
(`e2e/ui-stop-display.spec.mjs`, "a display left polling stays
bounded"): sixty polls of distinct rt snapshots plateau the rt
registry and the assembly's linear memory, and a forced ceiling
replaces the instance once per op with the same answer. It pins the
one instance too ("the display runs one linked module", and
ride-now's first spec): every capability asked answers in the UI's
assembly, holding the network once.

## External services

Parity holds the bus's answers; it does not say what a host may do
with something OFF the bus. External things are sorted by what
their answer BECOMES, not by how they are fetched (`/rt` and
`/geocode` are the same proxy and different kinds):

| kind | its answer becomes | examples | rule |
|---|---|---|---|
| source | a document on the bus, by hash | the GTFS zip, a street layer, the GTFS-rt feeds | a module registers its bytes; ops read it by hash |
| presentation | pixels — no op reads it | basemap tiles, glyphs, sprites | the map's; nothing else touches it |
| oracle | an input the user could have given by hand | the geocoder | below |

**The oracle test: could the user have done it themselves?** A
geocoded point is a tap on the map; a geocoded name is a label the
rider could have typed. An oracle only makes an input easier to
give, so every op stays deterministic in its inputs and cannot tell
where a point came from. A service whose answer no user could give
by hand — another agency's itineraries, say — is not an oracle: it
is a source (its answer becomes a document an op reads), or it is
not used.

The contract:

- **A host may** ask the oracle with the user's input only (the
  text typed, the point tapped, where the map looks), render the
  answer exactly as given, in the oracle's order, and turn the item
  the user picks into the input the user could have given: a point,
  and the label it was shown with.
- **A host may not** rank, filter, merge, or threshold the answers
  (that is policy), or join them with a document (that is an op).
  When the answers must meet model data — D-28's "rank stops and
  addresses together" — the oracle is PROMOTED: its candidates
  become an input document, like an rt snapshot, and an op joins
  them.
- **Ops may echo an oracle's label, never branch on it.** The
  router repeats a geocoded pin's name in the itinerary (D-37); no
  op decides anything by it.
- **The query policy is the deployment's** (which layers, how far,
  how many): the server's policy file (`server/geocoder.json`,
  `-geocoder-policy`), applied by its proxy, which forwards only the
  user's input from the page. Every host that asks through the path
  asks the same question.
- **Discovery is the manifest's.** `/manifest`'s `services` lists
  each service the deployment offers (`{geocoder: {path, query}}`);
  one it does not list is off, and the frames that need it say so —
  the host never probes.
- **Transport is the deployment's origin**, so a user's queries go
  no further than the service the deployment chose.
- **Durability:** nothing durable keeps a query or an oracle's id
  for a result (Pelias's `gid`) — only the user-equivalent input
  (the point and its label). The oracle's data drifts; a saved trip
  end must not drift with it.
- **Failure:** an oracle only makes input easier; down or slow,
  the user types a stop, taps the map, or shares a location, and
  nothing waits on it.
- **Parity:** caps-run asks an oracle the way a page does (`--ask`
  through the deployment's path) and chains its answer into the
  ops; the answers themselves drift with the oracle's data, so
  sessions record them as reference, never as fixtures.

## Why

One entry per load-bearing choice, headed by the code it governs.
**Holds** is what must stay true; change it only with the user, after
reading **Because**.

### Every page is a generated UI (`web/src/uis/<id>/`, the engine's one-op-caller gates)
- **Holds:** there is no other shell; every page is a generated UI that asks only through the engine. A capability no UI reads is a gap recorded in `docs/units/grade-debt.md`, never a page outside the engine.
- **Because:** one op caller is what lets the gates prove parity and journal every op; a page calling modules directly would be outside both.
- **Rejected:** migrating the original persona-tabbed app onto the engine; keeping it on the gate's allowlist.
- **Decided:** D-69.

### The geocoder is an oracle the host calls (`/geocode`, `services.geocoder`, `ride-now` place search)
- **Holds:** geocoding is an external service, not a document on the bus; place search finds the network's stops and whatever the deployment's geocoder answers (addresses, streets, venues, places); the host follows the oracle contract in "External services" above.
- **Because:** an op's answer depends only on the point, however the point was given (typed, tapped, located), so the geocoder only makes input easier. A gazetteer capability would rebuild a geocoder (interpolation, fuzzy matching, ranking, places) inside a module, and keeping places current is the hard part. The query policy is the deployment's, so every host asks the same question.
- **Cost accepted:** no address search offline or where no geocoder runs; typed addresses reach whoever runs it; answers drift with its data, so nothing durable keeps a query, only the point and the name shown.
- **Rejected:** a gazetteer document on the bus; leaving external answers to each host's judgment.
- **Decided:** D-27, D-36, D-38. **Pinned:** `server/geocode_test.go`, `web/e2e/published-network.spec.mjs` ("ride now searches addresses through the geocoder"), the `--ask` runner gate.

### Live feeds are named in a static file (`live-feeds.json`, `deploymentFeeds`)
- **Holds:** a deployment's GTFS-rt feeds are named in `live-feeds.json` beside the page, not in the manifest's `services`.
- **Because:** the rider pages must run as plain static files with no sync server ([static-deployment.md](static-deployment.md)), so their default feeds cannot depend on a server-built document.
- **Rejected:** `services.live` in the server's manifest.
- **Decided:** D-39.

### The basemap's labels are served locally when installed (`just basemap-assets`, `pickStyle`)
- **Holds:** the glyphs and sprites are fetched once into `web/public/basemap-assets/` (gitignored) and read from there when present, else from protomaps.github.io.
- **Because:** a stop display that loses its uplink would otherwise lose every map label; installs are the user's job, so the repo does not vendor them.
- **Rejected:** vendoring the assets into the repo; keeping the network fetch as a known offline limit.
- **Decided:** D-49. **Pinned:** `web/e2e/basemap-assets.spec.mjs`.
