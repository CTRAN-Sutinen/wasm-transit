# Deploying the rider apps

Ride-now on riders' phones and the stop display on Raspberry Pi 4
screens at stops (D-64), served by the agency beyond localhost as
static files — no server (D-131). Ready means: memory plateaus
however long a page stays open, the main thread only renders, every
document and device record is versioned, an open page updates the
way a web app does, it works offline, and the whole deployment is a
bundle any static host serves.

This unit owns what no other unit does, and is done only when what
it depends on is:

- **Memory** — the runaway check in
  [perf-harness.md](perf-harness.md): a soak fails when memory does
  not level off. There is no fixed budget (D-187).
- **Versioning** stands: a display's cached build that meets newer
  documents refuses them, naming both versions, and looks for the
  deployment's build at once (`docs/FORMATS.md`, "Rolling a version
  back").
- **The static bundle** stands (`just export`; docs/architecture/static-deployment.md).

The server's units ([store-durability.md](store-durability.md),
[observability-rollout.md](observability-rollout.md)) are not this
deployment's (D-131): a bundle ships each page with its assemblies,
and a rollback redeploys the previous bundle.

## Scope (owned here)

The pages' own lifecycle stands (docs/architecture/updates-offline.md): data re-pins without a reload, a build waits
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

- [perf-harness.md](perf-harness.md)'s runaway check stands.
- A real Cloudflare Workers deployment, set up as `DEPLOYMENTS.md`
  says, checked against it and recorded in `DEPLOYMENTS.md`:
  - each page's `Content-Security-Policy` against the deployment's
    live feeds and tiles, in the browser console of a phone and of
    the Pi 4 display (a blocked host shows as a violation);
  - whether Cloudflare compresses `application/wasm` as it does text
    types. If it does not, `DEPLOYMENTS.md`, "Headers every host must
    send", says so.
- Dogfood on C-TRAN, recorded here, served from that static host: a
  stop display on a Pi 4 left open 24 h across a service-day
  rollover and one redeploy (memory sampled, plateaued; the re-pin
  and a build applied at a poll), reading the fixed feeds directly;
  a ride-now session on a phone, its costs recorded, installed,
  then offline at a stop.

The pins that stand: `e2e/deploy-updates.spec.mjs` (re-pin, plateau),
`e2e/deploy-builds.spec.mjs` (offline boot, the build lifecycle),
`e2e/static-deploy.spec.mjs` (the bundle on a plain file server
under a prefix: boot, boards, a trip plan, a redeploy re-pinned,
offline, no edits or geocoder), `e2e/ui-stop-display.spec.mjs` (feeds fetched directly; after
midnight, live; the fall-back hour),
`crates/transit-dispatch/tests/overnight_law_test.rs`, and Go's
`proxy_test.go`.
