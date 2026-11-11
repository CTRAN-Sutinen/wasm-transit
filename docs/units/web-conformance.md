# Web conformance

The generated UIs, held to WCAG 2.2 AA and to the platform's current
practice (the `modern-web-guidance` corpus: accessibility, motion,
tables, forms, performance, security) in both deployment shapes. The
rider UIs come first: they are the MVP, deployed serverless (D-131),
and riders are the public. Nothing gates accessibility today, so every
item below can regress unseen; the unit lands with the gate that holds
it.

## Measured on C-TRAN (2026-11-11)

axe-core 4 (WCAG 2.2 A/AA + best practice) on every view of every UI,
desktop light and phone dark, on the store-mode server; the two rider
UIs again as static exports (`just export ride-now`, `just
export-feed-url stop-display`) on `web/e2e/static-server.mjs`, the
least a static host does. A keyboard walk of each page (30 tab stops,
plain and under forced colors), reflow at 320 px, the WCAG 1.4.12 text
spacing, the accessibility tree, and the load metrics.

| UI | axe violations | reflow 320 px | text spacing | worst CLS |
|---|---|---|---|---|
| picker | none | passes | passes | 0 |
| stop display (all layouts, both shapes) | none | passes | passes | 0.04 |
| ride-now (both shapes) | dark-mode button contrast | passes | passes | 0.04 |
| service design | 9 rules, 150+ nodes | passes | passes | 0.79 (phone, route view) |

Also passing: offline reload in both static shapes (the service worker
serves the shell and the last documents); the feed-URL display boots
to departures in ~3 s; the rider is asked for a location only from
the button unless they already allowed it; times format in the
agency's zone (`web/src/uis/kit.ts`).

## Scope, in order

### 1. The rider UIs (ride-now, stop display) — both shapes

- **White text on the dark-mode accent.** Dark `--accent` is `#fb923c`;
  white on it is 2.26:1 (4.5 needed). `web/src/uis/ride-now/Shell.svelte`
  (`.cta`), `RiderBoard.svelte` (`.track button`), `LiveFeeds.svelte`
  (`.primary`), `DisplaySetup.svelte` (`.back`). One `--on-accent`
  token in `web/src/uis/base.css`, dark text in dark mode.
- **A view change says nothing.** Ride-now swaps its panel (home, trip,
  stop, route, alerts) with no focus move and no announcement: no
  `.focus()` anywhere in `web/src`, and its only status region is the
  update banner. A screen-reader rider who asks for directions stays
  on the button and hears nothing (WCAG 2.4.3, 4.1.3). Focus the new
  view's heading (`tabindex="-1"`) on `show()`; one polite region for
  `ride.status` and the result counts.
- **Ride-now has no `<main>`.** Its panel is an `<aside>`; the panel is
  the page's main content.
- **Radio and tab roles without their keyboard.** `TripPlan.svelte`
  (`role="radio"` buttons) and `RouteView.svelte` and `DirectionTabs.svelte`
  (`role="tab"`, no tab panels) put every item in the tab order and take
  no arrow keys. Native radio inputs, or the full ARIA pattern (roving
  tabindex, arrows, `aria-controls`).
- **Motion that ignores `prefers-reduced-motion`.** No rule in the tree
  reads it. The loading shimmer (six copies: `RouteView`, `TripPlan`,
  `BusView`, `RouteList`, `StopCard`, `RiderBoard`), the live dot's
  endless pulse (`LiveFeeds.svelte`), the spinner and the sheet's slide
  (`ride-now/Shell.svelte`). One shared shimmer in `base.css`, motion
  opted in under `@media (prefers-reduced-motion: no-preference)`.
- **Focus rings vanish under forced colors.** Inputs draw focus as a
  `box-shadow` (`PlaceSearch.svelte`, `RouteList.svelte`), which Windows
  High Contrast drops; so do MapLibre's zoom buttons. An
  `outline`-based ring (or a `@media (forced-colors: active)` outline).
- **The live-feeds panel** (`LiveFeeds.svelte`) is a `role="dialog"`
  div: no Escape, no light dismiss, no focus move. The native Popover
  API does all three. Blocked: D-135 (Popover is Baseline since
  2025-01; whether older browsers get a polyfill or a lesser fallback).
- **Small targets.** "Use my location" is 19 px high; the phone sheet's
  handle 14 px (24 px minimum, WCAG 2.5.8).
- **A failed feed URL blames a server that is not there.** A
  serverless display or ride-now whose GTFS URL fails (unreachable, no
  CORS) shows "This server publishes no network — load the agency's
  GTFS zip to begin." (`stop-display/host.svelte.ts`,
  `ride-now/host.svelte.ts`): it names no failed feed, and a display
  has no one to load a zip. Say the agency's schedule could not be
  read and that the screen retries; a status region.

