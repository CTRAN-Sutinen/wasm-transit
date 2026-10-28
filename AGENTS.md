# AGENTS.md

Local-first transit software: GTFS static + GTFS-rt, parsed by
Rust, shipped as **capability modules** — one flat import-free
wasm ABI everywhere. The same DOCUMENT and the same contracts on
every host (browser, Node, Go + wazero) is the point of the
project; the bytes are assembled per user from the manifest. The
bus is the architecture: every host instantiates the capability
modules it needs.

**This file is load-bearing, and so are its pointers.** The detail
lives in the docs below; the rules that keep them from drifting:

- **Docs describe the repo as it is now.** History is git's: no
  "was", no "moved from", no unit numbers for finished work, no
  session narratives. Architecture docs are written in the present
  tense.
- One doc per topic. Architecture detail lives in
  `docs/ARCHITECTURE.md` (and `crates/AGENTS.md` for the three laws);
  open work lives only in `docs/units/`, one file per unit — never
  restate status here or anywhere else. Architecture docs may be
  many, but they must not overlap: each topic has exactly one home.
- When your work changes the architecture, the ops surface, naming,
  conventions, commands, or testing expectations — update this file
  AND every doc whose topic the change touches, in the same change,
  before you declare the work done.
- When content moves between docs, fix every reference to it in the
  same change. A pointer to a section that no longer exists is a
  broken build for the next agent.
- **Deleting a unit file is the DONE claim** (`docs/units/README.md`).
  Do not delete one whose work is not done. The change that deletes
  it NAMES ITS PIN — the gate, e2e spec, or dogfood session that
  proves it — and describes what landed in the architecture docs.
  On-the-bus, UI-reads-the-bus, and dogfooded are three different
  claims; a unit missing a grade stays open. A fixed QA finding is
  deleted the same way. Stale instructions are how the next agent
  breaks the build.
- If a section contradicts the code, the code wins for the moment
  and the section must be fixed immediately.

Where the detail lives:

- `crates/AGENTS.md` — the three architecture laws for everything
  under `crates/` (enforced by arch gates in `just test`)
- `docs/ARCHITECTURE.md` — the architecture reference: per-crate
  detail, server, web, the capability bus and its rules
- `docs/units/` — the open work, one file per unit (read its
  README before touching anything); a unit's file is deleted when
  it lands
- `docs/CONVENTIONS.md` — gotchas, Rust/wasm/ops and frontend
- `docs/TESTING.md` — the testing expectations and the dogfood-QA
  rules
- `docs/QA-FINDINGS.md` — the open real-data findings (defects,
  op/UI/data gaps), each pinned; deleted when fixed
- `docs/ROADMAP.md` — the sequenced feature plan (direction changes)

## Commands

`just` is the task runner. Two runners: `just test` is the
host-side gate (no browser) — run it before declaring any change
done; `just e2e` adds the browser runner, for UI-visible changes;
`just check` runs both, for changes that touch both sides.

```
just build                 # frontend (type-checks first — the TS layer is a gate)
just caps                  # capability modules + manifest (+ cap_sync.wasm for the server)
just registry              # publish the capability registry (gates → blobs + evidence)
node tools/caps-run.mjs    # the agent runner: list | tools <cap> | <cap> --op ...
                           # steps run in command-line order; --cap ID switches
                           # capability mid-session (documents cross by $ref);
                           # --as NAME names a result; $refs ($name.x, $load.x,
                           # $op.x, $N.path) + $TODAY/$NOW_SECS (a number);
                           # --load [N:]FILE for bulk ops; --op @FILE for big JSON
just test                  # the host-side runner (Rust + smokes + runner gates + web
                           # parity ratchets + Go -race) — no browser
just e2e                   # the browser runner (Playwright, builds first) — UI changes
just check                 # both runners — changes touching both sides
just types                 # regenerate TS bindings (gitignored; build runs it)
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

**Three laws govern every crate** (spelled out and enforced — see
`crates/AGENTS.md`): the NETWORK is the core data model, GTFS is a
serialization spec for external data sharing; nobody reinvents
the monolith inside a module; and one NEED per module — personas
assemble at wire time.

```
transit-core     GTFS codec substrate (parse, calendar, deterministic zip write)
transit-network  THE authored network model — trips are stored records (own id,
                 departure, times); materialize answers a day's instances;
                 authoring edits answer the next document (apply_edits);
                 where things are (pattern alignments, entity focus);
                 what exists, as the planner lists it (rows, route and stop composition,
                the timetable: one route, one direction, one service type)
