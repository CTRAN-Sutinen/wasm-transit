# Testing expectations

Every change lands with tests at the layer that owns the logic.

## Laws first — from the intent, never from the code

Every behavior change starts with the property tests (laws) that
define it. Write them before the implementation and run them red.
This repo has shipped wrong behavior whose property tests were written
to match the implementation, so the tests certified the bug. The rules:

- **Derive each law from the stated intent.** Use the user's directive,
  the GTFS spec, and the architecture laws. Never use what the code
  currently does. A law test's header names what it pins and where
  the intent lives: a GTFS spec section, an `OPS.md` section, or a
  decision id while its entry exists (`docs/DECISIONS.md`, "The
  rules"). Each law is one line on its test, in the user's intent. The
  header does not restate the op contract or the API.
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

`just test` runs a list of commands, each a test file or a gate
script with its broken fixture first: the gates that span the repo in
`tools/gates/` (the runner's sessions, the crate laws), the web's in
`web/gates/`, and each other gate beside the code it guards. A failure
prints `GATE FAIL:` and the command, which names its script.

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
`just store-dev` starts SeaweedFS and fake-gcs-server in
podman. Run `just store-contract` for changes to `server/store/` or an
adapter. `just test` runs the same laws on `mem://`. A managed
provider's bucket runs the same way: `just store-contract URL`, with
the provider's credentials. That is the operator's check before going
live (`DEPLOYMENTS-SERVER.md`, "Prepare the bucket").

**The modules the gates run.** `just test` and `just e2e` run over the
`wasm-fast` modules (`just caps-fast`: release semantics without
whole-program LTO, incremental). A library change then costs seconds to
a minute of module build, not an hour. LTO changes a module's size and
speed, not its answers, so a change passes `just test` (and `just e2e`
when the UI shows it) before you call it done. `just e2e` passes
Playwright's arguments through: one spec (`just e2e
e2e/ui-stops.spec.mjs`) or one test by name (`-g`) runs alone. The
release modules, the bytes the registry publishes, run when a unit is
done: `just test-release`, `just e2e-release`, `just check-release`.
Two more things keep the loop short without testing less:

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

`target/` grows without end. Cargo never deletes an old copy of a
build. A `-p` run unifies features over fewer packages, so it builds
its own copy of a shared dependency and of every crate above it. A
dependency change or a toolchain upgrade adds new copies of
everything. `just test` first checks the size of `target/`. Over 40 GB
(`TARGET_LIMIT_GB`), it runs `cargo clean --workspace`, which deletes
every copy of the workspace's own crates and keeps the third-party
dependencies. That gate then rebuilds the workspace crates. After a
toolchain upgrade, run `cargo clean`: every artifact is stale.

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
     with service is in exactly one service combination.
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
     as, and exports as dates again.
   - `transit-plans`'s chain tests hold the service-plan chain laws:
     resolution, windows, validity, and order-insensitivity.
   - transit-dispatch's `dispatch_laws_test` holds the diversion and
     ripple laws. Its `cancel_law_test` holds the trip-cancel laws.
   - `transit-network`'s `recovery_law_test` holds that a block's wait
     is judged against the recovery required there. The scheduling
     constraints' requirement wins, else the terminal's floor. The wait is never
     judged against its own authored layover.
   - `transit-runs`'s `scheduling_constraints_test` holds the
     scheduling constraints: every rule is optional, the most specific
     recovery rule wins, and order is not identity. Its
     `labor_rules_prop_test` holds the work rules over generated
     documents: order is not identity, a measure or class the document
     lacks is refused, a class replaces or adds rules as it says, and a
     gap is paid by its band. Its `run_judge_prop_test` holds a run
     judged and paid: the timeline has no hole, a measure totals its
     activities, pay is at least each guarantee, premiums raise pay no
     more than their rates, and every broken limit is named. Its
     `cut_prop_test` holds the runs cut: every blocked trip in exactly
     one piece, each run judged as `judge` judges it, and the same
     bytes for the same input. Its `work_rules_test` holds the migration
     chain. `fixtures`' `work_rules_fixture_test` holds C-TRAN's
     2022–2025 rules and their paired readings.
   - `transit-runs`'s `recovery_view_test` holds that a wait is short
     exactly where validation says layover_infeasible.
   - `transit-runs`'s `suggest_law_test` holds that suggested blocking
     covers every unassigned trip once, and legally: one place, an
     allowed pair, the required recovery. It also holds that the
     suggestion adopts into a document that validates clean with the
     peak it named.
2. **A dispatch test if a new op exists** (native, JSON round-trip).
   Add it in the module's `tests/dispatch_test.rs`. Add a flow in
   `crates/cap-abi/caps-smoke.mjs` and a chained session in
   `tools/gates/sessions.mjs`. `just test` runs both. `just test` and `just e2e` build
   `caps` first. The manifest records each op's worked example as the
   module answers it: add the example to the capability's
   `src/examples.rs` (its `examples` feature), the documents to load and
   the request. The generator runs it through the capability's own
   dispatch, so an op's docs cannot drift from the module.
   A new document kind starts as a line in the kind list
   (`crates/cap-abi/src/kinds.rs`), and `just schema-lock` writes the
   checked-in kind table. A kind a module reads also implements
   `transit_version::Load` in its library (`src/load.rs`): its bound
   (`Retain::Host` or `Retain::Last(n)`), its load, and its answer, with
   a worked example in the generator's `kernel_ops`. The generator holds
   the impl to its line.
   A kind whose fields name other records declares them
   (`x-references`), and its crate's `tests/references_law_test.rs`
   lists them (D-232). The kind
   also goes in the capability's `documents: [...]`. caps-smoke holds
   that every module answers the kernel ops (evict, held, about),
   matching its manifest entry. It also holds that each UI's linked
   assembly registers a document once for every capability. Pin a bound
   that matters to a long session in the owning crate's own test binary
   (`tests/registry_test.rs`). The registries are process-global, and
   eviction would race the dispatch tests. Capability tests reach the
   module seam through `invoke` / `invoke_bytes` (the capability alone,
   kernel loads included). They can also register documents directly
   (`cap_abi::kernel::kind::<Network>().register`). Assembly independence
   (`crates/cap-abi/independence.mjs`) re-runs every worked example
   inside each UI's assembly. An op whose answer leans on a derived
   entry that another capability computed fails it. The real-output
   gate (gen-manifest's tests, `generator::checks::real_output`) runs every worked example
   through its capability's `invoke` over the fixtures, and every
   document an answer holds must match its locked schema. A changed
   document shape fails `just caps` until its version is bumped and
   `just schema-lock` freezes it (docs/FORMATS.md, "The schema lock").
3. **A Go round-trip test** if server-visible behavior changes. A new
   or changed route, method, parameter, or body also changes
   `server/openapi.json`, the HTTP contract. The Go test binary checks
   every request a test sends and every response it reads against it,
   and a whole run must reach every operation it names. The change must
   pass the compatibility gate against `server/openapi.lock.json`
   (docs/architecture/release-artifacts.md, "The HTTP contract").
   A change to how the server writes the bucket runs under the fault
   harness (`faultBucket`, `server/resilience_test.go`): a lost answer,
   a copy stopped in the middle of a write, a slow bucket, and two
   copies on one bucket. A new way for a write to fail gets its own
   fault there.
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
     state (`ui-ride-now` and `ui-service-design`). The answer graph's
     laws (`web/src/uis/answers.test.mjs`, in `just test`) pin each
     node's state: unassembled, starved, unread, failed, and answered.
   - The inputs' laws (`web/src/uis/inputs.test.mjs`) pin one poll, a
     time limit on every read, the three outcomes of a read, and the
     spread of reads after a failure. The host contract suite
     (`web/src/uis/host-contract.json`) runs the same scenarios against
     the server (`server/hostcontract_test.go`) and the browser
     (`web/src/uis/host-contract.test.mjs`). A change to the rules in
     `docs/architecture/hosts.md` changes the scenarios first.
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
     the build. With deploy-rollback and offline-staff, these are the
     only specs that allow service workers. Every other spec runs with
     them blocked, because `page.route` never sees what a worker
     answers. The spec pins that both rider pages boot offline, that a
     new build applies at a display's poll and at a rider's next
     navigation, that a staff page applies one only when the person
     accepts, and that a staff page's assembly is kept on first use.
   - `web/e2e/offline-staff.spec.mjs` has its own store-mode server over
     a copy of the build. It takes the browser's network away for an
     outage (`context.setOffline`), and pins each staff page's outbox:
     it boots offline from the worker, a write made offline waits and
     is sent once when the server answers, and a write refused at send
     time is kept and shown with its reasons.
   - `web/e2e/deploy-rollback.spec.mjs` has its own store-mode server
     over another copy. It runs build A, then B, then `POST /builds`
     back to A. The display then runs A's bytes again.
   - `web/e2e/basemap-assets.spec.mjs` pins that the map's glyphs and
     sprites are local when installed and networked when not. It skips
     without a local `basemap.pmtiles`.
   - `web/e2e/journal.spec.mjs` pins the page's journal: the stop
     display's calls replay through `caps-run replay` to the same
     output hashes on the page's own assembly, and `--single` names
     the calls it cannot repeat (`docs/architecture/web.md`, "The
     journal").
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
5. **Web parity** (`web/gates/parity.mjs`, in `just test`). Each rule
   names what web/ may not do: a component imports the engine or the
   bus, host code reads raw rt `stop_updates`, code clones a document
   to mutate it, code outside the engine reaches the bus or a module,
   or a host tests a lifecycle status or fetches by hand
   (`docs/architecture/web.md`, "Parity"). Logic that a view needs
   lands as an op with expectations 1–2, never in TypeScript.
6. **A generated UI** (`src/uis/<id>/`) lands with
   `web/e2e/ui-<id>.spec.mjs` on the fixture feed, which walks its
   questions end to end. A dogfood session of its questions runs
   **before** the UI is built, and their op gaps become findings and
   the UI's blockers. QA runs once it is built
   (`docs/architecture/generated-uis.md`, the procedure; "Dogfooding
   and QA", below). The
   generated UIs gate (`web/gates/uis.mjs`) refuses a UI without its spec.
   The UI ops gate (`tools/ui-ops.mjs`) refuses a host that asks an op
   its `ui.json` does not list. The failure-table gate
   (`tools/failure-tables.mjs`) refuses an input whose `fails_as` names
   a row that the UI's failure table does not have.
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
8. **Dogfood and QA on real data** ("Dogfooding and QA", below). Work
   that a user workflow reads is dogfooded through that workflow on the
   C-TRAN feed, in a `caps-run` session. Work that a UI shows is QA'd: a
   walk of the UI that checks each answer against `caps-run`.
   Fixture-scale gates do not catch real-data defects. Work
   that no workflow reads directly (the store, a log) is held by its
   laws, contract runs, and the e2e of the pages that read it. A change to a document's shape also runs `just soak`, whose
   schema part checks what the C-TRAN feed's import and publish answer
   against the locks.

## Dogfooding and QA

Two activities check the work on real data, and each finds its own
kind of defect.

- **Dogfooding** checks the modules. An agent asks a workflow's
  questions through `caps-run`, the agents' UI, with no browser. It
  finds defects in the domain logic that lives in the modules, and
  questions that no op answers.
- **QA** checks parity between the two UIs. For the same question on
  the same data, the browser UI that a person uses and `caps-run` must
  give the same answer. It finds UI defects, and questions that one UI
  can ask and the other cannot.

Both run on `testdata/ctran-gtfs.zip`, the real C-TRAN GTFS. It is not
committed, because the data is licensed. Fetch it from c-tran.com. The
reference hashes are in `docs/QA-FINDINGS.md`'s header. A session on
the fixture feed is neither.

### Dogfooding — the modules on real data, through caps-run

`caps-run` runs the same capabilities, bytes, and laws that the pages
run. The founding C-TRAN session found four real defects that fixture
gates missed. A 64 KiB output truncation was one of them.

- **A session asks one user workflow's questions.** Capabilities map
  to user workflows, so a capability is dogfooded through the workflow
  that uses it, before that workflow's work is done. It is not required
  because the work is a capability. Import the feed once in a named
  session (`--session`). Later commands in the session reuse the
  import, and name it only by `$net.network_hash`. Or ask a running
  server (`just serve`) with `--upstream http://127.0.0.1:8080` and
  `$manifest.network_hash`. Find an op with `search`, browse a
  capability with `tools <cap>`, and read a part of an answer with
  `--print` (`docs/architecture/registry.md`).
- **An awkward question is an op gap.** A question that needs anything
  besides one op's answer is an op gap: a join, a threshold, a derived
  number, or a second op stitched to the first
  (`docs/architecture/generated-uis.md`, the procedure, step 3).
- **A one-off script is never tooling. It is a finding.** If a session
  needs bespoke glue (a bootstrap script, a summarizer), that is a
  missing op or runner feature. Log it in `docs/QA-FINDINGS.md`. Do not
  ship the script.

### QA — parity between the UI and caps-run

A passing spec proves the paths it names. It does not prove that each
control shows the op's answer where the person lands. QA walks the
page, control by control, and checks each answer against `caps-run`.

- **The walk.** Run `just qa`. It starts the page with hot reload over
  a server on the C-TRAN feed (:8091). The modules run under the
  wasm-fast profile. The server uses `server/dev.toml`'s settings (the
  stub work order system, and photos in memory). Drive the page with
  `tools/ui-shot.mjs`: open a selection by its hash, run steps, and
  read the screenshot, the status line, and the page's errors. Its
  browser profile (`.qa/profile`, or `.qa/NAME` with `--profile NAME`
  for a second loop on other ports) keeps the plans that a walk forked.
  A fix is then re-checked in seconds: a page edit needs no build, and
  a Rust edit needs only `just caps-fast` and a reload. Walk every
  control on every stage, on a forked plan, then the paths between
  stages. The `ui-qa` skill (`.claude/skills/ui-qa/`) runs these steps.
- **Each control is checked against caps-run.** For each answer the
  page shows, ask the op that the page asks, with the same arguments,
  through `caps-run --upstream http://127.0.0.1:8091`. Both read the
  same heads and the same documents. A forked plan's network and the
  live feeds the page read are in the page's journal, not on the
  server: pass the journal too (`--journal FILE.json`). The page's
  requests are listed by `caps-run replay FILE.json --show`. The
  numbers on the screen must match the line. A screen that shows
  something other than the op's answer is a UI defect.
- **A question the UI asks that caps-run cannot is a finding**: the
  page computes an answer that no op gives (`docs/architecture/web.md`,
  "Parity"). A question that caps-run answers and the UI does not show
  is a finding only when the UI's workflow needs it. Many ops serve
  agents or another UI.
- **Pin each control that you fix** in the UI's spec.
- **The standing end-to-end test** is `qa/route-99/`
  (`qa/route-99/run.sh`; its README has the steps). It runs one new
  route in the UI on `just qa`, from the stops app through service
  change to ride now and the stop display. It keeps its last run and
  its own findings.

### Findings

- **What is a finding, and what is not.**
  - A default that hides a failure a rider or an operator would see is
    a finding, not a policy choice. For example, an on-time window that
    counts an early departure as on time hides a missed bus.
  - Two sources that disagree, each shown as given, are not a defect.
    An alert that closes a stop beside trip updates that still predict
    departures there is the system showing its inputs. A finding never
    asks the code to pick a winner between sources. At most, it is a
    question for the user.
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
