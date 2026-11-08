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

- **Memory and the main thread** — the measured ceilings,
  [runtime-cost-gate.md](runtime-cost-gate.md).
  The budgets those ceilings must come under (D-68): wasm + JS heap
  at most 40 MB on the display, flat across 24 h; at most 80 MB for
  ride-now, its first board under 3 s cold and 1 s cached, no
  main-thread task over 50 ms after first paint.
- **Versioning** — [document-versioning.md](document-versioning.md),
  and the code's: retained builds, rollback, and contracts that do
  not break, [release-artifacts.md](release-artifacts.md).
- **Seeing production** — the display heartbeat, the fleet view,
  canary rings and the automatic revert,
  [observability-rollout.md](observability-rollout.md). The update
  lifecycle below applies a build through its rings.
- **The server's record** — the store on object storage, offline
  verification, and backup and restore,
  [store-durability.md](store-durability.md).

## Scope (owned here)

The pages' own lifecycle stands (docs/ARCHITECTURE.md, web section,
"Updates and offline"): data re-pins without a reload, a build waits
and applies per D-63, both pages install and boot offline (D-65),
boards answer the previous date's after-midnight trips and the
fall-back hour, and `/rt` is a dev relay off by default (D-67).
What is left here:

- **The feeds are fixed at the source (D-67), the agency's work,
  not this repo's.** Today C-TRAN's trip updates and vehicle
  positions (`gtfs-rt.c-tran.org/gtfsrealtime/…`) answer on plain
  HTTP only (HTTPS does not connect); its alerts
  (`https://gtfs-rt.c-tran.com/Alerts`)
  answer over HTTPS. None sends a CORS header, so an HTTPS page
  cannot read them. A deployed page needs each feed it reads to:
  - answer over HTTPS;
  - send `Access-Control-Allow-Origin` (the deployment's origin, or
    `*` for public feeds);
  - send a short `Cache-Control: max-age` (about the feed's refresh
    interval), so a CDN absorbs every screen and phone polling the
    same bytes;
  - state each trip instance's `start_date`, so a trip past midnight
    is joined to the day it started on (an undated instance is read
    as the current service day's).

  The deployment's `live-feeds.json` then names the fixed URLs and
  no `proxy`; the dev copy in `web/public/` keeps the relay until
  then.
- **The dogfood**, below.

## Done when

- [runtime-cost-gate.md](runtime-cost-gate.md),
  [document-versioning.md](document-versioning.md),
  [release-artifacts.md](release-artifacts.md),
  [observability-rollout.md](observability-rollout.md), and
  [store-durability.md](store-durability.md) are deleted,
  with ceilings under the D-68 budgets.
- Dogfood on C-TRAN, recorded here: a stop display on a Pi 4 left
  open 24 h across a service-day rollover and one republish (memory
  sampled, plateaued; the re-pin and a build applied at a poll),
  reading the fixed feeds directly; a ride-now session on a phone
  under the D-68 budgets, installed, then offline at a stop.

The pins that stand: `e2e/deploy-updates.spec.mjs` (re-pin, plateau),
`e2e/deploy-builds.spec.mjs` (offline boot, the build lifecycle),
`e2e/ui-stop-display.spec.mjs` (feeds fetched directly; after
midnight, live; the fall-back hour),
`crates/transit-dispatch/tests/overnight_law_test.rs`, and Go's
`proxy_test.go`.
