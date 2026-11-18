# The web (`web/`)

The web layer is Svelte 5 runes + Vite, in TypeScript end to end. It
is a multi-page build. `/` is the picker (`index.html` →
`src/picker/`), with one card per generated UI
([generated-uis.md](generated-uis.md)). Every page is a generated UI.
There is no other shell.

Every page lives at the bundle's root. Every fetch is relative, so a
bundle serves from any path. The relative fetches are:

- the built assets (Vite's `base: "./"`)
- `caps/manifest.json`
- `basemap.pmtiles`
- the deployment's `manifest` and `blobs/`

## Parity

An agent (caps-run), the Go server, and the browser a human uses run
the same ops and get the same bytes. If the browser computes an answer
in TypeScript, the other two hosts cannot give that answer. So `web/`
holds only two kinds of code:

- host glue: I/O, fetch and pagination, IndexedDB, the clock,
  settings, polling, the sync POST, and the bus worker
- render components: props in, events out, and formatting

A join, fallback, threshold, record builder, or geometry rule in
TypeScript is a parity break, however small. Put it in an op instead.

Two rulings follow from this:

- **The live document.** cap-rt answers "what is live" in model
  vocabulary. An rt snapshot plus the network resolve into a kernel
  document, addressed by hash. The document holds the delays per trip
  instance and stop, and the service alerts in force. cap-board (and
  the router) consume it by hash. The board decides which alert
  touches which departure, never the host. The op resolves rt trip
  ids, never a host. Thresholds (the on-time window) are op
  parameters.
- **Geometry is a model answer.** An op answers "what path does this
  trip take today" (diversions, the authored alignment, an itinerary
  leg's ridden segment). `web/` only draws the GeoJSON it receives.

The web gates in `just test` enforce parity:

- No component imports the bus.
- No host code reads raw rt `stop_updates`.
- Nothing clones a document to mutate it. Every change is a
  NetworkEdit through cap-author.
- The engine is the one op caller. Only `src/engine.ts` imports the
  bus (`src/bus.ts`). Only the bus spawns the bus worker. Only the
  worker loads or calls a module.

Each gate's allowlist only shrinks, and all are empty. The determinism
gate also holds the modules pure. No crate's library code uses a
clock, randomness, or the environment (hosts pass time in). So an
answer is a function of its inputs.

File ownership:

- `src/mapview.ts` only shapes GeoJSON. Every line the map draws is an
  answer from the bus (cap-router `itinerary_paths`, cap-directory
  `focus`). It turns `[{lat, lon}]` into `[lon, lat]` and computes no
  geometry.
- Components are render-only: props in, events out. No component
  imports the bus or a UI's host layer (the web gates and the
  generated UIs gates in `just test`).
- `src/engine.ts` is the engine. It is the one caller of a module's
  ops. "The engine (`src/engine.ts`)", below, describes it.
- `src/bus.ts` is the page's side of the bus worker. Only the engine
  imports it. It holds the one Worker, spawned on first use, and its
  calls as promises. It carries messages and decides nothing.
