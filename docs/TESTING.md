# Testing expectations

Every change lands with tests at the layer that owns the logic.

## Laws first — from the intent, never from the code

Every behavior change STARTS with the property tests (laws) that
define it, written before the implementation and run red. This repo
has shipped wrong behavior whose prop tests were written to match
the implementation, so the tests certified the bug. The rules:

- **Derive each law from the stated intent** — the user's directive,
  the GTFS spec, the architecture laws — never from what the code
  currently does. Write down, in the law's doc comment, the intent
  it encodes.
- **Independent oracles.** A law's expected answer is recomputed
  from the generated input with code that shares nothing with the
  implementation (`docs/CONVENTIONS.md`, native tests first).
- **Generators produce only valid input** for the domain the law
  claims (a spec-valid feed is valid against the GTFS spec, not
  against our parser). An invalid generator makes a law demand the
  impossible.
- **Audit the existing laws when intent changes.** A law that
  encodes the old behavior is rewritten or deleted in the same
  change that adds the new ones — never left to pass by accident.
- **A red law is fixed in the implementation.** A law is weakened
  or deleted only when the user's intent says the law is wrong, and
  the change says so.
- **Ambiguity is asked, not assumed.** When the intent does not
  settle a behavior the law must pin (an edge case, a spec
  conflict, two readings of a directive), stop and ask the user;
  list the decisions a new law encodes so they can be checked
  before the implementation starts. Every open question is an
  entry in `docs/DECISIONS.md`, which names the work it blocks; a
  law landed on an unconfirmed answer is entered there as
  *Encoded provisionally* and names its pins.

Two runners: `just test` is the host-side gate (no browser) — run
it before declaring any change done; `just e2e` adds the browser
runner, for changes that touch user-visible flows; `just check`
runs both, for changes that touch both sides. A third runner is
optional: `just spec` model-checks the sync store engine
(`server/store/store.tla`) and needs Java 11+ plus tla2tools.jar —
run it only when a change touches the sync engine (`server/store/`,
the store's HTTP surface, or the spec itself); nothing else requires
Java, and `just test` never does. Default bounds are the small pinned
`store.cfg`; the exhaustive tier is
`SPEC_CFG=server/store/store-exhaustive.cfg just spec` (hash and
process symmetry, larger bounds, still exhaustive, about 5 minutes;
`TLC_WORKERS`/`TLC_XMX` size the run). `store-race.cfg` is expected
to fail: it runs GC without its settle step and answers the trace
that step exists for. A fourth is optional too: `just store-contract`
runs the store's backend contract and every engine law against real
object stores (`just store-dev` starts SeaweedFS, Azurite and
fake-gcs-server in podman) — for changes to `server/store/` or an
adapter; `just test` runs the same laws on `mem://`. A managed
provider's bucket runs the same way (`STORE_TEST_URL` and its
credentials), recorded in the store-durability unit.

**The edit loop.** `just test-fast` runs every suite and gate `just
test` runs, over the `wasm-fast` modules (`just caps-fast`: release
semantics without whole-program LTO), so a library change costs
seconds to a minute of module build instead of about ten minutes. It
is the loop, not the gate: a change still passes `just test` before
it is called done, because only release modules are what ships. Two
more things keep the loop short without testing less:

- Dependencies are optimized in dev and test builds
  (`[profile.dev.package."*"]` in `Cargo.toml`); the workspace's own
  crates are not, and debug assertions and overflow checks stay on.
- The Go test binary compiles each server module once, all of them at
  the same time, and keeps the machine code on disk between runs,
  keyed by the module's bytes (`server/compile_test.go`). A damaged
  entry fails loudly. `WASM_TEST_CACHE=off` compiles every module
  afresh, which also runs wazero's compiler under `-race`;
  `WASM_TEST_CACHE=DIR` moves the cache. The server keeps no such cache.