transit-codec   the network codec: GTFS ⇄ network document (import/export)
transit-router  RAPTOR earliest-arrival planning over the network model's materialized day
transit-analysis planner analysis library (stats, service types, coverage)
transit-dispatch the operator's day library (patched materialized day, boards,
                 authored block chains with the layover ripple, diversion drafts)
transit-rt       GTFS-rt prost types + overview/progress join libraries
transit-live     THE live document (rt in model vocabulary, the on-time window — pure)
transit-sync     THE sync document (EditRecord, Diversion, merge, the record envelope — pure)
transit-plans    THE service-plan chain (resolve, periods, validity — pure, derived)
transit-streets  street graph for detour authoring (snap, route)
cap-abi          the capability seam (ABI macro, manifest, validator, gen-manifest)
cap-gtfs cap-router cap-analysis cap-network cap-rt cap-board cap-runcut cap-calendar cap-directory cap-streets cap-sync cap-plans cap-author cap-divert
                 the capability modules (docs/ARCHITECTURE.md, bus section)
server/          Go + wazero host over cap-sync — scaffolding, never a domain decision
web/             Svelte 5 + Vite frontend: / is a picker over the GENERATED UIs
                 (src/uis/<id>/, each its own page and host layer) and links
                 the original app (/app.html) (docs/ARCHITECTURE.md, web section,
                 "Generated UIs" — the recipe a fresh agent follows)
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
  planner publishes a new zip; the server's manifest pins
  `feed_hash` + the sync module's `wasm_hash`; everyone re-pins
  and revalidates.
- The sync protocol is **LWW per record id + tombstones**, not CRDT
  (deliberate — the terminal authority is the agency scheduling
  system; propose → review → accept). `transit_sync::merge` is a
  property-tested semilattice; `POST /edits` is the whole client
  protocol. The Go host treats documents as opaque JSON.

## Conventions

- **Clarify, don't assume.** When a directive is ambiguous, conflicts
  with the GTFS spec or an architecture law, or leaves a design
  decision open, ask before building on an answer you picked.
- **Git is the user's** — never commit, push, stash, or otherwise
  move refs. Do the work, leave the tree, report what changed.
- The TS bindings are GENERATED and gitignored (`just types`, run
  by `just build`; never hand-edit, never commit) — Rust is the
  source of truth, the manifest's computed examples the readable
  wire contract.
- **Parity**: the browser, caps-run, and the Go server run the
  same ops and get the same bytes — `web/` is host glue plus
  render components, never answers (`docs/ARCHITECTURE.md`, web
  section, "Parity"; ratcheted in `just test`).
- New ops go through the extraction pattern (crate boundary
  first, capability wrapper second): the monolith that ops were
  extracted FROM is gone, so a new op is a library + a capability
  module, never a host.

The rest of the gotchas: `docs/CONVENTIONS.md`.

## Testing

**Laws first.** Every behavior change starts with the property
tests that define it, derived from the user's stated intent and the
GTFS spec — never from the current code — and written before the
implementation. When the intent leaves a behavior ambiguous, ask
the user; do not pick an answer and encode it. The rules are in
`docs/TESTING.md`, "Laws first".

Every change lands with tests at the layer that owns the logic —
the expectations (unit/property in the owning crate, module
dispatch test, caps-smoke + runner gates, Go round-trip, Playwright
e2e, real-data dogfood) are spelled out in `docs/TESTING.md`.

## Deliberate non-goals (do not "fix" these)

- No CRDT/automerge for sync — LWW + tombstones is the design.
- transit-network is the authored model; GTFS stays OUT of it — the
  import/export codecs live at the boundary in transit-codec.
- Runcutting stops at blocks + layovers; duties/rosters are a future
  layer, not a `Block` extension.
- wasm-bindgen / JS glue is gone on purpose; every module is the
  raw rustc output with the same four exports — no custom calling
  conventions.
- The Go server stays scaffolding; domain logic the server needs
  becomes a capability module (it hosts cap-sync today).
- The bus is the architecture: no capability grows a private
  document format — shared data crosses modules as kernel
  documents addressed by content hash only.
