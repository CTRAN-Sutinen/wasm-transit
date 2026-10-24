# AGENTS.md

Local-first transit software: GTFS static + GTFS-rt, parsed by Rust,
shipped as **one wasm artifact** that runs in the browser (JS), in
Node (smoke tests), and on the sync server (Go + wazero). The same
bytes everywhere is the point of the project — do not break it.

## Commands

`just` is the task runner. `just check` is the full gate — run it
before declaring work done (Rust suites, Go suite, ABI smoke in Node,
frontend build, Playwright e2e).

```
just build / just wasm     # frontend+artifact / artifact only
just test                  # host-side tests only
just e2e                   # browser e2e (builds first)
just serve [feed.zip ["Publisher"]]   # sync server on :8080
just dev                   # vite hot reload + server alongside
just fixtures              # regenerate the e2e GTFS fixture
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
                patterns, timing profiles, calendar rules, services,
                runcut. Trips are DERIVED (`materialize`) — trips are
                never stored. GTFS is a codec at the boundary, not the
                model; no GTFS types leak in here.
transit-router  RAPTOR earliest-arrival planning.
transit-rt      prost types from the official GTFS-rt proto.
transit-sync   THE sync document: EditRecord, FeedManifest, EditReport,
                pure validation/merge logic. Shared by every host.
transit-ops     dispatch(Request) -> Response: the host-agnostic seam
                + per-feed registry (feed_id keyed).
transit-wasm    THE artifact: flat import-free C ABI over dispatch.
                No wasm-bindgen — do not reintroduce host glue.
server/         Go + wazero host: scaffolding only (HTTP, manifest,
                proxy). It must never make a domain decision.
web/            Svelte 5 runes + Vite. src/wasm.js wraps the artifact
                with a stable GtfsHandle API; components never see
                the ABI.
```

**Ownership rules** (violations here caused real bugs; keep them):

- Hosts own I/O, files, and the clock. The wasm module owns domain
  logic. Dates cross every boundary as "YYYY-MM-DD" strings, times as
  u32 seconds since service-day midnight (values past 86400 are
  legitimate — after-midnight trips).
- The **base feed is immutable**. Diversions produce patched copies
  per service date; `apply_diversions` never mutates its input.
- **JS owns the edit document** (`$state.raw` array in
  `src/state.svelte.js`, replaced wholesale, never mutated in place —
  that is what makes IndexedDB cloning and future sync work). wasm
  holds only a materialized view: `set_diversions(records)` is the
  single entry point, returns per-record validation reports, and
  invalid records are *reported, not rejected*.
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

- **serde_json must keep the `float_roundtrip` feature** (root
  Cargo.toml workspace dep): the default parser is 1 ULP off on some
  f64s (`-122.7 + 0.005` reproducibly), which broke document
  round-trip equality. Found via a transit-network property test.
- **Add new operations at the transit-ops seam**: extend
  `Request`/`Response` (serde-tagged; JSON form is `{"op": ...}`),
  implement in `dispatch`, expose in each host (wasm.js method, Go
  helper). Response payloads must be typed serde structs.
- **Native tests first**: logic tests belong in the crate that owns
  the logic (`transit-ops/tests/dispatch_test.rs` runs the real seam
  without wasm). Property tests use *independent oracles* — see
  `transit-core/tests/prop_test.rs`; proptest regressions seeds are
  kept in `*.proptest-regressions`.
- **wazero `memory.Read` returns a view** — copy bytes out before
  freeing the buffer, or the Rust allocator corrupts them (this bit
  us once).
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
- Port 8080 is the server default — check `ss -tlnp` before starting
  one; `reuseExistingServer` in Playwright will silently test
  against whatever is already there.
- e2e fixture changes: edit `web/e2e/make_fixtures.py`, run
  `just fixtures`; the rt protobuf fixture is generated by
  `cargo run -p transit-rt --example encode_sample`.

## Testing expectations

Every change lands with tests at the layer that owns the logic:

1. Unit/property tests in the owning crate (oracle style for laws).
2. A dispatch test if a new op exists (native, JSON round-trip).
3. Extend `web/e2e/abi-smoke.mjs` for ABI contract changes.
4. A Go round-trip test if server-visible behavior changes.
5. Playwright e2e for user-visible flows (Svelte components should
   not need changes when only the ABI underneath changes).

## Deliberate non-goals (do not "fix" these)

- No CRDT/automerge for sync — LWW + tombstones is the design.
- transit-network is the authored model; GTFS stays OUT of it. The
  GTFS import (inference) and export (materialization) codecs live at
  the boundary and are not written yet — do not grow GTFS-shaped
  fields into the model for codec convenience.
- Runcutting in transit-network stops at blocks + layovers; operator
  duties/rosters are a future layer, not a `Block` extension.
- wasm-bindgen / JS glue in the artifact is gone on purpose; the
  artifact is the raw rustc output with four exports.
- The Go server stays scaffolding; domain logic that the server needs
  becomes a transit-ops op.
