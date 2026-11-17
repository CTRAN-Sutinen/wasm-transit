# The registry and the runner

The registry has two roles. It holds the modules that agents and people
run (this section). It also holds the published schema of every document
kind ("Document schemas", below).

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
when the registry was published from another tree. In a session, it says
so once for each registry. `just test` gates the agreement.

The registry is the artifact that agents and humans pull sandboxed
compute from. `tools/caps-run.mjs` verifies blob bytes against the
pinned hash. It runs ops behind a worker-termination wall-clock guard
(`--timeout`).

A session runs its steps in command-line order. It may span
capabilities: `--cap` switches the capability. Each module is verified
and instantiated once. Documents cross between modules by reference
(`"network":"$net.network"`). A host hands the same bytes to each
module the same way. `--as` names results, so references survive
reordering.

A session holds each result as the module's JSON text. It parses one
only when a reference reaches into it. A parsed real-size answer is
millions of objects, and every later collection re-marks them.

A session can last across commands. `--session NAME` saves each result
named with `--as` in the session's store, and a later command with the
same `--session` reaches those results by name. So a later command
loads an imported network from the store instead of importing the feed
again. On C-TRAN, the later command answers a board in about 0.5 s.

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
  itself: when an op answers `unknown <kind> <hash>: <load op> first`,
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
  minted, replay registers it in the asking instance first, as a page's
  linked assembly shares it. Assembly independence makes the answer the
  same (`docs/architecture/bus.md`, "Bus rules"). A document that never
  left a linked assembly cannot cross, and replay says so.
- `evict` and `held` answer about the instance. Replay runs them, and
  compares their answers only on the recorded module. Elsewhere it
  counts them as `unchecked`.
- `--module CAP=FILE` replays the capability against a candidate
  module. To replace a module, replay the journals that use it against
  the candidate.

On C-TRAN, a session that imports the feed and then asks the board,
the directory, and the router journals 9 calls, keeps 3.8 MB of
gzipped requests, and replays in about 5 s.

**The clock.** The runner reads the clock in the agency's zone, never
the machine's (D-175). `$TODAY` is the agency's service date, and
`$NOW_SECS` counts seconds from noon minus 12 hours on that date. Both
come from `web/src/clock.ts`, the browser's own service clock, so the
runner and the pages give the same date at the same instant.

- `--tz ZONE` names the zone.
- Without `--tz`, the zone is that of the last network the session saw:
  its operators' zone in a document an op loads, or the `timezone` that
  an import or a document answer states. A saved session keeps the
  zone.
- With no zone known, `$TODAY` and `$NOW_SECS` fail and name `--tz`.

`--brief` prints each whole document as its kind, version, and size:
`{"omitted": {"kind": "network", "schema_version": 2, "bytes": …}}`.
A document is any object that carries `kind` and `schema_version`
("Every document says what it is", in [bus.md](bus.md)). The session
keeps the whole document, so references still reach it. On C-TRAN, an
import and `service_types_overview` print 3.3 KB with `--brief`, and
9.6 MB without it.

**Discovery.** `caps-run tools <capability>` prints every op with its
full description and worked example. The examples carry whole fixture
documents, so the analysis catalog is about 100 KB.

- `caps-run tools <capability> --brief` prints each op's first sentence
  and its example request, with each document omitted: about 13 KB for
  analysis, and 3 to 8 KB for the others.
- `caps-run tools <capability> <op>` prints one op in full: its
  description and its example request and answer.

Registry records and the browser manifest are both generated by
`cargo run -p cap-abi --features gen-manifest --bin gen-manifest`. One
source gives the contracts, and two surfaces read them.

A capability's agent context is its purpose and each op's description.
It lives in the `OPS.md` beside its crate. It is Markdown that an agent
edits like any doc. The generator reads it. The generator refuses an
`OPS.md` whose op sections are not the capability's ops. The generator
computes the worked examples. Nobody writes them by hand.

An op description is paragraphs and lists. A line that starts with `- `
is a list item, and a line indented by two spaces that starts with `- `
is an item under it. The runner prints the text as written, and the
docs site renders the lists. An op that takes a tagged union of edits
(`apply` in author) gives one item for each edit, with its arguments and
its rule. It does not give one long paragraph.
 `just test`
gates the runner itself. The fixture feed hash is stable, so registry or
fixture drift fails the gate.

## The docs site

`just docs` writes a static site to `docs-site/` (gitignored). It is
generated by `tools/docs-site.mjs`, which reads `registry/` and nothing
else. Run `just caps` first, so the pages match the modules.
`just docs serve [PORT]` builds the site, then serves it on
`http://127.0.0.1:8085/` (or PORT) until you stop it. The server reads
only the site's own `.html` files and listens on loopback only.

- `index.html` lists each capability and each document kind.
- `cap-<id>.html` gives a capability's purpose and, for each op, its
  description and worked example (the same collapsible tree).
- `kernel.html` lists the kernel ops with their codes.
- `doc-<kind>.html` gives a document kind's schema and example. The
  schema is a property tree: each property with its type, a mark when it
  is required, and its description. A type that names a definition links
  to it, in the list below the tree. The example is a collapsible JSON
  tree. The raw JSON of both is in a closed block.

The site has no content of its own. To change a page, edit the
capability's `OPS.md` and run `just caps`. `tools/docs-site.test.mjs`
runs in `just test`. It checks that every capability, op, kernel op, and
document kind has its place, and that no local link is broken.

## Document schemas

Every `just caps` also publishes each document kind's JSON Schema and a
worked example. gen-manifest generates the schema from the kind's serde
type (`schemars`). It computes the example. It writes:

- `registry/schemas/<kind>.v<N>.schema.json`
- `registry/examples/<kind>.v<N>.json`
- the index `registry/documents.json`

The index holds, per kind: its bus `kind`, its `store_kind` (the store
envelope's kind, when the store persists it), its `schema_version`, the
files' paths and hashes, and the registry's evidence.

A reader maps a blob's envelope (`kind`, `schema_version`) to its schema
in that index (`docs/FORMATS.md`). `server/reader_test.go` checks every
example against its schema. It reads a store with nothing but the
bucket, the schemas, and FORMATS.md.

The schemas are generated, not kept. `registry/` is gitignored and every
`just caps` rewrites it. So it holds the version each kind writes today
and no other. Nothing gates a serde change that moves one. No deployment
serves them. Keeping every version, the gate, and serving them are in
`docs/units/document-versioning.md`, item 5.
