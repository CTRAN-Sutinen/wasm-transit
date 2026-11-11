# Updates and offline

A page that reads the deployment's network follows it, and a page
whose `ui.json` says `"offline": true` (ride-now and the stop display)
installs as an app — one lifecycle (D-63, D-65):

- **Data re-pins without a reload.** On every poll the host asks the
  engine which network the deployment serves today
  (`servedNetworkPin`: the manifest, revalidated by its ETag, and the
  pick in force on the agency's date). A republish, a new pick, or
  the date crossing a pick's effective date moves it, and the host
  adopts the new network quietly (the page stays on screen) and names
  its keep-set, so the old network leaves every module. A zip loaded
  by hand stops the page following.
- **A build waits, then applies.** The build writes the service
  worker (`web/service-worker.js` → `dist/sw.js`, `vite.config.js`'s
  plugin) with the build's identity (a hash over its shell) and the
  shell it precaches: the offline UIs' pages, every hashed asset,
  `caps/manifest.json`, and those UIs' linked assemblies. A new build
  is a byte-different worker; each poll asks the browser to look for
  it (kit's `buildWaiting`), and it installs in the background and
  waits. A rider's page applies it at the next cold start or
  navigation (`installApp`, before anything on the page is unsaved)
  or when the rider accepts "a new version is ready"; a display
  applies it itself at a poll — only while the server answers
  (`serverAnswers`: not the worker's last copy) and its setup is
  closed. A page another tab moved past counts as one with a build
  waiting. The worker keeps the previous build's shell beside the
  current one, so a page still on it loads its files.
- **Offline.** The worker answers the shell from its precache;
  content-addressed files (hashed assets, modules, `blobs/<hash>` and
  `blobs/<hash>.gz`) from a cache filled on first use, forever; the
  published documents and config (`manifest`, `network/document`,
  `network/boundary`, `streets`, `walks`, `live-feeds.json`, the
  basemap's glyphs and sprites) from the network, else the last copy (marked
  `X-Served-From: last-copy`). Live feeds, `/rt`, `/edits`, writes,
  and the geocoder are never cached: offline, a board is the schedule
  and says so. The worker reads every path relative to its scope, so a
  bundle served under a prefix caches as one at the root. The map's tiles (`basemap.pmtiles`, range requests)
  are not cached.

The e2e runs every other spec with service workers blocked
(`page.route` never sees what a worker answers);
`e2e/deploy-builds.spec.mjs` allows them on its own server and
publishes a new build by rewriting that server's copy of `sw.js`.

Layouts are hand-placed Svelte; layouts as documents are
`docs/units/wire-time-assembly.md`.

Frontend conventions and gotchas: `docs/CONVENTIONS.md`.

## Why

One entry per load-bearing choice, headed by the code it governs.
**Holds** is what must stay true; change it only with the user, after
reading **Because**.

### A new build waits, then applies (`sw.js`, `buildWaiting`, `installApp`, `applyBuild`, `serverAnswers`)
- **Holds:** a new build downloads in the background and waits. A rider's page applies it at the next cold start or navigation, or when the rider accepts; a display applies it itself at a poll, only while the deployment answers and its setup is closed; nothing applies over unsaved device data. A new network re-pins with no reload.
- **Because:** these are web apps with real update semantics: a display stays open for weeks and a rider's tab for days, so waiting for a person to reload leaves an old page meeting documents it may not read. Applying only while the deployment answers means an outage never reloads a working display into an empty one.
- **Rejected:** reloading as soon as new code appears (reloads under the rider); waiting for a manual reload.
- **Decided:** D-63.

### Offline at launch (`"offline": true` in `ui.json`, the service worker's caches)
- **Holds:** ride-now and the stop display install and boot offline: the shell and modules precached, content-addressed files kept forever, the manifest and config kept as the last copy; a board offline answers from the schedule and says so. Live feeds and the edit document are never cached. The worker's scope is the deployment's root, so it controls every page on the origin, yet it answers only its offline pages from the shell: any other page's capability manifest (the bus worker asks `caps/manifest.json?ui=<id>`) comes from the network, since the shell's copy names one build's modules and a planner page that read it after the next build would ask for modules that are gone.
- **Because:** a rider at a stop with no signal is the case local-first is for. Content-addressed files can be cached forever without going stale; a cached prediction replayed as live would be wrong.
- **Rejected:** the HTTP cache only, offline later.
- **Decided:** D-65.

### Budgets from what the page needs (D-68 numbers; `just perf`, the cost ratchet)
- **Holds:** the stop display at most 40 MB of wasm + JS heap, flat across 24 h, on a Raspberry Pi 4; ride-now at most 80 MB, first board under 3 s cold and 1 s cached, no main-thread task over 50 ms after first paint, no polling while hidden. Ride-now has budgets, not a device floor. A deployment that follows a GTFS URL is exempt (D-132).
- **Because:** budgets come from what the page needs, never the hardware's headroom ("why would we be allowing a departure board to tax 2 GB of RAM"); most phones handle a properly optimized page, so ride-now targets browsers rather than a minimum device.
- **Rejected:** a named minimum phone and a looser page budget.
- **Decided:** D-64, D-68.
