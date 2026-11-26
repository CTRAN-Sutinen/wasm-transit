# The registry and the runner

The registry holds the modules that agents and people run, and the
published schema of every document kind ("Document schemas", below).

The registry (`registry/`) is published by every `just caps`. `just
registry` runs the gates first. The registry holds immutable blobs, one
evidence record per capability, and a pointer. Each evidence record
holds:

- the contracts and the agent context
- which gates ran (none for a plain `just caps`)
- the commit the build ran at
- the digest of the source tree that built it
- the cargo profile: `release`, or `wasm-fast` from `just caps-fast`

`wasm-fast` is the iteration loop's build. A gated publish refuses it.

`caps-run` recomputes the source digest on every run. It says on stderr
when the registry was published from another tree, once for each
registry in a session. `just test` gates the agreement.

Agents and people pull sandboxed compute from the registry.
`tools/caps-run.mjs` verifies blob bytes against the pinned hash. It
runs ops behind a wall-clock guard that terminates the worker
(`--timeout`).

A session runs its steps in command-line order. It may span
capabilities: `--cap` switches the capability. Each module is verified
and instantiated once. Documents cross between modules by reference
(`"network":"$net.network"`). A host hands the same bytes to each
module the same way. `--as` names results, so references survive
reordering.

A session holds each result as the module's JSON text. It parses one
only when a reference reaches into it, because a parsed real-size
answer is millions of objects that every later collection re-marks.

A session can last across commands. `--session NAME` saves each result
named with `--as` in the session's store, and a later command with the
same `--session` reaches those results by name. So a later command
loads an imported network from the store instead of importing the feed
again.

- The store is `.caps-sessions/<name>/` in the tree (gitignored), or
  the directory that `CAPS_RUN_SESSIONS` names. It holds `index.json`
  and one gzipped outcome for each name, `<name>.json.gz`. It also
  holds the session's journal (below).
- Only `--as` names reach across commands. `$load`, `$ask`, `$post`,
  `$op`, and `$N` address the results of the current command.
- A name in the current command replaces the same name from an earlier
  command. The same name twice in one command is still an error.
- A command saves its named results only when every step succeeds.
- Each command makes new module instances. A stored document registers
  itself. When an op answers `unknown <kind> <hash>: <load op> first`
  and the session holds that document under that hash, the runner loads
  it through the kind's kernel load and runs the op again. The result
  line lists it in `registered`. So `$net.network_hash` is enough, and a
  `load_network` step is not needed.
- `caps-run session` lists the sessions in the store. `caps-run session
  NAME` lists one session's names, each with its op, its capability,
  its documents and their hashes, and the zone it states.

**The journal.** A session records every module call in its journal.
The format and replay are in `tools/journal.mjs`.

- `journal.jsonl` has one line for each call: the run (one command),
  the module instance, the capability, the hash of the module bytes,
  the op, the hash of the request bytes, and the hash of the outcome
  text. A line holds hashes only.
- `blobs/<hex>.gz` holds each request's bytes once, gzipped, by their
  hash. So a feed, an rt snapshot, and a loaded document are all kept,
  and replay can repeat every call.
- A call that a module refuses because it does not hold a document
  (`unknown <kind> <hash>: <load op> first`) is not recorded. The load
  that follows it and the answer are recorded.
- `caps-run replay NAME` runs a session's journal again, in order, on
  fresh instances. `caps-run replay FILE.json` does the same for a
  page's journal ([web.md](web.md), "The journal"). Replay compares
  each output hash with the recorded one, and reports the calls that
  differ and the calls it cannot repeat. It exits 1 when there is one
  of either.
- Each call runs on the module it recorded when the runner has those
  bytes: a capability's registry blob, or a page's linked assembly in
  `web/public/caps`. A linked assembly gets one instance for each
  instance of the journal, and each request is sent as recorded.
  Otherwise the call runs on the capability's own module, with one
  instance for each capability. `--single` makes every call run that
  way, which checks assembly independence on the journal.
- When a call asks for a document that another instance registered or
  created, replay registers it in the asking instance first, as a page's
  linked assembly shares it. Assembly independence makes the answer the
  same (`docs/architecture/bus.md`, "Bus rules"). A document that never
  left a linked assembly cannot cross, and replay says so.
