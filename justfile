# rustwasm — task runner. Run `just` (or `just --list`) to see all
# recipes. The project has four layers, one recipe group each:
#
#   Rust  — transit-core/-router/-rt/-sync/-ops (cargo), transit-wasm artifact
#   Go    — server/ (the sync server: wazero host, manifest, proxy)
#   Node  — crates/transit-wasm/abi-smoke.mjs (runs the artifact directly)
#   Web   — web/ (Svelte + Vite) and Playwright e2e

default:
    @just --list

# ---------------------------------------------------------------
# build
# ---------------------------------------------------------------

# Build the wasm artifact only (cargo release → web/pkg/transit_wasm.wasm).
wasm:
    cd web && npm run build:wasm

# Build everything the server serves: artifact + frontend (web/dist).
build:
    cd web && npm run build

# ---------------------------------------------------------------
# test
# ---------------------------------------------------------------

# All host-side tests: Rust suites, Go suite, artifact smoke in Node.
test:
    cargo test
    cd server && go test ./...
    node crates/transit-wasm/abi-smoke.mjs

# Browser e2e (Chromium via Playwright, against the Go server).
# Builds first; the runner starts the server unless :8080 is taken.
e2e: build
    cd web && npx playwright test

# Everything: host tests + e2e.
check: test e2e

# Regenerate the e2e GTFS fixture (web/e2e/fixtures/sample-gtfs.zip).
fixtures:
    cd web/e2e && python3 make_fixtures.py

# ---------------------------------------------------------------
# run
# ---------------------------------------------------------------

# Serve the built app and APIs on :8080. Arguments are positional:
#
#   just serve                                   # static + /rt only
#   just serve path/to/gtfs.zip                  # + /feed, /manifest
#   just serve path/to/gtfs.zip "Test Transit"   # + manifest publisher
#   just serve path/to/gtfs.zip "" 127.0.0.1:8081
#
# The feed path is resolved relative to the repo root, wherever you
# run just from.
serve feed="" publisher="" addr="127.0.0.1:8080":
    #!/usr/bin/env bash
    set -e
    cd server
    args=(-addr "{{ addr }}")
    if [ -n "{{ feed }}" ]; then args+=(-feed "{{ absolute_path(feed) }}"); fi
    if [ -n "{{ publisher }}" ]; then args+=(-publisher "{{ publisher }}"); fi
    exec go run . "${args[@]}"

# Frontend dev loop: vite with hot reload on :5173 (proxies /rt),
# with the sync server alongside on :8080 (logs: /tmp/transit-server.log).
dev:
    #!/usr/bin/env bash
    set -e
    (cd server && go build -o /tmp/transit-server . && \
      /tmp/transit-server >/tmp/transit-server.log 2>&1 & echo $! >/tmp/transit-server.pid)
    trap 'kill "$(cat /tmp/transit-server.pid)" 2>/dev/null || true' EXIT
    cd web && npx vite

# ---------------------------------------------------------------
# basemap
# ---------------------------------------------------------------

# Extract a regional basemap from the Protomaps daily planet build
# (needs the pmtiles CLI; ~94 MB for a metro area at z15).
#
#   just basemap "-123.20,45.25,-122.10,45.95"
# just basemap "-123.20,45.25,-122.10,45.95" 14
basemap bbox maxzoom="15":
    pmtiles extract "https://build.protomaps.com/$(date +%Y%m%d).pmtiles" \
        web/public/basemap.pmtiles --bbox={{ bbox }} --maxzoom={{ maxzoom }}