For a narrower loop, run one owner's tests directly: `cargo test -p
<crate>`, or `go test -race -run <Test> .` in `server/`.

Publishing the registry is its own gate: `just registry` runs `cargo
test` and the module smoke, then `just reproducible` — a second clean
build of the tree, at another path with another cargo home, must match
every module byte for byte (about five minutes; docs/architecture/release-artifacts.md) — and publishes only under the pinned toolchain.
Run it for a change to the build itself (the toolchain, the lockfile,
the profile, the recipes) and before a release.

1. **Unit/property tests in the owning crate** (oracle style for
   laws — e.g. `transit-network`'s `trip_law_test` holds that trips
   are stored records (ids, times, edits that keep them, documents
   written before trips were stored), and its
   `combination_law_test` that every date with service is in exactly
   one service combination (D-55); `transit-analysis`'s
   `analysis_prop_test` that a combination's pooled numbers are the
   service on each of its dates; `transit-codec`'s
   `import_prop_test` holds that the imported document is valid by
   construction over every spec-valid feed and deterministic, its
   `trip_import_law_test` that the feed's trips are the document's,
   its `codec_prop_test` the export round trip, and its
   `spec_round_trip_test` that
   GTFS → network → GTFS loses nothing in the GTFS Schedule spec,
   one law per file, against a raw-table oracle independent of the
   parser, and its `dated_calendar_law_test` that a service
   published as dates alone imports as the weekly rule its dates read
   as and exports as dates again (D-105); `transit-plans`'s chain tests hold the
   service-plan chain laws (resolution, windows, validity,
   order-insensitivity); transit-dispatch's `dispatch_laws_test`
   holds the diversion/ripple laws and its `cancel_law_test` the
   trip-cancel laws; `transit-network`'s
   `recovery_law_test` holds that a block's wait is judged against
   the recovery required there — the work rules' requirement wins,
   else the terminal's floor — never its own authored layover (D-9);
   `transit-runs`'s `work_rules_test` the work-rule document (every
   rule optional, the most specific rule, order is not identity),
   `recovery_view_test` that a wait is short exactly where validation
   says layover_infeasible, and `suggest_law_test` that suggested
   blocking covers every unassigned trip once, legally (one place, an
   allowed pair, the required recovery), and adopts into a document
   that validates clean with the peak it named).
2. **A dispatch test if a new op exists** (native, JSON round-trip)
   — the module's `tests/dispatch_test.rs`, plus a flow in
   `crates/cap-abi/caps-smoke.mjs` and a chained runner gate in the
   justfile (`just test` runs both; `just test` and `just e2e`
   build `caps` first). The manifest is generated with REAL
   computed examples — extend `gen_manifest`'s `example_for` so an
   op's docs cannot drift from the module.
   A new document kind lands in `cap_abi::kernel` (its registry and
   bound — `Retain::Host` or `Retain::Last(n)` — and its load, with a
   worked example in gen-manifest's `kernel_ops`) and in the
   capability's `documents: [...]`; caps-smoke holds that every
   module answers the kernel ops (evict, held, about — matching its
   manifest entry) and that each UI's linked assembly registers a
   document once for every capability. A bound that matters to a
   long session is pinned in the owning crate's own test binary
   (`tests/registry_test.rs` — the registries are process-global,
   and eviction would race the dispatch tests). Capability tests
   reach the module seam through `invoke` / `invoke_bytes` (the
   capability alone, kernel loads included), or register documents
   directly (`cap_abi::kernel::NETWORK.register`). Assembly
   independence (`crates/cap-abi/independence.mjs`) re-runs every
   worked example inside each UI's assembly: an op whose answer
   leans on a derived entry another capability computed fails it.
3. **A Go round-trip test** if server-visible behavior changes.
4. **Playwright e2e for user-visible flows** (Svelte components
   should not need changes when only the ABI underneath changes).
   External services are answered by the spec (`page.route`): the
   live feeds (the dev config names the `/rt` relay; a spec that
   proves direct fetches serves its own config), the edit document, and the geocoder
   (the manifest's `services` entry and `/geocode`) — no spec needs
   a running Pelias, and the e2e servers list none, so the
   no-geocoder path is the default. A runner gate stands in for an
   oracle with a `data:` URL (`--ask`).
   The runtime's bounds are pinned there too: a display left
   polling distinct rt snapshots plateaus the rt module's registry
   and linear memory, and a module forced past its memory ceiling
   answers the same bytes from a fresh instance
   (`ui-stop-display`, through `window.__bus`).
   The bus as a page sees it is pinned by the generated UIs' specs
   (assembly and per-frame degraded state, `ui-ride-now` and
   `ui-service-design`), `web/e2e/published-network.spec.mjs`
   (a document refused unless it registers under the pinned hash)
   and `web/e2e/published-picks.spec.mjs` (its own store-mode server
   on a pinned date, `-today`: a published pick is the network a rider
   page loads on its date, a retired one never is, and the planner
   publishes from service design), `web/e2e/deploy-updates.spec.mjs`
   (the same server: open pages re-pin a republished network without
   a reload, one network held, memory flat),
   `web/e2e/deploy-builds.spec.mjs` (its own server over a copy of the
   build — with deploy-rollback the only specs with service workers
   allowed; every other runs with them blocked, since `page.route`
   never sees what a worker answers: both rider pages boot offline, and
   a new build applies at a display's poll and a rider's next
   navigation), `web/e2e/deploy-rollback.spec.mjs` (its own store-mode
   server over another copy: build A, then B, then `POST /builds` back
   to A, and the display runs A's bytes again),
   `web/e2e/basemap-assets.spec.mjs`
   (the map's glyphs and sprites are local when installed, networked
   when not; skipped without a local `basemap.pmtiles`),
   `web/e2e/viewport.spec.mjs` (the pure camera rules — where a map
   opens, when it frames the network) and
   `web/e2e/ride-map-load.spec.mjs` (the ride map opens on the last
   fix, or with none on the last network frame, and makes no move whose
   tiles are thrown away; the tile
   archive's header is fetched once, with no probe — that last one
   skips without a local `basemap.pmtiles`);
   extend those for bus-visible changes. Capability law tests
   (migration laws, validator mutation law, the dispatch diversion
   laws) follow expectation 1.
5. **Web parity ratchets** (`just test`, "WEB GATES"): components
   never import the engine or the bus, no host code reads raw
   rt `stop_updates`, and nothing clones a document to mutate it
   — each against an allowlist that only shrinks, all empty
   (`docs/architecture/web.md`, "Parity"). Logic a view needs
   lands as an op with expectations 1–2, never in TypeScript; a
   unit that empties an allowlist entry removes it in the same
   change.
6. **A generated UI** (`src/uis/<id>/`) lands with
   `web/e2e/ui-<id>.spec.mjs` on the fixture feed, which walks its
   questions end to end, and with a dogfood session (below) in which
   its numbers on the real feed match the runner's. That session
   runs BEFORE the UI is built: the questions' op gaps become
   findings and the UI's blockers (`docs/architecture/generated-uis.md`, the procedure). The GENERATED
   UIs gate in `just test` refuses one without its spec, and
   `e2e/picker.spec.mjs` covers its card with no edit.
7. **Dogfood on real data**: capability work touching feeds or
   payload size runs a `caps-run` session against the C-TRAN feed
   before it is done — fixture-scale gates do not catch real-data
   defects.

## Dogfood QA — the agent interface mirrors the human UI

`caps-run` exercises the same capabilities, bytes, and laws the app
runs; a human asking planner questions through it is doing real QA.
The rules this paradigm earned (the founding C-TRAN session found
four real defects fixture gates missed — a 64 KiB output truncation
among them):

- **All capability work is dogfooded on real data before it is
  done.** Anything touching feeds or payload size runs against
  `testdata/ctran-gtfs.zip` (the real C-TRAN GTFS, NOT COMMITTED —
  licensed data; fetch from c-tran.com) through `caps-run`. The
  reference hashes are in `docs/QA-FINDINGS.md`'s header.
- **A one-off script is never tooling — it is a finding.** If a QA
  session needs bespoke glue (a bootstrap script, a summarizer),
  that is a missing op or runner feature; log it in
  `docs/QA-FINDINGS.md`, don't ship the script.
- **The mirror cuts both ways.** A question that is one op via
  the CLI but absent from the UI is a UI gap; a question awkward
  via the CLI is an op gap. Dogfood sessions generate both
  signals; both land in `docs/QA-FINDINGS.md`.
- **A UI is walked, control by control, before it is done.** A
  passing spec proves the paths it names; it does not prove that each
  control shows its answer where the planner lands. Run `just qa` (the
  page with hot reload over a server on the C-TRAN feed, modules under
  the wasm-fast profile) and drive it with `tools/ui-shot.mjs`: open a
  selection by its hash, run steps, read the screenshot, the status
  line, and the page's errors. Its browser profile (`.qa/`) keeps the
  plans a walk forked, so a fix is re-checked in seconds: a page edit
  needs no build, a Rust edit only `just caps-fast` and a reload. Every
  control on every stage, on a forked plan, then the paths between
  stages; each control fixed is pinned in the UI's spec.
- **Findings are open work, not a log.** A session adds new
  findings (and re-confirmation lines on old ones) to
  `docs/QA-FINDINGS.md`; the change that fixes a finding deletes it
  and names its pin. The session narrative itself is not kept —
  what it proved lands as tests, and what it found lands as
  findings.

## Performance baselines — measured, never gated

`just perf` drives a generated UI in Chromium against the real feed
and reports what it costs the browser; it fails only when the UI
cannot be driven, never on a number (timings on a dev box are noisy).
Its specs are `web/perf/<ui>.perf.mjs` (ride-now; the stop display,
left running at a transit center — `PERF_STOP` names it, `PERF_LOOPS`
replays the recording that many times over), its probe
`web/perf/probe.mjs` (the DevTools protocol, test-side only — the page
is the shipped page, observed).

- **What it measures, per step:** wall time; main-thread, script,
  layout and style time; long tasks and long animation frames (their
  scripts named); two sampled CPU profiles — the page's and the bus
  worker's (where the capability modules run; the probe attaches to
  it through the browser's DevTools session) — each summarized by
  self time per bucket (a capability module's crate,
  `wasm:transit_router`; a JS bundle file; the engine's GC and
  compile) and saved as a `.cpuprofile` for DevTools' Performance
  panel. Memory, after a forced collection: the page's JS heap and
  the worker's, the array buffers outside them, and per capability
  module (read through the engine's `window.__bus`) its linear
  memory (the JS heap counters do not see it, and it never shrinks),
  the documents it holds, and its resets; DOM nodes and listeners.
- **The build it measures** is a profiling build: the capability
  modules keep their name sections (`CARGO_PROFILE_RELEASE_STRIP=
  debuginfo`; the code is the shipped code) and the frontend is
  unminified (`web/dist-perf`). The next `just caps` restores the
  stripped modules.
- **The live feeds are recorded, then replayed** one snapshot per
  poll, with the browser clock installed at the recording's first
  header timestamp and fast-forwarded a poll at a time — a 45-minute
  soak takes minutes and every run sees the same data.
  `node web/perf/record-rt.mjs [count] [dir] [interval]` records the
  feeds `web/public/live-feeds.json` names into `testdata/perf-rt*/`
  (gitignored — agency data); `PERF_RT=testdata/perf-rt-45m just perf`
  picks one.
- **The soak** leaves the UI open while every remaining snapshot
  arrives, sampling memory every 10 polls: growth across the samples
  that does not level off is a leak to chase with a heap snapshot.
- **Reports** land in `web/perf-results/<stamp>/` (`summary.md`,
  `report.json`, `profiles/`, gitignored). Headless Chromium renders
  the map with software WebGL, so map rendering costs more there than
  on a device with a GPU — compare runs with each other, not with a
  phone.
