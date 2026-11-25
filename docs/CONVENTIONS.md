# Conventions and gotchas

Git is the user's (see the root AGENTS.md). The frontend's shape and
file ownership are in `docs/architecture/web.md`. This file holds the
gotchas for Rust, wasm, ops, and the frontend. It also holds the
writing rules and the rules that keep code traceable. The
documentation sweep is the `doc-sweep` skill
(`.claude/skills/doc-sweep/SKILL.md`).

- **Keep the `float_roundtrip` feature on serde_json** (the workspace
  dependency in the root Cargo.toml). Without it, the parser is 1 ULP
  off on some f64 values (`-122.7 + 0.005` gives a different value
  each time). A document then does not read back equal to itself. A
  transit-network property test found this.
- **Add a new operation as a library and a capability module.** Put
  the domain logic in a library crate. The module's `Request` and
  `Response` types are the seam. They are serde-tagged, and their JSON
  form is `{"op": ...}`. A host loads the module: the browser loads it
  in its bus worker, and the Go server loads it with `NewWasmHost`. A
  response payload must be a typed serde struct.
- **Use the planner's words in the model, not the words of GTFS.** The
  model uses these terms:
  - A `ServiceType` is the service (the GTFS `service_id`). It is the
    unit that a planner designs and links to days.
  - A route runs in named `Direction`s. A pattern runs in one
    direction or none. The GTFS `direction_id` is only how the codec
    encodes a direction.
  - A `PatternTrips` holds the trips of one pattern under one service
    type.
  - A `Trip` is a stored record. Its id is its identity. For an
    imported trip, the id is the agency's id. A trip has its own
    departure (`Departs`, one start or a frequency) and its own times.
  - A `Network` document is a service plan.

  "Schedule" is not a model word. To a planner, a schedule or a
  timetable is one direction of a route under one service combination,
  across all its patterns. A service combination is the set of service
  types that run together (D-55). The model calls that view the
  derived `Timetable` (directory op `timetable`), so it does not use
  the word for the list of trips in one pattern. A document without
  stored trips is network v1, and cap-migrate brings it forward. Do
  not rename these terms back toward the calendar and service words of
  GTFS.
- **Keep every bug that a scratch test finds as a permanent test.** A
  scratch repro spec lives in /tmp with its own Playwright config,
  never in `web/e2e`. Delete it after use. Before the work is done,
  add each bug that it found to a permanent suite as an assertion.
- **Write native tests first.** A logic test belongs in the crate that
  owns the logic. Each capability module's `tests/dispatch_test.rs`
  runs the real seam without wasm. The library under the module holds
  the law tests. A property test uses an independent oracle (see
  `transit-codec/tests/spec_round_trip_test.rs`). Keep the proptest
  regression seeds in `*.proptest-regressions`.
- **Copy the bytes out of wazero's `memory.Read` before you free the
  buffer.** The call returns a view into module memory. After the free,
  the Rust allocator writes over the bytes.
- **A wasm module is not reentrant.** One instance runs one invoke at a
  time, because its heap and linear memory belong to that instance. The
  Go host holds one lock over the whole alloc-to-free sequence in
  `WasmHost.call`, because net/http serves each request on its own
  goroutine. `just test` runs `go test -race`, and
  `server/race_test.go` gives the race detector concurrent calls to
  check. If you change that lock, make sure that the test still fails
  without it.
- **just 1.59 does not parse `key=value` named arguments.** Recipes use
  positional arguments.
- **The GTFS-rt proto is proto2.** A required field is not an `Option`
  in prost. Nested types live in modules (`gtfs_rt::trip_update::*`).
- **A date can have more than one active service key.** Every
  departure row and block row carries its `service_id`. Do not assume
  that a date has one.
- **Measure before you change `[profile.release]`** in the root
  Cargo.toml. The profile is tuned for registration speed (opt-level 3,
  LTO, panic=abort, strip; D-72). The streamed network decode is
  per-byte code, and opt-level "s" does not inline it. Measure with
  `just perf` and the module sizes.
- **Keep `debug = "line-tables-only"` in `[profile.dev]`.** The test
  profile inherits it. Backtraces then name the file and the line, and
  `target/` stays a fraction of its size with full DWARF. Do not raise
  the shared profile to full debug info. For a local debugging session,
  set `CARGO_PROFILE_DEV_DEBUG=true` in that session only.
