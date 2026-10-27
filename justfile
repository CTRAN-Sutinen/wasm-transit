# rustwasm — task runner. Run `just` (or `just --list`) to see all
# recipes. The project has four layers, one recipe group each:
#
#   Rust  — the transit-* libraries and cap-* capability modules (cargo)
#   Go    — server/ (the sync server: wazero host over cap-sync, proxy)
#   Node  — crates/cap-abi/caps-smoke.mjs (runs the modules directly)
#   Web   — web/ (Svelte + Vite) and Playwright e2e
#
# There is no monolith artifact anymore (docs/PLAN.md U7): every
# host instantiates the capability modules it needs — the browser
# fetches them from web/public/caps/, the sync server hosts
# cap-sync at web/pkg/cap_sync.wasm.

default:
    @just --list

# ---------------------------------------------------------------
# build
# ---------------------------------------------------------------

# Build the capability modules (the capability bus: horizontal
# capabilities assembled per user) and regenerate the capability
# manifest — hash-pinned modules in web/public/caps/, served under
# their content hash so the HTTP cache holds them forever. The sync
# server's module also lands at the stable path web/pkg/cap_sync.wasm
# (server default -wasm flag; rebuilt with the modules, never by hand).
caps:
    #!/usr/bin/env bash
    set -e
    cargo build --release -p cap-gtfs -p cap-router -p cap-analysis -p cap-network -p cap-rt -p cap-board -p cap-runcut -p cap-calendar -p cap-directory -p cap-streets -p cap-sync -p cap-plans -p cap-author -p cap-divert --target wasm32-unknown-unknown
    cargo run --quiet -p cap-abi --bin gen-manifest
    mkdir -p web/pkg
    cp target/wasm32-unknown-unknown/release/cap_sync.wasm web/pkg/cap_sync.wasm

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

