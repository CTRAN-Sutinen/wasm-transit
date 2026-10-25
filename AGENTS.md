# AGENTS.md

Local-first transit software: GTFS static + GTFS-rt, parsed by Rust,
shipped as **one wasm artifact** that runs in the browser (JS), in
Node (ABI smoke), and on the sync server (Go + wazero). The same
bytes everywhere is the point of the project — do not break it.

**This file is load-bearing.** When your work changes the
architecture, the ops surface, naming, conventions, commands, or
testing expectations — update this file in the same change, before
you declare the work done. Stale instructions here are how the next
agent breaks the build. If a section here contradicts the code, the
code wins for the moment and the section must be fixed immediately.

The sequenced feature plan lives in `docs/ROADMAP.md` — update it
when direction changes, not as part of every code change.

## Commands

`just` is the task runner. `just check` is the full gate — run it
before declaring work done (Rust suites, Go suite, ABI smoke in Node,
frontend build, Playwright e2e).

```
just build / just wasm     # frontend+artifact / artifact only
just caps                  # capability modules + regenerate the manifest (PoC bus)
just test                  # host-side tests only (Rust + ABI smokes + Go -race)
just e2e                   # browser e2e (builds first)
just check                 # test + e2e — the full gate
just corpus                # regenerate the golden corpus (AFTER just wasm)
just provenance            # refresh testdata/provenance.json after Rust changes
just types                 # regenerate TS bindings only
just serve [feed.zip ["Publisher"]]   # sync server on :8080
just dev                   # vite hot reload + server alongside
just fixtures              # regenerate the e2e GTFS fixture
just soak                  # real-data soak (COUNTY_GEOJSON=… county roads)
```

Toolchains live in `~/.cargo/bin` and `~/.local/bin` (rustc, pmtiles,
just, go at `~/.local/go/bin`) — export PATH if a shell lacks them.
Installs (toolchains, release binaries) are the user's job; cargo/npm/
go module fetches are fine.

## Architecture — the rules that are load-bearing

**Layering (bottom up):**

```
transit-core    GTFS codec: model, zip/CSV parse, calendar, queries,
                diversion patching. Pure, no I/O.
transit-network The authored network model (planner authority):
                patterns, timing profiles, service types (the
                service identifier, linked to days by its calendar
                rule), pattern schedules (one pattern's trips under
                one service type), runcut. Trips are DERIVED
                (`materialize`) — trips are never stored. GTFS is a
                codec at the boundary, not the model; no GTFS types
                leak in here. Transfer points are authored flags on
                stops, composed over their WALKING GROUP (the stop +
                siblings sharing a parent, the parent, children —
                transfers happen across a street, not at one pin).
                Every document is a SERVICE PLAN (`PlanMeta`: id,
                name, effective_from) — agencies implement a few
                picks a year at defined dates; empty identity =
                unversioned (a fresh import).
transit-router  RAPTOR earliest-arrival planning.
transit-rt      prost types from the official GTFS-rt proto.
transit-sync   THE sync document: EditRecord, FeedManifest, EditReport,
                pure validation/merge logic. Shared by every host.
transit-streets Street routing graph for detour authoring: GeoJSON in,
                snap (nearest street position) + shortest-path route
                out (one-way aware — TIGER `OneWay` and OSM `oneway`
                conventions; turn restrictions are a non-goal until a
                restriction table ships in the data). Pure reference
                geometry — its own registry at the seam (streets_id
                keyed), never mixed with feed state.
transit-ops     dispatch(Request) -> Response: the host-agnostic seam
                + per-feed registry (feed_id keyed). The network
                codec lives in src/network.rs, both directions:
                network_import infers a document from a loaded feed
                (the planner's bootstrap), network_validate reports
                every issue, network_load materializes a document
                into the same registry a parsed zip lands in. The
                import is VALID BY CONSTRUCTION over spec-valid feeds
                (the law, property-tested in tests/import_prop_test)
                and every resolution it applies (dropped block
                trips, unusable transfer points, …) is REPORTED as
                an ImportNote — never silent. Analysis lives in
                src/analysis.rs (network_stats, service-type
                overview/detail/coverage, population coverage over
                an imported layer registry) — the planner's numbers
                are computed in the artifact, never in JS. The
                service-plan chain lives in src/plans.rs
                (plans_resolve / plans_validate / plans_periods) —
                cross-document domain logic, the same standing as
                transit-sync's merge. Streets ops here too:
                streets_load / street_snap / street_route.
transit-wasm    THE artifact: flat import-free C ABI over dispatch.
                No wasm-bindgen — do not reintroduce host glue.
server/         Go + wazero host: scaffolding only (HTTP, manifest,
                proxy). It must never make a domain decision.
web/            Svelte 5 runes + Vite, TypeScript end to end. ONE
                shell: a sidebar (persona tabs + the active
                workflow) beside a PERSISTENT map canvas that never
                remounts on a persona switch; persona-owned layers
                DERIVE from app.persona. The map is A VIEW, not a
                fixture: data screens hide it and the rail takes the
                full width (`.map-side.hidden`; entity and
                service-type screens frame things on it). Data
                sources (feed, rt, streets, population layer) is a
                MODAL, not a floating panel. The Planner persona is
                a WORKSPACE of screens with a back stack, ROOTED AT
                SERVICE PLANS (the chain: in force / upcoming /
                superseded, fork = the service-change workflow);
                below that the network overview (per-service-type
                analysis — planners design types, not calendar
                dates) and drill-downs per entity. There is no
                planner drawer. src/wasm.ts wraps the artifact
                (GtfsHandle + every op); components never see the
                ABI. src/planner.ts owns authoring mutations (clone
                → edit → replace → revalidate) and the screen stack;
                src/state.svelte.ts owns the documents (edit
                document, plan set — both IndexedDB-persisted,
                wholesale-replaced) and MapView owns all map state
                (window.__map).
```

