# Testing expectations

Every change lands with tests at the layer that owns the logic.

## Laws first — from the intent, never from the code

Every behavior change starts with the property tests (laws) that
define it. Write them before the implementation and run them red.
This repo has shipped wrong behavior whose property tests were written
to match the implementation, so the tests certified the bug. The rules:

- **Derive each law from the stated intent.** Use the user's directive,
  the GTFS spec, and the architecture laws. Never use what the code
  currently does. Write the intent that the law encodes in the law's
  doc comment.
- **Independent oracles.** Recompute a law's expected answer from the
  generated input. Use code that shares nothing with the implementation
  (`docs/CONVENTIONS.md`, native tests first).
- **Generators produce only valid input** for the domain the law
  claims. A spec-valid feed is valid against the GTFS spec, not against
  our parser. An invalid generator makes a law demand the impossible.
- **Audit the existing laws when intent changes.** A law that encodes
  the old behavior is rewritten or deleted in the same change that adds
  the new ones. Do not leave it to pass by accident.
- **A red law is fixed in the implementation.** Weaken or delete a law
  only when the user's intent says the law is wrong, and the change
  says so.
- **Ambiguity is asked, not assumed.** Sometimes the intent does not
  settle a behavior that the law must pin (an edge case, a spec
  conflict, two readings of a directive). Then stop and ask the user.
  List the decisions that a new law encodes, so the user can check them
  before the implementation starts. Every open question is an entry in
  `docs/DECISIONS.md`, which names the work it blocks. A law that lands
  on an unconfirmed answer is entered there as *Encoded provisionally*
  and names its pins.

There are two runners:

- `just test` is the host-side gate (no browser). Run it before you
  declare any change done.
- `just e2e` adds the browser runner, for changes that touch
  user-visible flows.
- `just check` runs both, for changes that touch both sides.

A third runner is optional: `just spec` model-checks the sync store
engine (`server/store/store.tla`). It needs Java 11+ plus
tla2tools.jar. Run it only when a change touches the sync engine
(`server/store/`, the store's HTTP surface, or the spec itself).
Nothing else requires Java, and `just test` never does. The default
bounds are the small pinned `store.cfg`. The exhaustive tier is
`SPEC_CFG=server/store/store-exhaustive.cfg just spec`. It uses hash
and process symmetry and larger bounds, it is still exhaustive, and it
takes about 5 minutes. `TLC_WORKERS` and `TLC_XMX` size the run.
`store-race.cfg` is expected to fail. It runs GC without its settle
step and answers the trace that the settle step exists for.

A fourth runner is optional too: `just store-contract` runs the store's
backend contract and every engine law against real object stores.
`just store-dev` starts SeaweedFS, Azurite and fake-gcs-server in
podman. Run `just store-contract` for changes to `server/store/` or an
adapter. `just test` runs the same laws on `mem://`. A managed
provider's bucket runs the same way (`STORE_TEST_URL` and its
credentials). The store-durability unit records that run.

**The edit loop.** `just test-fast` runs every suite and gate `just
test` runs, over the `wasm-fast` modules (`just caps-fast`: release
semantics without whole-program LTO). A
library change then costs seconds to a minute of module build, not
about ten minutes. It is the loop, not the gate. A change still passes
`just test` before you call it done, because only release modules
ship. `just e2e-fast` is the browser side of the same loop. It runs the
specs over the `wasm-fast` modules and passes Playwright's arguments
through. One spec (`just e2e-fast e2e/ui-stops.spec.mjs`) or one test
by name (`-g`) runs alone. `just e2e` is the gate. Two more things keep
the loop short without testing less:

- Dependencies are optimized in dev and test builds
  (`[profile.dev.package."*"]` in `Cargo.toml`). The workspace's own
  crates are not optimized. Debug assertions and overflow checks stay
  on.
- The Go test binary compiles each server module once, all of them at
  the same time. It keeps the machine code on disk between runs, keyed
  by the module's bytes (`server/compile_test.go`). A damaged entry
  fails loudly. `WASM_TEST_CACHE=off` compiles every module afresh,
  which also runs wazero's compiler under `-race`.
  `WASM_TEST_CACHE=DIR` moves the cache. The server keeps no such
  cache.

