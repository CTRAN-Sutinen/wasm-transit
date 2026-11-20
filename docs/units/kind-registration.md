# A document kind, declared once, and its references checked

A kind is one type of document the agency keeps, such as stops,
alerts, or notes. Adding a kind costs too much. The notes kind is named
in 14 source files outside its tests (2026-11-20). Six of them belong
there: its library and its capability (`transit-notes`, `cap-notes`),
the two crates that read notes (`transit-disruptions`,
`cap-disruptions`), and its page's two files (the disruptions host and
`DisruptionDetail.svelte`). The other eight are plumbing:

- in the kernel: `crates/cap-abi/src/kernel.rs`,
  `src/bin/common/kind_table.rs`, and `src/bin/gen_manifest.rs`;
- in the server: `server/notes.go`, `server/logwrite.go`, and
  `server/main.go`;
- in the runner: `tools/caps-run.mjs`;
- in the engine: `web/src/engine.ts`.

Each log kind has Go of its own, in files of about 130 to 400 lines.
The notes, alerts, and stops logs each repeat five functions: `withX`,
`xLog`, `assembleX`, `xHead`, and `xLogDoc`. The plans log repeats
four of them. The stop requests are written in the stops log, and have
`withRequests` and `requestsHead`. Each kind then has its handler.
`cap-abi` depends on 18 domain crates (five of them optional, for
gen-manifest), so the kernel is a hub that every new kind edits. Runs, the work
rules, and the policy in logs are next (`storage-model.md`), so this
cost limits each of them.

The goal: a new kind touches its declaration, its library, its
capability, and its UI. Nothing else names it.

A record also names other records: a note names an alert, an alert
names a detour, a stop request names a stop. Nothing checks these
names today. The record checks blobs only, through the `content-hash`
marks that `store verify` and `store gc` follow. The declaration and
the receive op are where those checks belong. So this unit also checks
each reference by the rules of the kinds at both ends (D-229). A
reference is declared with its kind, and has a stage and a form
(D-232). The receive op replays the writer's own check as of the seq
the writer read. It refuses a write that was wrong in the world the
writer saw, and warns about a write that the world made wrong later
(D-233).

## Scope

1. **One declaration for each kind.** The declaration states:
   - the kind's tag and version;
   - its library type;
   - its log, or the fact that it is a document with no log;
   - its routes;
   - the capability that validates its writes;
   - the fields that name other records or things, each with its
     target, external targets too (D-232). Each field also states its
     target in its schema (`x-references`), and the kind table carries
     them (`references`). A law in each owning crate lists its
     reference fields and checks that each is declared.

   The declarations are one explicit list in one file. A kind exists
   only if it has a line in that list. Nothing discovers kinds: no
   link-time registration, and no scan of the crates. The kind table
   (`web/pkg/kinds.json`, D-202) is generated from the list. It is
   checked in, so a reviewer sees a new kind in the diff, and a gate
   fails when it differs from the list.
2. **The kernel takes its kinds from the list.** Each kind's load
   lives in its owning library. The kernel reads the list, not one
   hand-written function for each kind. Each line of the list names
   the library function that loads its kind, so a reader goes from
   the list to the code in one hop.

   Whether `cap-abi` stops depending on the domain crates, and an
   assembly links only the libraries of the kinds its capabilities
   read, changes how the kernel and the assemblies are built. A
   proposal goes to the user first, with what it costs the arch
   gates and the module sizes.