- `evict` and `held` answer about the instance. Replay runs them, and
  compares their answers only on the recorded module. Elsewhere it
  counts them as `unchecked`.
- `--module CAP=FILE` replays the capability against a candidate
  module.
- `--show` lists the calls and runs nothing: each call's place, its
  capability, its op, and its request, shaped as an answer is. A load's
  document prints as its kind and size, a bulk request's other bytes as
  their size, and a page's `cap` field is dropped. So a request that a
  page made can be asked again as an `--op`, which is how QA checks a
  control against caps-run (`docs/TESTING.md`, "Dogfooding and QA").
  `--full` prints each request whole. To replace a module, replay the journals that use it against
  the candidate.

**The clock.** The runner reads the clock in the agency's zone, never
the machine's: a machine set to Tokyo read C-TRAN's Wednesday services on a Tuesday. `$TODAY` is the agency's service date, and
`$NOW_SECS` counts seconds from noon minus 12 hours on that date. Both
come from `web/src/clock.ts`, the browser's own service clock, so the
runner and the pages give the same date at the same instant.

- `--tz ZONE` names the zone.
- Without `--tz`, the zone is that of the last network the session saw:
  its operators' zone in a document an op loads, or the `timezone` that
  an import or a document answer states. A saved session keeps the
  zone.
- With no zone known, `$TODAY` and `$NOW_SECS` fail and name `--tz`.

**The output is shaped for an agent's context.** caps-run is the host
for agents, so an answer prints small unless the agent asks for more.
The session keeps every answer whole, so references still reach it.

- A document prints as its kind, version, and size, with the reference
  that prints it:
  `{"omitted": {"kind": "network", "schema_version": 2, "bytes": …}, "print": "$net.network"}`.
  A document is any object that carries `kind` and `schema_version`
  ("Every document says what it is", in [bus.md](bus.md)). The C-TRAN
  import prints 718 bytes, where its network is 9.6 MB.
- An array of more than 10 items prints its first 3 and
  `{"more": N, "print": REF}`. A map of more than 20 keys prints its
  first 5 and `"…": {"more": N, "print": REF}`. A string of more than
  2000 characters prints its first 200 and the reference.
- `--print REF` (repeatable) prints only the values that the
  references name, one line each, after every step. The named value
  prints whole at its top level, and shaped below it. A step that fails
  still prints its line. `--print '$net.network'` browses the C-TRAN
  network in 17 KB.
- `--full` prints every answer whole.
- A reference's path may hold any key without a `.` (a route alignment
  named `105 NB AM`). `*` maps the rest of the path over each item of
  an array or each value of a map, and a missing value under it is
  null. `{a,b}` keeps those fields of an object. So
  `--print '$b.board.rows.*.{route,departure_secs}'` prints two fields
  of each row, and `"ids":"$s.stops.*.stop_id"` hands an op the ids.
- `--op NAME field=string field:=json …` spells a request as HTTPie
  does, so a request needs no shell-quoted JSON:
  `--op stop_board network_hash='$net.network_hash' stop_id=A date='$TODAY' from_secs:=0 limit:=5`.
- A step that answers an error still prints its line, with a `hint`:
  the op's fields and the command that prints it in full, or what to
  do about a document that no one holds. The command then exits 1.

**Discovery.** Each discovery command prints a line for each item and
names the next command. `--full` prints the registry's JSON.

- `caps-run list` prints a line for each capability (its op count and
  purpose) and the kernel ops.
- `caps-run search <word>…` prints a line for each op whose capability,
  name, or description holds every word, at most 15. A word in the
  op's name ranks it higher than a word in its first sentence.
- `caps-run tools <capability>` prints a line for each op: its name,
  the fields of its example request, and its first sentence. Each is 3
  KB or less.
- `caps-run tools <capability> <op>` prints one op in full: its
  description and its example request and answer, each document
  omitted. An op that takes a tagged vocabulary lists its kinds by
  name, and `caps-run tools <capability> <op> <kind>` prints one kind's
  example.
- `caps-run kind` prints a line for each kind. `caps-run kind <tag>`
  prints one kind's path through the code, from the kind table: each
  route and its Go handler, its log, its load, check, and receive ops
  with the library functions behind them, its read rights, and its
  store layout (`tools/kind-trace.mjs`). `just test` fails when a
  printed handler or op does not exist.

