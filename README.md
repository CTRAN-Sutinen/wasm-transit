# rustwasm — local-first transit exploration

Learning project: GTFS static + GTFS-rt feeds, parsed by Rust, running in
the browser via WebAssembly. No backend beyond a static server and a
localhost proxy — the browser is the consumer, nothing is uploaded.

## Layout

```
crates/
├── transit-core   GTFS static: data model, zip/CSV parsing, calendar
│                  logic, departure queries, diversion patching.
│                  Pure Rust, no I/O deps.
├── transit-network The authored network model (the planner's view):
│                  patterns, timing profiles, calendar rules, services,
│                  and the runcut; trips are derived by `materialize`.
│                  GTFS is a codec at the boundary, never the shape.
│                  Property-tested against an independent expansion
│                  oracle.
├── transit-router Earliest-arrival trip planning (RAPTOR): trip
│                  patterns, footpaths, round-based search, itinerary
│                  reconstruction. Property-tested against a
│                  brute-force oracle.
├── transit-ops   The host-agnostic operations layer: a typed
│                  `dispatch(Request) -> Response` seam (serde-tagged,
│                  so it serializes to `{"op": ...}` JSON) plus the
│                  per-feed registry. Native tests call it directly.
├── transit-rt     GTFS-realtime: Rust types generated from the official
│                  gtfs-realtime.proto (see proto/), plus a decode helper.
├── transit-sync  The sync document: EditRecord (an operator edit plus
│                  its lifecycle/status envelope), FeedManifest (the
│                  authority pointer to a feed artifact), validation
│                  reports. Pure functions, shared by wasm and the
│                  server — the document is the contract.
└── transit-wasm   THE wasm artifact: a flat, import-free C ABI
                   (`transit_alloc/free/invoke/invoke_bytes`) over the
                   dispatch seam. The rustc output IS the deployment
                   artifact — the same bytes run in the browser, in
                   Node, and in the server's wazero runtime.
server/            Go + wazero: the sync server. Scaffolding only —
                   static serving, /rt proxy, /manifest (pins the feed
                   and wasm hashes), /feed. All domain logic runs in
                   the artifact it hosts.
web/               Svelte 5 + Vite frontend (see below).
```

## Commands

`just` is the task runner (`just --list` shows everything). The raw
commands live in the justfile; the common entries:

```
just build         # wasm artifact + frontend (web/dist)
just test          # Rust suites + Go suite + ABI smoke (Node)
just e2e           # browser e2e (builds first, Chromium + Playwright)
just check         # everything above
just serve         # the sync server on :8080
just serve path/to/gtfs.zip "Test Transit"   # + /feed, /manifest
just dev           # vite hot reload (:5173) + server alongside
just fixtures      # regenerate the e2e GTFS fixture
just basemap "-123.20,45.25,-122.10,45.95"    # regional pmtiles extract
```

First time: `npm install` in `web/`, and Go 1.22+ (go.mod pulls wazero
on first build).

The test stack, bottom-up: `just test` covers the host side — Rust
suites (unit + property tests against independent oracles), the Go
suite (including the round-trip proof that the server runs the exact
artifact bytes), and `web/e2e/abi-smoke.mjs`, which instantiates the
artifact directly in Node with zero imports. `just e2e` runs real
Chromium against the built app served by the Go server and covers:
clean load (no console errors), feed loading, the departure board,
trip planning (including the clipped ridden segment), unreachable-plan
feedback, IndexedDB restore across reloads, GTFS-rt delay joins (chips,
predicted times, vehicle colors), and the full diversion flow (schedule
shift, skipped stop, planner compare, block layover warnings,
persistence). Fixture feed + protobuf live in `web/e2e/fixtures/`.

Run the sync server, then open http://127.0.0.1:8080:

```
just serve                                          # static + /rt
just serve path/to/gtfs.zip "Test Transit"          # + /feed, /manifest
```

The Go server is scaffolding around the wasm artifact: it serves
`web/dist` (stdlib `http.ServeContent` handles the Range requests
PMTiles needs) and exposes:

- `/rt?url=<encoded>` — localhost-only proxy that fetches agency feeds
  and returns them with CORS headers, so realtime feeds without
  `Access-Control-Allow-Origin` can be polled from the page.
- `/manifest` — `{ feed_hash, feed_version, wasm_hash, generated_at,
  publisher }`: which GTFS artifact is authoritative (sha256), which
  wasm bytes every host runs (sha256), and who says so. The feed
  version is parsed *by the artifact* (`feed_info.txt` op) — the host
  never duplicates GTFS knowledge.
- `/feed` — the configured GTFS artifact itself. Republishing the file
  is picked up on the next request (mtime-checked reload): the planner
  persona publishes, everyone else re-pins.