3. **One receive op for each kind, and one server path.** The steps of
   a write are one op of the kind's capability (D-228). It takes the
   write's intent and the records the write changes, links
   `transit-log`, and answers the entry and the head it depends on, or
   the refusals. The server makes the conditional create, and on a
   conflict asks again. So every log's write is the same in Go: read
   the head, make one call, create. One route serves every log, and the
   kind's line in the list names its capability. The steps the server
   sequences today become the receive op:
   - `writePlan` (`server/planslog.go`): `freeze`, then `adopt` with
     the frozen document;
   - `checkStops` (`server/stopslog.go`): `attribute_surveys`, then
     `validate`;
   - `runStopsOps`, `runNotesOps`, and `runAlertsOps`: each op, then
     `log_records`.

   The edit document joins last, with its merge inside its receive op.
   The per-kind Go files (`notes.go`, `alerts.go`, `requests.go`, and
   the log parts of `stopslog.go` and `planslog.go`) are deleted. The
   routes and their bytes stay the same, and `server/openapi.json`
   compares them. Each error and log line of the route names its kind.

   The receive op takes the seq the write read for each log it depends
   on. It runs the kind's check as of that seq, under the module the
   client names, and refuses a failure. It runs the check again as of
   now, and warns about a failure (D-233). Check what the outbox
   (D-189) carries today.
4. **One reader in the engine and one loader in caps-run.**
   `served(kind)` replaces `servedStops`, `servedNotes`, and the
   others. caps-run's `--load KIND:FILE` reads the kind table.
5. **Generated lists in the docs.** `docs/FORMATS.md`, `AGENTS.md`,
   `docs/architecture/server.md`, and `storage.md` list the kinds by
   hand today. They point to the kind table and the registry's docs
   site instead.
6. **The line holds.** A gate in `just test` fails when a kind's name
   appears outside its declaration, its library, its capability, a
   capability or library that reads it, its UI, and their tests. A
   capability reads a kind when its `capability! { documents: [...] }`
   names it (cap-disruptions and transit-disruptions read notes). The
   gate refuses only the plumbing: the kernel, the server, the
   runner, and the engine.

Every item follows `docs/CONVENTIONS.md`, "Traceable". Where a kind
differs from the others, the difference is a named field in its line,
never a hook.

## The references

These start when step 3's receive ops exist.

7. **Existence.** cap-log answers each broken loose reference from the
   kind table and the folds: as of a seq, and as of now. `store verify`
   reports them, each with the entry that broke it. The receive op
   answers them as warnings. A revert or a retire answers the
   references that name what it removes.
8. **A rule for each reference.** Each owning module states which
   target states each referrer state needs, with a justification for
   each refusal. Start with stop request to stop. A law checks that
   every state of a target kind is covered by each rule that names the
   kind. A rule over a lifecycle that is a Rust enum is a `match` with
   no wildcard arm, so the compiler holds that law. The stop request's
   `status` is a string: find out why before the first rule, then make
   it an enum or give it a coverage test. The rules for notes and
   alerts land with `ui-disruptions.md`. The network references of
   undated records are Blocked: D-234.
9. **The transitions.** An adoption warns about the live references
   it breaks (D-230). Compaction skips a retired record that a live
   reference names (D-231). A promotion pins each loose reference, or
   is refused (rule C). A note's target pins to `(log, seq, entry hash)`.
10. **Ids across networks.** The stops document refuses an event after
    `retired`. Check whether an imported network can reuse a retired
    stop id, and refuse or flag it if it can.

An attachment that a record names is a gc root. It lands with the
working store (`storage-model.md`, step 2).

## Done when

- The gate runs in `just test`. Every kind passes it, the notes kind
  among them.
- The next new kind (or a test kind added and removed in a Go and Rust
  test) lands touching only the places the gate allows.
- `go test` and the openapi compatibility gate pass with the per-kind
  Go files deleted.
- Each kind's receive op has a dispatch test, and no Go function calls
  more than one op for a write.
- The kind lists in the docs point to the generated sources.
- The laws in each owning crate: the reference declarations, the
  existence check as of a seq, each reference rule and its coverage,
  and the parity law (a write the client passes as of seq s, the server
  passes as of s).
- A Go round trip: a write that fails as of its read seq is refused, a
  write that fails only as of now is accepted with its warning, and
  `store verify` reports the broken reference with its entry.
- The stops page shows the warning on a stop request whose stop was
  retired after the planner read it, in an e2e spec.
- A dogfood session on C-TRAN data: a planner retires a stop that an
  open request names, and the person who wrote the request sees it.
