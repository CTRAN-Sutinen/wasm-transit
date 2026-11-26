# rustwasm — task runner. Run `just` (or `just --list`) to see all
# recipes. The project has four layers, one recipe group each:
#
#   Rust  — the transit-* libraries and cap-* capability modules (cargo)
#   Go    — server/ (the sync server: wazero host over cap-sync, proxy)
#   Node  — crates/cap-abi/caps-smoke.mjs (runs the modules directly)
#   Web   — web/ (Svelte + Vite) and Playwright e2e
#
# There is no monolith artifact: each host links the capabilities it
# needs into one module — the browser fetches its UI's from
# web/public/caps/, and the sync server runs web/pkg/server.wasm.

default:
    @just --list

# ---------------------------------------------------------------
# build
# ---------------------------------------------------------------

# Build the modules and regenerate the capability manifest. Every
# module is an ASSEMBLY (gen-assemblies writes assemblies/): each
# capability alone (module-<cap>: caps-run, agents, the registry's
# blobs), each generated UI's capabilities linked into one module over
# one kernel registry (module-ui-<id>: the page loads it), and the
# server's (module-server). Hash-pinned modules land in
# web/public/caps/, served under their content hash so the HTTP cache
# holds them forever. The server's module lands at web/pkg/server.wasm
# (the server's default -wasm), with the kind table
# web/pkg/kinds.json (-kinds) and the
# schemas it names in web/pkg/schemas/ (D-202). The
# registry also publishes every document kind's JSON Schema and a
# worked example (registry/documents.json, docs/FORMATS.md).
caps:
    #!/usr/bin/env bash
    set -e
    profile="${CAPS_PROFILE:-release}"
    cargo run --quiet -p cap-abi --bin gen-assemblies
    # A new UI's assembly, or a new crate, is a workspace member the
    # lock does not list yet: record it (offline; only the workspace's
    # own entries change), so the --locked build below finds it.
    cargo update --workspace --offline --quiet
    just _modules target "$profile"
    # gen-manifest: the generator's binary, which gen-assemblies wrote
    # beside the modules (every capability, with its worked examples).
    CAPS_PROFILE="$profile" cargo run --quiet -p gen-manifest
    mkdir -p web/pkg
    # The server's one module: gen-assemblies links it from
    # server/capabilities.json and the capabilities the kind list names.
    rm -f web/pkg/cap_*.wasm
    cp "target/wasm32-unknown-unknown/$profile/module_server.wasm" web/pkg/server.wasm

# The modules the tests run: `just caps` under the wasm-fast profile
# (Cargo.toml — no whole-program LTO, incremental), so a change to one
# library rebuilds its modules in seconds to a minute, not an hour. They
# behave as the release modules do; only their size differs. The pages,
# e2e specs, and caps-run read them the same way (the local registry's
# evidence says `profile: wasm-fast`). `just test` and `just e2e` build
# them; `just test-release`, `just e2e-release`, and `just registry`
# build release themselves.
caps-fast:
    CAPS_PROFILE=wasm-fast just caps

# The docs site: static HTML from the registry (run `just caps` first),
# into docs-site/ (gitignored). `just docs` builds it. `just docs serve
# [PORT]` builds it, then serves it on http://127.0.0.1:8085/ (or PORT).
# Its content comes from the code: the registry holds the purposes, the
# op descriptions, the examples, and the schemas, and the architecture
# page draws the crates from `cargo metadata`. Only its two traces and
# its bucket stories are written by hand, and its test checks the code they
# name
# (docs/architecture/registry.md, "The docs site").
docs mode="build" port="8085":
    #!/usr/bin/env bash
    set -e
    case "{{mode}}" in
        build) node tools/docs-site.mjs docs-site ;;
        serve) node tools/docs-site.mjs docs-site --serve {{port}} ;;
        *) echo "just docs [build|serve [PORT]]"; exit 1 ;;
    esac

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
    just reproducible
    CAPS_PUBLISH_GATED=1 just caps

# The modules' build (`just caps` runs it): release wasm for every
# assembly, under the pinned toolchain (rust-toolchain.toml; the
# registry's evidence records it), --locked (Cargo.lock is the
# dependency pin), and with the build machine's paths remapped — the
# cargo home to /cargo, the tree to /src — so a panic location names
# no home directory and the bytes are a function of the tree alone.
_modules target_dir="target" profile="release":
    #!/usr/bin/env bash
    set -e
    export RUSTFLAGS="--remap-path-prefix=${CARGO_HOME:-$HOME/.cargo}=/cargo --remap-path-prefix=$PWD=/src"
    export CARGO_TARGET_DIR="{{target_dir}}"
    # One build per assembly: cargo unifies a dependency's features
    # across every package one build selects, so building them together
    # lets a feature one capability asks for move every module's bytes.
    # Apart, a module's bytes are a function of its own closure.
    # gen-manifest, the generator's binary, is no module.
    for crate in $(cd assemblies && ls -d module-*); do
        cargo build --quiet --locked --profile "{{profile}}" -p "$crate" --target wasm32-unknown-unknown
    done

