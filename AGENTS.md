# AGENTS.md

Local-first transit software: GTFS static + GTFS-rt, parsed by Rust,
shipped as **one wasm artifact** that runs in the browser (JS), in
Node (ABI smoke), and on the sync server (Go + wazero). The same
bytes everywhere is the point of the project — do not break it.
(The capability-bus PoC restates this for a per-user future: the
same DOCUMENT and the same contracts everywhere; capability bytes
are assembled per user.)

**This file is load-bearing, and so are its pointers.** The detail
lives in the docs below; the rules that keep them from drifting:

- When your work changes the architecture, the ops surface, naming,
  conventions, commands, or testing expectations — update this file
  AND every doc it points to that the change touches, in the same
  change, before you declare the work done.
- When content moves between docs, fix every reference to it in the
  same change. A pointer to a section that no longer exists is a
  broken build for the next agent.
- The "Current state" section below is part of that contract: do
  not mark work done there (or in `docs/CAPS-PLAN.md`) that is not
  done. Stale instructions are how the next agent breaks the build.
- If a section contradicts the code, the code wins for the moment
  and the section must be fixed immediately.

Where the detail lives:

- `crates/AGENTS.md` — the two architecture laws for everything
  under `crates/` (enforced by arch gates in `just test`)
- `docs/ARCHITECTURE.md` — per-crate reference behind the tree below
- `docs/CAPABILITIES.md` — the capability bus: modules, bus rules,
  the registry, dogfood-QA rules
- `docs/WEB.md` — the frontend: shape, file ownership, conventions
- `docs/CONVENTIONS.md` — Rust/wasm/ops gotchas
- `docs/TESTING.md` — the testing expectations
- `docs/ROADMAP.md` — the sequenced feature plan (direction changes)
- `docs/CAPS-PLAN.md` — the capability-bus work queue (the authority
  for what is done and what is not)

## Commands

`just` is the task runner. `just check` is the full gate — run it
before declaring work done.

```
just build / just wasm     # frontend+artifact / artifact only
just caps                  # capability modules + regenerate the manifest
just registry              # publish the capability registry (gates → blobs + evidence)
node tools/caps-run.mjs    # the agent runner: list | tools <cap> | <cap> --op ...
                           # $refs ($load.x, $op.x, $N.path) + $TODAY/$NOW_SECS;
                           # --load [N:]FILE for bulk ops; --op @FILE for big JSON
just test                  # host-side tests (Rust + smokes + runner gates + Go -race)
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
Installs (toolchains, release binaries) are the user's job;
cargo/npm/go module fetches are fine.

## Architecture

**Two laws govern every crate** (spelled out and enforced — see
`crates/AGENTS.md`): the NETWORK is the core data model, GTFS is a
serialization spec for external data sharing; and nobody reinvents
the monolith inside a module.

```
transit-core     GTFS codec substrate (parse, calendar, deterministic zip write)
transit-network  THE authored network model — trips are derived (materialize)
transit-codec   the network codec: GTFS ⇄ network document (import/export)
transit-router  RAPTOR earliest-arrival planning
transit-analysis planner analysis library (stats, service types, coverage)
transit-dispatch the operator's day library (patched materialized day, boards,
                 authored block chains with the layover ripple)
transit-rt       GTFS-rt prost types + overview/progress join libraries
transit-sync     THE sync document (EditRecord, Diversion, merge — pure)
transit-streets  street graph for detour authoring (snap, route)
transit-ops      the monolith seam: dispatch(Request) -> Response + feed registry
transit-wasm     THE artifact: flat import-free C ABI over dispatch
cap-abi          the capability seam (ABI macro, manifest, validator, gen-manifest)
cap-gtfs cap-router cap-analysis cap-network cap-rt cap-dispatch
                 the capability modules (docs/CAPABILITIES.md)