# All host-side tests: Rust suites, Go suite, artifact smoke in Node,
# capability-module smoke (needs `caps` built — hash-pinned modules
# and the manifest the smoke verifies against).
# TS_RS_EXPORT_DIR keeps the ts-rs bindings regenerating into
# web/src/types on every run (.cargo/config.toml pins the same path
# for plain cargo runs; without either, ts-rs scatters them into
# crates/*/bindings).
test: caps
    #!/usr/bin/env bash
    set -e
    export TS_RS_EXPORT_DIR="$PWD/web/src/types"
    cargo test
    node crates/cap-abi/caps-smoke.mjs
    # The agent runner, gated with chained sessions — every op
    # references its documents through $load/$op result references
    # (no hand-copied hashes; the runner resolves them in-session).
    # The directory capability's gate: the pickers' one question over
    # the network document, one chained session — stops, the names
    # join, routes, and route 10's patterns (the model's directions).
    node tools/caps-run.mjs directory \
      --op @web/public/caps/sample-network-load.json \
      --op '{"op":"stops","network_hash":"$load_network.network_hash","limit":2}' \
      --op '{"op":"stops_by_ids","network_hash":"$load_network.network_hash","ids":["A","ghost","STN"]}' \
      --op '{"op":"routes","network_hash":"$load_network.network_hash"}' \
      --op '{"op":"route_stop_sequence","network_hash":"$load_network.network_hash","route_id":"10"}' \
      | grep -q '"stop_id":"A"'
    # The board capability's gate: the station pin answers the union
    # of its bays (walking-group pooling), and the personal board
    # chains its watchlist document — all references chained, no
    # hand-copied hashes.
    node tools/caps-run.mjs board \
      --op @web/public/caps/sample-network-load.json \
      --op '{"op":"stop_board","network_hash":"$load_network.network_hash","stop_id":"STN","date":"2026-10-23","from_secs":25200,"limit":10}' \
      --op '{"op":"load_watchlist","document":{"version":"v2","stops":[{"id":"A","label":"home"}]}}' \
      --op '{"op":"watchlist_board","watchlist_hash":"$load_watchlist.watchlist_hash","network_hash":"$load_network.network_hash","date":"2026-10-23","from_secs":25200,"limit":3}' \
      | grep -q '"label":"home"'
    # The network capability's gate: load → import, chained; the
    # dropped single-stop trip must be REPORTED (never silent).
    node tools/caps-run.mjs network --load web/public/caps/sample-gtfs.zip \
      --op '{"op":"import","feed_hash":"$load.feed_hash"}' | grep -q '"kind":"dropped_trips"'
    # The feed's identity — the feed document capability's one
    # question.
    node tools/caps-run.mjs gtfs --load web/public/caps/sample-gtfs.zip \
      --op '{"op":"feed_info","feed_hash":"$load.feed_hash"}' \
      | grep -q '"feed_version":"2026.10.1"'
    # The realtime capability's gate: the dispatch leg as a JOIN — the
    # fixture feed on bulk op 1, the fixture payload on op 0, and the
    # dispatcher's most-asked question answered in one op (t10's
    # predicted departure at A: 08:00 + 120 s).
    node tools/caps-run.mjs rt --load 1:web/public/caps/sample-gtfs.zip \
      --load web/e2e/fixtures/sample-rt.pb \
      --op '{"op":"trip_progress","feed_hash":"$1.feed_hash","rt_hash":"$2.rt_hash","trip_id":"t10"}' \
      | grep -q '"predicted_departure":28920'
    # The clock: $TODAY resolves host-side (the runner is the host) —
    # a valid date must answer, whatever day the gate runs on.
    node tools/caps-run.mjs calendar \
      --op @web/public/caps/sample-network-load.json \
      --op '{"op":"active_services","network_hash":"$load_network.network_hash","date":"$TODAY"}' \
      | grep -q '"op":"active_services"'
    # The cross-capability session: one session imports the fixture
    # feed, hands the DOCUMENT to the board by reference (no file in
    # between), addresses results by --as name, and feeds the numeric
    # clock to a seconds field — the identities must agree across the
    # two modules (one document, one truth).
    node tools/caps-run.mjs network --load web/public/caps/sample-gtfs.zip \
      --as net --op '{"op":"import","feed_hash":"$load.feed_hash"}' \
      --cap board --as board_net --op '{"op":"load_network","network":"$net.network"}' \
      --op '{"op":"stop_board","network_hash":"$board_net.network_hash","stop_id":"A","date":"$TODAY","from_secs":"$NOW_SECS","limit":1}' \
      | node -e 'const r = require("fs").readFileSync(0, "utf8").trim().split("\n").map(JSON.parse); const id = (n) => r.find((x) => x.as === n).ok.network_hash; if (id("net") !== id("board_net") || r.at(-1).ok?.op !== "stop_board") process.exit(1)'
    # The live board — the parity chain every host runs (browser,
    # runner, server): import → trip map → cap-rt resolves the rt
    # snapshot into THE live document → the board reads it by hash.
    # 08:01 at A: t10 (08:00, +120 s) is still to come, expected 08:02.
    node tools/caps-run.mjs network --as feed --load web/public/caps/sample-gtfs.zip \
      --as net --op '{"op":"import","feed_hash":"$feed.feed_hash"}' \
      --as map --op '{"op":"trip_map","network_hash":"$net.network_hash"}' \
      --cap rt --as rtfeed --load 1:web/public/caps/sample-gtfs.zip \
      --as snap --load web/e2e/fixtures/sample-rt.pb \
      --as rtmap --op '{"op":"load_trip_map","trip_map":"$map.map"}' \
      --as live --op '{"op":"live","feed_hash":"$rtfeed.feed_hash","rt_hash":"$snap.rt_hash","trip_map_hash":"$rtmap.trip_map_hash"}' \
      --cap board --as bnet --op '{"op":"load_network","network":"$net.network"}' \
      --as blive --op '{"op":"load_live","live":"$live.live"}' \
      --op '{"op":"stop_board","network_hash":"$bnet.network_hash","stop_id":"A","date":"2026-10-23","from_secs":28860,"limit":3,"live_hash":"$blive.live_hash"}' \
      | node -e 'const r = require("fs").readFileSync(0, "utf8").trim().split("\n").map(JSON.parse); const ok = (n) => r.find((x) => x.as === n).ok; const row = r.at(-1).ok.board.rows.find((x) => x.expected_secs != null); if (ok("live").live_hash !== ok("blive").live_hash || !row || row.expected_secs !== 28920 || row.status !== "late") process.exit(1)'
    # The runcut capability's gate: the day's blocks (the index) into
    # one block's chain (the detail), with the edit ripple chained in
    # session (the connection-health read: 600 − 300).
    node tools/caps-run.mjs runcut \
      --op @web/public/caps/sample-network-load.json \
      --op '{"op":"blocks","network_hash":"$load_network.network_hash","date":"2026-10-23"}' \
      --op '{"op":"load_edits","network_hash":"$load_network.network_hash","records":[{"id":"e1","diversion":{"id":"d1","route_id":"10","direction_id":null,"first_kept_stop":"A","last_kept_stop":"B","skipped_stops":["C"],"detour":null,"added_secs":300,"start_date":"2026-10-23","end_date":"2026-10-23"},"status":"accepted","rev":1,"updated_at":1}]}' \
      --op '{"op":"block_trips","network_hash":"$load_network.network_hash","edits_hash":"$load_edits.edits_hash","block_id":"B1","date":"2026-10-23"}' \
      | grep -q '"layover_remaining_secs":300'
    # The map's geometry is answered, never computed by a host (W4):
    # a plan's ridden segment (router), the diverted 08:40 trip drawn
    # through its patched stops A→B (runcut), and a route framed by
    # its two patterns (directory) — one chained session.
    node tools/caps-run.mjs router \
      --op @web/public/caps/sample-network-load.json \
      --as plan --op '{"op":"plan","network_hash":"$load_network.network_hash","from":"A","to":"B","date":"2026-10-23","depart_secs":25200,"max_legs":4}' \
      --op '{"op":"itinerary_paths","network_hash":"$load_network.network_hash","date":"2026-10-23","itinerary":"$plan.itinerary"}' \
      --cap runcut --op @web/public/caps/sample-network-load.json \
      --as edits --op '{"op":"load_edits","network_hash":"$load_network.network_hash","records":[{"id":"e1","diversion":{"id":"d1","route_id":"10","direction_id":null,"first_kept_stop":"A","last_kept_stop":"B","skipped_stops":["C"],"detour":null,"added_secs":300,"start_date":"2026-10-23","end_date":"2026-10-23"},"status":"accepted","rev":1,"updated_at":1}]}' \
      --as trip --op '{"op":"trip_path","network_hash":"$load_network.network_hash","edits_hash":"$edits.edits_hash","schedule":"WK:pat-10-0-1","start_secs":31200,"date":"2026-10-23"}' \
      --cap directory --op @web/public/caps/sample-network-load.json \
      --as focus --op '{"op":"focus","network_hash":"$load_network.network_hash","kind":"route","id":"10"}' \
      | node -e 'const r = require("fs").readFileSync(0, "utf8").trim().split("\n").map(JSON.parse); const ok = (n) => r.find((x) => x.as === n).ok; const legs = r.find((x) => x.ok?.op === "itinerary_paths").ok.paths; if (legs[0]?.kind !== "transit" || legs[0].path.length < 2 || ok("trip").path.length !== 2 || ok("focus").focus.lines.length !== 2) process.exit(1)'
    # The router capability's gate: planning over the network document
    # — the plan, then the diversion delta (compare) chained on the
    # same edits document (08:15 against the unpatched 08:10).
    node tools/caps-run.mjs router \
      --op @web/public/caps/sample-network-load.json \
      --op '{"op":"plan","network_hash":"$load_network.network_hash","from":"A","to":"B","date":"2026-10-23","depart_secs":25200,"max_legs":4}' \
      --op '{"op":"load_edits","network_hash":"$load_network.network_hash","records":[{"id":"e1","diversion":{"id":"d1","route_id":"10","direction_id":null,"first_kept_stop":"A","last_kept_stop":"B","skipped_stops":["C"],"detour":null,"added_secs":300,"start_date":"2026-10-23","end_date":"2026-10-23"},"status":"accepted","rev":1,"updated_at":1}]}' \
      --op '{"op":"plan_compare","network_hash":"$load_network.network_hash","edits_hash":"$load_edits.edits_hash","from":"A","to":"B","date":"2026-10-23","depart_secs":25200,"max_legs":4}' \
      | grep -q '"arrival_hhmmss":"08:15:00"'
    # The map's click-through, on the bus (U6): the trip detail — the
    # patched day's t30 keeps A→B, C skipped (the answer carries no
    # geometry; the host's view code draws).
    node tools/caps-run.mjs runcut \
      --op @web/public/caps/sample-network-load.json \
      --op '{"op":"load_edits","network_hash":"$load_network.network_hash","records":[{"id":"e1","diversion":{"id":"d1","route_id":"10","direction_id":null,"first_kept_stop":"A","last_kept_stop":"B","skipped_stops":["C"],"detour":null,"added_secs":300,"start_date":"2026-10-23","end_date":"2026-10-23"},"status":"accepted","rev":1,"updated_at":1}]}' \
      --op '{"op":"trip_stops","network_hash":"$load_network.network_hash","edits_hash":"$load_edits.edits_hash","schedule":"WK:pat-10-0-1","start_secs":31200,"date":"2026-10-23"}' \
      | grep -q '"stop_id":"B"'
    # The street capability's gate: the detour editor's need, one
    # chained session — the street fixture on bulk op 0, two snaps,
    # and the route between them (the Elbow corner, not the diagonal:
    # the committed trace follows street geometry).
    node tools/caps-run.mjs streets --load web/e2e/fixtures/streets.geojson \
      --op '{"op":"snap","streets_hash":"$load.streets_hash","lat":45.504,"lon":-73.503}' \
      --op '{"op":"snap","streets_hash":"$load.streets_hash","lat":45.508,"lon":-73.508}' \
      --op '{"op":"route","streets_hash":"$load.streets_hash","from":"$2.position","to":"$3.position"}' \
      | grep -q '"lat":45.504,"lon":-73.508'
    # The sync capability's gate: the protocol's one step — a draft
    # client against an empty server converges to the submitted
    # document (rev bumped, host clock stamped).
    node tools/caps-run.mjs sync \
      --op '{"op":"sync_edits","client":[{"id":"e1","diversion":{"id":"d1","route_id":"10","direction_id":null,"first_kept_stop":"A","last_kept_stop":"B","skipped_stops":["C"],"detour":null,"added_secs":300,"start_date":"2026-10-23","end_date":"2026-10-23"},"status":"draft","rev":1,"updated_at":1}],"server":[],"now_ms":1000}' \
      | grep -q '"status":"submitted"'
    # The plan-chain capability's gate: which pick governs — the
    # spring pick in force, the fall pick taking over after it, and
    # the derived windows.
    node tools/caps-run.mjs plans \
      --op '{"op":"resolve","plans":[{"id":"spring","name":"Spring 2026","effective_from":"2026-03-01"},{"id":"fall","name":"Fall 2026","effective_from":"2026-09-07"}],"date":"2026-10-23"}' \
      --op '{"op":"periods","plans":[{"id":"spring","name":"Spring 2026","effective_from":"2026-03-01"},{"id":"fall","name":"Fall 2026","effective_from":"2026-09-07"}]}' \
      | grep -q '"plan":"fall"'
    # The authoring capability's gate: the fixture network stamped as
    # a pick in one atomic change, then the answered document read by
    # the day views in the same session — chained by the hash the
    # author answered (the next change's address), never re-uploaded.
    node tools/caps-run.mjs author \
      --op @web/public/caps/sample-network-load.json \
      --as pick --op '{"op":"apply","network_hash":"$load_network.network_hash","edits":[{"edit":"set_plan","plan":{"id":"fall","name":"Fall 2026","effective_from":"2026-10-19"}}]}' \
      --cap calendar --op '{"op":"load_network","network":"$pick.network"}' \
      --op '{"op":"active_services","network_hash":"$pick.network_hash","date":"2026-10-23"}' \
      | grep -q '"services":\["WK"\]'
    # The diversion-authoring capability's gate: a draft anchored end
    # to end on route 10's default pattern, proposed into an empty
    # edit document, and the answered document validated by the board
    # in the same session — the record the browser's editor writes.
    node tools/caps-run.mjs divert \
      --op @web/public/caps/sample-network-load.json \
      --as prop --op '{"op":"propose","network_hash":"$load_network.network_hash","draft":{"route_id":"10","pattern":"pat-10-0-1","first_kept_stop":"A","last_kept_stop":"B","served_stops":[],"slices":[],"added_mins":5,"start_date":"2026-10-23","end_date":"2026-10-23"},"records":[],"record_id":"e1","now_ms":1000}' \
      --cap board --op @web/public/caps/sample-network-load.json \
      --op '{"op":"load_edits","network_hash":"$load_network.network_hash","records":"$prop.records"}' \
      | grep -q '"valid":true'
    # ARCHITECTURE GATES — the laws as assertions (spelled out in
    # crates/AGENTS.md), because prose does
    # not enforce:
    #   1. the NETWORK is the core data model; GTFS is a serialization
    #      spec for external data sharing. Model-side capabilities do
    #      not depend on the GTFS layer, and the boundary codec's
    #      whole-calendar expansion never leaves the codec's homes.
    #   2. nobody reinvents the monolith inside a module.
    # Law 1a: model-side capabilities (and their shared libraries) do
    # not depend on the GTFS layer (transit-core = the GTFS-shaped
    # layer, transit-codec = the boundary codec) in [dependencies].
    # Dev-dependencies may build fixture documents through the codec —
    # tests compose over real documents; production code does not.
    # Sanctioned consumers: cap-gtfs, cap-network, cap-rt (external
    # rt joining external ids). The router left the feed in U5 — it
    # plans over the network document and the effective day.
    for crate in cap-analysis cap-board cap-runcut cap-calendar cap-directory cap-sync cap-plans cap-author cap-divert transit-analysis transit-dispatch transit-live; do
        for dep in transit-core transit-codec; do
            if awk '/^\[dev-dependencies\]/{exit} /^'"$dep"' =/{found=1} END{exit !found}' "crates/$crate/Cargo.toml" 2>/dev/null; then
                echo "ARCH FAIL: $crate depends on $dep — the network is the core model; compose over the network document (crates/AGENTS.md, Law 1)"; exit 1
            fi
        done
    done
    # Law 1b: whole-calendar expansion (network_to_gtfs) is the
    # codec's export path — sanctioned only in the codec's homes and
    # the dying monolith's own code. A hit anywhere else is the
    # monolith rebuilt with new clothes.
    violations=$(grep -rl "network_to_gtfs" crates --include="*.rs" \
        | grep -v "crates/transit-codec/" \
        | grep -v "crates/cap-network/" \
        | grep -v "crates/cap-abi/" || true)
    if [ -n "$violations" ]; then
        echo "ARCH FAIL: network_to_gtfs outside the codec's homes (Law 2 — no whole-calendar expansion in capabilities):"
        echo "$violations"
        exit 1
    fi
    # WEB GATES — parity (docs/PLAN.md, "The web cleanup"): the
    # browser runs the same ops as caps-run and the Go server, so
    # web/ holds host glue and render components, never answers.
    # Ratchets: each allowlist names today's violators and may only
    # SHRINK — a new violator fails, and so does a listed file that
    # no longer violates (take it off the list in the same change).
    ratchet() { # name, allowlist (space-separated), violators
        local name="$1" allowed=" $2 " found="$3" f
        for f in $found; do
            if [[ "$allowed" != *" $f "* ]]; then
                echo "WEB GATE FAIL ($name): $f — $4 (docs/PLAN.md, the web cleanup)"; exit 1
            fi
        done
        for f in $2; do
            if [[ " $found " != *" $f "* ]]; then
                echo "WEB GATE FAIL ($name): $f no longer violates — remove it from the allowlist"; exit 1
            fi
        done
    }
    # W0a: components never call the bus — the host layer does.
    ratchet resolver-in-components \
        "web/src/App.svelte web/src/components/CapabilityDemo.svelte web/src/components/MapView.svelte web/src/components/TripPlanner.svelte" \
        "$(grep -l 'capabilities\.svelte' web/src/App.svelte web/src/components/*.svelte | tr '\n' ' ')" \
        "a component imports the capability resolver; take node values as props"
    # W0b: no host-side rt join — delays come from cap-rt's live
    # document (the allowlist reached empty with W1: a hard gate now).
    ratchet rt-join-in-web \
        "" \
        "$(grep -rl 'stop_updates' web/src --include='*.ts' --include='*.svelte' | grep -v '^web/src/types/' | sort | tr '\n' ' ')" \
        "reads raw rt stop_updates; the live join is cap-rt's"
    # W0c: no clone-and-mutate of documents in web/ — every change to
    # a network document is a NetworkEdit through cap-author (W2).
    ratchet clone-and-mutate \
        "" \
        "$(grep -rl 'structuredClone' web/src --include='*.ts' --include='*.svelte' | grep -v '^web/src/types/' | sort | tr '\n' ' ')" \
        "clones a document to mutate it; send NetworkEdits through cap-author"
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

# Regenerate the Clark County population layer
# (testdata/clark-population.geojson, from public-domain census
# sources — the real-data dogfood asset for analysis.type_coverage).
population:
    node testdata/generate-population.mjs

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
dev: caps types
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
    cargo mutants -p transit-core -p transit-router -p transit-sync -p transit-plans
