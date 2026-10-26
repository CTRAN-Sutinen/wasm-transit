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

# Build the capability modules (the capability-bus PoC: horizontal
# capabilities assembled per user) and regenerate the capability
# manifest — hash-pinned modules in web/public/caps/, served under
# their content hash so the HTTP cache holds them forever.
caps:
    #!/usr/bin/env bash
    set -e
    cargo build --release -p cap-gtfs -p cap-router -p cap-analysis -p cap-network -p cap-rt --target wasm32-unknown-unknown
    cargo run --quiet -p cap-abi --bin gen-manifest

# Publish the registry: gates first (the records report them), then
# build and publish — immutable blobs, evidence records per
# capability, and the pointer. This is the artifact an agent (or a
# human dogfooding `caps run`) pulls compute from.
registry:
    #!/usr/bin/env bash
    set -e
    export TS_RS_EXPORT_DIR="$PWD/web/src/types"
    cargo test
    node crates/cap-abi/caps-smoke.mjs
    just caps

# Build everything the server serves: artifact + frontend (web/dist).
# Type-checks first — the TS layer is part of the gate.
build:
    cd web && npm run typecheck
    cd web && npm run build

# Regenerate web/src/types/*.ts from the Rust model (ts-rs). The TS
# contract is generated, never hand-written; regenerate after model
# changes and commit the diff like any other artifact.
types:
    #!/usr/bin/env bash
    export TS_RS_EXPORT_DIR="$PWD/web/src/types"
    cargo test --workspace export_bindings >/dev/null
    echo "$(ls web/src/types | wc -l) type files in web/src/types"

# ---------------------------------------------------------------
# test
# ---------------------------------------------------------------

# All host-side tests: Rust suites, Go suite, artifact smoke in Node,
# capability-module smoke (needs `caps` built — hash-pinned modules
# and the manifest the smoke verifies against).
# TS_RS_EXPORT_DIR keeps the ts-rs bindings regenerating into
# web/src/types on every run (plain `cargo test` would scatter them
# into crates/*/bindings).
test: caps
    #!/usr/bin/env bash
    export TS_RS_EXPORT_DIR="$PWD/web/src/types"
    cargo test
    node crates/transit-wasm/abi-smoke.mjs
    node crates/cap-abi/caps-smoke.mjs
    # The agent runner, gated: pull gtfs by its pinned hash and ask a
    # question — the fixture's feed hash is stable, so a fixture or
    # registry change fails here (the corpus spirit, one layer up).
    node tools/caps-run.mjs gtfs --load web/public/caps/sample-gtfs.zip \
      --op '{"op":"stops","feed_hash":"sha256:d87f14e29e498e9e6587448f38ddcfa289df7e9aefbd98d96290beaf57f2f4e2","limit":2}' | grep -q '"stop_id":"A"'
    # The network capability's gate: the same session an agent (or a
    # planner) dogfoods — load the feed by its pinned hash, import
    # (bootstrap), and the dropped single-stop trip must be REPORTED
    # (the never-silent law, one layer up).
    node tools/caps-run.mjs network --load web/public/caps/sample-gtfs.zip \
      --op '{"op":"import","feed_hash":"sha256:d87f14e29e498e9e6587448f38ddcfa289df7e9aefbd98d96290beaf57f2f4e2"}' | grep -q '"kind":"dropped_trips"'
    # The realtime capability's gate: the fixture payload decodes on
    # the bulk seam with its known delay — the dispatch leg, one
    # command an agent or dispatcher runs.
    node tools/caps-run.mjs rt --load web/e2e/fixtures/sample-rt.pb \
      | grep -q '"departure_delay":120'
    # -race: the artifact is one non-reentrant instance; the host must
    # serialize invokes (see server/race_test.go — the gate watches it).
    cd server && go test -race ./...

# Browser e2e (Chromium via Playwright, against the Go server).
# Builds first; the runner starts the server unless :8080 is taken.
e2e: caps build
    cd web && npx playwright test