For a narrower loop, run one owner's tests directly: `cargo test -p
<crate>`, or `go test -race -run <Test> .` in `server/`.

Publishing the registry is its own gate. `just registry` runs `cargo
test` and the module smoke, then `just reproducible`. That recipe makes
a second clean build of the tree, at another path with another cargo
home. The second build must match every module byte for byte (about
five minutes; docs/architecture/release-artifacts.md). `just registry`
publishes only under the pinned toolchain. Run it for a change to the
build itself (the toolchain, the lockfile, the profile, the recipes)
and before a release.

1. **Unit/property tests in the owning crate** (oracle style for laws).
   Examples:
   - `transit-network`'s `trip_law_test` holds that trips are stored
     records: ids, times, edits that keep them, and documents written
     before trips were stored.
   - `transit-network`'s `combination_law_test` holds that every date
     with service is in exactly one service combination (D-55).
   - `transit-analysis`'s `analysis_prop_test` holds that a
     combination's pooled numbers are the service on each of its dates.
   - `transit-codec`'s `import_prop_test` holds that the imported
     document is valid by construction over every spec-valid feed, and
     that the import is deterministic.
   - `transit-codec`'s `trip_import_law_test` holds that the feed's
     trips are the document's.
   - `transit-codec`'s `codec_prop_test` holds the export round trip.
   - `transit-codec`'s `spec_round_trip_test` holds that
     GTFS → network → GTFS loses nothing in the GTFS Schedule spec. It
     has one law per file, against a raw-table oracle that is
     independent of the parser.
   - `transit-codec`'s `dated_calendar_law_test` holds that a service
     published as dates alone imports as the weekly rule its dates read
     as, and exports as dates again (D-105).
   - `transit-plans`'s chain tests hold the service-plan chain laws:
     resolution, windows, validity, and order-insensitivity.
   - transit-dispatch's `dispatch_laws_test` holds the diversion and
     ripple laws. Its `cancel_law_test` holds the trip-cancel laws.
   - `transit-network`'s `recovery_law_test` holds that a block's wait
     is judged against the recovery required there. The work rules'
     requirement wins, else the terminal's floor. The wait is never
     judged against its own authored layover (D-9).
   - `transit-runs`'s `work_rules_test` holds the work-rule document:
     every rule is optional, the most specific rule wins, and order is
     not identity.
   - `transit-runs`'s `recovery_view_test` holds that a wait is short
     exactly where validation says layover_infeasible.
   - `transit-runs`'s `suggest_law_test` holds that suggested blocking
     covers every unassigned trip once, and legally: one place, an
     allowed pair, the required recovery. It also holds that the
     suggestion adopts into a document that validates clean with the
     peak it named.
2. **A dispatch test if a new op exists** (native, JSON round-trip).
   Add it in the module's `tests/dispatch_test.rs`. Add a flow in
   `crates/cap-abi/caps-smoke.mjs` and a chained runner gate in the
   justfile. `just test` runs both. `just test` and `just e2e` build
   `caps` first. The manifest is generated with real computed examples.
   Extend `gen_manifest`'s `example_for`, so an op's docs cannot drift
   from the module.
   A new document kind lands in `cap_abi::kernel`. That change adds its
   registry and bound (`Retain::Host` or `Retain::Last(n)`) and its
   load, with a worked example in gen-manifest's `kernel_ops`. The kind
   also goes in the capability's `documents: [...]`. caps-smoke holds
   that every module answers the kernel ops (evict, held, about),
   matching its manifest entry. It also holds that each UI's linked
   assembly registers a document once for every capability. Pin a bound
   that matters to a long session in the owning crate's own test binary
   (`tests/registry_test.rs`). The registries are process-global, and
   eviction would race the dispatch tests. Capability tests reach the
   module seam through `invoke` / `invoke_bytes` (the capability alone,
   kernel loads included). They can also register documents directly
   (`cap_abi::kernel::NETWORK.register`). Assembly independence
   (`crates/cap-abi/independence.mjs`) re-runs every worked example
   inside each UI's assembly. An op whose answer leans on a derived
   entry that another capability computed fails it.
3. **A Go round-trip test** if server-visible behavior changes.
4. **Playwright e2e for user-visible flows.** Svelte components should
   not need changes when only the ABI underneath changes.
   The spec answers external services with `page.route`:
   - the live feeds (the dev config names the `/rt` relay). A spec that
     proves direct fetches serves its own config;
   - the edit document;
   - the geocoder (the manifest's `services` entry and `/geocode`).

   No spec needs a running Pelias. The e2e servers list none, so the
   no-geocoder path is the default. A runner gate stands in for an
   oracle with a `data:` URL (`--ask`).
   The specs pin the runtime's bounds too. A display left polling
   distinct rt snapshots plateaus the rt module's registry and linear
   memory. A module forced past its memory ceiling answers the same
   bytes from a fresh instance (`ui-stop-display`, through
   `window.__bus`).
   These specs pin the bus as a page sees it:
   - The generated UIs' specs pin assembly and per-frame degraded
     state (`ui-ride-now` and `ui-service-design`).
   - `web/e2e/published-network.spec.mjs` pins that a document is
     refused unless it registers under the pinned hash.
   - `web/e2e/published-picks.spec.mjs` has its own store-mode server
     on a pinned date (`-today`). It pins three facts. A published pick
     is the network that a rider page loads on its date. A retired pick
     is never loaded. The planner publishes from service design.
   - `web/e2e/deploy-updates.spec.mjs` uses the same server. It pins
     that open pages re-pin a republished network without a reload, and
     that one network is held and memory stays flat.
   - `web/e2e/deploy-builds.spec.mjs` has its own server over a copy of
     the build. With deploy-rollback, these are the only specs that
     allow service workers. Every other spec runs with them blocked,
     because `page.route` never sees what a worker answers. The spec
     pins that both rider pages boot offline, and that a new build
     applies at a display's poll and at a rider's next navigation.
   - `web/e2e/deploy-rollback.spec.mjs` has its own store-mode server
     over another copy. It runs build A, then B, then `POST /builds`
     back to A. The display then runs A's bytes again.
   - `web/e2e/basemap-assets.spec.mjs` pins that the map's glyphs and
     sprites are local when installed and networked when not. It skips
     without a local `basemap.pmtiles`.
   - `web/e2e/viewport.spec.mjs` pins the pure camera rules: where a
     map opens, and when it frames the network.
   - `web/e2e/ride-map-load.spec.mjs` pins that the ride map opens on
     the last fix, or with none on the last network frame. It also pins
     that the map makes no move whose tiles are thrown away, and that
     the tile archive's header is fetched once, with no probe. That
     spec skips without a local `basemap.pmtiles`.

   Extend those specs for bus-visible changes. Capability law tests
   (migration laws, validator mutation law, the dispatch diversion
   laws) follow expectation 1.
5. **Web parity ratchets** (the web gates of `just test`). Three ratchets
   exist: components never import the engine or the bus, no host code
   reads raw rt `stop_updates`, and nothing clones a document to
   mutate it. Each runs against an allowlist that only shrinks, and all
   are empty (`docs/architecture/web.md`, "Parity"). Logic that a view
   needs lands as an op with expectations 1–2, never in TypeScript. A
   unit that empties an allowlist entry removes it in the same change.
6. **A generated UI** (`src/uis/<id>/`) lands with
   `web/e2e/ui-<id>.spec.mjs` on the fixture feed, which walks its
   questions end to end. It also lands with a dogfood session (below)
   in which its numbers on the real feed match the runner's. That
   session runs **before** the UI is built. The questions' op gaps
   become findings and the UI's blockers
   (`docs/architecture/generated-uis.md`, the procedure). The
   generated UIs gate in `just test` refuses a UI without its spec.
   `e2e/picker.spec.mjs` covers its card with no edit.
7. **Accessibility and delivery gates** (`just e2e`). A change to a
   rider page, a shared component, or the static headers keeps these
   green:
   - `web/e2e/a11y.spec.mjs` runs axe-core over every view of ride now
     and the stop display, and over the picker. It checks WCAG 2.2 A and
     AA and the best-practice rules, at a desktop width in light mode
     and a phone width in dark mode, from the server and from the
     static exports. Any violation fails the test and names the rule
     and the nodes. A new view of a rider UI adds its walk there. Service
     design is not in the gate yet (`docs/units/web-conformance.md`).
   - `web/e2e/a11y-keyboard.spec.mjs` pins what axe cannot see: focus
     moves to the new view's heading and a polite region speaks, the
     one `<main>`, the arrow keys in the radio and tab groups, the
     live-feeds panel's Escape and focus, the accent text token in both
     modes, reduced motion, and the message when an agency's GTFS URL
     fails.
   - `server/headers_test.go` and `web/e2e/headers.spec.mjs` pin the
     headers: compression, `nosniff`, and each rider page's policy.
     The spec runs each page under its policy, from the server and from
     the export, and fails on any reported violation. The `_headers`
     the export writes is asserted in `web/e2e/static-deploy.spec.mjs`.
   A screen reader run by hand (VoiceOver, TalkBack, NVDA) is the part
   no gate covers. It is a dogfood session
   (`docs/units/web-conformance.md`).
8. **Dogfood on real data.** Capability work that touches feeds or
   payload size runs a `caps-run` session against the C-TRAN feed
   before it is done. Fixture-scale gates do not catch real-data
   defects.

## Dogfood QA — the agent interface mirrors the human UI

`caps-run` exercises the same capabilities, bytes, and laws that the
app runs. A human who asks planner questions through it does real QA.
The founding C-TRAN session found four real defects that fixture gates
missed. A 64 KiB output truncation was one of them. The session earned
these rules:

- **All capability work is dogfooded on real data before it is done.**
  Anything that touches feeds or payload size runs against
  `testdata/ctran-gtfs.zip` through `caps-run`. This is the real C-TRAN
  GTFS. It is not committed, because the data is licensed. Fetch it
  from c-tran.com. The reference hashes are in `docs/QA-FINDINGS.md`'s
  header. Import the feed once in a named session (`--session`), and
  use `--brief` so the 9.6 MB network does not print. Later commands in
  the session reuse the import, and name it only by
  `$net.network_hash`. Browse a capability with `tools <cap> --brief`
  (`docs/architecture/registry.md`).
- **A one-off script is never tooling. It is a finding.** If a QA
  session needs bespoke glue (a bootstrap script, a summarizer), that
  is a missing op or runner feature. Log it in `docs/QA-FINDINGS.md`.
  Do not ship the script.
- **The mirror cuts both ways.** A question that is one op through the
  CLI but absent from the UI is a UI gap. A question that is awkward
  through the CLI is an op gap. Dogfood sessions generate both signals.
  Both land in `docs/QA-FINDINGS.md`.
- **Walk a UI, control by control, before it is done.** A passing spec
  proves the paths it names. It does not prove that each control shows
  its answer where the planner lands. Run `just qa`. It starts the page
  with hot reload over a server on the C-TRAN feed. The modules run
  under the wasm-fast profile. The server uses `server/dev.toml`'s
  settings (the stub work order system, and photos in memory). Drive it with
  `tools/ui-shot.mjs`: open a selection by its hash, run steps, and
  read the screenshot, the status line, and the page's errors. Its
  browser profile (`.qa/profile`, or `.qa/NAME` with `--profile NAME`
  for a second loop on other ports) keeps the plans that a walk forked.
  A fix is then re-checked in seconds: a page edit needs no build, and
  a Rust edit needs only `just caps-fast` and a reload. Walk every
  control on every stage, on a forked plan, then the paths between
  stages. Pin each control that you fix in the UI's spec.
- **The standing end-to-end test** is `qa/route-99/`
  (`qa/route-99/run.sh`; its README has the steps). It runs one new
  route in the UI on `just qa`, from the stops app through service
  change to ride now and the stop display. It keeps its last run and
  its own findings.
- **Findings are open work, not a log.** A session adds new findings
  (and re-confirmation lines on old ones) to `docs/QA-FINDINGS.md`. The
  change that fixes a finding deletes it and names its pin. The session
  narrative itself is not kept. What the session proved lands as tests,
  and what it found lands as findings.

## Performance baselines — measured, never gated

`just perf` drives a generated UI in Chromium against the real feed
and reports what it costs the browser. It fails only when the UI cannot
be driven, never on a number (timings on a dev box are noisy). Its
specs are `web/perf/<ui>.perf.mjs`. These are ride-now, and the stop
display, left running at a transit center (`PERF_STOP` names it,
`PERF_LOOPS` replays the recording that many times over). Its probe is
`web/perf/probe.mjs` (the DevTools protocol, test-side only). The page
is the shipped page, observed.

- **What it measures, per step:**
  - wall time;
  - main-thread, script, layout and style time;
  - long tasks and long animation frames (their scripts named);
  - two sampled CPU profiles: the page's, and the bus worker's. The bus
    worker is where the capability modules run. The probe attaches to
    it through the browser's DevTools session. Each profile is
    summarized by self time per bucket (a capability module's crate,
    `wasm:transit_router`; a JS bundle file; the engine's GC and
    compile). Each profile is saved as a `.cpuprofile` for DevTools'
    Performance panel.
  - memory, after a forced collection: the page's JS heap and the
    worker's, and the array buffers outside them;
  - per capability module (read through the engine's `window.__bus`):
    its linear memory, the documents it holds, and its resets. The JS
    heap counters do not see linear memory, and it never shrinks;
  - DOM nodes and listeners.
- **The build it measures** is a profiling build. The capability
  modules keep their name sections (`CARGO_PROFILE_RELEASE_STRIP=
  debuginfo`; the code is the shipped code). The frontend is
  unminified (`web/dist-perf`). The next `just caps` restores the
  stripped modules.
- **The live feeds are recorded, then replayed**, one snapshot per
  poll. The browser clock is installed at the recording's first header
  timestamp and fast-forwarded a poll at a time. A 45-minute soak takes
  minutes, and every run sees the same data.
  `node web/perf/record-rt.mjs [count] [dir] [interval]` records the
  feeds that `web/public/live-feeds.json` names into
  `testdata/perf-rt*/` (gitignored, because it is agency data).
  `PERF_RT=testdata/perf-rt-45m just perf` picks one.
- **The soak** leaves the UI open while every remaining snapshot
  arrives. It samples memory every 10 polls. Growth across the samples
  that does not level off is a leak. Chase it with a heap snapshot.
- **Reports** land in `web/perf-results/<stamp>/` (`summary.md`,
  `report.json`, `profiles/`, gitignored). Headless Chromium renders
  the map with software WebGL, so map rendering costs more there than
  on a device with a GPU. Compare runs with each other, not with a
  phone.