# REPRODUCIBLE BUILDS (`just registry` gates on it): two clean builds
# of one tree give the same module hashes. The second builds a copy of
# the tree (what git sees, tracked and untracked, plus the generated
# assemblies) at another path, with another cargo home, into its own
# target directory, and
# every module's bytes must match this tree's build. A difference is a
# leak — a path, a timestamp, an unpinned input — and fails the gate.
# A full release build (~minutes); its work directory is
# target/reproducible, removed after.
reproducible:
    #!/usr/bin/env bash
    set -e
    cargo run --quiet -p cap-abi --bin gen-assemblies
    # A new UI's assembly, or a new crate, is a workspace member the
    # lock does not list yet: record it (offline; only the workspace's
    # own entries change), so the --locked build below finds it.
    cargo update --workspace --offline --quiet
    just _modules
    work="$PWD/target/reproducible"
    rm -rf "$work"
    trap 'rm -rf "$work"' EXIT
    mkdir -p "$work/tree"
    git ls-files -z --cached --others --exclude-standard \
      | tar --null -T - --ignore-failed-read -cf - 2>/dev/null | tar -xf - -C "$work/tree"
    cp -r assemblies "$work/tree/"
    # Another cargo home too (the same sources, reached by another
    # path), so a leaked registry path differs between the builds.
    mkdir -p "$work/cargo"
    ln -s "${CARGO_HOME:-$HOME/.cargo}/registry" "$work/cargo/registry"
    (cd "$work/tree" && CARGO_HOME="$work/cargo" just _modules "$work/target")
    differ=0
    for crate in $(cd assemblies && ls -d module-*); do
        m="${crate//-/_}.wasm"
        if ! cmp -s "target/wasm32-unknown-unknown/release/$m" "$work/target/wasm32-unknown-unknown/release/$m"; then
            echo "not reproducible: $m" >&2; differ=1
        fi
    done
    test "$differ" = 0
    echo "reproducible: $(cd assemblies && ls -d module-* | wc -l) modules, byte for byte"

# Build everything the server serves: artifact + frontend (web/dist).
# Regenerates the TS bindings, then type-checks — the TS layer is
# part of the gate, checked against the contract as the Rust stands.
build: types
    cd web && npm run typecheck
    cd web && npm run build

# Regenerate web/src/types/*.ts from the Rust model (ts-rs). The TS
# contract is generated, never hand-written, and never committed
# (gitignored). The directory is wiped first: a type deleted from the
# Rust must not linger as a stale file the frontend still compiles
# against.
types:
    #!/usr/bin/env bash
    set -e
    rm -rf web/src/types
    export TS_RS_EXPORT_DIR="$PWD/web/src/types"
    cargo test --workspace export_bindings >/dev/null
    echo "$(ls web/src/types | wc -l) type files in web/src/types"

# ---------------------------------------------------------------
# test
# ---------------------------------------------------------------

# The host-side runner, and the check a change runs before it is
# called done: Rust suites, Go -race, artifact smoke in Node,
# capability-module smoke (hash-pinned modules and the manifest the
# smoke verifies against), the runner's sessions, the source and web gates. No
# browser — pair with a real-data caps-run session (docs/TESTING.md,
# dogfood QA). Browser cover is `e2e`. It builds the wasm-fast modules
# (`caps-fast`: no whole-program LTO, incremental), which behave as the
# release modules do, so a library change costs a minute of module
# build, not an hour of LTO. `test-release` runs the same over the
# release modules. TS_RS_EXPORT_DIR keeps the ts-rs bindings
# regenerating into web/src/types on every run (.cargo/config.toml
# pins the same path for plain cargo runs; without either, ts-rs
# scatters them into crates/*/bindings). First, `_target-limit` keeps
# target/ under its size limit.
test: _target-limit caps-fast _host-tests

# Keep target/ under a size limit (TARGET_LIMIT_GB, 40 by default).
# Cargo never deletes an old copy of a build: a `-p` run, a dependency
# change, or a toolchain upgrade writes new copies beside the old ones,
# and cargo has no stable way to delete only the old ones. Over the
# limit, `cargo clean --workspace` deletes every copy of the
# workspace's own crates (nearly all of target/) and keeps the
# third-party dependencies, the slow part of a cold build. The gate
# then rebuilds the workspace crates, a few minutes. It runs first, so
# it never deletes a tree that the gate just built.
_target-limit:
    #!/usr/bin/env bash
    set -e
    limit="${TARGET_LIMIT_GB:-40}"
    [ -d target ] || exit 0
    gb=$(( $(du -sk target | cut -f1) / 1024 / 1024 ))
    if [ "$gb" -ge "$limit" ]; then
        echo "target/ is ${gb} GB, over its ${limit} GB limit: cargo clean --workspace"
        cargo clean --workspace
    fi

# The same suites and gates over the release modules (`caps`: whole-
# program LTO, the bytes the registry publishes). Run it when a unit
# is done, and before a release; `registry` builds release itself.
test-release: caps _host-tests