### 2. Delivery — both shapes

- **Compression.** The Go server sends JS, CSS, and the assemblies
  uncompressed: ride-now's assembly is 4.99 MB, its script 1.24 MB, the
  map's 1.1 MB (gzip takes an assembly to ~1.5 MB). Either the server
  compresses those types (never `application/gzip` documents, which
  the modules decode themselves) or `DEPLOYMENTS-SERVER.md` requires
  the load balancer to. Static: Cloudflare compresses text types;
  whether it compresses `application/wasm` is checked on a real deploy.
- **Caching rules are host-neutral requirements.** On a host without
  `DEPLOYMENTS.md`'s `_headers` rules, a first visit fetches every large
  file twice (the page, then the service worker's precache and its
  forever cache cannot reuse an uncached response): 33 MB for ride-now
  cold on the bare host. The guide states the rules for Cloudflare
  only; say what any host must send.
- **Security headers.** Neither shape sends a Content-Security-Policy,
  `Referrer-Policy`, `Permissions-Policy`, or frame-ancestors; the
  server sends `X-Content-Type-Options` only on a 404. A policy needs the
  deployment's own origins — the feed URL (D-132), the live feeds
  (`live-feeds.json`, D-67), the tiles (OpenFreeMap or the bundled
  pmtiles), the geocoder — plus `'wasm-unsafe-eval'` and the bus
  worker. The export knows each one, so it writes the `_headers` lines;
  the server sends the same. Ride-now allows geolocation for itself;
  the stop display allows none.

### 3. Service design (server only; after its current edits land)

- **Contrast, light mode.** `--muted` on `--accent-soft` is 4.02:1 (every
  selected chip, tab, plan row); the frequency grid's white numbers
  switch on at 55% strength, 3.33:1 (`FrequencyGrid.svelte`, `fg`); its
  "N dep" labels 3.59:1.
- **Names that differ from the visible text** (WCAG 2.5.3): the context
  chips (`aria-label="Clear route 002"` over "Route 002",
  `ContextBar.svelte`), the hourly bars (`aria-label="24:00 — 10 trips"`,
  `DayProfile.svelte`),
  `GesturePanel.svelte`'s "Insert stop id after it" (named "stop to
  insert").
- **A control inside a control**: the route view's string-line chart
  (`StringLine.svelte`, `tabindex="-1"` inside a button).
- **Small targets on a phone**: the hourly bars (`DayProfile.svelte`, 13 px wide, 7 px
  apart); the context bar's selects (21 px high).
- **Unlabelled**: the coverage layer's file input
  (`CoveragePanel.svelte`, `layer-input`); the plans table's empty
  header (`PlanList.svelte`).
- **Tables**: none of the eight has a `<caption>` or `scope="col"`; the
  six that scroll sideways (`TimetableGrid`, `FrequencyGrid`,
  `HubMatrix`, `CorridorTable`, `Redline`, `GroupTable`) scroll in a
  wrapper the keyboard cannot reach — `tabindex="0"`, `role="region"`,
  named by the caption.
- **Structure**: the coverage view has no `<h1>`; no skip link past the
  context bar; the plans view's date inputs show no focus ring.
- **Layout shift on a phone**: the route view 0.79, plans 0.28, network
  0.23 (0.1 is good) — the side panel and the status line arrive after
  the first paint and push the view down. Reserve their space.
- `setActivePlan` (`web/src/plans.svelte.ts`) writes `localStorage`
  outside a `try`; a private or blocked context throws.

### 4. Outside this audit

A screen reader by hand (VoiceOver and TalkBack on ride-now on a phone,
NVDA on service design), the trip-planning and map-picking flows past
boot, and the stop display on its Pi 4 at the display's distance.
These are the dogfood half of the pins below.

## Done when

- `web/e2e/a11y.spec.mjs` in `just e2e`: axe-core (a dev dependency)
  over every UI's views on the fixture feed, light and dark, phone and
  desktop width, failing on any WCAG 2.2 A/AA violation; the rider UIs
  also from their static exports.
- A keyboard spec: ride-now's view changes move focus and announce;
  the radio and tab groups take arrow keys; the live-feeds panel closes
  on Escape.
- The headers: a Go test over the server's served headers (compression,
  security), and `static-deploy.spec.mjs` asserting the `_headers` the
  export writes.
- A dogfood session with a screen reader on ride-now on a phone,
  recorded here or in `docs/QA-FINDINGS.md`.
- `docs/TESTING.md` names the gate; `docs/CONVENTIONS.md` (frontend) the
  accent text token, the focus ring, and reduced motion;
  `DEPLOYMENTS.md` and `DEPLOYMENTS-SERVER.md` the headers.