**Ownership rules** (violations here caused real bugs; keep them):

- Hosts own I/O, files, and the clock. The wasm module owns domain
  logic. Dates cross every boundary as "YYYY-MM-DD" strings, times as
  u32 seconds since service-day midnight (values past 86400 are
  legitimate — after-midnight trips).
- The **base feed is immutable**. Diversions produce patched copies
  per service date; `apply_diversions` never mutates its input.
- **JS owns the edit document** (`$state.raw` array in
  `src/state.svelte.ts`, replaced wholesale, never mutated in place —
  that is what makes IndexedDB cloning and future sync work). wasm
  holds only a materialized view: `set_diversions(records)` is the
  single entry point, returns per-record validation reports, and
  invalid records are *reported, not rejected*.
- **JS owns the plan documents the same way**: the plan set is
  `$state.raw` in `src/state.svelte.ts` (keyed by a store id; the
  sync identity is `plan.id` inside each document), wholesale-
  replaced, IndexedDB-persisted — service changes span weeks and
  survive reloads. A service change is a FORK (the next pick copies
  the whole world it changes — permanent structure changes are only
  allowed at picks); pick dates are targets that move until they're
  real, and the chain (who is in force, governing periods) is
  always derived by the artifact, never stored.
- **Retirement is the tombstone**: "delete" means `status: "retired"`,
  rev bumped. The record stays in the document so sync sees the
  deletion. No hard deletes.
- Schedules change by **artifact replacement, not mutation**: the
  planner persona publishes a new zip; the manifest (`/manifest`)
  pins `feed_hash` + `wasm_hash`; everyone re-pins and revalidates.
- The sync protocol is **LWW per record id + tombstones**, not CRDT.
  Deliberate: the terminal authority is the agency scheduling system;
  propose → review → accept, not merge. `transit_sync::merge` is a
  semilattice (commutative/associative/idempotent — property-tested);
  `POST /edits` is the whole client protocol: push your document,
  adopt the merged response. The Go host treats documents as opaque
  JSON — merge, submit, and validation all happen in the artifact.

## Conventions and gotchas

- **The frontend is TypeScript, end to end** — every type that
  crosses the wasm boundary is GENERATED (`just types`, refreshed by
  `just test`): ts-rs derives on the model crates emit
  `web/src/types/*.ts`; commit the diff like any artifact. The TS
  contract IS the artifact's contract. Plain `cargo test` scatters
  bindings into `crates/*/bindings` — always set `TS_RS_EXPORT_DIR`
  (the justfile does). `npm run typecheck` (svelte-check, wired into
  `just build`) gates all of src/ — components carry
  `lang="ts"`, the runes modules are `.svelte.ts`. TypeScript is
  pinned to 6.x: svelte-check does not yet accept TS 7.
- **serde_json must keep the `float_roundtrip` feature** (root
  Cargo.toml workspace dep): the default parser is 1 ULP off on some
  f64s (`-122.7 + 0.005` reproducibly), which broke document
  round-trip equality. Found via a transit-network property test.
- **Add new operations at the transit-ops seam**: extend
  `Request`/`Response` (serde-tagged; JSON form is `{"op": ...}`),
  implement in `dispatch`, expose in each host (wasm.ts method, Go
  helper). Response payloads must be typed serde structs. Response
  FIELD names are snake_case (the serde default) — a TS cast with a
  camelCase field name compiles and typechecks fine but reads
  `undefined` at runtime, which `JSON.stringify` then drops so the
  next request fails with a cryptic serde "missing field" error
  (this shipped once: `streets_id` vs `streetsId`).
