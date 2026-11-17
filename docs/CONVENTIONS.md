# Conventions and gotchas

Git is the user's (see the root AGENTS.md). Frontend shape and file
ownership are in `docs/architecture/web.md`. This file holds the
gotchas, for Rust, wasm, ops, and the frontend.

- **serde_json must keep the `float_roundtrip` feature** (root
  Cargo.toml workspace dep). The default parser is 1 ULP off on some
  f64s (`-122.7 + 0.005` reproducibly). This broke document
  round-trip equality. A transit-network property test found it.
- **Add new operations as a library + capability module.** The domain
  logic lands in a library crate. The module's `Request`/`Response`
  (serde-tagged; JSON form is `{"op": ...}`) is the seam. Hosts load
  the module: the browser in its bus worker, the Go server via
  `NewWasmHost`. Response payloads must be typed serde structs.
- **Model vocabulary is planner vocabulary, not GTFS's.** These are
  the model's terms:
  - A `ServiceType` is the service (the GTFS service_id). It is the
    day-linked design unit.
  - A route runs in named `Direction`s. A pattern runs in at most one.
    GTFS's binary direction_id is only the codec's encoding of them.
  - `PatternTrips` are one pattern's trips under a service type.
  - A `Trip` is a stored record. Its id is its identity (the agency's
    id, for an imported trip). It has its own departure (`Departs`:
    one start, or a frequency) and its own times.
  - A `Network` document is a service plan.

  "Schedule" is not a model word. To a planner, a schedule or
  timetable is a route's direction under one service combination (the
  types that run together, D-55), across all its patterns. That is the
  derived `Timetable` (directory `timetable`). So the model never
  names the per-pattern list that way. Documents written before trips
  were stored still read: they are network v1, which cap-migrate
  brings forward. Do not rename these terms back toward GTFS's
  calendar and service words.
- **Diagnostics are measurements. Regressions are assertions.** Scratch
  repro specs live in /tmp with their own Playwright config (never in
  web/e2e). Delete them after use. Every bug that they catch must land
  as a permanent suite assertion before the work is done. If a test is
  worth writing, it is worth keeping.
- **Native tests first.** Logic tests belong in the crate that owns
  the logic. Each capability module's `tests/dispatch_test.rs` runs
  the real seam without wasm. The library under it holds the law
  tests. Property tests use *independent oracles* (see
  `transit-codec/tests/spec_round_trip_test.rs`). Proptest regression
  seeds are kept in `*.proptest-regressions`.
- **wazero `memory.Read` returns a view.** Copy the bytes out before
  you free the buffer. Otherwise the Rust allocator corrupts them.
  This happened once.
- **A wasm module is not reentrant.** One instance runs one invoke at
  a time, because its heap and linear memory are single-instance
  state. The Go host serializes the whole alloc→free sequence in
  `WasmHost.call` (net/http is per-goroutine). `go test -race` is part
  of `just test`, and `server/race_test.go` gives it something to
  watch. If you touch that lock, verify that the test still fails.
- **just 1.59 does not parse `key=value` named args.** Recipes use
  positional args.
- The GTFS-rt proto is **proto2**. Required fields are non-Option in
  prost. Nested types live in modules (`gtfs_rt::trip_update::*`).
- A date can have **multiple active service keys**. Every
  departure/block row carries its service_id. Do not assume one.
- `#[profile.release]` in the root Cargo.toml is tuned for
  registration speed (opt-level 3, LTO, panic=abort, strip; D-72). The
  streamed network decode is per-byte code that "s" leaves un-inlined.
  Measure (`just perf`, and the module sizes) before you change it.
- `[profile.dev]` (and test, which inherits it) keeps
  `debug = "line-tables-only"`. Backtraces then name file and line,
  and `target/` stays a fraction of full-DWARF size. Do not raise it
  to full debug info in the shared profile. A local debugging session
  sets `CARGO_PROFILE_DEV_DEBUG=true` for itself.