- **An op answers through `cap_abi::answer` or `outcome`.** Do not
  build the answer as a `serde_json::json!` tree. The tree is a second
  copy of the answer. A document that the answer carries as its
  canonical bytes (`cap_abi::Canonical`) also stops being those bytes
  inside the tree. Runner gates search an answer's own fields in struct
  field order, and a carried document's keys in canonical order
  (RFC 8785, D-169). A gate reads the whole answer
  (`grep … > /dev/null`). Do not use `grep -q`: it stops at the first
  match and closes the pipe while the runner still writes, and the
  runner then fails with EPIPE.
- **Do not decode a whole document through the tagged `Request`
  envelope.** `#[serde(tag = "op")]` is an internal tag. Serde reads the
  entire input into a generic tree before it knows the variant, and
  then decodes the tree again. On the C-TRAN network (2.7 MB), the peak
  is about 19 MB. A direct parse peaks at 2.3 MB. Wasm linear memory
  does not shrink, so the peak stays as a memory cost for each module
  in each tab. So every document registers through the kernel's loads
  (`cap_abi::kernel`). The bulk op parses the bytes directly into the
  type. The JSON lane removes the envelope and keeps the document as
  raw JSON (`RawValue`). No capability's `Request` carries a document
  to register.

## Writing

These rules apply to code comments and the developer documents. The
developer documents are `AGENTS.md`, `crates/AGENTS.md`, everything
under `docs/` except `docs/guide/`, the capabilities' `OPS.md` files,
and `qa/`. The guide (`docs/guide/`) and the operator guides
(`DEPLOYMENTS.md` and `DEPLOYMENTS-SERVER.md`) follow ISO 24495-1
(plain language).

### What to write

Write no comment by default. A comment earns its place only when it
tells a reader something that the code cannot show them:

- a reason that a maintainer needs to avoid a wrong change;
- a unit, or the range of a value;
- an invariant or a limit that other code depends on;
- an outside constraint: a spec, a host, a user's decision (its id);
- behavior that a caller would not expect from the name and the
  signature.

Read the item's name, its signature, and its body. If a reader can
see what the comment says there, delete the comment. This includes an
edge case that the body handles where a reader can see it. Do not
reword a comment that should not exist.

| Delete | Why | Keep | Why |
|---|---|---|---|
| `/// The stops of the pattern, in order. Stops that the network lacks are skipped.` on a five-line `filter_map` | The body shows both. | `/// Seconds after the service-day anchor. Past 86400 is after midnight.` | A unit and a range that the type `u32` does not show. |
| `//! Errors of the network model` on `error.rs` | The file name and the enum say it. | "The kernel holds each day, so capabilities that read one day share one copy." | A reason. Without it, someone adds a second cache. |
| `/// This capability, as an assembly links it.` on `CAPABILITY` | It restates the name. | "Write files whole. The store has no append (D-113)." | An outside constraint and its decision. |
| "This shipped once: `streets_id` vs `streetsId`." | History. | "A camelCase cast compiles but reads `undefined` at runtime." | A behavior that a reader would not expect. |

