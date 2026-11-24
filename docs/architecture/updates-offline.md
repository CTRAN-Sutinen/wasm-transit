# Updates and offline

A page that reads the deployment's network follows it. A page whose
`ui.json` says `"offline": true` installs as an app. Every generated UI
is such a page: the rider pages (ride-now, the stop display) and the
staff pages (service design, service change, stops, disruptions; D-189).
They share one lifecycle (D-63, D-65):

- **Data re-pins without a reload.** The page's network is one of its
  inputs ([generated-uis.md](generated-uis.md), kit's "The inputs"),
  read after the manifest input on every poll. Its reader asks the
  engine which network the deployment serves today
  (`servedNetworkPin`), over the manifest the page holds (revalidated
  by its ETag) and the pick in force on the agency's date. A republish
  moves the pin. So does a new pick, or the date crossing a pick's
  effective date. Only a moved pin reads the network again. The host
  adopts the new network quietly, and the page stays on screen. The
  host names its keep-set, so the old network leaves every module. A
  zip loaded by hand stops the page following.
- **A build waits, then applies.** The build writes the service worker
  (`web/service-worker.js` → `dist/sw.js`, by `vite.config.js`'s plugin).
  The worker carries the build's identity, a hash over its shell. It
  precaches the shell:
  - the offline UIs' pages,
  - every hashed asset,
  - `caps/manifest.json`, and
  - the rider UIs' linked assemblies. A UI whose `ui.json` says
    `"precache_assembly": false` (each staff page, D-223) keeps its
    assembly out of the shared shell, so a rider's install never
    downloads a planner's modules. The worker keeps that assembly on
    its first use, by its hash. A UI built alone (`just build-ui`)
    precaches its own assembly.

  A new build is a byte-different worker. Each poll asks the browser to
  look for it (kit's `buildWaiting`). The new worker installs in the
  background and waits.

  A rider's page applies it at the next cold start or navigation
  (`installApp`, before anything on the page is unsaved). It also
  applies it when the rider accepts "a new version is ready."

  A staff page applies it the same way, and shows "A new version is
  ready" as soon as the build has downloaded (kit's `onBuildReady`).
  The person applies it with Reload, which waits while a form or a task
  is open. The outbox is on the device, so a reload keeps every write
  that waits.

  A display applies it itself at a poll. It does so only while the
  server answers (its manifest input's last read answered, not from the
  worker's last copy) and its setup is closed. It stops its inputs'
  reads first: the old build's worker hands over only when no request
  is in flight.

  A document another build wrote also makes the page look at once
  (`web/src/versions.ts`, D-79). A module refuses a version newer than
  it reads, or the server refuses a write over a copy it reads ahead
  read-only. The engine and `writeFetch` pass every such refusal on,
  and `installApp` asks the browser for the deployment's worker. The
  page then applies the build by its rule above.

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
  cached. Offline, a board is the schedule, and it says so. `network`
  (the planner pages' `GET /network`) is kept as the last copy too.

  A staff page says when the server last confirmed its documents. The
  engine's `readManifest` tells the manifest input whether the server
  answered, and the time of its answer (the `Date` the worker kept).
  The page shows "Offline · data as of 4:05 PM" while it works from the
  last copy. After a write is sent, the page reads the manifest and the
  documents it pins at once, so the banner follows the server's
  answer. The disruptions page keeps its own last good edit document
  (kit's `readHeadKept`, in IndexedDB), and labels it with the time the
  server last confirmed it. A page never shows an old answer as
  current.
- **The outbox (D-189).** A staff page's write that cannot reach the
  server waits on the device, in order, and is sent when the server
  answers. The server stays the authority on writes.
  - `src/outbox.ts` is the core, and its laws are `outbox.test.mjs`.
    `src/outbox.svelte.ts` keeps the list in IndexedDB (`outbox/<ui>`),
    and sends through `writeFetch`.
  - A write is made when the person saves, with an idempotency key.
    Every attempt sends the same key. A log's write is its intent,
    `{key, ops, read_seq}` to `POST /logs/<log>` (stops, alerts, notes;
    [storage.md](storage.md), "The intent write"). A publish
    (`POST /plans`) is its document, run at send time against the plan
    set as it is then. A work order request carries its key. A photo is
    named by the hash of its bytes, waits as its bytes, and is queued
    before the survey that names it, so it is sent first.
  - A write that waited carries `claimed_at_ms`, the device's time it
    was made (D-222).
  - A network failure or a `5xx` keeps the write waiting, and the
    writes after it wait behind it. A `401` holds the outbox until the
    person signs in, and fails nothing. A token refresh that cannot
    reach the provider keeps the person signed in. A refusal (`400`,
    `403`, `409`, `422`) keeps the write, held with the server's
    reasons in words. Only the person sends it again or discards it.
  - The page sends what waits when it saves, at each poll, every 15 s
    while something waits, and when the device comes back online.
  - The page shows how many writes wait, the held writes with their
    reasons, and whether the server answers (the `SyncStatus`
    component). A write the server refuses at once stays in its form
    with the reasons, and does not stay in the outbox.
  - A write the server kept whose answer names `references` (the
    references it left broken, D-233) shows "Saved with a warning", each
    reference in words ("Stop request R-1 names stop 6124, which isn't
    on record."), until the person dismisses it. The warning lives in
    the page only: a reload drops it, and `store verify` still lists
    the reference.

  The worker reads every path relative to its scope. So a bundle served
  under a prefix caches as one at the root. The map's tiles
  (`basemap.pmtiles`, range requests) are not cached.

The e2e runs every other spec with service workers blocked. `page.route`
never sees what a worker answers. `e2e/deploy-builds.spec.mjs` allows
workers on its own server. It publishes a new build by rewriting that
server's copy of `sw.js`. `e2e/offline-staff.spec.mjs` allows them on a
store-mode server of its own, and takes the browser's network away for
an outage.

Layouts are hand-placed Svelte, and ship with each UI's build (D-176).

Frontend conventions and gotchas: `docs/CONVENTIONS.md`.

## Why

Each entry covers one choice that the design depends on. Its heading
names the code the choice governs. **Holds** is what must stay true.
Change it only with the user, and only after you read **Because**.

### A new build waits, then applies (`sw.js`, `buildWaiting`, `installApp`, `applyBuild`)
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
  A page that is not an offline page reads its capability manifest from
  the network. The bus worker asks for `caps/manifest.json?ui=<id>`. The
  shell's copy names one build's modules. A page outside the shell that
  read it after the next build would ask for modules that are gone.
- **Because:** A rider at a stop with no signal is the case that
  local-first serves. Content-addressed files can be cached forever.
  They do not go stale. A cached prediction replayed as live would be
  wrong.
- **Rejected:** the HTTP cache only, offline later.
- **Decided:** D-65.

### Staff pages work offline and queue their writes (`"offline": true`, `src/outbox.ts`, `POST /logs/<log>`)
- **Holds:** Every staff page installs and boots offline like a rider
  page. Between the moments it needs the server, the server being away
  does not stop the work. A write that cannot reach the server waits on
  the device and is sent through the same receive path when it answers.
  Nothing the person saved is lost or written twice, and nothing is
  dropped without a word.
- **Because:** "There is a difference between needing a server
  occasionally and needing a server always." A planner, a dispatcher,
  or a crew in the field keeps working through a short outage. The
  server is still the authority: the ops run again over what it holds,
  and the key makes a resend land once.
- **Rejected:** online only; offline with a write that fails while the
  server is away.
- **Decided:** D-189, D-222.

### A staff page's assembly is cached on first use (`"precache_assembly": false`)
- **Holds:** In a build of every UI, the shell precaches the rider
  pages' assemblies only. A staff page's assembly is kept by the worker
  the first time the page loads it.
- **Because:** Every install downloads the shell. A rider's phone must
  not download about 70 MB of planner modules. A staff page is opened
  online at least once before it is used offline.
- **Cost accepted:** a staff page never opened online does not boot
  offline. A person who applies a new build and loses the network
  before the page loads its new assembly waits for the network.
- **Rejected:** precaching every offline UI's assembly.
- **Decided:** D-223 (provisional).

### Costs are measured, not budgeted (`just perf`)
- **Holds:** No page has a fixed memory or time budget. `just perf`
  measures what each step costs, and a person weighs a change's cost
  against what it adds. Memory that does not level off while a page
  stays open is wrong for every deployment. The stop display runs on a
  Raspberry Pi 4. Ride-now targets browsers, with no minimum device.
  The page does not poll while hidden.
- **Because:** Deployments and features change. A fixed number would
  fail a feature that is worth its cost on one deployment and not on
  another. What is always wrong is use that is out of control. Most
  phones handle a properly optimized page.
- **Rejected:** fixed budgets for each page (40 MB for the display,
  80 MB for ride-now); a ratchet whose ceilings only come down; a named
  minimum phone.
- **Decided:** D-64, D-187.
