# Updates and offline

A page that reads the deployment's network follows it. A page whose
`ui.json` says `"offline": true` installs as an app. Ride-now and the
stop display are such pages. They share one lifecycle (D-63, D-65):

- **Data re-pins without a reload.** On every poll, the host asks the
  engine which network the deployment serves today. The engine answers
  with `servedNetworkPin`. It reads the manifest, revalidated by its
  ETag, and the pick in force on the agency's date. A republish moves
  the pin. So does a new pick, or the date crossing a pick's effective
  date. The host adopts the new network quietly, and the page stays on
  screen. The host names its keep-set, so the old network leaves every
  module. A zip loaded by hand stops the page following.
- **A build waits, then applies.** The build writes the service worker
  (`web/service-worker.js` → `dist/sw.js`, by `vite.config.js`'s plugin).
  The worker carries the build's identity, a hash over its shell. It
  precaches the shell:
  - the offline UIs' pages,
  - every hashed asset,
  - `caps/manifest.json`, and
  - those UIs' linked assemblies.

  A new build is a byte-different worker. Each poll asks the browser to
  look for it (kit's `buildWaiting`). The new worker installs in the
  background and waits.

  A rider's page applies it at the next cold start or navigation
  (`installApp`, before anything on the page is unsaved). It also
  applies it when the rider accepts "a new version is ready."

  A display applies it itself at a poll. It does so only while the
  server answers (`serverAnswers`: not the worker's last copy) and its
  setup is closed.

  A page another tab moved past counts as one with a build waiting. The
  worker keeps the previous build's shell beside the current one. So a
  page still on it loads its files.
- **Offline.** The worker answers the shell from its precache. It
  answers content-addressed files from a cache filled on first use, and
  it keeps them forever. These files are hashed assets, modules,
  `blobs/<hash>`, and `blobs/<hash>.gz`. It answers the published
  documents and config from the network. If the network fails, it
  answers with the last copy, marked `X-Served-From: last-copy`. These
  documents are `manifest`, `network/document`, `network/boundary`,
  `streets`, `walks`, `live-feeds.json`, and the basemap's glyphs and
  sprites.

  Live feeds, `/rt`, `/edits`, writes, and the geocoder are never
  cached. Offline, a board is the schedule, and it says so.

  The worker reads every path relative to its scope. So a bundle served
  under a prefix caches as one at the root. The map's tiles
  (`basemap.pmtiles`, range requests) are not cached.

The e2e runs every other spec with service workers blocked. `page.route`
never sees what a worker answers. `e2e/deploy-builds.spec.mjs` allows
workers on its own server. It publishes a new build by rewriting that
server's copy of `sw.js`.

Layouts are hand-placed Svelte. Layouts as documents are open work:
`docs/units/wire-time-assembly.md`.

Frontend conventions and gotchas: `docs/CONVENTIONS.md`.

## Why

Each entry covers one choice that the design depends on. Its heading
names the code the choice governs. **Holds** is what must stay true.
Change it only with the user, and only after you read **Because**.

### A new build waits, then applies (`sw.js`, `buildWaiting`, `installApp`, `applyBuild`, `serverAnswers`)
- **Holds:** A new build downloads in the background and waits. A
  rider's page applies it at the next cold start or navigation, or when
  the rider accepts. A display applies it itself at a poll. It does so
  only while the deployment answers and its setup is closed. Nothing
  applies over unsaved device data. A new network re-pins with no
  reload.
- **Because:** These are web apps with real update semantics. A display
  stays open for weeks. A rider's tab stays open for days. If the page
  waits for a person to reload, an old page can meet documents it may
  not read. Applying only while the deployment answers means an outage
  never reloads a working display into an empty one.
- **Rejected:** reloading as soon as new code appears (reloads under the
  rider); waiting for a manual reload.
- **Decided:** D-63.

### Offline at launch (`"offline": true` in `ui.json`, the service worker's caches)
- **Holds:** Ride-now and the stop display install and boot offline. The
  shell and modules are precached. Content-addressed files are kept
  forever. The manifest and config are kept as the last copy. Offline, a
  board answers from the schedule and says so. Live feeds and the edit
  document are never cached.

  The worker's scope is the deployment's root. So it controls every page
  on the origin. Yet it answers only its offline pages from the shell.
  Any other page's capability manifest comes from the network. The bus
  worker asks for `caps/manifest.json?ui=<id>`. The shell's copy names
  one build's modules. A planner page that read it after the next build
  would ask for modules that are gone.
- **Because:** A rider at a stop with no signal is the case that
  local-first serves. Content-addressed files can be cached forever.
  They do not go stale. A cached prediction replayed as live would be
  wrong.
- **Rejected:** the HTTP cache only, offline later.
- **Decided:** D-65.

### Budgets from what the page needs (D-68 numbers; `just perf`, the cost ratchet)
- **Holds:** The stop display uses at most 40 MB of wasm and JS heap.
  The heap stays flat across 24 h, on a Raspberry Pi 4. Ride-now uses at
  most 80 MB. Its first board appears under 3 s cold and under 1 s
  cached. No main-thread task runs over 50 ms after first paint. The
  page does not poll while hidden. Ride-now has budgets, not a device
  floor. A deployment that follows a GTFS URL is exempt (D-132).
- **Because:** Budgets come from what the page needs. They never come
  from the hardware's headroom: "why would we be allowing a departure
  board to tax 2 GB of RAM". Most phones handle a properly optimized
  page. So ride-now targets browsers, not a minimum device.
- **Rejected:** a named minimum phone and a looser page budget.
- **Decided:** D-64, D-68.