- `/edits` — edit-document sync. `POST` your local document, receive
  the LWW-merged result back (drafts submitted, tombstones propagated,
  validated against the server's feed when one is configured); `GET`
  reads the server's document. Merge and the submit transition run in
  the artifact — the host treats documents as opaque JSON, so client
  and server agree by construction. `-edits <path>` persists the
  document across restarts.

`go test ./...` in `server/` is the round-trip proof: the server loads
the exact `web/pkg/transit_wasm.wasm` bytes (hash-pinned in the
manifest) and gets the same domain answers the browser does — feeds,
boards, plans, edit documents.

For frontend development with hot reload:

```
just dev      # vite on :5173 (proxies /rt to :8080) + server alongside
```

## Frontend (web/)

Svelte 5 (runes) + Vite. Shared reactive state lives in
`src/state.svelte.js` as a single `$state` object; every component —
including each MapLibre layer — follows exactly one slice of it. That
structure is what keeps feeds from stomping on each other (a TripUpdates
poll can never wipe vehicle dots drawn from a VehiclePositions feed).

The wasm boundary is unchanged: JS owns networking, files, and the
clock; Rust receives bytes and dates, and returns plain data. The
artifact has zero imports — `src/wasm.js` is a thin hand-written
wrapper (fetch, instantiate, JSON over the flat ABI) that preserves the
`GtfsHandle` API, so components and tests never see the ABI.

## Diversions (operator edits)

Beyond passenger information, the wasm binary can *edit* the feed. A
diversion models what a scheduler does when a road closes: pick a route,
anchor the segment (first/last stop kept), skip interior stops, draw a
detour on the map, add running time, and set a date window.

Implementation shape:

- `transit-core::diversion` — the `Diversion` spec plus
  `apply_diversions(base, date, edits)`, which produces a patched copy
  of the feed for one service date (skip + renumber, time shift from
  the last anchor onward, shape splice). The base feed is never
  mutated; property tests check the patched feed against an oracle.
- `transit-wasm` serves every date-scoped query (departures, blocks,
  plans, trip paths) from the *effective* feed. `plan_trip_compare`
  returns both the diverted plan and the baseline, so the UI shows
  "without the diversion: arrive 08:10".
- **The edit document is the sync unit and JS owns it**: a
  `$state.raw` array of transit-sync `EditRecord`s, replaced
  wholesale (never mutated in place — that is what makes it safe to
  structured-clone into IndexedDB and, later, to sync to an ops
  server). wasm holds only a materialized view: one
  `set_diversions(records)` call replaces the whole document and
  returns a validation report per record. Every UI mutation flows
  through a single `mutateEdits(fn)` (adopt → push to wasm → bump
  `feedEpoch` → one debounced persist), so mirrors cannot drift.
- Retirement is the tombstone: unchecking an edit sets
  `status: "retired"` (rev bumped) — the record stays in the
  document for sync to see the deletion, and can be restored.
- The UI is a Diversions section: pick route + direction, click the
  two anchors (list or map), toggle interior skips, optionally draw
  the detour by clicking the map, set added minutes and dates. Active
  detours draw as dashed orange lines; block chains flag tight (<5 min)
  and overlapping layovers that a shift creates downstream.
- Edits persist in IndexedDB next to the feed bytes and are
  revalidated against whatever feed loads next — records that no
  longer validate (feed republished underneath) stay in the document
  with their error shown, they just never apply. A trip's block
  ripple is visible but not automatic — shifting the *next* trip on
  the block is a separate operator decision.

## Basemap

The map prefers `basemap.pmtiles` if it exists in `web/public/` (Vite
copies it into the build; transit-server serves it with Range requests)
and renders it with the Protomaps v4 style; otherwise it falls back to
OpenFreeMap (free vector tiles, no API key).

The easy path to a regional basemap is the `pmtiles` CLI (from
github.com/protomaps/go-pmtiles) — it can extract a region straight
from the remote daily planet build over HTTP range requests, so you
never download the full planet. Example (Portland metro, z0-15,
94 MB):

```
pmtiles extract https://build.protomaps.com/<yyyymmdd>.pmtiles \
  web/public/basemap.pmtiles \
  --bbox=-123.20,45.25,-122.10,45.95 --maxzoom=15
```

Current build dates are listed at maps.protomaps.com/builds. To build a
region from scratch instead (requires Java 21+):

```
curl -LO https://github.com/onthegomap/planetiler/releases/latest/download/planetiler.jar
curl -LO https://download.geofabrik.de/<your-region>-latest.osm.pbf
java -jar planetiler.jar --osm-path=<your-region>-latest.osm.pbf --output=web/public/basemap.pmtiles
```

After adding or changing the basemap, run `npm run build` in `web/`.
A metro-area extract is small enough to keep next to the GTFS zip,
making the whole app (feed, queries, basemap) fully offline-capable.

## Design notes

- **The artifact is the deployment unit.** All domain logic lives in
  `transit-ops` behind the dispatch seam; `transit-wasm` is a flat,
  import-free C ABI over it. The rustc output runs unmodified in the
  browser (via `src/wasm.js`), in Node (smoke tests), and in the Go
  server (wazero) — and the manifest pins its sha256 so "same artifact
  everywhere" is a checkable claim, not a discipline.
- Hosts own I/O, files, and the clock; the wasm module owns domain
  logic and nothing else. JS fetches (or drops) raw bytes; Go serves
  files and the manifest; neither makes a domain decision.
- GTFS static parsing is intentionally ours (no `gtfs-structures`
  dependency): it is CSV in a zip, and owning the data model is the point
  of the exercise.
- GTFS-rt protobuf decoding uses `prost` with types generated at build
  time from the official spec proto — generated by `build.rs`, protoc
  vendored via `protoc-bin-vendored` (no system protoc needed).
- Times are stored as seconds since service-day midnight (`u32`), because
  GTFS times legitimately exceed 24:00:00 for after-midnight trips.
- Service calendar: `calendar.txt` defines weekly patterns within a date
  range; `calendar_dates.txt` adds (1) or removes (2) individual dates.
  One date can have several active service keys at once —
  `active_services(date)` returns them, and every departure/block row
  carries its `service_id`.
- Property-based tests (`transit-core/tests/prop_test.rs`) generate
  random feeds and check laws against independent oracles; failure seeds
  are kept in `tests/prop_test.proptest-regressions`.
- **All domain logic lives behind transit-ops' dispatch seam** — the
  flat ABI exports in transit-wasm marshal bytes over it, native tests
  call it directly (`crates/transit-ops/tests/dispatch_test.rs`), and
  the Go sync server's wazero host calls the same ops through the
  same artifact. Feeds live in a module-level registry keyed by
  `feed_id`; hosts never hold typed handles across the boundary.