- `src/documents.svelte.ts` is the device's document store. Each
  record is keyed by the hash of its bytes, as the sync store keys its
  blobs. A document's identity on the bus is its module's load answer.
  `current` is a pointer. A record a migration made names the record
  it came from (`produced_by`), and that record stays. IndexedDB
  persists the store. ride-now keeps its watchlist and its device
  records (`recent_places`, `live_feeds`) there. A kind written on
  every use replaces its history (`replace`). `localStorage` keeps
  only presentation preferences (the map's last frame and fix).
  - ride-now reads a stored watchlist through the board's load. One
    the board refuses comes forward through cap-migrate, fetched alone
    beside the page's assembly only then, as a new record.
- `src/plans.svelte.ts` owns the service plans. These are the authored
  network documents that service-design keeps on the device: plain
  data, wholesale-replaced, persisted in IndexedDB. Stored plans
  restore through a module. `restorePlans(readStoredSet)` reads the set
  through cap-network (`stored_plans`): its plans, or a refusal of
  another kind or a newer version, and then the page never writes over
  it. `restorePlans(readStoredSet, readStoredNetwork)` also reads each
  plan through cap-author (an `apply` with no edits answers the
  canonical document). So a plan saved under older field names comes
  back in the current shape. `readPlan(id, readStoredNetwork)` reads
  one plan on first use. When a migration brought a plan forward, the
  set as stored is kept first in the document store (`plan_set`). The
  host never sniffs a shape.
- `src/basemap.ts` is the basemap choice that every map view shares
  (local pmtiles, else OpenFreeMap). It also holds the MapLibre worker
  wiring. It probes the archive by reading its header with the one
  `PMTiles` instance that the protocol then serves every tile from.
  There is no separate probe request, and the page fetches the header
  once. The local style's glyphs and sprites are `basemap-assets/`
  beside the tiles, when `just basemap-assets` installed them. The
  code probes for them and never configures them. Otherwise the style
  uses protomaps.github.io. `pickStyle(near)` returns the view the map
  opens on. Each map component owns its map (`window.__map` is the e2e
  hook).
- `src/viewport.ts` decides where a map's camera starts and when it
  frames the whole network. These are pure presentation rules:
  - `startView` chooses the rider's last fix where tiles cover it.
    Otherwise it chooses the view the network was last framed on.
    Otherwise it chooses the archive's view.
  - `shouldFrameNetwork` is true only when nothing better is known:
    no fix is pending, no fix arrived, and the camera did not open
    inside the network.
  - `sameView` stops the code from moving a camera that is already on
    a view.

  The purpose is that no camera move precedes the move the rider ends
  on. Each such move fetches tiles that are thrown away. The last fix
  and the last network frame are the rider app's own device data
  (`ride-now:last-fix`, `ride-now:network-view`). Its host layer keeps
  them in `localStorage` and passes them to RideMap as `start` and
  `framed`. RideMap reports a frame it gave as `onframed` (D-90).

### The engine (`src/engine.ts`)

A host asks `ask(capability, op, args)`. The request types are
generated. Every `*_hash` argument takes a `Doc`. A `Doc` is a
document version: its kind, its value or bytes, the document it is
read over, and the hash its server pins.

A generated UI's host names its page first (`assemble("<id>")`).
Every capability that its linked assembly links answers in that one
module instance (the bus worker, below). A capability that the
assembly does not link answers alone.

**Registration and answers.** These functions register documents and
return answers:

- The engine registers a `Doc` in a module instance on first use. It
  uses the kernel's load of the document's kind. `load` answers the
  load's whole answer (a feed's summary, a layer's headcount). So a
  document registers once per page, however many capabilities read
  it.
- The engine asserts that every instance answers the same identity.
- The engine caches answers under the journal's key. The key is the
  module's pinned hash (the assembly's hash) and the request's bytes.
  In a linked module, the request's bytes name the capability.
- `produced` adopts a document that an op answered (an import, an
  `apply`, the live document) as registered where it was made. It
  does this for every capability linked there.
- A page can decline to receive a document. The stop display's live
  document is an example (`with_document: false`). Such a document
  carries the ask that mints it again (`remint`).
- `together` says whether capabilities share an instance, so a host
  can decline a document.
- `available` says whether a capability assembled. `ask` throws
  `Unavailable` when the capability did not assemble.

**Served documents.** These functions read the deployment's published
documents:

- `servedNetwork`, `servedStreets`, and `servedWalks` read the
  deployment's published documents. They refuse a document that does
  not answer its pinned hash. `servedNetwork` does this in the first
  of the page's named readers that assembles, so no module holds a
  copy only to check it.
