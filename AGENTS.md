# AGENTS.md

Local-first transit software: GTFS static + GTFS-rt, parsed by Rust,
shipped as **one wasm artifact** that runs in the browser (JS), in
Node (ABI smoke), and on the sync server (Go + wazero). The same
bytes everywhere is the point of the project — do not break it.
(The capability-bus PoC restates this for a per-user future: the
same DOCUMENT and the same contracts everywhere; capability bytes
are assembled per user. The monolith stays deployed until the bus
graduates — see the capability bus section.)

**This file is load-bearing.** When your work changes the
architecture, the ops surface, naming, conventions, commands, or
testing expectations — update this file in the same change, before
you declare the work done. Stale instructions here are how the next
agent breaks the build. If a section here contradicts the code, the
code wins for the moment and the section must be fixed immediately.

The sequenced feature plan lives in `docs/ROADMAP.md` — update it
when direction changes, not as part of every code change. The
capability-bus work queue lives in `docs/CAPS-PLAN.md`.

## Commands

`just` is the task runner. `just check` is the full gate — run it
before declaring work done (Rust suites, Go suite, ABI smoke in Node,
frontend build, Playwright e2e).

```
just build / just wasm     # frontend+artifact / artifact only
just caps                  # capability modules + regenerate the manifest (PoC bus)
just registry              # publish the capability registry (gates → blobs + evidence)
node tools/caps-run.mjs    # the agent runner: caps-run list | tools <cap> | <cap> --op ...
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
                diversion patching, and the WRITE direction —
                deterministic zip bytes (src/write.rs), so a produced
                feed has one identity everywhere. Pure, no I/O.
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
transit-codec   The network codec, both directions: a network document
                expanded into the derived-feed view (the publish
                path) and a GTFS feed inferred into an authored
                document (the planner's bootstrap). The import is
                VALID BY CONSTRUCTION over spec-valid feeds (the law,
                property-tested in tests/import_prop_test) and every
                resolution it applies (dropped block trips, unusable
                transfer points, …) is REPORTED as an ImportNote —
                never silent. A LIBRARY shared by cap-network and
                transit-ops's tests — the extraction pattern: crate
                boundary first, capability wrapper second, monolith
                deletion last.
transit-analysis Planner analysis over an authored network document:
                stats, service-type overview/detail, population
                coverage. A LIBRARY shared by transit-ops (parse-only
                coverage handle) and the cap-analysis module — the
                extraction pattern: crate boundary first, capability
                wrapper second, monolith deletion last.
transit-rt      prost types from the official GTFS-rt proto, plus the
                OVERVIEW view (delays, vehicles) over them — a
                LIBRARY shared by cap-rt and any host that holds the
                bytes.
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
                + per-feed registry (feed_id keyed). The codec ops
                (network_import / network_validate / network_load)
                and the analysis ops are DELETED — they live in
                cap-network and cap-analysis over the shared
                transit-codec / transit-analysis libraries (the
                monolith keeps shrinking; the corpus and the ABI
                smoke pin the deletions). What remains: the parsed-
                feed op surface (departures, planning, blocks,
                geojson, streets, sync), the plans chain in
                src/plans.rs (plans_resolve / plans_validate /
                plans_periods — cross-document domain logic, the
                same standing as transit-sync's merge), and the
                parse-only coverage handle. Streets ops here too:
                streets_load / street_snap / street_route.
transit-wasm    THE artifact: flat import-free C ABI over dispatch.
                No wasm-bindgen — do not reintroduce host glue.

**The capability bus (graduating):** horizontal capability modules
assembled per user from a manifest — the thesis is "ship capabilities
to match the user," so capability bytes differ per user while the
DOCUMENT and the contracts stay identical everywhere. Three
extractions are DONE, all by the same three-stage shape (crate
boundary first, capability wrapper second, monolith op deletion
last):
* analysis (stats, service types, coverage) lives in
  `transit-analysis`, shared by cap-analysis (the module, hash-keyed
  document registries) and transit-ops — the planner UI reads numbers
  through the bus and degrades loudly (authoring continues, numbers
  off) when the capability cannot assemble;
* the network codec lives in `transit-codec`, shared by cap-network
  and transit-ops's tests — the planner's bootstrap (derive), the
  authoring feedback loop (validate), and the publish path
  (export → deterministic GTFS zip bytes → LoadFeed) all cross the
  bus; the planner persona no longer needs the monolith for them;
* the realtime decoder lives in `transit-rt` (the overview view over
  the prost types), shared by cap-rt and every host — the rider's
  live board and the dispatcher's vehicle map decode through the bus
  (cap-rt is a pure decoder over ephemeral payloads; the monolith
  keeps no copy).
Future extractions follow the same shape. The monolith remains the
deployed artifact for everything not yet extracted (blocks, active
services, the plans chain, streets, sync).

```
cap-abi         The seam crate: the flat C ABI as a macro (every
                capability module is the same four exports), the
                capability-manifest contract (speaks/migrates/
                consumes + agent context), the ASSEMBLY VALIDATOR
                (compose-time proof: closure, version coherence,
                migration-path reachability — total and property-
                tested; gen-manifest self-validates before writing),
                and gen-manifest (the capability set as code; the
                manifest diff IS the reviewable assembly change).
                Also caps-smoke.mjs: Node ABI smoke + the law that
                every advertised op is documented (undocumented ops
                fail the build — agent context cannot drift).
