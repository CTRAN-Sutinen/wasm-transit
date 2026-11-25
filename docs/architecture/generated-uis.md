# Generated UIs

A generated UI is a layout built for a set of questions, made from thin
components over its own host layer. Each one is a directory. The build,
the picker, and the gates discover it, so a new UI edits nothing
central.

Each generated UI is its own product. It builds alone (`just build-ui
<id>`) and deploys on its own, with the server or, when it needs no
server, as a static site ([README.md](README.md), "Each UI is its own
product"; [static-deployment.md](static-deployment.md)). A new UI adds a
row to that README's table, which says what the UI needs the server
for.

```
web/<id>.html                  the page (copy service-design.html; change the script src)
web/src/uis/<id>/ui.json       the descriptor the picker renders: id (= the directory),
                               title, summary, questions, ops ("<cap>.<op>"; the
                               capabilities they name are what its linked assembly
                               links; the kernel's loads are not ops), components,
                               session (the dogfood session: date, feed, open
                               findings), spec, generated, `offline: true`
                               for a page that installs and works offline
                               ([updates-offline.md](updates-offline.md)), and
                               `precache_assembly: false` for one whose assembly
                               the worker keeps on first use (a staff page, D-223)
web/src/uis/<id>/main.ts       imports ../base.css (+ maplibre css if it maps), mounts
                               Shell, then awaits the host's boot (the layout paints first)
web/src/uis/<id>/Shell.svelte  placement only: reads the host's answers state, passes
                               props, routes events to host functions
web/src/uis/<id>/host.svelte.ts the only file of the UI that asks the engine; owns
                               I/O, the selection, and one $state object of answers
web/e2e/ui-<id>.spec.mjs       the UI's e2e spec (fixture feed: e2e/fixtures/)
```

Shared by every generated UI:

- `src/uis/base.css` holds the color tokens (`--surface --text --muted
  --line --hover --accent --accent-soft --bad --warn --ok`) in light and
  dark, the page base, and the class names the shared components render
  with (`.muted`, `.table-scroll`, `.chip`, `.picker-row`). `.wb-scroll`
  is a table that scrolls in its own box with an opaque sticky header.
- `src/uis/kit.ts` (host layer only):
  - The clock. `today()` and `nowSecs()` read the instant in the
    agency's zone, never the device's, through `src/clock.ts`. The zone
    is the manifest's `timezone`. The host's manifest input sets it with
    `setAgencyZone()` before the page reads its network, and a loaded
    zip's zone is set the same way. `today()` gives the agency's date.
    `nowSecs()` gives the seconds from noon minus 12 h on that date.
    This is the GTFS anchor. It is midnight except on a clock-change
    day, and the fall-back hour before it clamps to 0.
  - `previousDay()` is the same instant on the previous date's clock and
    that date's length. A board takes it as `previous` (the previous
    date's after-midnight trips, and the fall-back hour read truly).
  - The readers an input uses. `readHead()` reads a head that the server
    serves with no hash (`GET edits`), with the server's age.
    `readHeadKept()` does the same and keeps it on the device, for a
    staff page that boots offline. `readLiveFeeds()` reads the live
    feeds that `live-feeds.json` names. `readFeed()` reads one live
    feed, directly or through the dev relay that the config's `proxy`
    names (`/rt`). The engine reads the manifest and the documents
    it pins (`readManifest`, `servedNetwork`, `served`).
  - `askOracle()` asks an oracle that the manifest lists (the geocoder)
    on a person's action, with a time limit.
  - `installApp()`, `buildWaiting()`, and `applyBuild()` are the build
    lifecycle ([updates-offline.md](updates-offline.md)).
  - `clockEvery()` moves the clock while the page is visible, for the
    answers that read the time and the outbox's sends. It reads nothing,
    because a page's reads are its inputs.
  - The inputs (`src/uis/inputs.ts`, through kit's `inputGraph`) read
    each host's documents and feeds. A UI declares its inputs in its
    `ui.json` (`inputs`), as data, so the gates and the docs site read
    them. Each declaration names:
    - `id` and `name`: the reader's id, and the words a label uses
      ("the detours");
    - `from`: where it comes from;
    - `every_s`: how often it is read (null: once, at boot);
    - `limit_s`: the time limit of one read;
    - `keeps`: what the page keeps when a read fails (`last`, or `none`
      for a live feed, because an old prediction is never shown as
      live);
    - `old_after_s`: the age past which the input is old;
    - `feeds`: the answer graph's inputs it changes;
    - `after`: the inputs read before it in the same poll;
    - `fails_as`: the rows of the UI's failure table that say what the
      page does while its reads fail.

    The host gives each input a reader at the top of its file. `read`
    answers the document, null for none, or throws `ReadFailed`, and
    `adopt` writes it into the host's state. The kit reads every input
    with one poll while the page is visible and stops a read at its time
    limit. A read has one of three outcomes: a document, none kept, or
    failed since a time. A failed read waits its interval, or the
    upstream's `Retry-After`, and a random part of the interval, so the
    open pages do not all read again at the same moment. Each input's
    state is in the host's `inputs` record. `inputLabel()` says it in
    the server's words: an answer from a working copy says its age, and
    a read with no answer says since when. A read that answered with
    something the page refused (a module's refusal, not a `ReadFailed`)
    is `refused`, and its label gives the reason. A changed document
    changes the input's `feeds` in one request for each poll.
    `read(...ids)` reads inputs now, after a write moved their head.
    `stop()` ends the reads. A page that applies a new build stops them,
    because the old build's worker hands over only when no request is in
    flight. The property tests are `src/uis/inputs.test.mjs`.
  - The answer graph (`src/uis/answers.ts`, through kit's
    `answerGraph`) runs each host's flows. A UI's flows are code, not
    data. A host declares its answers as `AnswerNode`s: the
    inputs each reads, the frame that renders it, when that frame is on
    screen, the capabilities its ops ask (`caps`), a document it cannot
    answer without (`requires`), the inputs it cannot answer without
    (`inputs`), and what it clears (`clear`). `answerGraph` answers the
    nodes downstream of a changed input in declared order. It yields
    between nodes, so each frame paints as its answer lands. A request
    made mid-pass supersedes the pass. A hidden frame's node waits until
    the frame shows. The graph carries no domain knowledge, and the host
    asks the engine (`src/engine.ts`) for every op that a node runs.
  - The answer graph keeps each frame's state in the host's `frames`
    record. A frame that answered has none. Otherwise the state is one
    of these, and the node does not run in the first three:
    - unassembled: a capability in `caps` did not assemble;
    - starved: `requires` names a missing document;
    - unread: an input in `inputs` fails and the page holds nothing of
      it;
    - failed: the node threw.

    Each state runs the node's `clear`, except a failure of a node that
    keeps its last answer (`keepOnFailure`, the stop display's board).
    The next answer clears the state. `request.retry()` runs each failed
    node again. A frame with several nodes shows the first that has a
    state. A document read (frame `""`) that fails goes to the page's
    handler. `reason(state)` gives the state as a sentence for a prop
    that takes text. The property tests are `src/uis/answers.test.mjs`.
- `src/plans.svelte.ts` holds the service plans (`getAllPlans`,
  `putPlan`, `setActivePlan`, `restorePlans`, `readPlan`), which
  JavaScript owns ([web.md](web.md), "Parity"). A host reads every plan
  it uses through cap-author before it uses the plan. It reads the set
  through cap-network at boot (`restorePlans(readStoredSet)`), and each
  plan then too (`restorePlans(readStoredSet, readStoredNetwork)`) or on
  first use (`readPlan(id, readStoredNetwork)`). The service-design host
  reads on first use, because a workspace of many forks reads only the
  two it opens. JavaScript has no serde aliases, so a stored plan read
  without this step may have old field names. Only the `plan` frame may
  be read before this read, because every stored shape spells it alike.
- `src/basemap.ts` holds the basemap style choice and the MapLibre
  worker wiring for any map view ([web.md](web.md)).

The component catalog. Every component is thin: props in, events out,
formatting only. A component imports neither the bus nor a UI's host
layer (the gates check both). `src/components/` lists them, and each
takes its op's answer as props. These rules are not visible in a
component's code:

- Unavailable renders a frame with no answer. It states what is missing
  and why (an op or decision the frame waits on, or a capability that
  failed) and offers a retry.
- Combobox carries its own styles. Its list floats over the page, and a
  pick gives the focus back, so the page's keys work.
- Service design: GroupLedger shows the peak with the trips that no
  block carries chained in. ContextBar marks its status line when
  it is a refusal, an error, or a change that left issues. A network
  with no plan frame is the "Base network" in PlanList. Readiness shows
  the issues and warnings that network `validate` or cap-author give,
  each as a sentence with clock times (`designFormat.ts`'s
  `issueWords`). Publish posts the plan to `POST /plans` and shows the
  server's answer as given: published, or each refusal. On a block whose
  trip arrives after its next trip leaves, a hold event carries the
  issue's own trip and time. CoveragePanel picks the layer's weight
  field and label from the load's `fields`, and its resolution comes
  from the answer's reading.
- Rider: RideMap opens on the last fix, else the last network frame, and
  frames the network only by `viewport.ts`'s rules. PlaceSearch shows
  the stops from directory `stops` and, under them, the geocoder's
  addresses when the host passes them: two lists in one box (D-28). A
  trip that leaves now shows "Leave in N min" in TripPlan.
- Stop display: DisplayGroup shows a group with no times left as
  detoured, cancelled, or when it runs next. That text is the board's
  `next` ("Next: 5:12 am tomorrow", "Next: Monday 5:12 am"), or says that
  nothing runs this week. A group with times left says when a trip of it
  was cancelled. DisplayAlerts is always on screen.
- Disruptions: DisruptionsMap frames what it draws once, and again when
  its size changes, until the reader moves it. AlertReach joins the
  fields of one entity by AND, and each entity applies on its own. It
  warns where an entity reaches nothing, names a station, names an
  unknown id, or names no single trip instance. AlertPreview opens in a
  dialog from the form's Preview, shows a board over the feed encoded
  with the alert published (rt `encode_alerts`), and downloads that feed
  as a .pb file. AlertEditor emits every edit as an event with the whole
  alert, and shows alerts `put`'s refusals and `flags`' notices in words.

`designFormat.ts`, `disruptionsFormat.ts`, and `rideFormat.ts` hold the
formatting of their sets (clock times, a row's wait, status words and
tones). Each frame whose capability can fail takes an `off` prop. A
capability that did not assemble turns its frame off, and the frame says
so.

The procedure. Every generated UI follows these steps in order, and a
step that is not done blocks the next. A UI's open work is its unit,
`docs/units/ui-<id>.md` (`docs/units/README.md`). Write the unit before
any code, and delete it when the UI lands.

1. **Name the need and its questions.** Open the unit. Write the need
   that the UI serves, named for the need and never a persona (a
   persona is one example of who has the need). The rider, dispatcher,
   and planner cuts are examples, not the right cuts. A persona never
   becomes a type, an enum, a layout, or a branch in code. Write the
   questions that the UI answers, in the order they are asked.
2. **Inventory the ops, from the manifest.** For each question, find
   the op that answers it (`caps-run tools <cap>`), or the open finding
   that says no op does (`docs/QA-FINDINGS.md`). This first draft is a
   list of what to test, not yet a list of what works.
3. **Dogfood the questions on real data** through `caps-run`
   (`docs/TESTING.md`, the dogfood rules), one op per question.
   A question is an op gap when it needs anything besides passing one
   op's answer through to the screen. Put each op gap into
   `docs/QA-FINDINGS.md` as a finding. These needs are op gaps:
   - a host join
   - a threshold
   - a derived number
   - a second op stitched to the first
   - bisecting inputs until an answer changes
   - a rule that the rendering would have to apply

   A session re-confirms the findings it hits. The unit's inventory then
   names, for each question, its op (as a dogfooded fact) or its blocker
   (`QAF-N`, and `Blocked: D-N` when the fix waits on the user).
4. **Build only what is unblocked.** Do not write a host, shell, or
   component before the session in step 3 has run. An op unblocks a
   blocked question (property tests first, library first, the
   capability wrapper second; `crates/AGENTS.md`), and host code never
   does. The UI may land answering only its unblocked questions.
   `ui.json` lists the questions that the UI answers, and the unit keeps
   the blocked ones with their findings.
5. **Draw the answer graph before the layout.** In the unit, write one
   row per node. Each row has:
   - the op (or a run of ops over the same inputs)
   - the inputs it reads (documents, selection keys, the clock, a
     live snapshot, a visibility flag)
   - the frame that renders it
   - when that frame is on screen

   The host's nodes are written from this table. Some questions cannot
   be expressed in it: a timer, a stream, or a node whose inputs depend
   on another node's answer in a way `needs` cannot say. Ask the user
   before you extend the answer graph in `kit.ts` to cover one.

   Then write the failure table below it
   ([README.md](README.md), "Built for the outage"). Write one row for
   each way an input can fail. The inputs are the server, the bucket
   behind it, a live feed, an oracle, sign-in, an adjacent system, and
   the device's network. An input can fail in more than one way: it
   does not answer, it answers slowly, it refuses, or it answers with
   old data as if it were new (a live feed whose buses stop moving).
   Each way gets its own row, with these cells:
   - how the page knows that the input failed: a network error, a
     status code, a header, or a time inside the data. A failure that
     the page cannot detect is a finding.
   - what the page still shows, and how it says how old that is
   - what the page refuses, and how it says so
   - what the person does instead
   - how the page catches up when the input comes back
   - the pin: the test that holds the row

   What the page still shows and what it refuses follow from the answer
   graph. The nodes that need the failed input fail or go stale, and
   every other node keeps answering.

   The agency names what its staff do instead. Ask the user and do not
   invent a procedure. Until the agency names it, the cell is "—". A row
   the page cannot meet yet is open work in the unit, and a row that
   needs an op is a finding. When the UI lands, its table moves into
   `generated-uis/<id>.md`, and the unit keeps the rows that are still
   open.

   Then declare the inputs in `ui.json` (`inputs`; kit's "The inputs",
   above). Each document or feed that the table's nodes read from
   outside the page is one input, with its interval, its time limit,
   what the page keeps when a read fails, and the failure-table rows it
   fails as (`fails_as`). A gate in `just test`
   (`tools/failure-tables.mjs`) fails when an input names a row that the
   table does not have.
6. **Write the host from the table.** `host.svelte.ts` declares the
   table's nodes as `AnswerNode`s. It requests inputs from event
   handlers. The host `ask`s the engine for every op that a node runs,
   over the `Doc`s the host holds. A host may do only these things:
   - read its declared inputs, through a reader for each, and write
     I/O (files, IndexedDB, the outbox). A gate in `just test` finds
     no `fetch(` and no hand-made poll in a host.
   - read the clock and pass it as an op argument
   - hold the selection (including repairing a selection that the
     new answer no longer holds)
   - track which frames are on screen
   - adopt answers as-is

   A host keeps no failure state of its own. A node declares its
   `caps` and `clear`, and the answer graph records why its frame has
   no answer. The shell renders the state: a planner shell through
   `Unavailable` (`state`), a rider shell through each component's
   `off` and `error` props (the host's `off(frame)` and
   `failure(frame)`).

   A host never joins, filters, sorts, counts, thresholds, or falls
   back by a domain rule. A policy value is an op parameter, and the op
   owns its default. The test is whether `caps-run` or the Go server
   would have to re-implement the rule to get the same answer. If so, it
   belongs in an op.
7. **Then the components, then the shell.** Write thin components in
   `src/components/` (props in, events out, formatting only). Then
   write `Shell.svelte` (placement only).
8. **Land it.** Add `ui.json`, the page, and `e2e/ui-<id>.spec.mjs`.
   The host layer names its page (`assemble("<id>")`). `just caps`
   generates its linked assembly from `ui.json`'s `ops`. Run the UI in
   the browser on the real feed (`just serve testdata/ctran-gtfs.zip`).
   Check its numbers against the runner session's, and record the
   session in the descriptor's `session` field. Describe the UI in its
   own file, `generated-uis/<id>.md`: its purpose, the rules its code
   does not show (with their decision ids), and its failure table, with
   no tour of its controls. Add its link to "The UIs today". The file
   opens with the line "`<id>` (`src/uis/<id>/`) …", which the
   failure-table gate reads. Give the UI a short plain-language entry in
   the guide (`docs/guide/apps.md`). Delete the unit if no question is
   left blocked, and name its pins. Otherwise edit the unit down to the
   blocked questions.

Gates (`just test`, generated UIs and the web gates):

- Every `src/uis/<id>/` has its `ui.json` (with fields complete, and id
  equal to the directory), its page, and its spec. The build also
  refuses a UI directory without its page (`vite.config.js`).
- No `.svelte` file imports the bus. No UI imports another UI's files.
  No component imports a UI's host layer.
- Every op that a host asks is in its `ui.json` `ops` list, unless it is
  a kernel op, and every capability a host names has an op there
  (`tools/ui-ops.mjs`). An op missing from the list would turn its frame
  off at runtime with no error.
- An op of a capability that the page fetches alone, only when the
  deployment serves it, is in `fetched_ops` instead, and `just caps`
  does not link it into the UI's assembly. ride-now lists cap-migrate's
  ops there, because it fetches cap-migrate only for a stored watchlist
  older than the board reads.
- The picker's spec (`e2e/picker.spec.mjs`) checks one card per
  `ui.json`.

## The UIs today

Each UI has one file in `generated-uis/`: its purpose, the rules its
code does not show, and its failure table. The failure-table gate reads
them (`tools/failure-tables.mjs`).

- [service-design](generated-uis/service-design.md)
- [service-change](generated-uis/service-change.md)
- [ride-now](generated-uis/ride-now.md)
- [stop-display](generated-uis/stop-display.md)
- [disruptions](generated-uis/disruptions.md)
- [stops](generated-uis/stops.md)