# The host-side suites and gates over whichever modules were built
# last (`test` and `test-release` build them first).
_host-tests:
    #!/usr/bin/env bash
    set -e
    trap 'echo "GATE FAIL: $BASH_COMMAND" >&2' ERR
    export TS_RS_EXPORT_DIR="$PWD/web/src/types"
    # `cargo test` includes THE SCHEMAS DESCRIBE THE REAL BYTES
    # (gen-manifest's tests, generator::checks::real_output;
    # docs/FORMATS.md, "The schema lock"): every capability's worked
    # examples, run through its own
    # dispatch over the fixtures, answer documents that match their
    # locked schemas. It reads the manifest `just caps` just wrote.
    cargo test
    node crates/cap-abi/caps-smoke.mjs
    # ASSEMBLY INDEPENDENCE: every worked example answers alike
    # from the capability alone and inside each UI's linked assembly,
    # in either order of its capabilities — a shared derived entry is
    # only a cache, never a dependency. The check's broken fixture
    # runs first.
    node --test crates/cap-abi/independence.test.mjs
    node crates/cap-abi/independence.mjs
    # UNKNOWN_DOCUMENT BY CODE (docs/architecture/bus.md, "Answers"): every
    # worked example that names a
    # document, run on a module that holds nothing, refuses with the
    # unknown_document code, its hash, and its load op.
    node crates/cap-abi/refusal-law.mjs
    # The pages' service clock (web/src/clock.ts): the agency's date
    # and GTFS seconds (noon minus 12 h) in the agency's zone, never the
    # device's — across midnight and both 2026 clock changes, from a
    # host in another zone.
    TZ=Asia/Kolkata node --test web/src/clock.test.mjs
    # The answer graph (web/src/uis/answers.ts): each node's state, and
    # a failure belonging to its frame alone.
    node --test web/src/uis/answers.test.mjs
    # The inputs (web/src/uis/inputs.ts): one poll, a time limit on
    # every read, three outcomes (a document, none kept, failed since),
    # what the page keeps, an input's age, and Retry-After with a
    # random spread.
    node --test web/src/uis/inputs.test.mjs
    # The host contract suite, the browser's side
    # (web/src/uis/host-contract.json; docs/architecture/hosts.md): the
    # upstream does not answer, answers slowly, answers again, and moves
    # its head while the host holds a copy. The server runs the same
    # scenarios (server/hostcontract_test.go, in `go test` below).
    node --test web/src/uis/host-contract.test.mjs
    # FAILURE TABLES (tools/failure-tables.mjs): every input a UI
    # declares in its ui.json names rows that its failure table in
    # docs/architecture/generated-uis/<id>.md has. The broken fixtures run
    # first.
    node --test tools/failure-tables.test.mjs
    node tools/failure-tables.mjs
    # A module error read by its code (web/src/refusals.ts): an
    # evicted document and a newer version are found whatever the message
    # says, and only a newer version asks for the deployment's build. No
    # host code matches an error's text, and the gate's broken fixture runs
    # first.
    node --test web/src/refusals.test.mjs
    node --test web/src/error-text.test.mjs
    # The outbox (web/src/outbox.ts): writes in order, the same
    # key on every attempt, a network failure or a 5xx waiting, a 401
    # holding the outbox, a refusal held with its reasons, a reload
    # keeping what waits, and the device's time as a claim.
    node --test web/src/outbox.test.mjs
    # The docs site (tools/docs-site.mjs): static pages from the
    # registry. Each capability, op, kernel op, and document kind has
    # its place, and no local link is broken.
    node --test tools/docs-site.test.mjs
    # UI OPS (tools/ui-ops.mjs): every op a generated UI's host asks is
    # in its ui.json, so its assembly links it. A missing op would turn
    # a frame off at runtime with no error. The broken fixtures run
    # first.
    node --test tools/ui-ops.test.mjs
    node tools/ui-ops.mjs
    # DEPENDENCIES (tools/deps-gate.mjs): the module builds' crates, the
    # server binary's Go modules, and the pages' run-time npm packages
    # equal tools/dependencies.txt, each with its reason. The broken
    # fixtures run first.
    node --test tools/deps-gate.test.mjs
    node tools/deps-gate.mjs
    # THE KIND TRACE (tools/kind-trace.mjs; `caps-run kind`): every kind
    # in the kind table traces to a Go handler, ops, and library functions
    # that exist. The broken fixtures run first.
    node --test tools/kind-trace.test.mjs
    node tools/caps-run.mjs kind > /dev/null
    # THE JOURNAL (tools/journal.mjs; docs/architecture/registry.md,
    # "The journal"): the replay's broken fixtures.
    node --test tools/journal.test.mjs
    # THE RUNNER'S SESSIONS (tools/gates/sessions.mjs): each question a
    # host asks, through caps-run over the fixtures, chained by result
    # references, with the answer it must give. It includes the registry's
    # source digest (QAF-77), the agency's clock, a session across
    # commands, and its journal's replay. The broken fixtures run first.
    node --test tools/gates/sessions.test.mjs
    node tools/gates/sessions.mjs
    # ARCHITECTURE GATES — the laws as assertions (spelled out in
    # crates/AGENTS.md), because prose does
    # not enforce:
    #   1. the NETWORK is the core data model; GTFS is a serialization
    #      spec for external data sharing. Model-side capabilities do
    #      not depend on the GTFS layer, and the boundary codec's
    #      whole-feed write never leaves the codec's homes.
    #   2. nobody reinvents the monolith inside a module.
    # Law 1a, and the rule that keeps capabilities from calling
    # capabilities once they link into one binary: the
    # dependency graph from `cargo metadata`, deny by default — a
    # capability depends only on cap-abi and libraries, only the
    # generated assemblies depend on capabilities, and only the
    # codec's homes read the GTFS layer (transit-codec) in production
    # code (dev-dependencies may build fixtures through it). The same
    # bin checks every capability crate forbids `unsafe` (the unsafe
    # exports are the kernel's `assembly!`) and declares no state of
    # its own (no static, no thread_local: documents live in the
    # kernel's registries, whose constructor is private to the kernel
    # — the compile_fail doctests on cap_abi::registry::Registry).
    # Each gate fails on a broken fixture in
    # crates/cap-abi/tests/arch_gates_test.rs.
    cargo run --quiet -p cap-abi --bin arch-gates
    # Law 1b and determinism over the crates' source
    # (tools/gates/crate-laws.mjs): the whole-feed write only in the
    # codec's homes, and no clock, randomness, or environment in a
    # module. The broken fixtures run first.
    node --test tools/gates/crate-laws.test.mjs
    node tools/gates/crate-laws.mjs
    # WEB GATES (web/gates/): parity (docs/architecture/web.md, "Parity":
    # web/ holds host glue and render components, never answers), and
    # the generated UIs' layout (docs/architecture/generated-uis.md: each
    # UI's descriptor, page, and spec, and its own host layer). The broken
    # fixtures run first.
    node --test web/gates/gates.test.mjs
    node web/gates/parity.mjs
    node web/gates/uis.mjs
    # -race: the artifact is one non-reentrant instance; the host must
    # serialize invokes (see server/race_test.go — the gate watches it).
    # The test binary compiles each module once, all at the same time,
    # and keeps the machine code on disk between runs
    # (server/compile_test.go; WASM_TEST_CACHE=off compiles afresh).
    cd server && go test -race ./...