- **An op answers through `cap_abi::answer` / `outcome`**, never a
  `serde_json::json!` tree. The tree is a second copy of the answer
  and sorts its keys, so a document inside it stops being its
  canonical bytes. Runner gates grep answers in struct field order.
  They read the whole answer (`grep … > /dev/null`, never `grep -q`).
  `grep -q` quits at the first match and closes the pipe while the
  runner is still writing, and the runner dies with EPIPE.
- **A whole document never decodes through the tagged `Request`
  envelope.** `#[serde(tag = "op")]` is internally tagged. Serde
  buffers the entire input into a generic tree before it knows the
  variant, then decodes again from the tree. On the C-TRAN network
  (2.7 MB), that peaks at ~19 MB, against 2.3 MB for a direct parse.
  Wasm linear memory never shrinks, so the peak is a permanent
  per-module tab cost. Every document registers through the kernel's
  loads (`cap_abi::kernel`). The bulk op parses the bare bytes
  straight into the type. The JSON lane peels the envelope and leaves
  the document raw (`RawValue`). No capability's Request carries a
  document to register.

## Writing

Write code comments and the developer documents at about 80% of
ASD-STE100, Simplified Technical English. The developer documents are
`AGENTS.md`, `crates/AGENTS.md`, everything under `docs/` except
`docs/guide/`, the capabilities' `OPS.md` files, and `qa/`. The guide
(`docs/guide/`) and the operator guides (`DEPLOYMENTS.md`,
`DEPLOYMENTS-SERVER.md`) follow ISO 24495-1. The two standards agree.

- Write sentences of 20 words or fewer when you can. A sentence that
  describes may have 25 words.
- Write one instruction in each sentence. Write an instruction as a
  command.
- Use the active voice. Name the thing that does the action.
- Use one name for each concept. A code name is permitted. A synonym is
  not. When the code and a document use different names for one
  concept, fix the document.
- Do not use dash asides, words in capitals for emphasis, metaphors, or
  aphorisms.
- Do not put a decision number or a finding number inside a sentence.
  Put it at the end of the paragraph, or in a Why entry.
- Keep each paragraph to one topic.
- Keep every fact. A shorter text that drops a fact is wrong.

"About 80%" means: apply the sentence rules above, and do not apply the
STE dictionary. Technical words and code names stay.

### Why: about 80% of ASD-STE100

- **Holds:** the developer documents and the code comments follow the
  rules above. New and changed text follows them now. Older text
  changes when it is next edited, or in the plain-docs unit
  (`docs/units/plain-docs.md`).
- **Because:** people read these documents as well as agents. The
  earlier style was difficult for a new developer to read. It had dash
  asides, words in capitals, aphorisms, and decision numbers inside
  sentences, and one concept often had two names. Agents read plain
  sentences as well as dense ones. So the plain style costs nothing.
- **Rejected:** the full standard, because its dictionary removes
  technical words a developer needs. One reported test also found that
  a model told to apply the full standard dropped facts. No rule was
  also rejected, because each documentation sweep brings the old
  style back.
- **Decided:** D-174.


## Frontend

- **TypeScript end to end.** Every type that crosses the wasm boundary
  is generated (`just types`, which wipes and regenerates). `just build`
  and `just dev` run it, and `just test` refreshes it.
  - ts-rs derives on the model crates emit `web/src/types/*.ts`.
    These files are gitignored. Never commit them and never edit them
    by hand.
  - Rust is the source of truth. The wire shape that serde attributes
    produce (tags, flatten, optional fields) is readable in the
    manifest's computed op examples.
  - `.cargo/config.toml` pins `TS_RS_EXPORT_DIR` to `web/src/types`
    for every cargo run. So a plain `cargo test` regenerates the
    contract in place. Without the pin, ts-rs scatters bindings into
    `crates/*/bindings` (gitignored).
  - `npm run typecheck` gates all of src/. It is svelte-check, wired
    into `just build`. Components carry `lang="ts"`. The runes modules are
    `.svelte.ts`.
  - TypeScript is pinned to 6.x, because svelte-check does not yet
    accept TS 7.