server/          Go + wazero host — scaffolding, never a domain decision
web/             Svelte 5 + Vite frontend (docs/WEB.md)
```

Per-crate detail: `docs/ARCHITECTURE.md`.

**Ownership rules** (violations here caused real bugs; keep them):

- Hosts own I/O, files, and the clock. The wasm module owns domain
  logic. Dates cross every boundary as "YYYY-MM-DD" strings, times
  as u32 seconds since service-day midnight (values past 86400 are
  legitimate — after-midnight trips).
- The **base feed is immutable**. Diversions produce patched copies
  per service date; patching never mutates its input.
- **JS owns the documents** (edit document, plan set): `$state.raw`
  in `src/state.svelte.ts`, wholesale-replaced, never mutated in
  place — that is what makes IndexedDB cloning and sync work. wasm
  holds only materialized views. A service change is a FORK (the
  next pick copies the whole world it changes); the chain (who is
  in force, governing periods) is always derived by the artifact,
  never stored.
- **Retirement is the tombstone**: "delete" means
  `status: "retired"`, rev bumped. No hard deletes.
- Schedules change by **artifact replacement, not mutation**: the
  planner publishes a new zip; the manifest pins `feed_hash` +
  `wasm_hash`; everyone re-pins and revalidates.
- The sync protocol is **LWW per record id + tombstones**, not CRDT
  (deliberate — the terminal authority is the agency scheduling
  system; propose → review → accept). `transit_sync::merge` is a
  property-tested semilattice; `POST /edits` is the whole client
  protocol. The Go host treats documents as opaque JSON.

## Current state (update in the same change as the work)

The monolith remains the deployed artifact; the bus is a PoC
alongside it. Where things actually stand:

DONE — monolith ops deleted, UI reads the bus:
analysis (transit-analysis/cap-analysis), the network codec
(transit-codec/cap-network), rt decode (transit-rt/cap-rt), and the
gtfs dispatch reads active_services / blocks / feed_info (cap-gtfs).

DONE on the bus, NOT yet serving the UI:
cap-dispatch + the transit-dispatch library — the operator day view
(board, station board, authored block chains with the layover
ripple). The UI board and block chains still read the monolith.

NOT done (docs/CAPS-PLAN.md is the open queue):
the UI board/blocks rewire to cap-dispatch, then the monolith's
`departures` and `block_trips` die; `set_diversions` dies when
geojson/planning move; the trip-id map for rt joins; sync
persistence; cap-authz.

## Conventions

- **Git is the user's** — never commit, push, stash, or otherwise
  move refs. Do the work, leave the tree, report what changed.
- The TS bindings are GENERATED (`just types`; commit the diff) —
  the TS contract IS the artifact's contract.
- New monolith ops go at the transit-ops seam; new capability ops
  go through the extraction pattern (crate boundary first,
  capability wrapper second, monolith deletion last).

The rest of the gotchas: `docs/CONVENTIONS.md` (Rust/wasm/ops) and
`docs/WEB.md` (frontend).

## Testing

Every change lands with tests at the layer that owns the logic —
the six expectations (unit/property in the owning crate, dispatch
test, smokes + corpus, Go round-trip, Playwright e2e, real-data
dogfood) are spelled out in `docs/TESTING.md`.

## Deliberate non-goals (do not "fix" these)

- No CRDT/automerge for sync — LWW + tombstones is the design.
- transit-network is the authored model; GTFS stays OUT of it — the
  import/export codecs live at the boundary in transit-codec.
- Runcutting stops at blocks + layovers; duties/rosters are a future
  layer, not a `Block` extension.
- wasm-bindgen / JS glue in the artifact is gone on purpose; the
  artifact is the raw rustc output with four exports. Capability
  modules keep the same flat ABI — no custom calling conventions.
- The Go server stays scaffolding; domain logic the server needs
  becomes a transit-ops op.
- The bus is a PoC alongside the monolith: no capability grows a
  private document format — shared data crosses modules as kernel
  documents addressed by content hash only.