# A UI at its own URL (docs/architecture/release-artifacts.md,
# "Each UI at its own URL"; DEPLOYMENTS-SERVER.md): one of the UI's
# logged builds (build.<id>, from the server's -ui-dist), written out
# to OUT for the UI's host. The head unless BUILD names one. OUT must be
# absent or empty. A rollback is POST /builds?ui=<id>, then this again.
#
#   just ui-deploy https://transit.agency.example stops out/stops
ui-deploy server ui out build="":
    node tools/ui-deploy.mjs "{{ server }}" "{{ ui }}" "{{ out }}" {{ if build == "" { "" } else { quote(build) } }}

# The HTTP contract's lock (docs/architecture/release-artifacts.md,
# "The HTTP contract"): server/openapi.json copied to
# server/openapi.lock.json once the compatibility gate passes. Run it
# when a release ships, never to make a breaking change pass.
http-lock:
    cd server && go test -count=1 -run 'TestTheContractIsCompatibleWithTheLock|TestEveryRouteIsInTheContract' .
    cp server/openapi.json server/openapi.lock.json

# The documents' schema lock (docs/FORMATS.md, "The schema lock"):
# freeze a new version of a kind, or record an added optional field or
# an added reference declaration in the same version (D-232),
# into the owning crate's locks/. It refuses every other change: that
# one needs a version bump. It also writes the checked-in kind table
# (crates/cap-abi/kinds.json) from the kind list (crates/cap-abi/src/
# kinds.rs). `just caps` (and so `just test`) fails while a kind
# differs from its lock, or the table from the list. It needs no
# module build: it writes the generator's binary (gen-assemblies) and
# runs it.
schema-lock:
    #!/usr/bin/env bash
    set -e
    cargo run --quiet -p cap-abi --bin gen-assemblies
    cargo update --workspace --offline --quiet
    cargo run --quiet -p gen-manifest -- --lock

# Browser e2e (Chromium via Playwright, against the Go server), over
# the wasm-fast modules (`caps-fast`), the pages, and the UIs the specs
# serve alone (the two rider UIs, and the stops and service-change UIs
# at their own origins). The runner starts the server unless :8080 is
# taken. For UI-visible changes: each generated UI's spec, the picker,
# and the published-network spec (module surfaces as the UIs read
# them). Playwright's arguments pass through — one spec, one test by
# name, or every spec when none is given:
#   just e2e e2e/ui-stops.spec.mjs
#   just e2e e2e/ui-service-change.spec.mjs -g "a published pick"
e2e *args: caps-fast types
    cd web && npm run typecheck && npm run build
    cd web && for ui in ride-now stop-display stops service-change; do UI=$ui npx vite build || exit 1; done
    cd web && npx playwright test {{ args }}

# Every spec over the release modules (`caps`). Run it when a unit is
# done, and before a release.
e2e-release: caps build (build-ui "ride-now") (build-ui "stop-display") (build-ui "stops") (build-ui "service-change")
    cd web && npx playwright test

# Both runners — for changes that touch both sides.
check: test e2e

# Both runners over the release modules: when a unit is done.
check-release: test-release e2e-release

# Browser performance baselines (docs/TESTING.md, "Performance
# baselines"): the real feed and recorded live feeds through a UI,
# measured over the DevTools protocol — reports, never gates. The
# profiling build keeps the capability modules' name sections (so a
# CPU profile names Rust functions) and serves an unminified frontend
# from web/dist-perf; the next `just caps` (or `just test`) restores
# the stripped modules. PERF_RT picks the recording
# (`node web/perf/record-rt.mjs`), PERF_FEED the feed; the stop
# display's soak takes PERF_STOP (the stop, by name) and PERF_LOOPS
# (how many times the recording replays).
perf *args:
    #!/usr/bin/env bash
    set -e
    CARGO_PROFILE_RELEASE_STRIP=debuginfo just caps
    just types
    cd web && npx vite build --minify false --outDir dist-perf --emptyOutDir
    npx playwright test -c perf/playwright.config.mjs {{args}}

# Real-data soak. Hermetic tests skip both parts.
# - transit-streets against the downloaded county export: point
#   COUNTY_GEOJSON at the file (e.g. the CTRAN Trapeze roads geojson
#   from the open data portal).
# - The schemas against the agency's feed (docs/FORMATS.md, "The
#   schema lock"; generator::checks::soak, run by gen-manifest's
#   tests): SOAK_FEED, else
#   testdata/ctran-gtfs.zip, through the ops a server runs on it. Every
#   document they answer matches its locked schema. A mismatch is a QA
#   finding.
soak:
    #!/usr/bin/env bash
    set -e
    if [ -z "$COUNTY_GEOJSON" ]; then
        echo "set COUNTY_GEOJSON to the downloaded county roads geojson"
        exit 1
    fi
    cargo test --release -p transit-streets --test county_data -- --nocapture
    SOAK_FEED="$(realpath "${SOAK_FEED:-testdata/ctran-gtfs.zip}")" cargo test --release -p gen-manifest --test schemas the_agencys_feed -- --nocapture

# Regenerate the e2e GTFS fixture (web/e2e/fixtures/sample-gtfs.zip).
fixtures:
    cd web/e2e && python3 make_fixtures.py

# Regenerate the Clark County population layer
# (testdata/clark-population.geojson, from public-domain census
# sources — the real-data dogfood asset for analysis.type_coverage).
population:
    node testdata/generate-population.mjs

# ---------------------------------------------------------------
# run
# ---------------------------------------------------------------

