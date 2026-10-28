# wasm-transit — local-first transit software

GTFS static and GTFS-rt, read by Rust and shipped as **capability
modules**: small WebAssembly modules with one flat, import-free ABI,
each answering one question over hash-addressed documents. The same
modules, the same documents, and the same bytes run in the browser,
in Node, and in a Go server — the browser is the main consumer, and
nothing is uploaded.

The core data model is the authored **network** (routes, patterns,
trips with their own ids and times, service types, blocks, fares,
stations, demand-responsive service). GTFS is how it is shared: a
feed imports into a network and exports back, losslessly over the
whole GTFS Schedule spec.

## Where things are

- `AGENTS.md` — the crate tree, the commands, the ownership rules,
  and the conventions. Start here, human or agent.
- `docs/ARCHITECTURE.md` — per-crate detail, the server, the web
  app, and the capability bus.
- `docs/TESTING.md` — what every change is tested with, and the
  real-data dogfood rules.
- `docs/units/` — the open work. `docs/DECISIONS.md` — the questions
  waiting on a decision. `docs/QA-FINDINGS.md` — what real-data
  sessions found. `docs/ROADMAP.md` — where it is going.

```
crates/   Rust: the domain libraries (transit-*) and the capability
          modules (cap-*) over them
server/   Go + wazero: the sync server — static files, the rt proxy,
          the manifest, the edit document
web/      Svelte 5 + Vite: a picker over the generated UIs, and the
          original app (/app.html)
testdata/ the authored-network fixture, real-data assets
```

## Getting started

Toolchains: Rust (with the `wasm32-unknown-unknown` target), Go
1.22+, Node, and `just`. Then:

```
cd web && npm install && cd ..
just build                                   # capability modules + frontend
just serve                                   # the server on :8080
just serve path/to/gtfs.zip "Test Transit"   # + /feed and the manifest's feed
just dev                                     # vite hot reload + the server
```

Open http://127.0.0.1:8080. `just test` is the host-side gate,
`just e2e` the browser runner, `just check` both; the full command
list is in `AGENTS.md`.

The agent runner drives the same modules from the command line —
`node tools/caps-run.mjs list` shows the capabilities,
`node tools/caps-run.mjs tools <capability>` their ops.

## Basemap

The map prefers `basemap.pmtiles` in `web/public/` (Vite copies it
into the build; the server serves it with Range requests) and falls
back to OpenFreeMap (free vector tiles, no API key).

`just basemap "<bbox>" [maxzoom]` extracts a region from the daily
Protomaps planet build over HTTP range requests, so the planet is
never downloaded (needs the `pmtiles` CLI from
github.com/protomaps/go-pmtiles; a metro area at z15 is about 94 MB):

```
just basemap "-123.20,45.25,-122.10,45.95"
```

To build a region from OpenStreetMap instead (Java 21+):

```
curl -LO https://github.com/onthegomap/planetiler/releases/latest/download/planetiler.jar
curl -LO https://download.geofabrik.de/<your-region>-latest.osm.pbf
java -jar planetiler.jar --osm-path=<your-region>-latest.osm.pbf --output=web/public/basemap.pmtiles
```

Rebuild the frontend afterwards (`just build`). A metro extract is
small enough to keep next to the GTFS zip, which makes the whole app
— feed, queries, basemap — work offline.