cap-gtfs        The gtfs capability: GTFS zip → content-hash feed
                registry, departures/stops queries, and the watchlist
                document (v1/v2) with its v1→v2 migration — pure,
                deterministic, loss REPORTED (MigrationNotes, never
                silent; identity-preservation is a property law —
                migrations never touch the sync key space).
cap-router      The router capability: RAPTOR over a feed document
                gtfs produced — a pure-function capability; it
                writes nothing.
cap-analysis    The analysis capability: planner numbers (stats,
                service types, coverage) over hash-keyed network and
                population-layer registries. The domain code lives
                in the shared transit-analysis library — the first
                extraction where a monolith op family is DELETED
                (the planner UI has no monolith fallback for it).
                Consumes network documents from cap-network (the
                registry edge the manifest carries).
cap-network     The network codec capability: load_feed (zip bytes →
                content-hash feed registry), import (the planner's
                bootstrap — the derived document registers itself
                and its hash is in the response), load_document,
                validate (the authoring feedback loop), export (the
                publish path — deterministic GTFS zip bytes under
                their content hash, base64 across the JSON seam).
                The domain code lives in the shared transit-codec
                library — the monolith keeps no copy; the planner
                persona's bootstrap, validation, and materialize
                cross the bus and degrade loudly without it.
cap-rt          The realtime capability: decode — a GTFS-rt protobuf
                payload (bulk-bytes op 0, the ephemeral bytes an
                agency serves every few seconds) into the overview
                view (delays per trip/stop, live vehicles). A PURE
                DECODER: it writes nothing, and the monolith keeps
                no copy (decode_rt is deleted; the ABI smoke pins the
                retired op). Assembles for riders and dispatchers
                alike.
web/src/        capabilities.svelte.ts: the resolver — manifest
                validation, demand-driven load, byte verification
                against pinned hashes, per-capability degradation,
                and the IMMUTABLE document store (every version
                hash-chained by produced_by; `current` is a pointer;
                migrations move it forward, rollback moves it back;
                IndexedDB-persisted). CapabilityDemo.svelte is the
                visible story (assembly, hash handoff, degraded
                state, migrate/rollback) in the DataSources modal.
```

Bus rules that are load-bearing: capabilities compose through
DOCUMENTS addressed by content hash, never module-to-module — the
host loads the same bytes into each module and ASSERTS the returned
hashes agree (`assertSameDocument` is the law); the MODULE is the
hash authority — hosts never mint document identity (JS
JSON.stringify differs from the module's canonical serde bytes in
float formatting; query with the hash the module returned); one
Request enum per capability is the whole surface (only planners
write networks — readers have no authoring ops by construction);
`#[no_mangle]` ABI exports collide if one binary links two cap
crates (tests assert against the hash formula instead); cap crates
need `crate-type = ["cdylib", "rlib"]` for native tests to link.