- `servedNetwork` takes the page's local date. When the manifest pins
  a published plan set (`plans_hash`), it asks the plans module which
  pick is in force (`in_force`). It then loads that pick's network by
  hash (`blobs/<network_hash>.gz`) with the default boundary.
- Before the first pick takes effect, `servedNetwork` loads the
  import by the manifest's pins (`blobs/<network_hash>.gz`,
  `blobs/<boundary_hash>`). One manifest read names both pins, so the
  boundary always belongs to the network.
- Every pin is the manifest's, so a static host serves the same
  ([static-deployment.md](static-deployment.md)).
- `servedNetwork` throws `Unavailable` when picks are published and
  the plans module does not assemble. A page never guesses which
  schedule runs.
- In either case, `servedNetwork` holds the network as the gzipped
  bytes that the server sends. The page never unzips, parses, or
  re-encodes it. Every module that it registers in decodes the stream
  itself. A heading asks the directory's `about` for the network's
  name.
- `servedNetworkValue` is the planner's function. It returns the
  network as a value that the planner forks.
- `publishedPlanSet` and `publishedPickValue` are the planner's
  timeline. They return every published pick, and one pick's network
  as a value. The page fetches them gzipped and decompresses them (a
  value is parsed in either case) (D-139).
- `servedStops` returns the deployment's stops document. It is the
  manifest's `stops_hash`, and the page never parses the bytes
  (D-153).
- `stopWarnings` returns the warnings of the stops that a plan
  visits and that are not in service over its period. It uses cap-plans
  `periods`, then cap-stops `visit_warnings`.
- `servedFeed` returns the agency's feed as the deployment imported
  it. It is the manifest's `network_hash`, with `feed_hash` as its
  id. The stops page uses it to show the feed's recorded changes
  (D-150).
- `withStopVersions` returns a plan with the stops' history over its
  period. It uses `export_versions`. Each stop's record is on its pick
  date. A stop whose record changes inside the period becomes one stop
  per version. An export carries this, as the server's `GET /feed`
  does (D-151, D-157).