- **Model vocabulary is planner vocabulary, not GTFS's**: a
  `ServiceType` IS the service (the GTFS service_id — the day-linked
  design unit); a `PatternSchedule` is one pattern's trips under a
  service type; block trips reference schedules by
  `(schedule, start_secs)`; a `Network` document is a service plan.
  Do not rename these back toward GTFS's calendar/service words.
- **Regenerate the golden corpus through the harness**: `just
  corpus` depends on `just wasm`, so the replay always runs against
  a freshly built artifact — never `node testdata/generate-corpus.mjs`
  by hand against a stale `web/pkg` (a no-op diff means the golden
  test keeps failing on the real change).
- **Diagnostics are measurements; regressions are assertions.**
  Scratch repro specs live in /tmp with their own Playwright config
  (never in web/e2e) and get deleted after use; every bug they
  catch must land as a permanent suite assertion before the work is
  done. If it is worth writing a test, it is worth keeping.
- **Native tests first**: logic tests belong in the crate that owns
  the logic (`transit-ops/tests/dispatch_test.rs` runs the real seam
  without wasm). Property tests use *independent oracles* — see
  `transit-core/tests/prop_test.rs`; proptest regressions seeds are
  kept in `*.proptest-regressions`.
- **wazero `memory.Read` returns a view** — copy bytes out before
  freeing the buffer, or the Rust allocator corrupts them (this bit
  us once).
- **The artifact is NOT reentrant** — one module instance, one
  invoke at a time. The Go host serializes the whole alloc→free
  sequence in `WasmHost.call` (net/http is per-goroutine); the
  artifact's internal Mutex only guards its registry map, not its
  heap. `go test -race` is part of `just test`, and
  `server/race_test.go` is what gives it something to watch —
  verify the test still fails if you touch that lock.
- **Svelte `$state` proxies are not structured-cloneable** — persist
  plain data (`$state.raw`, wholesale replacement), never proxies.
- **await arrayBuffer() before crossing into wasm** — a Promise in a
  `&[u8]` slot decodes as zero bytes, silently.
- **just 1.59 does not parse `key=value` named args** — recipes use
  positional args.
- GTFS-rt proto is **proto2**: required fields are non-Option in
  prost; nested types live in modules (`gtfs_rt::trip_update::*`).
- A date can have **multiple active service keys**; every
  departure/block row carries its service_id. Don't assume one.
- `#[profile.release]` in the root Cargo.toml is tuned for artifact
  size (opt-level "s", LTO, panic=abort, strip). Measure before
  changing it.
- Port 8080 is the dev-server default — check `ss -tlnp` before
  starting one. Playwright e2e runs its own server on :8081 with
  `reuseExistingServer: false` (hermetic: a shared server carries a
  live edit document, and test records must not land in a dev
  server).
- MapLibre's `LngLatBounds.extend` parses a bare array-of-arrays as
  the `[[west, south], [east, north]]` form and silently ignores the
  rest — extend one point at a time (`boundsOf` in MapView).
- e2e fixture changes: edit `web/e2e/make_fixtures.py`, run
  `just fixtures`; the rt protobuf fixture is generated by
  `cargo run -p transit-rt --example encode_sample`.

## Testing expectations

Every change lands with tests at the layer that owns the logic:

1. Unit/property tests in the owning crate (oracle style for laws —
   e.g. the import law: `import_prop_test` holds that the inferred
   document is valid by construction over every spec-valid feed and
   deterministic; `plans_prop_test` holds the service-plan chain
   laws with independent oracles).
2. A dispatch test if a new op exists (native, JSON round-trip).
3. Extend `crates/transit-wasm/abi-smoke.mjs` for ABI contract
   changes; regenerate `testdata/corpus.json` (`just wasm` then `just
   corpus`) when response shapes change — the corpus is the
   cross-host byte-identical replay (Rust `golden_test`, Go
   `golden_test.go`).
4. A Go round-trip test if server-visible behavior changes.
5. Playwright e2e for user-visible flows (Svelte components should
   not need changes when only the ABI underneath changes).

## Deliberate non-goals (do not "fix" these)

- No CRDT/automerge for sync — LWW + tombstones is the design.
- transit-network is the authored model; GTFS stays OUT of it. The
  GTFS import (inference) and export (materialization) codecs live
  at the boundary in transit-ops `src/network.rs` — do not grow
  GTFS-shaped fields into the model for codec convenience.
- Runcutting in transit-network stops at blocks + layovers; operator
  duties/rosters are a future layer, not a `Block` extension.
- wasm-bindgen / JS glue in the artifact is gone on purpose; the
  artifact is the raw rustc output with four exports.
- The Go server stays scaffolding; domain logic that the server needs
  becomes a transit-ops op.