- Give each fact one home (AGENTS.md, "Keeping the documents
  correct"). An op's contract lives in its `OPS.md`. A decision's
  reasoning lives in its decision file. Point to the home. Do not
  restate it.
- Do not write history or process. Describe what the code does now. A
  finding id (QAF-n) is history once its finding is closed and gone
  from `docs/QA-FINDINGS.md`. Delete it.

### How to write it

- Give each sentence a verb, and keep the articles "a", "an", and
  "the".
- Use the active voice. Name the thing that does the action.
- Write sentences of 20 words or fewer when you can. A sentence that
  describes may have 25 words.
- Use one name for each concept: the code's name. When the code and a
  document use different names for one concept, fix the document.
- Put a decision number or a finding number at the end of the sentence
  or the paragraph.
- Keep each paragraph to one topic.

### Use these words

Use the plain or industry word, not the repo's own coinage.

| Do not write | Write |
|---|---|
| seam, "the seam over X" | interface, or "cap-X wraps transit-X" |
| JSON lane, bulk lane, the lanes | the JSON entry point (`transit_invoke`), the bytes entry point (`transit_invoke_bytes`) |
| keep-set | keep list (the evict op's `keep` field) |
| held day | cached day |
| mint | create |
| receive path | write path |
| module shell, ABI shell | the four exports |
| hash authority | "the module computes the hash" |
| law, law test (a property that a test checks) | property, property test |
| the three architecture laws | the three architecture rules |

A code name stays until the code is renamed. For example,
`*_law_test.rs` keeps its file name.

### The faults to remove

Each fault below came from this repo. Fix each one as its example
shows.

| Fault | Before | After |
|---|---|---|
| A fragment with no verb | "Four exports, zero imports, every host trivial." | "A module has four exports and no imports, so a host needs little code to call it." |
| A coined term | "the compose-time tier of the proof stack" | "the assembly validator" |
| A label joined to a statement by a colon | "The kernel ops: the same in every module." | "The kernel ops are the same in every module." |
| "Not X" framing that no reader needs | "A capability differs in which ops it answers, never in how it is called." | "Capabilities differ only in the ops that their dispatch answers." |
| A slogan, or parallel fragments for rhythm | "Diagnostics are measurements. Regressions are assertions." | "Keep every bug that a scratch test finds as a permanent test." |
| "X, as Y does it" | "The kind's name, as its tag writes it." | "The tag that a document of this kind carries." |
| A doc comment that restates its item | `/// This capability, as an assembly links it.` on `CAPABILITY` | Delete it. |
| A module comment that says what the file obviously holds | `//! Errors of the network model` on `error.rs` | Delete it. Keep only what the file name and the types do not say. |
| History | "This shipped once: `streets_id` vs `streetsId`." | "A camelCase cast reads `undefined` at runtime." |
| A dash aside, or capitals or italics for emphasis | "the board — NOT the feed — decides" | "The board decides. The feed does not." |
| Two statements joined by a semicolon | "It holds no cache; the kernel does." | "It holds no cache. The kernel holds the days." |
| A question as a title or a lead | "cap-divert: what does this operator edit amount to over the network?" | "cap-divert reports what each operator edit changes in the network." |
| A teaser as a lead | "What riding costs. Two generations of fare description live side by side." | "Fares, in the two forms that agencies publish: zone fares (GTFS Fares v1) and fare products (GTFS Fares v2)." |
| An intensifier | "A record the network cannot answer simply patches nothing." | "A record that the network cannot answer patches nothing." |
| "The one X" | "`wait` is the one rule: the only place a departure time becomes how long." | "Only `wait` turns a departure time into a wait." |
| An arrow chain in prose | "propose → retire → restore walks the states." | "The test proposes, retires, and restores a record." Keep arrows in diagrams. |
| Stacked parentheses | "the stop it leaves from (the bay, on a pooled station board)" | "the stop that it leaves from. On a pooled station board, this is the bay." |

## Traceable

A reader must get from a symptom to the code that caused it with
search and reading alone. The reader must not need to know a framework
to do it. Code that removes repetition must keep each connection
visible.

- **Use explicit lists.** Each kind, input, route, or capability is
  one entry in one list that a person reads. Do not find entries by
  discovery. Do not use link-time registration, a scan of a folder, or
  an entry that exists because a crate is linked.
- **Keep each connection to one hop.** A declaration names the code
  that it drives, and that code names the declaration. A generic path
  names the entry that it acts for in each error and log line
  (`log notes: refused …`).
- **Write generic code only where the cases are the same.** Where an
  entry differs, give it a named field in its declaration. Do not add
  a hook or an override.
- **Make each generated file say where it comes from.** A generated
  file starts with its source and the command that writes it, and it
  tells the reader not to edit it. One command writes it, and a person
  can read the result.
- **Record why a thing happened.** Where code runs because something
  changed, a record names the change. The journal records each module
  call (`docs/architecture/web.md`, "The journal").

### Why: traceable

- **Holds:** the rules above apply to every change that removes
  repetition. This includes declared kinds, declared inputs, the
  answer graph, and the generated files.
- **Because:** a reader can read code that is repeated in many files,
  but the reader cannot know that they found every copy. Code that a
  declaration drives removes the copies. If the reader cannot follow
  the connection, the code is not clearer. Each rule keeps one
  connection open to search.
- **Rejected:** keeping repetition for explicitness. A gate that counts
  the copies can stop new ones, but a reader still cannot see them.

## Frontend

- **Write UI copy as an application does.** Headings, labels, step
  names, and buttons are one to three words of standard UI vocabulary
  ("Location", "Requested by", "Suspend", "Edit details"). Do not
  write conversational phrases ("Where it goes", "Close it for a
  while").
  - Show a status as a short tag ("No alert", "Edited 2:29 PM").
  - Do not put a sentence on the screen that explains how the app
    works or why. Give design reasoning to the reviewer.
  - Put hint text under a field only when a user would otherwise fill
    the field wrong.
  - The plain-language rule (ISO 24495-1) applies to the guide, not to
    the UI. These rules also apply to mockups.
- **Generate every type that crosses the wasm boundary.** `just types`
  deletes the bindings and writes them again. `just build` and
  `just dev` run it, and `just test` refreshes it.
  - ts-rs derives on the model crates write `web/src/types/*.ts`.
    These files are gitignored. Do not commit them or edit them by
    hand.
  - Rust is the source of truth. The manifest's computed op examples
    show the wire shape that the serde attributes produce (tags,
    flatten, optional fields).
  - `.cargo/config.toml` sets `TS_RS_EXPORT_DIR` to `web/src/types`
    for every cargo run, so a plain `cargo test` writes the bindings in
    place. Without that setting, ts-rs writes bindings into
    `crates/*/bindings` (gitignored).
  - `npm run typecheck` checks all of `src/` with svelte-check, and
    `just build` runs it. Components use `lang="ts"`. The rune modules
    are `.svelte.ts` files.
  - TypeScript is pinned to 6.x, because svelte-check does not accept
    TS 7.
- **Read module response fields in snake_case** (the serde default). A
  TS cast with a camelCase field name compiles and type-checks, but it
  reads `undefined` at runtime (for example, `streets_id` read as
  `streetsId`).
- **Persist plain data, not Svelte `$state` proxies.** A proxy cannot
  be structured-cloned. Use `$state.raw` and replace the value whole.
- **Await `arrayBuffer()` before you pass bytes to wasm.** A Promise in
  a `&[u8]` slot decodes as zero bytes, with no error.
- **Extend MapLibre's `LngLatBounds` one point at a time.** `extend`
  reads a bare array of arrays as the `[[west, south], [east, north]]`
  form and ignores the other points. RideMap, FocusMap, and CoverageMap
  each extend point by point.
- **Give a shared component its own styles** in a scoped `<style>`
  block. `base.css` holds the tokens and the few class names that
  components render with. A style in a page's stylesheet goes away with
  that page, and the component then renders without it.
- **Use only widely available web features** (D-135). A feature must
  be Baseline widely available, which means 30 months in every major
  browser. A newer feature waits. Add no polyfill. For this reason, the
  live-feeds panel handles Escape, an outside press, and focus itself.
- **Write to the accessibility gate** (`web/e2e/a11y.spec.mjs`,
  `docs/TESTING.md`). These rules come from what the gate found:
  - Text on an accent fill or a warn fill uses `var(--on-accent)`, not
    `#fff`. White on the dark accent has a contrast of 2.26:1. A route
    badge with an authored route color keeps white (D-184).
  - Draw focus as an `outline`, not as a `box-shadow` alone. Forced
    colors (Windows High Contrast) remove a box-shadow. `base.css`
    gives every `:focus-visible` a ring. A control that sets
    `outline: none` must draw its own.
  - Put every animation and transition inside
    `@media (prefers-reduced-motion: no-preference)`. The loading
    placeholder is `.skeleton span` in `base.css`. A panel sets only its
    height and radius.
  - A view change moves focus. The shell focuses the new view's heading
    (`h1` or `h2`, with `tabindex="-1"`). A polite region
    (`role="status"`, class `sr-only`) announces the status line or the
    size of the answer. A message must also appear somewhere other
    than the status line.
  - The page has one `<main>`, and all content is inside a landmark.
    The map is a labelled region, and the corner controls are a
    `<header>`.
  - A radio group or a tab list takes the arrow keys. Use the `roving`
    action (`components/roving.ts`). The selected item has
    `tabindex="0"`, and the others have `-1`. A tab list names its
    panel (`aria-controls`, `role="tabpanel"`).
  - A combobox option is the `role="option"` element itself, not a
    button inside one. The input names the active option with
    `aria-activedescendant`.
  - A target is at least 24 by 24 CSS pixels. `--muted` text must keep
    a contrast of 4.5:1 on `--hover` and on `--accent-soft`.
- **Give a clipped panel in a column `flex: none`.** A flex child with
  `overflow: hidden` has no minimum height, so a scrolling column can
  shrink it to one line. A spec that checks only the panel's text still
  passes, so check its box.
- **To change an e2e fixture, edit `web/e2e/make_fixtures.py`** and run
  `just fixtures`. `cargo run -p transit-rt --example encode_sample`
  generates the rt protobuf fixture.
- **Ask for a capability the same way each time.** A frame either asks
  whether its capability assembled (`available`), or calls `ask` and
  catches `Unavailable`. Both are memoized. Do not skip the check when
  no manifest has loaded yet. Another frame may have loaded the
  manifest first, and the frame that skipped then assembles nothing.
- **Build before you run Playwright.** Playwright serves the built
  bundle (`web/dist` through the Go server). After a source change,
  `npx playwright test` alone runs the previous build. `just e2e`
  builds first. You can also run `npx vite build`.
- **Playwright e2e runs its own server on :8081** with
  `reuseExistingServer: false`. This keeps the tests hermetic, because
  a shared server carries a live edit document, and test records must
  not land in a dev server. Port 8080 is the default for the dev
  server. Run `ss -tlnp` before you start one.