# Everything: host tests + e2e.
check: test e2e

# Real-data soak: transit-streets against the downloaded county
# export. Hermetic tests skip it; point COUNTY_GEOJSON at the file to
# run it (e.g. the CTRAN Trapeze roads geojson from the open data
# portal).
soak:
    #!/usr/bin/env bash
    if [ -z "$COUNTY_GEOJSON" ]; then
        echo "set COUNTY_GEOJSON to the downloaded county roads geojson"
        exit 1
    fi
    cargo test --release -p transit-streets --test county_data -- --nocapture

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

# ---------------------------------------------------------------
# correctness infrastructure
# ---------------------------------------------------------------

# Regenerate the golden corpus (testdata/corpus.json) from the
# artifact itself — ALWAYS freshly built (the dependency is the
# harness's guarantee: replaying against a stale web/pkg artifact
# writes a no-op diff and the golden test keeps failing). Run after
# any response-shape change, then commit the diff — that diff IS the
# reviewable contract change.
corpus: wasm
    node testdata/generate-corpus.mjs

# Provenance + reproducibility: rebuild the artifact from scratch and
# verify byte-identical output, then record what built it. This is the
# chain of custody that makes "the artifact is what matters" checkable.
provenance:
    #!/usr/bin/env bash
    set -e
    cargo build --release -p transit-wasm --target wasm32-unknown-unknown
    a1=$(sha256sum target/wasm32-unknown-unknown/release/transit_wasm.wasm | cut -d' ' -f1)
    CARGO_TARGET_DIR=/tmp/transit-repro cargo build --release -p transit-wasm --target wasm32-unknown-unknown >/dev/null 2>&1
    a2=$(sha256sum /tmp/transit-repro/wasm32-unknown-unknown/release/transit_wasm.wasm | cut -d' ' -f1)
    if [ "$a1" != "$a2" ]; then
        echo "NOT REPRODUCIBLE: $a1 != $a2"
        exit 1
    fi
    commit=$(git rev-parse HEAD 2>/dev/null || echo "no-git")
    dirty_bool=false
    if [ -n "$(git status --porcelain 2>/dev/null)" ]; then dirty_bool=true; fi
    rustc=$(rustc --version | awk '{print $2}')
    os=$(uname -srm)
    printf '{\n  "artifact": "web/pkg/transit_wasm.wasm",\n  "sha256": "%s",\n  "source_commit": "%s",\n  "dirty_tree": %s,\n  "rustc": "%s",\n  "built_on": "%s",\n  "reproducible": true\n}\n' \
        "$a1" "$commit" "$dirty_bool" "$rustc" "$os" > testdata/provenance.json
    cp target/wasm32-unknown-unknown/release/transit_wasm.wasm web/pkg/transit_wasm.wasm
    echo "artifact sha256:$a1 reproducible from $commit (rustc $rustc)"
    cat testdata/provenance.json

# Supply-chain audit: runs whatever is installed, reports what is
# missing. Recommended installs:
#   cargo install cargo-audit cargo-mutants
# go install golang.org/x/vuln/cmd/govulncheck@latest
audit:
    #!/usr/bin/env bash
    set +e
    status=0
    if command -v cargo-audit >/dev/null; then
        cargo audit || status=1
    else
        echo "SKIP: cargo-audit not installed (cargo install cargo-audit)"
    fi
    if command -v govulncheck >/dev/null; then
        (cd server && govulncheck ./...) || status=1
    else
        echo "SKIP: govulncheck not installed (go install golang.org/x/vuln/cmd/govulncheck@latest)"
    fi
    (cd web && npm audit --audit-level=high) || status=1
    exit $status

# Mutation-test the domain crates: every surviving mutant is a place
# where "tests passed" is a lie. Generated code (transit-rt) and the
# ABI shim (transit-wasm) are excluded; needs cargo-mutants:
# cargo install cargo-mutants
mutants:
    cargo mutants -p transit-core -p transit-router -p transit-sync -p transit-ops