# Serve the built app and APIs on :8080, on the settings in
# server/dev.toml (the fixture feed and streets, a store in memory,
# attachments, the stub work order system, the dev feed relay) and
# server/local.toml over it if you keep one (copy
# server/local.example.toml: your feed, store, sign-in, geocoder).
# The positional arguments beat both:
#
#   just serve                                   # the settings files only
#   just serve path/to/gtfs.zip                  # another feed
#   just serve path/to/gtfs.zip roads.geojson    # + another street layer
#   just serve path/to/gtfs.zip "" "Test Transit"   # + manifest publisher
#   just serve path/to/gtfs.zip "" "" 127.0.0.1:8081
#
# The feed and street layer are a path (resolved relative to the repo
# root, wherever you run just from), an http(s) URL, or bucket:<key>
# (the object inputs/<key> in the store's bucket).
serve feed="" streets="" publisher="" addr="":
    #!/usr/bin/env bash
    set -e
    # The settings are server/dev.toml, then server/local.toml if you
    # keep one (your feed, store, sign-in, geocoder — copy
    # local.example.toml); the arguments here beat both. A URL or
    # bucket:<key> source passes as given; a path is resolved
    # from the repo root.
    src() { case "$1" in http://*|https://*|bucket:*) echo "$1" ;; *) realpath -m "$1" ;; esac; }
    args=(-config dev.toml)
    if [ -f server/local.toml ]; then args+=(-config local.toml); fi
    if [ -n "{{ feed }}" ]; then args+=(-feed "$(src "{{ feed }}")"); fi
    if [ -n "{{ streets }}" ]; then args+=(-streets "$(src "{{ streets }}")"); fi
    if [ -n "{{ publisher }}" ]; then args+=(-publisher "{{ publisher }}"); fi
    if [ -n "{{ addr }}" ]; then args+=(-addr "{{ addr }}"); fi
    cd server
    exec go run . "${args[@]}"

# One UI built alone (web/dist-ui/<ui>/): only its page, its scripts,
# and what it runs (vite.config.js, "ONE UI") — what `just export`
# deploys. Each rider UI deploys on its own.
build-ui ui: caps types
    cd web && npm run typecheck && UI={{ ui }} npx vite build

# One UI as a static deployment (docs/architecture/static-deployment.md; the operator's guide is DEPLOYMENTS.md):
# the server boots on the feed (and the street layer, if given) exactly
# as `just serve` does, writes the bundle into OUT, and exits — that
# UI's page, `manifest`, and every published document as blobs/<hash>
# and blobs/<hash>.gz, the paths the server itself answers. Upload OUT
# to any static host; never delete blobs an earlier upload wrote (an
# open page pinned to the old manifest still reads them). LIVE_FEEDS
# names the deployment's own live-feeds.json (the agency's feeds, no
# proxy), replacing the dev copy. The rider UIs (ride-now,
# stop-display) are the ones that run without a server.
#
#   just export ride-now out/ path/to/gtfs.zip [roads.geojson ["Publisher"]]
#   LIVE_FEEDS=deploy/live-feeds.json just export stop-display out/ gtfs.zip
export ui out feed streets="" publisher="": (build-ui ui)
    #!/usr/bin/env bash
    set -e
    src() { case "$1" in http://*|https://*|bucket:*) echo "$1" ;; *) realpath -m "$1" ;; esac; }
    out=$(realpath -m "{{ out }}"); feed=$(src "{{ feed }}"); streets="{{ streets }}"
    if [ -n "$streets" ]; then streets=$(src "$streets"); fi
    live="${LIVE_FEEDS:-}"; if [ -n "$live" ]; then live=$(realpath -m "$live"); fi
    cd server
    args=(-export "$out" -dist "../web/dist-ui/{{ ui }}" -feed "$feed")
    if [ -n "$streets" ]; then args+=(-streets "$streets"); fi
    if [ -n "{{ publisher }}" ]; then args+=(-publisher "{{ publisher }}"); fi
    if [ -n "$live" ]; then args+=(-live-feeds "$live"); fi
    exec go run . "${args[@]}"

# One UI as a static deployment that follows the agency's GTFS URL:
# the same bundle as `just export`, but the manifest names the
# feed's URL and no network is imported — each page fetches the zip,
# imports it itself, and checks the URL once a day, so a new feed needs
# no redeploy. The URL must answer with CORS. A street layer still ships
# (walking from a place); the walk table between stops needs the
# network, so it does not.
#
#   just export-feed-url stop-display out/ https://agency.example/gtfs.zip [roads.geojson ["Publisher"]]
export-feed-url ui out url streets="" publisher="": (build-ui ui)
    #!/usr/bin/env bash
    set -e
    src() { case "$1" in http://*|https://*|bucket:*) echo "$1" ;; *) realpath -m "$1" ;; esac; }
    out=$(realpath -m "{{ out }}"); streets="{{ streets }}"
    if [ -n "$streets" ]; then streets=$(src "$streets"); fi
    live="${LIVE_FEEDS:-}"; if [ -n "$live" ]; then live=$(realpath -m "$live"); fi
    cd server
    args=(-export "$out" -dist "../web/dist-ui/{{ ui }}" -feed-url "{{ url }}")
    if [ -n "$streets" ]; then args+=(-streets "$streets"); fi
    if [ -n "{{ publisher }}" ]; then args+=(-publisher "{{ publisher }}"); fi
    if [ -n "$live" ]; then args+=(-live-feeds "$live"); fi
    exec go run . "${args[@]}"

# The sync store's offline tools (server/cmd/store; only verify runs a module):
#   just store verify [-wasm M] [-kinds K] <url>
#                                    read the whole store, exit 1 on the first
#                                    break; cap-log (M) folds each log checkpoint,
#                                    and reports each broken reference of each
#                                    log's head under the kind table K (default
#                                    web/pkg/kinds.json; empty: none), with the
#                                    entry that broke it, and the records in a
#                                    version newer than K reads (unjudged)
#   just store gc [-grace 168h] <url>  collect orphans — the collector's
#                                    own delete-capable credentials
#   just store dump <url> <dir>      write every object to a folder (each at
#                                    <dir>/<key>, a blob's envelope beside it), to
#                                    read a store with no object storage tools;
#                                    read only. <dir> is relative to server/: give
#                                    an absolute path
#   just store copy <dir> <url>      carry an older build's directory store into
#                                    a bucket, then verify it
# The store's offline tools: verify | gc | dump | copy | revert (a URL is passed as given).
[positional-arguments]
store +args:
    cd server && go run ./cmd/store "$@"

# The server as one container image, built with ko (no Dockerfile):
# the Go binary on a distroless base, with the server's module
# (web/pkg/, with the kind table), the pages (web/dist), each UI's own bundle
# (web/dist-ui/<id>, for -ui-dist), and the geocoder policy in
# server/kodata/, which ko copies into the image at $KO_DATA_PATH.
# The flags' defaults find them there, and the listen address is
# :8080, so the image runs with only its deployment's flags (-store,
# -feed, ...). With no repo it writes target/server-image.tar for
# `podman load`; with a repo (ghcr.io/agency/transit-server) it pushes
# linux/amd64 and linux/arm64 under the tag. Operator's guide:
# DEPLOYMENTS-SERVER.md.
server-image repo="" tag="latest": caps build (build-ui "ride-now") (build-ui "stop-display") (build-ui "stops") (build-ui "service-change") (build-ui "service-design")
    #!/usr/bin/env bash
    set -e
    rm -rf server/kodata
    mkdir -p server/kodata/pkg server/kodata/dist-ui
    # The server's module (`just caps`).
    cp web/pkg/server.wasm server/kodata/pkg/
    # The kind table and the schemas it names (-kinds; D-202): the
    # receive path reads the table, and the store keeps the schemas.
    cp web/pkg/kinds.json server/kodata/pkg/
    cp -r web/pkg/schemas server/kodata/pkg/schemas
    cp -r web/dist server/kodata/dist
    # Each UI's own bundle, for -ui-dist <id>=/var/run/ko/dist-ui/<id>
    # (each UI at its own URL).
    for ui in ride-now stop-display stops service-change service-design; do
        cp -r "web/dist-ui/$ui" "server/kodata/dist-ui/$ui"
    done
    cp server/geocoder.json server/kodata/
    cd server
    if [ -z "{{ repo }}" ]; then
        KO_DOCKER_REPO=transit-server ko build --bare --push=false \
          --tarball ../target/server-image.tar --tags "{{ tag }}" .
        echo "image: target/server-image.tar — podman load -i target/server-image.tar"
    else
        KO_DOCKER_REPO="{{ repo }}" ko build --bare --tags "{{ tag }}" \
          --platform linux/amd64,linux/arm64 .
    fi

store_dev_image := "docker.io/chrislusf/seaweedfs:4.50"
fake_gcs_image := "docker.io/fsouza/fake-gcs-server@sha256:797ce226d62f947c009dc40246b30cfb456b8473d8241407f9d6f2c04e4d69ef"
store_dev_env := "AWS_ACCESS_KEY_ID=dev AWS_SECRET_ACCESS_KEY=devsecret"
store_dev_url := "s3://transit/dev?endpoint=http://127.0.0.1:8333"

# Local object stores for the sync store, in podman: SeaweedFS (S3, on
# 127.0.0.1:8333, its data in the volume transit-store-dev so a store
# survives a restart — the one `just serve` uses) and fake-gcs-server
# (GCS, :4443, for the contract only). Local validation only — the engine assumes the object
# contract (server/store/backend.go) and gives no endpoint a code path
# of its own; `just store-contract` says whether each honors it.
store-dev:
    #!/usr/bin/env bash
    set -euo pipefail
    podman run -d --replace --name transit-store-dev --memory 1g \
      -p 127.0.0.1:8333:8333 -v transit-store-dev:/data \
      -e AWS_ACCESS_KEY_ID=dev -e AWS_SECRET_ACCESS_KEY=devsecret \
      {{ store_dev_image }} server -dir=/data -s3 -s3.port=8333 -master.telemetry=false >/dev/null
    podman run -d --replace --name transit-fake-gcs --memory 512m -p 127.0.0.1:4443:4443 \
      {{ fake_gcs_image }} -scheme http -port 4443 -public-host 127.0.0.1:4443 >/dev/null
    for i in $(seq 1 60); do
      if curl -s -o /dev/null http://127.0.0.1:8333/ \
         && curl -s -o /dev/null http://127.0.0.1:4443/storage/v1/b; then break; fi
      sleep 1
    done
    echo "s3.bucket.create -name transit" | podman exec -i transit-store-dev weed shell >/dev/null 2>&1 || true
    echo "SeaweedFS :8333, fake-gcs-server :4443 (just store-dev-stop ends them)."
    echo "Serve over SeaweedFS:"
    echo "  store = \"{{ store_dev_url }}\" in server/local.toml, then"
    echo "  {{ store_dev_env }} just serve"

store-dev-stop:
    podman rm -f transit-store-dev transit-fake-gcs >/dev/null 2>&1 || true

# The store's laws — the backend contract and every engine law — run
# against each local store (`just store-dev` first), each test under its
# own fresh prefix. Opt-in, never in `just test`. With URL, they run
# against that store alone, with the provider's usual credentials: an
# operator's check of their own bucket before going live
# (DEPLOYMENTS-SERVER.md, "Prepare the bucket").
store-contract URL="":
    #!/usr/bin/env bash
    set -euo pipefail
    cd server
    if [ -n "{{ URL }}" ]; then
      echo "== {{ URL }}"
      STORE_TEST_URL='{{ URL }}' go test -race -count=1 ./store/
      exit 0
    fi
    echo "== s3 (SeaweedFS)"
    {{ store_dev_env }} STORE_TEST_URL='s3://transit/contract?endpoint=http://127.0.0.1:8333' \
      go test -race -count=1 ./store/
    echo "== gs (fake-gcs-server)"
    STORAGE_EMULATOR_HOST=127.0.0.1:4443 STORE_TEST_URL='gs://transit/contract' \
      go test -race -count=1 ./store/

# Frontend dev loop: vite with hot reload on :5173 (proxies /rt — the
# dev feed relay, -rt-proxy — and /geocode), with the sync server
# alongside on :8080 (logs:
# /tmp/transit-server.log), on the same settings files as serve.
dev: caps types
    #!/usr/bin/env bash
    set -e
    # The server on server/dev.toml (+ server/local.toml), as `just serve`.
    cfg=(-config dev.toml)
    if [ -f server/local.toml ]; then cfg+=(-config local.toml); fi
    (cd server && go build -o /tmp/transit-server . && \
      /tmp/transit-server "${cfg[@]}" >/tmp/transit-server.log 2>&1 & echo $! >/tmp/transit-server.pid)
    trap 'kill "$(cat /tmp/transit-server.pid)" 2>/dev/null || true' EXIT
    cd web && npx vite

# The UI QA loop: `dev` for walking a page on real data. Modules build
# under the wasm-fast profile (a library change in ~10 s, not ~10 min);
# the sync server serves FEED (default: the real C-TRAN feed when it is
# fetched, else the e2e fixture) from an in-memory store, so publishing
# works, on :8091 (logs: /tmp/transit-qa.log); vite serves the pages with
# hot reload on :5174 — ports of its own, beside `just serve` and `just
# dev`. A page edit shows without a build; after a Rust
# change, `just caps-fast` and reload — the server stays up. Drive it
# with tools/ui-shot.mjs (one browser profile, so a forked plan stays).
qa feed="": caps-fast types
    #!/usr/bin/env bash
    set -e
    feed="{{feed}}"
    if [ -z "$feed" ]; then
        feed=testdata/ctran-gtfs.zip; [ -f "$feed" ] || feed=web/e2e/fixtures/sample-gtfs.zip
    fi
    feed="$(realpath "$feed")"
    # server/dev.toml (+ local.toml), on its own port, its store in memory.
    cfg=(-config dev.toml)
    if [ -f server/local.toml ]; then cfg+=(-config local.toml); fi
    (cd server && go build -o /tmp/transit-qa-server . && \
      /tmp/transit-qa-server "${cfg[@]}" -addr 127.0.0.1:8091 -store mem:// -feed "$feed" >/tmp/transit-qa.log 2>&1 & echo $! >/tmp/transit-qa.pid)
    trap 'kill "$(cat /tmp/transit-qa.pid)" 2>/dev/null || true' EXIT
    for i in $(seq 1 60); do curl -sf -o /dev/null http://127.0.0.1:8091/manifest && break; sleep 1; done
    curl -sf -o /dev/null http://127.0.0.1:8091/manifest || { echo "qa: the server did not start (/tmp/transit-qa.log):"; tail -3 /tmp/transit-qa.log; exit 1; }
    echo "qa: $feed on :8091; pages on http://127.0.0.1:5174/"
    cd web && TRANSIT_SERVER=http://127.0.0.1:8091 npx vite --host 127.0.0.1 --port 5174 --strictPort

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

# Fetch the basemap's glyphs (fonts) and sprites (icons) once into
# web/public/basemap-assets/ (gitignored, ~14 MB: protomaps/basemaps-assets
# at a pinned commit, the light sprites and every font stack; fonts are
# SIL OFL, sprites derive from MIT tangrams/icons — the licenses ride
# along). The map probes for them beside the tiles and reads labels
# from there, so a display that loses its uplink loses no label; without
# them the style reads protomaps.github.io (option 2). Rebuild the
# frontend afterwards (`just build`). A re-run at the same pin is a no-op.
#
#   just basemap-assets
basemap_assets_commit := "028c18f713baecad011301ff7a69acc39bcc2ae7"

basemap-assets:
    #!/usr/bin/env bash
    set -euo pipefail
    dest="{{ justfile_directory() }}/web/public/basemap-assets"
    if [ "$(cat "$dest/.commit" 2>/dev/null)" = "{{ basemap_assets_commit }}" ]; then
        echo "basemap-assets already at {{ basemap_assets_commit }}"
        exit 0
    fi
    tmp="$(mktemp -d)"
    trap 'rm -rf "$tmp"' EXIT
    curl -fsSL "https://codeload.github.com/protomaps/basemaps-assets/tar.gz/{{ basemap_assets_commit }}" \
        | tar -xz -C "$tmp" --strip-components=1 --wildcards '*/fonts' '*/sprites/v4/light*'
    echo "{{ basemap_assets_commit }}" > "$tmp/.commit"
    chmod 755 "$tmp"
    rm -rf "$dest"
    mv "$tmp" "$dest"
    trap - EXIT
    du -sh "$dest"

# ---------------------------------------------------------------
# geocoder
# ---------------------------------------------------------------

# Address search is an external service, not a document: a local
# Pelias (github.com/pelias/docker, its portland-metro project —
# OSM, OpenAddresses, Who's On First, TIGER interpolation) in
# .pelias/ (gitignored: pelias/docker at a pinned commit in docker/,
# never written to; data/ ~6.5 GB). The server proxies it at /geocode when its
# settings name it (geocoder = "http://127.0.0.1:4000" in server/local.toml):
#
#   just geocoder-build      # once: fetch, download, import (~1 h)
#   just geocoder            # start it on 127.0.0.1:4000
#   just serve
#   just geocoder-stop       # stop every container
#
# Needs docker compose (or podman + podman-compose; rootless podman
# gets a keep-id compose override and a docker.io short-name config
# here). The
# project's transit importer is skipped: stops come from the
# network, never the geocoder.
pelias_commit := "3dfa07d580416edd7a27c2d4ff5976c8c1cc6ebc"

[private]
pelias *args:
    #!/usr/bin/env bash
    set -e
    # .pelias/ holds pelias/docker at the pinned commit (docker/, a
    # plain tarball extract — no nested git repository — never written
    # to), its data (data/), and the files this recipe generates
    # (podman/).
    root="{{ justfile_directory() }}/.pelias"
    clone="$root/docker"
    if [ "$(cat "$clone/.commit" 2>/dev/null)" != "{{ pelias_commit }}" ]; then
        rm -rf "$clone" && mkdir -p "$clone"
        curl -fsSL "https://codeload.github.com/pelias/docker/tar.gz/{{ pelias_commit }}" \
            | tar -xz -C "$clone" --strip-components=1
        echo "{{ pelias_commit }}" > "$clone/.commit"
    fi
    project="$clone/projects/portland-metro"
    mkdir -p "$root/data"
    # The environment wins over the project's .env, in the pelias CLI
    # and in compose alike.
    export DATA_DIR="$root/data" COMPOSE_PROJECT_NAME=pelias
    if command -v podman >/dev/null; then
        # Rootless podman: unqualified images resolve on docker.io, and
        # every container runs as the invoking user (keep-id, outside a
        # pod — podman refuses keep-id in one) so the data bind mount
        # is writable.
        mkdir -p "$root/podman"
        printf 'unqualified-search-registries = ["docker.io"]\nshort-name-mode = "permissive"\n' > "$root/podman/registries.conf"
        { echo 'x-podman: { in_pod: false }'; echo 'services:'
          awk '/^services:/ { on = 1; next } /^[^ ]/ { on = 0 }
               on && /^  [a-z-]+:$/ { print $0 " { userns_mode: keep-id }" }' "$project/docker-compose.yml"
        } > "$root/podman/compose.override.yml"
        export CONTAINERS_REGISTRIES_CONF="$root/podman/registries.conf"
        export COMPOSE_FILE="$project/docker-compose.yml:$root/podman/compose.override.yml"
    fi
    cd "$project" && "$clone/pelias" {{ args }}

geocoder-build:
    just pelias compose pull
    just pelias elastic start
    just pelias elastic wait
    just pelias elastic create
    just pelias download wof
    just pelias download oa
    just pelias download osm
    just pelias download tiger
    just pelias prepare polylines
    just pelias prepare placeholder
    just pelias prepare interpolation
    just pelias import wof
    just pelias import oa
    just pelias import osm
    just pelias import polylines
    just pelias compose down

geocoder:
    just pelias compose up -d elasticsearch api libpostal placeholder pip interpolation

geocoder-stop:
    just pelias compose down

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
# where "tests passed" is a lie. Generated code (transit-rt) is
# excluded; needs cargo-mutants:
# cargo install cargo-mutants
mutants:
    cargo mutants -p transit-codec -p transit-router -p transit-sync -p transit-plans

# Model-check the sync store engine (server/store/store.tla, run with
# TLC) — processes sharing one bucket and the offline GC beside them:
# every record names a durable blob, the log chains, records never
# change, the head only moves forward (docs/FORMATS.md). OPTIONAL: run it only when a change
# touches the sync engine (server/store/, the /pointers + /blobs +
# /edits store surface, or the spec itself) — nothing else needs Java.
# Needs Java 11+ (current tla2tools) and tla2tools.jar from
# github.com/tlaplus/tlaplus/releases (the user's install; TLA_JAR or
# ~/bin/tla2tools.jar). Deliberately NOT part of `just test`.
# On an immutable desktop (Fedora Atomic), do not layer Java onto the
# base with rpm-ostree. Use a Temurin tarball under ~/.local, symlinked
# into ~/.local/bin, or a toolbox (`sudo dnf install java-21-openjdk`
# inside it; toolbox shares $HOME, so the jar is already there) and run
# `toolbx run -c <name> bash -lc 'just spec'`.
# Bounds: store.cfg (small, pinned) by default; the exhaustive tier is
# SPEC_CFG=server/store/store-exhaustive.cfg just spec (hash and
# process symmetry, bigger bounds, still exhaustive; ~5 min).
# SPEC_CFG=server/store/store-race.cfg is EXPECTED TO FAIL: GC without
# its settle step, and the trace that shows why it has one.
# TLC_WORKERS / TLC_XMX size the run.
spec:
    #!/usr/bin/env bash
    set -e
    jar="${TLA_JAR:-$HOME/bin/tla2tools.jar}"
    if ! command -v java >/dev/null 2>&1 || [ ! -f "$jar" ]; then
        echo "just spec is OPTIONAL (only for sync-engine changes) and needs Java 11+ and tla2tools.jar"
        echo "(set TLA_JAR=/path/to/tla2tools.jar; get it from github.com/tlaplus/tlaplus/releases)"
        echo "nothing else needs Java: 'just test' runs without it"
        exit 1
    fi
    # Bounds: store.cfg (small, the pinned quick check) by default;
    # SPEC_CFG=server/store/store-exhaustive.cfg for the bigger run
    # (blob-hash symmetry makes it exhaustive at much larger bounds).
    # TLC_WORKERS (default auto) and TLC_XMX (e.g. 8g) size the run.
    cfg="${SPEC_CFG:-server/store/store.cfg}"
    # TLC's state files (hundreds of MB) go to a scratch directory, never
    # the tree, and a failing check prints its trace instead of writing
    # store_TTrace_* files beside the spec.
    meta=$(mktemp -d)
    trap 'rm -rf "$meta"' EXIT
    java -XX:+UseParallelGC ${TLC_XMX:+-Xmx"$TLC_XMX"} -cp "$jar" tlc2.TLC \
        -workers "${TLC_WORKERS:-auto}" -metadir "$meta" -noGenerateSpecTE \
        -config "$cfg" server/store/store.tla
