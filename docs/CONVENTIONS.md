# Conventions and gotchas

Git is the user's — see the root AGENTS.md. Frontend shape and file
ownership live in `docs/architecture/web.md`; this file
holds the gotchas, Rust/wasm/ops and frontend alike.

- **serde_json must keep the `float_roundtrip` feature** (root
  Cargo.toml workspace dep): the default parser is 1 ULP off on
  some f64s (`-122.7 + 0.005` reproducibly), which broke document
  round-trip equality. Found via a transit-network property test.
- **Add new operations as a library + capability module**: the
  domain logic lands in a library crate, the module's
  `Request`/`Response` (serde-tagged; JSON form is `{"op": ...}`)
  is the seam, and hosts load the module (the browser in its bus
  worker, the Go server via `NewWasmHost`). Response payloads
  must be typed serde structs.
- **Model vocabulary is planner vocabulary, not GTFS's**: a
  `ServiceType` IS the service (the GTFS service_id — the day-linked
  design unit); a route runs in named `Direction`s and a pattern in
  at most one (GTFS's binary direction_id is only the codec's
  encoding of them); `PatternTrips` are one pattern's trips under a
  service type; a `Trip` is a stored record — its id IS its identity
  (the agency's, for an imported trip), with its own departure
  (`Departs`: one start, or a frequency) and its own times; a
  `Network` document is a service plan. "Schedule" is NOT a model
  word: to a planner a schedule or timetable is a route's direction
  under one service combination (the types that run together, D-55)
  across all its patterns — that is the
  derived `Timetable` (directory `timetable`) — so the model never
  names the per-pattern list that way (documents written before trips
  were stored still read: they are network v1, which cap-migrate
  brings forward). Do not rename
  these back toward GTFS's calendar/service words.
- **Diagnostics are measurements; regressions are assertions.**
  Scratch repro specs live in /tmp with their own Playwright config
  (never in web/e2e) and get deleted after use; every bug they
  catch must land as a permanent suite assertion before the work is
  done. If it is worth writing a test, it is worth keeping.
- **Native tests first**: logic tests belong in the crate that
  owns the logic (each capability module's
  `tests/dispatch_test.rs` runs the real seam without wasm, the
  library under it holds the law tests). Property tests use
  *independent oracles* — see
  `transit-codec/tests/spec_round_trip_test.rs`;
  proptest regression seeds are kept in `*.proptest-regressions`.
- **wazero `memory.Read` returns a view** — copy bytes out before
  freeing the buffer, or the Rust allocator corrupts them (this bit
  us once).
- **A wasm module is NOT reentrant** — one instance, one invoke
  at a time (its heap and linear memory are single-instance
  state). The Go host serializes the whole alloc→free sequence in
  `WasmHost.call` (net/http is per-goroutine). `go test -race` is
  part of `just test`, and `server/race_test.go` is what gives it
  something to watch — verify the test still fails if you touch
  that lock.
- **just 1.59 does not parse `key=value` named args** — recipes use
  positional args.
- GTFS-rt proto is **proto2**: required fields are non-Option in
  prost; nested types live in modules (`gtfs_rt::trip_update::*`).
- A date can have **multiple active service keys**; every
  departure/block row carries its service_id. Don't assume one.
- `#[profile.release]` in the root Cargo.toml is tuned for
  registration speed (opt-level 3, LTO, panic=abort, strip; D-72):
  the streamed network decode is per-byte code "s" leaves
  un-inlined. Measure (`just perf`, and the module sizes) before
  changing it.
- `[profile.dev]` (and test, which inherits it) keeps
  `debug = "line-tables-only"`: backtraces name file and line, and
  `target/` stays a fraction of full-DWARF size. Don't raise it to
  full debug info in the shared profile; a local debugging session
  sets `CARGO_PROFILE_DEV_DEBUG=true` for itself.
- **An op answers through `cap_abi::answer` / `outcome`**, never a
  `serde_json::json!` tree: the tree is a second copy of the answer
  and sorts its keys, so a document inside it stops being its
  canonical bytes. Runner gates grep answers in struct field order,
  and read the whole answer (`grep … > /dev/null`, never `grep -q`:
  quitting at the first match closes the pipe while the runner is
  still writing, and it dies with EPIPE).
- **A whole document never decodes through the tagged `Request`
  envelope.** `#[serde(tag = "op")]` is internally tagged: serde
  buffers the ENTIRE input into a generic tree before it knows the
  variant, then decodes again from the tree. On the C-TRAN network
  (2.7 MB) that peaks at ~19 MB against 2.3 MB for a direct parse —
  and wasm linear memory never shrinks, so the peak is a permanent
  per-module tab cost. Every document registers through the
  kernel's loads (`cap_abi::kernel`): the bulk op parses the bare
  bytes straight into the type, and the JSON lane's envelope is
  peeled with the document left raw (`RawValue`) — no capability's
  Request carries a document to register.

## Frontend

- **TypeScript end to end** — every type that crosses the wasm
  boundary is GENERATED (`just types`, which wipes and regenerates;
  `just build` and `just dev` run it, `just test` refreshes it):
  ts-rs derives on the model crates emit `web/src/types/*.ts`,
  gitignored — never committed, never hand-edited. Rust is the
  source of truth; the wire shape serde attributes produce (tags,
  flatten, optional fields) is readable in the manifest's computed
  op examples. `.cargo/config.toml` pins
  `TS_RS_EXPORT_DIR` to `web/src/types` for every cargo run, so a
  plain `cargo test` regenerates the contract in place (without it,
  ts-rs scatters bindings into `crates/*/bindings` — gitignored).
  `npm run typecheck` (svelte-check, wired into `just
  build`) gates all of src/ — components carry `lang="ts"`, the
  runes modules are `.svelte.ts`. TypeScript is pinned to 6.x:
  svelte-check does not yet accept TS 7.
- Response FIELD names from the modules are snake_case (the serde
  default) — a TS cast with a camelCase field name compiles and
  typechecks fine but reads `undefined` at runtime (this shipped
  once: `streets_id` vs `streetsId`).
- **Svelte `$state` proxies are not structured-cloneable** — persist
  plain data (`$state.raw`, wholesale replacement), never proxies.
- **await arrayBuffer() before crossing into wasm** — a Promise in
  a `&[u8]` slot decodes as zero bytes, silently.
- MapLibre's `LngLatBounds.extend` parses a bare array-of-arrays as
  the `[[west, south], [east, north]]` form and silently ignores
  the rest — extend one point at a time (RideMap, FocusMap, and
  CoverageMap each do).
- e2e fixture changes: edit `web/e2e/make_fixtures.py`, run `just
  fixtures`; the rt protobuf fixture is generated by `cargo run -p
  transit-rt --example encode_sample`.
- A frame asks whether its capability assembled (`available`) or
  just `ask`s and catches `Unavailable` — every time (both are
  memoized), never gated on "no manifest yet": another frame may
  have loaded the manifest first, and a gated frame then assembles
  nothing.
- Playwright serves the BUILT bundle (`web/dist` via the Go
  server): after a source change, `npx playwright test` alone runs
  the previous build — `just e2e` builds first, or `npx vite build`.
- Playwright e2e runs its own server on :8081 with
  `reuseExistingServer: false` (hermetic: a shared server carries a
  live edit document, and test records must not land in a dev
  server). Port 8080 is the dev-server default — check `ss -tlnp`
  before starting one.