**Dropped documents.** A module may drop what it holds (see "The bus
worker", below). Asking for a dropped document gives the module's
unknown-hash error. The engine then registers the document again (or
mints it again) and asks once more. A host never sees this.

`keep` names what survives. Every module instance drops every
document that `keep` does not name, or that a named document is not
read over. The rider pages call `keep` when they adopt a network and
when the edit document changes. service-design calls it after every
change it adopts: the plan set's current networks and the layer. Each
`apply` mints a network, and undo registers a dropped version again.

`window.__bus` is the e2e's inspection hook. It shows:

- the module instance that a capability answers in
- what the instance holds
- its memory and resets
- the ceiling
- a replay of the last op asked of a capability, past the answer
  cache

### The journal

A page can journal every module call in the runner's format
([registry.md](registry.md), "The journal"; `tools/journal.mjs`). So a
page's session replays through `caps-run`.

- `?journal` in the page's URL turns it on (`src/uis/kit.ts`). The
  journal is off by default, because it keeps the bytes of every
  request: each document the page registers and each rt snapshot.
- The bus worker records each call when the module answers. A line
  names the page's run, the instance (the module and its resets), the
  capability, the module hash (the page's linked assembly), the op, and
  the hashes of the request and the outcome. A refusal that the engine
  recovers from (a dropped document) is not recorded.
- `transitJournal()` (the console, or a test) gives the journal as one
  file: `{journal, blobs}`, with each request's bytes in base64 by
  hash. `caps-run replay FILE.json` replays it.
- Replay runs the page's journal on the page's own linked assembly
  (from `web/public/caps`), so every call answers the same. With
  `--single`, it runs each capability on its own module. A document
  that stayed in the page's assembly (the stop display's live
  document) then cannot cross, and replay says which calls it cannot
  repeat.

On C-TRAN, the stop display at Fisher's Landing TC journals 8 calls
and 0.7 MB of requests (the network crosses as the server's gzipped
bytes), and replays in under a second. `e2e/journal.spec.mjs` pins
both replays on the fixture.

## The bus worker

`src/bus.worker.ts` is the only wasm loader. Every module that a page
assembles lives in one module Worker, off the thread that the page
renders on.

A generated UI's page runs one module instance: its linked assembly
(the manifest's `assemblies`). The assembly holds every capability
that the page uses, over one kernel registry. So a document registers
once per page. It crosses between capabilities by hash without
leaving the instance (D-77). A capability that the assembly does not
link runs alone in its own module instance. When the assembly does not
load, every capability runs alone in its own module instance.

The worker does these steps:

- It validates the manifest at the seam.
- It fetches each module on demand.
- It verifies the module's bytes against the pinned hash.
- It compiles and instantiates the module.
- It degrades per capability. A module that fails to assemble is
  `Unavailable` for the capabilities it would answer. The other
  capabilities answer.

Each `ensure` answers a capability's route. The route holds:

- the instance it answers in
- that module's hash
- its index there
- whether the module links several capabilities

Document bytes cross to the worker per registration. The worker
copies them and never keeps them. The page owns the documents, and a
reset registers them again from the page.

Wasm linear memory never shrinks. Eviction lets a module reuse what it
freed, but a peak stays allocated for the instance's life. So the
worker holds a memory ceiling per instance: 128 MiB. This is twice the
largest working set measured on C-TRAN. service-design holding five
plans' networks is 66 MB. A rider page holds one network, 16–18 MB
(D-70).

The worker applies the ceiling before an op, never before a
registration. It replaces an instance when all of these are true:

- The instance is past the ceiling.
- The instance has answered an op since it was made.
- The instance was not reset in the last minute.

The fresh instance is of the same compiled module. It drops the state
of every linked capability together. The reply says that this
happened. The engine voids that instance's registrations (a generation
per instance). Its recovery registers again what the op names, each
document once. Modules are pure, so the fresh instance answers the
same bytes. The guards stop a working set above the ceiling from
looping.

The e2e pins both bounds (`e2e/ui-stop-display.spec.mjs`, "a display
left polling stays bounded"):

- Sixty polls of distinct rt snapshots plateau the rt registry and the
  assembly's linear memory.
- A forced ceiling replaces the instance once per op, with the same
  answer.

The e2e also pins the one instance ("the display runs one linked
module", and ride-now's first spec). Every capability asked answers in
the UI's assembly, which holds the network once.

## Signing in

A page reads without signing in (D-58). A write carries the signed-in
person's token. `src/signin.svelte.ts` is the pages' one sign-in. The
pages are a public single-page client of the agency's provider. The
manifest's `auth` names the issuer, the client id, and the scopes (the
server's `-oidc-client-id`).

Sign-in uses the authorization code flow with PKCE:

- The page reads the provider's discovery document.
- The page goes out to the provider's authorize endpoint and back to
  the same page and hash.
- The page exchanges the code at the provider's token endpoint.
- The tab's session storage keeps the tokens. The page refreshes them
  with the refresh token.

A read that needs the signed-in person (an attachment) goes through
`readFetch` (D-162). Every page write goes through `writeFetch`
(`POST /stops`, `/stop-requests`, `/plans`). A 401 says "sign in" in a
person's words. A survey's draft waits on the device meanwhile.
`SignInControl` shows who is signed in where the deployment asks for
it: the stops page's header, and beside Publish on the planner pages.

Nothing here decides who may write. The server's policy decides. The
person that a record names is the server's reading of the token
(D-163).

## External services

Parity holds the bus's answers. It does not say what a host may do
with something off the bus. External things are sorted by what their
answer becomes, not by how they are fetched. `/rt` and `/geocode` use
the same proxy and are different kinds.

| kind | its answer becomes | examples | rule |
|---|---|---|---|
| source | a document on the bus, by hash | the GTFS zip, a street layer, the GTFS-rt feeds | a module registers its bytes; ops read it by hash |
| presentation | pixels (no op reads it) | basemap tiles, glyphs, sprites | the map's; nothing else touches it |
| oracle | an input the user could have given by hand | the geocoder | below |

**The oracle test: could the user have done it themselves?** A
geocoded point is a tap on the map. A geocoded name is a label that
the rider could have typed. An oracle only makes an input easier to
give. So every op stays deterministic in its inputs and cannot tell
where a point came from.

A service whose answer no user could give by hand is not an oracle.
Another agency's itineraries are an example. Such a service is a
source (its answer becomes a document that an op reads), or it is not
used.

The contract:

- **A host may** ask the oracle with the user's input only: the text
  typed, the point tapped, or where the map looks. The host renders
  the answer exactly as given, in the oracle's order. It turns the
  item that the user picks into the input that the user could have
  given: a point, and the label it was shown with.
- **A host may not** rank, filter, merge, or threshold the answers
  (that is policy). It may not join them with a document (that is an
  op). When the answers must meet model data, the oracle is promoted.
  An example is D-28's "rank stops and addresses together". The
  oracle's candidates then become an input document, like an rt
  snapshot, and an op joins them.
- **Ops may echo an oracle's label, never branch on it.** The router
  repeats a geocoded pin's name in the itinerary. No op decides
  anything by the name (D-37).
- **The query policy is the deployment's** (which layers, how far, how
  many). The policy is the server's policy file
  (`server/geocoder.json`, `-geocoder-policy`). The server's proxy
  applies it and forwards only the user's input from the page. Every
  host that asks through the path asks the same question.
- **Discovery is the manifest's.** `/manifest`'s `services` lists each
  service that the deployment offers (`{geocoder: {path, query}}`). A
  service that it does not list is off. The frames that need it say
  so. The host never probes.
- **Transport is the deployment's origin.** So a user's queries go no
  further than the service that the deployment chose.
- **Durability:** nothing durable keeps a query or an oracle's id for
  a result (Pelias's `gid`). Only the user-equivalent input stays: the
  point and its label. The oracle's data drifts, and a saved trip end
  must not drift with it.
- **Failure:** an oracle only makes input easier. If it is down or
  slow, the user types a stop, taps the map, or shares a location.
  Nothing waits on the oracle.
- **Parity:** caps-run asks an oracle the way a page does (`--ask`
  through the deployment's path). It chains the answer into the ops.
  The answers drift with the oracle's data. So sessions record them as
  reference, never as fixtures.

## Why

Each entry covers one choice that the design depends on. The entry's
heading names the code that it governs. **Holds** is what must stay
true. Change it only with the user, after you read **Because**.

### Every page is a generated UI (`web/src/uis/<id>/`, the engine's one-op-caller gates)
- **Holds:** there is no other shell. Every page is a generated UI that asks only through the engine. A capability that no UI reads is a gap recorded in `docs/units/grade-debt.md`, never a page outside the engine.
- **Because:** one op caller lets the gates prove parity and journal every op. A page that calls modules directly is outside both.
- **Rejected:** migrating the original persona-tabbed app onto the engine; keeping it on the gate's allowlist.
- **Decided:** D-69.

### The geocoder is an oracle the host calls (`/geocode`, `services.geocoder`, `ride-now` place search)
- **Holds:** geocoding is an external service, not a document on the bus. Place search finds the network's stops and whatever the deployment's geocoder answers (addresses, streets, venues, places). The host follows the oracle contract in "External services" above.
- **Because:** an op's answer depends only on the point, however the point was given (typed, tapped, located). So the geocoder only makes input easier. A gazetteer capability would rebuild a geocoder (interpolation, fuzzy matching, ranking, places) inside a module, and keeping places current is the hard part. The query policy is the deployment's, so every host asks the same question.
- **Cost accepted:** no address search offline or where no geocoder runs. Typed addresses reach whoever runs the geocoder. Answers drift with its data, so nothing durable keeps a query, only the point and the name shown.
- **Rejected:** a gazetteer document on the bus; leaving external answers to each host's judgment.
- **Decided:** D-27, D-36, D-38. **Pinned:** `server/geocode_test.go`, `web/e2e/published-network.spec.mjs` ("ride now searches addresses through the geocoder"), the `--ask` runner gate.

### Live feeds are named in a static file (`live-feeds.json`, `deploymentFeeds`)
- **Holds:** a deployment's GTFS-rt feeds are named in `live-feeds.json` beside the page, not in the manifest's `services`.
- **Because:** the rider pages must run as plain static files with no sync server ([static-deployment.md](static-deployment.md)). So their default feeds cannot depend on a server-built document.
- **Rejected:** `services.live` in the server's manifest.
- **Decided:** D-39.

### The pages use only widely available web features (the live-feeds panel in `LiveFeeds.svelte`)
- **Holds:** a page uses only features that are Baseline widely available: 30 months in every major browser. A newer feature waits until it has been available that long. The pages ship no polyfill. The live-feeds panel is a plain element with its own dialog habits: opening moves focus in, Escape closes it and returns focus to the pill, and a press outside it closes it. About 30 lines.
- **Because:** the rider pages run on riders' own phones, and many are old. A polyfill is one more dependency to keep, and a fallback is a second code path that no one tests on the device that needs it. The Popover API does the same three things natively. It is Baseline since 2025-01-27, so it is newly available until mid-2027.
- **Rejected:** Popover with a small fallback; Popover with a polyfill (`@oddbird/popover-polyfill`); a named browser floor.
- **Decided:** D-135.

### A UI's flows are code, run by the answer graph (`src/uis/answers.ts`, `answerGraph` in `src/uis/kit.ts`, each `host.svelte.ts`)
- **Holds:** a host declares its flows as `AnswerNode`s in TypeScript. The answer graph schedules them, and knows nothing about transit. A UI's layout is its `Shell.svelte`. Both ship with the UI's build. There is no flow or layout document.
- **Because:** a node's body builds op arguments from host state. As data, that needs an expression language that the host interprets. That moves the host's code into a JSON dialect, and takes no logic out of the host. A layout document would put product code into the agency's record. It would then need versions, migrations, and a receive path, and a second rollback beside the UI's own.
- **Rejected:** flow maps as data in a UI manifest, interpreted by the engine; layouts as documents over a component catalog.
- **Decided:** D-176.

### A failure belongs to its frame (`FrameState`, each host's `frames`)
- **Holds:** the answer graph keeps why each frame has no answer: unassembled, starved, or failed. A host declares a node's capabilities and what it clears, and keeps no failure state of its own. Every other frame answers.
- **Because:** each host kept failures by hand, two ways: a `framed` wrapper and `failed[frame]` in the planner hosts, and per-capability `off` flags and error fields in the rider hosts. Each node repeated the same checks. A capability that did not load silently emptied some frames and failed others.
- **Rejected:** one failure box for every UI. The rider components say an outage in their own words (`off`, `error`), so the rider shells map the state onto those props.
- **Pinned:** `web/src/uis/answers.test.mjs`.

### The basemap's labels are served locally when installed (`just basemap-assets`, `pickStyle`)
- **Holds:** the glyphs and sprites are fetched once into `web/public/basemap-assets/` (gitignored). They are read from there when present, else from protomaps.github.io.
- **Because:** a stop display that loses its uplink would otherwise lose every map label. Installs are the user's job, so the repo does not vendor them.
- **Rejected:** vendoring the assets into the repo; keeping the network fetch as a known offline limit.
- **Decided:** D-49. **Pinned:** `web/e2e/basemap-assets.spec.mjs`.