**The upstream.** `--upstream URL|DIR` names the server or an export
folder. caps-run reads them as a page does, on the same paths
(`tools/upstream.mjs`; [hosts.md](hosts.md)).

- The runner reads the heads that the pages read (each UI's `ui.json`,
  `inputs`), once for each command: `manifest` always, and `edits` (the
  server's edit document) and `network` (the server's import) when the
  command names `$edits` or `$network`. `$manifest.<field>`, `$edits`,
  and `$network.<field>` resolve against them. The manifest's
  `timezone` is the clock's zone when `--tz` and the session name none.
- A head that the upstream does not have (404, or no file in an export
  folder) is absent, and a reference to it fails and says so. An export
  has no `edits`. A head whose ETag the working copy holds is asked
  with `If-None-Match`, so an unchanged `network` (9.6 MB on C-TRAN)
  comes back as a 304.
- When an op names a document by a hash that the session does not
  hold, the runner reads `blobs/<hash>.gz`, keeps it, and registers it.
  The line lists it in `registered`, as `<kind> <hash> from the
  upstream`. So
  `--upstream http://127.0.0.1:8080 --op '{"op":"stop_board","network_hash":"$manifest.network_hash",…}'`
  asks about the deployment's network, and no document passes through
  the command.
- The working copy is `.upstreams/<upstream>/` in the session store:
  each head's last answer with the time the upstream gave it
  (`heads/<name>.json`), and each blob by hash.
- A head that the upstream does not answer within 10 s comes from the
  working copy, and so does every head of an export folder that does
  not exist. Each line then carries
  `"stale": {"<head>": {"as_of_ms": …, "why": …}}`, where `as_of_ms` is
  the time of the upstream's last answer. A server answer that carries
  `X-Store-Stale` passes the server's `X-Store-As-Of` down. With no
  copy, the command fails.

**A journal as a source.** `--journal FILE.json|NAME` (repeatable)
names a page's journal ([web.md](web.md), "The journal") or a
session's. When an op names a document by a hash that the session does
not hold, the runner tries the journal's loads of that kind, in order,
until one answers the hash, and then reads the upstream. The line lists
it as `<kind> <hash> from the journal`. So QA asks a page's request
about a forked plan's network, which the server never holds, or about
the rt snapshot the page read.

gen-manifest (`just caps`) generates the registry records and the
browser manifest. One source gives the contracts, and two surfaces read
them.

A capability's agent context is its purpose and each op's description.
It lives in the `OPS.md` beside its crate, as Markdown that an agent
edits like any doc. The generator refuses an `OPS.md` whose op sections
are not the capability's ops. The generator computes the worked
examples, and nobody writes them by hand.

An op description is paragraphs and lists. A line that starts with `- `
is a list item, and a line indented by two spaces that starts with `- `
is an item under it. The runner prints the text as written, and the
docs site renders the lists. An op that takes a tagged union of edits
(`apply` in author) gives one item for each edit, with its arguments and
its rule, not one long paragraph.

`just test` gates the runner itself (`tools/gates/sessions.mjs`). The
fixture feed hash is stable, so registry or fixture drift fails the
gate.

## The docs site

`just docs` writes a static site to `docs-site/` (gitignored). It is
generated by `tools/docs-site.mjs`, which reads `registry/`. The
architecture page also reads the workspace's crates from `cargo
metadata` (no build) and the kind table (`crates/cap-abi/kinds.json`).
Run `just caps` first, so the pages match the modules.
`just docs serve [PORT]` builds the site, then serves it on
`http://127.0.0.1:8085/` (or PORT) until you stop it. The server reads
only the site's own `.html` files and listens on loopback only.