- Response field names from the modules are snake_case (the serde
  default). A TS cast with a camelCase field name compiles and
  typechecks, but it reads `undefined` at runtime. This shipped once:
  `streets_id` vs `streetsId`.
- **Svelte `$state` proxies are not structured-cloneable.** Persist
  plain data (`$state.raw`, wholesale replacement), never proxies.
- **await arrayBuffer() before you cross into wasm.** A Promise in a
  `&[u8]` slot decodes as zero bytes, silently.
- MapLibre's `LngLatBounds.extend` parses a bare array-of-arrays as
  the `[[west, south], [east, north]]` form and silently ignores the
  rest. Extend one point at a time (RideMap, FocusMap, and CoverageMap
  each do).
- **A shared component carries its own styles** (a scoped `<style>`
  block). `base.css` holds tokens and the few class names that
  components render with. A look kept in a page's stylesheet is lost
  when that page goes. Combobox's list rendered as bare bullets this
  way.
- **Use only widely available web features** (D-135): Baseline widely
  available, which is 30 months in every major browser. A newer feature
  waits. Add no polyfill. The live-feeds panel handles Escape, an outside
  press, and focus itself, for this reason.
- **Accessibility is a gate, so write to it** (`web/e2e/a11y.spec.mjs`,
  `docs/TESTING.md`). These rules come from what the gate caught:
  - **Text on an accent or warn fill is `var(--on-accent)`**, never
    `#fff`. White on the dark accent is 2.26:1. A route badge with an
    authored route color keeps white (D-184).
  - **Draw focus as an `outline`**, never as a `box-shadow` alone.
    Forced colors (Windows High Contrast) drop a box-shadow.
    `base.css` gives every `:focus-visible` a ring. A control that sets
    `outline: none` must draw its own.
  - **Motion is opt-in.** Put every animation and transition inside
    `@media (prefers-reduced-motion: no-preference)`. The loading
    placeholder is `.skeleton span` in `base.css`. A panel sets only
    its height and radius.
  - **A view change moves focus.** The shell focuses the new view's
    heading (`h1` or `h2`, which gets `tabindex="-1"`). A polite region
    (`role="status"`, class `sr-only`) says the status line or the size of
    the answer. The status line is never the only place a message
    lives.
  - **The page has one `<main>`**, and no content sits outside a
    landmark. The map is a labelled region, and the corner controls are
    a `<header>`.
  - **A radio group or a tab list takes the arrow keys.** Use the
    `roving` action (`components/roving.ts`): the selected item has
    `tabindex="0"`, the others `-1`. A tab list names its panel
    (`aria-controls`, `role="tabpanel"`).
  - **A combobox option is the `role="option"` element itself**, never
    a button inside one. The input names the active option with
    `aria-activedescendant`.
  - **A target is at least 24 by 24 CSS pixels.** `--muted` text must
    keep 4.5:1 on `--hover` and `--accent-soft`.
- **A clipped panel in a column needs `flex: none`.** A flex child with
  `overflow: hidden` has no minimum height. A scrolling column then
  squeezes it to a line (the pick timeline did). A spec that only
  checks its text still passes, so check its box.
- To change an e2e fixture, edit `web/e2e/make_fixtures.py` and run
  `just fixtures`. The rt protobuf fixture is generated by
  `cargo run -p transit-rt --example encode_sample`.
- A frame asks whether its capability assembled (`available`), or it
  just calls `ask` and catches `Unavailable`. Do one of the two every
  time (both are memoized). Never gate it on "no manifest yet".
  Another frame may have loaded the manifest first, and a gated frame
  then assembles nothing.
- Playwright serves the built bundle (`web/dist` via the Go server).
  After a source change, `npx playwright test` alone runs the previous
  build. `just e2e` builds first, or run `npx vite build`.
- Playwright e2e runs its own server on :8081 with
  `reuseExistingServer: false`. This keeps it hermetic: a shared
  server carries a live edit document, and test records must not land
  in a dev server. Port 8080 is the dev-server default. Check
  `ss -tlnp` before you start one.
