# Deploying the rider apps

Ride-now on riders' phones and the stop display on Raspberry Pi 4
screens at stops (D-64), served by the agency beyond localhost.
Ready means: memory plateaus however long a page stays open, the
main thread only renders, every document and device record is
versioned, an open page updates the way a web app does, it works
offline, the server exposes nothing it should not, and a display
recovers without a person.

This unit owns what no other unit does, and is done only when the
units it depends on are:

- **Memory and the main thread** — eviction, the bus worker, and
  reclaiming memory, [module-runtime.md](module-runtime.md); the
  measured ceilings, [runtime-cost-gate.md](runtime-cost-gate.md).
  The budgets those ceilings must come under (D-68): wasm + JS heap
  at most 40 MB on the display, flat across 24 h; at most 80 MB for
  ride-now, its first board under 3 s cold and 1 s cached, no
  main-thread task over 50 ms after first paint.
- **Versioning** — [document-versioning.md](document-versioning.md).

## Scope (owned here)

- **Updates (D-63).** The pages read `/manifest` once, at boot; a
  display open for weeks never sees a new import or a new build. On
  the poll they already run, both pages revalidate `/manifest` (an
  ETag, a 304 when nothing moved).
  - A new `network_hash` re-pins without a reload: the worker
    fetches and registers the new network, the boards re-answer, and
    the old network leaves every registry through the eviction op's
    keep-set.
  - A new build (a module hash, the kernel, the app shell) downloads
    in the background and waits. A rider's page applies it at the
    next cold start or navigation, or when the rider accepts "a new
    version is ready"; a display applies it itself at its next poll
    while the server answers. No page applies it over unsaved device
    data.
- **Offline (D-65).** Both pages install and work with no
  connection: a service worker caches the shell, the modules (hash-
  addressed, so cached forever), and the last network; boards
  answer from the schedule and say so. The service worker IS the
  update mechanism above — one lifecycle, not two. The map's labels
  still come from protomaps.github.io (D-49, QAF-84).
- **Polling.** A hidden page does not poll (`visibilitychange`); it
  re-reads on return. Today both hosts poll on a bare `setInterval`.
- **The service day.** The hosts pass the calendar date and the
  seconds since local midnight. Unpinned: whether a board at 00:30
  shows the previous service day's after-midnight trips (a 24:45
  departure), and the answer on a DST change day (GTFS times count
  from noon minus 12 h). Pin both on the fixture first; a wrong
  answer is a finding against the board's library, not host
  arithmetic.
- **A display that recovers.** A failed poll keeps the last board
  and says it is the schedule, never live; a thrown answer node does
  not blank the screen; an update applies only while the server
  answers, so an outage never reloads a working display into an
  empty one.
- **The shell's caching.** Vite's hashed assets and the
  hash-addressed modules are served `immutable`; the HTML pages and
  `/manifest` `no-cache`. Today `handleStatic` serves everything by
  modification time.
- **The proxy goes (D-67).** `/rt?url=` (the server's
  `handleProxy`, Vite's dev proxy, and the hosts that call it) is
  removed; pages fetch the agency's feeds directly.
- **The feeds are fixed at the source (D-67), the agency's work,
  not this repo's.** Today C-TRAN's GTFS-rt endpoints
  (`gtfs-rt.c-tran.org/gtfsrealtime/…`) answer on plain HTTP only
  (HTTPS does not connect) and send no CORS header, so an HTTPS page
  cannot read them. A deployed page needs each feed it reads to:
  - answer over HTTPS;
  - send `Access-Control-Allow-Origin` (the deployment's origin, or
    `*` for public feeds);
  - send a short `Cache-Control: max-age` (about the feed's refresh
    interval), so a CDN absorbs every screen and phone polling the
    same bytes;
  - publish service alerts (the Alerts URL answers 404; no alert has
    been read on real data, `ui-ride-now.md` and
    `ui-stop-display.md`).
  The proxy is removed in the same change that points
  `web/public/live-feeds.json` at the fixed URLs; until then dev
  keeps it.

## Done when

- [module-runtime.md](module-runtime.md),
  [runtime-cost-gate.md](runtime-cost-gate.md), and
  [document-versioning.md](document-versioning.md) are deleted,
  with ceilings under the D-68 budgets.
- e2e: the server republishes the fixture network while a stop
  display and ride-now are open; both re-pin without a reload, and
  each module's registry and linear memory are back at their plateau
  after the old network is evicted.
- e2e: a new build is published; a rider's page offers it and
  applies it at the next navigation, a display applies it at its
  next poll, and neither applies it over unsaved device data.
- e2e: both pages boot offline from the service worker and answer a
  board from the schedule, marked as the schedule; a hidden page
  makes no requests.
- e2e: a board on the fixture across midnight and on a DST day
  answers as the GTFS spec reads.
- Go: `/rt` is gone; static responses carry the cache headers above.
- Dogfood on C-TRAN, recorded here: a stop display on a Pi 4 left
  open 24 h across a service-day rollover and one republish (memory
  sampled, plateaued), reading the fixed feeds directly; a ride-now
  session on a phone under the D-68 budgets.