- `index.html` lists each capability and each document kind.
- `architecture.html` draws the architecture from the code. The pages
  use no outside resource, so they work offline.
  - The crate map: every crate in its column, and a line for each
    normal dependency. A crate's column is the longest path from it to
    a crate with no dependency, so every line points left. Its classes
    are the arch gates' classes. Each crate's panel gives its Cargo
    `description`, what it depends on, and what uses it.
  - The bus: a line from each capability to each kind of document it
    reads (its `documents`). The kind table names a load's kind as its
    schema page does. A kind's marker says whether the agency keeps it
    in the bucket, or whether it is an outside format.
  - Two traces, as sequence diagrams to step through: one request (a
    stop display asks for its departures) and one write (a dispatcher
    saves a note). They are the site's only hand-written content, in
    `TRACES` in `tools/docs-site.mjs`. Each step names a file and a
    text in it, and the test fails when either is gone. So when a
    rename breaks a trace, fix the trace in the same change.
  - Where things are kept: a map of the bucket, drawn from the kind
    table, that places each kind in its log, its pointer history, or
    its own keys. Under it, three stories, step by step, of the files
    in the bucket. One saved note shows a log: the entry, the fold, two
    servers racing for one entry, the manifest's blob and pointer
    record, a revert, and a checkpoint. One new feed shows pointer
    histories: the zip's pointer record, the derived network, the
    derivation record, a start that does no work, two servers that make
    the same bytes, and reading every version. One published service
    change shows both: a draft on the planner's device, its network
    frozen in the plan's pointer history, and the plans log entry that
    names it by hash. Each step names one new word, and its code. The
    stories are written by hand like the traces (`BUCKET_STORIES`).
  - What each UI reads, and what it does through an outage: each UI's
    inputs, drawn from its `ui.json` (`inputs`): where each comes from,
    how often it is read, its time limit, and what the page keeps when
    a read fails, beside the rows of the UI's failure table that it
    fails as (`docs/architecture/generated-uis/<id>.md`). Both are read as
    data through `tools/failure-tables.mjs`. The test fails when a
    declared input is not drawn.
  - What each UI links: a table of capabilities and UI assemblies.
- `cap-<id>.html` gives a capability's purpose and, for each op, its
  description and worked example (the same collapsible tree).
- `kernel.html` lists the kernel ops with their codes.
- `doc-<kind>.html` gives a document kind's schema and example. The
  schema is a property tree: each property with its type, a mark when it
  is required, and its description. A type that names a definition links
  to it, in the list below the tree. The example is a collapsible JSON
  tree. The raw JSON of both is in a closed block.

Apart from the two traces and the bucket stories, the site has no
content of its own. To change a page, edit the capability's `OPS.md`, or
a crate's `description` in its `Cargo.toml`, and run `just caps`.
`tools/docs-site.test.mjs` runs in `just test`. It checks that every
capability, op, kernel op, and document kind has its place, and that no
local link is broken. It also checks the architecture page against the
same sources: every crate, dependency, read, and link of a UI is drawn
and marked, every line points left, every kind in the kind table is on
the bucket map, and the code that each trace step and each bucket step
names still exists.

## Document schemas

Every `just caps` also publishes each document kind's JSON Schema and a
worked example, for every version in the kind's schema lock. Before it
writes anything, gen-manifest generates the current schema from the
kind's serde type (`schemars`), computes the example, and compares both
with the lock. Any difference fails the run (`docs/FORMATS.md`, "The
schema lock"). It then copies the locks out:

- `registry/schemas/<tag>.v<N>.schema.json`
- `registry/examples/<tag>.v<N>.json`
- the index `registry/documents.json`
- the kind table beside the server's module, `web/pkg/kinds.json`,
  with each schema it names as `web/pkg/schemas/<hex>.json` (D-202;
  `docs/FORMATS.md`, "The kind table"). It is generated from the kind
  list (`crates/cap-abi/src/kinds.rs`), and the run stops while the
  checked-in copy, `crates/cap-abi/kinds.json`, differs from it. A
  document type whose tag has no line stops the run too.

The index holds, for each kind and version, the document's tag, its bus
`kind`, its `store_kind` (the store envelope's kind, when the store
persists it), whether this build writes it (`written`), its owner crate,
the files' paths and hashes, and the registry's evidence.

A reader maps a blob's envelope (`kind`, `schema_version`) to its schema
in that index (`docs/FORMATS.md`). `server/reader_test.go` checks every
example against its schema. It reads a store with nothing but the
bucket, the schemas, and FORMATS.md.

`registry/` is gitignored, and every `just caps` writes it again from
the locks: each kind's frozen schemas and examples, kept in the crate
that owns it (`crates/<owner>/locks/`). The same files go into every
page build, so every deployment serves them at `/schemas/`
(`docs/FORMATS.md`, "The schema lock").