**The registry** (`registry/`, published by `just registry`):
immutable blobs + one evidence record per capability (contracts,
agent context, which gates ran at which commit) + a pointer. It is
the artifact agents and humans pull SANDBOXED COMPUTE from —
`tools/caps-run.mjs` verifies blob bytes against the pinned hash and
runs ops behind a worker-termination wall-clock guard (`--timeout`).
Registry records and the browser manifest are BOTH generated by
`cargo run -p cap-abi --bin gen-manifest` — one source of contracts,
two surfaces. `just test` gates the runner itself (the fixture feed
hash is stable, so registry/fixture drift fails the gate).

**Dogfood QA — the agent interface mirrors the human UI.**
`caps-run` exercises the same capabilities, bytes, and laws the app
runs; a human asking planner questions through it is doing real QA.
The rules this paradigm earned (the founding C-TRAN session found
four real defects fixture gates missed — a 64 KiB output
truncation among them):

- **All capability work is dogfooded on real data before it is
  done.** Anything touching feeds or payload size runs against
  `testdata/ctran-gtfs.zip` (the real C-TRAN GTFS, NOT COMMITTED —
  licensed data; fetch from c-tran.com) through `caps-run`.
- **A one-off script is never tooling — it is a finding.** If a
  QA session needs bespoke glue (a bootstrap script, a
  summarizer), that is a missing op or runner feature; route it
  into `docs/CAPS-PLAN.md`, don't ship the script.
- **The mirror cuts both ways.** A question that is one op via
  the CLI but absent from the UI is a UI gap (`network_stats` was
  dead UI surface its whole life — the day-shape numbers
  trips/routes/stops/revenue-hours/span have no screen); a
  question awkward via the CLI is an op gap. Dogfood sessions
  generate both signals; both land in the plan doc.
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

- **Git is the user's** — never commit, push, stash, or otherwise
  move refs. Do the work, leave the tree dirty (staged files are
  fine to inspect via `git status`/`git diff`), and report what
  changed; the user handles every git action.
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
   e.g. the import law: `transit-codec`'s `import_prop_test` holds
   that the inferred document is valid by construction over every
   spec-valid feed and deterministic, and its `codec_prop_test` holds
   the export round trip; `plans_prop_test` holds the service-plan
   chain laws with independent oracles).
2. A dispatch test if a new op exists (native, JSON round-trip).
3. Extend `crates/transit-wasm/abi-smoke.mjs` for ABI contract
   changes; regenerate `testdata/corpus.json` (`just wasm` then `just
   corpus`) when response shapes change — the corpus is the
   cross-host byte-identical replay (Rust `golden_test`, Go
   `golden_test.go`). Capability-module changes extend
   `crates/cap-abi/caps-smoke.mjs` the same way (`just test` runs it;
   `just test` and `just e2e` build `caps` first).
4. A Go round-trip test if server-visible behavior changes.
5. Playwright e2e for user-visible flows (Svelte components should
   not need changes when only the ABI underneath changes). The
   capability bus keeps its own spec (`web/e2e/caps.spec.mjs`:
   assembly, composition, degraded state, migrate/rollback) — extend
   it for bus-visible changes; `caps-prop`/law tests (migration
   laws, validator mutation law) follow expectation 1.
6. Dogfood on real data: capability work touching feeds or payload
   size runs a `caps-run` session against the C-TRAN feed before
   it is done (see the dogfood-QA rules in the capability bus
   section) — fixture-scale gates do not catch real-data defects.

## Deliberate non-goals (do not "fix" these)

- No CRDT/automerge for sync — LWW + tombstones is the design.
- transit-network is the authored model; GTFS stays OUT of it. The
  GTFS import (inference) and export (materialization) codecs live
  at the boundary in transit-codec (shared by cap-network and the
  monolith's tests) — do not grow GTFS-shaped fields into the model
  for codec convenience.
- Runcutting in transit-network stops at blocks + layovers; operator
  duties/rosters are a future layer, not a `Block` extension.
- wasm-bindgen / JS glue in the artifact is gone on purpose; the
  artifact is the raw rustc output with four exports. Capability
  modules keep the same flat ABI (cap-abi's macro) — do not give a
  capability a custom calling convention.
- The Go server stays scaffolding; domain logic that the server needs
  becomes a transit-ops op.
- The capability bus is a PoC alongside the monolith: do not split the
  monolith's ops into capabilities before the registry exists, and
  do not let a capability grow a private document format — shared
  data crosses modules as kernel documents addressed by content hash
  only.
