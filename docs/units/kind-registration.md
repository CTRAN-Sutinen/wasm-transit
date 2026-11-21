# A document kind, declared once, and its references checked

A kind is one type of document the agency keeps, such as stops,
alerts, or notes. Each kind has one line in the kind list
(`crates/cap-abi/src/kinds.rs`), and the kind table, the registry's
kernel loads, caps-run's `--load`, and the engine's loads and
`served(kind)` read it. Adding a kind still costs too much. Outside its
library, its capability, the crates that read it, and its page, the
notes kind is named in this plumbing (2026-11-20):

- in the kernel: `crates/cap-abi/src/kernel.rs` (its registry and
  load) and `src/bin/gen_manifest.rs` (its schema's example and its
  load's description);
- in the server: `server/flags.go` (`-notes-wasm`) and `server/main.go`
  (the module paths by capability);
- in the engine: `web/src/engine.ts`, as the capability in its typed op
  map.

Every log the server keeps is written through one server path
(`server/logkinds.go`; `docs/architecture/server.md`, "One path for a
log"): its capability's `receive` op. A pick's checks before the plans
log's write are still sequenced in Go (`adoptNetwork`: cap-network's
check, then cap-stops `read_pick`).
`cap-abi` depends on 18 domain crates (five of them optional, for
gen-manifest), so the kernel is a hub that every new kind edits. Runs, the work
rules, and the policy in logs are next (`storage-model.md`), so this
cost limits each of them.

The goal: a new kind touches its declaration, its library, its
capability, and its UI. Nothing else names it.

A record also names other records: a note names an alert, an alert
names a detour, a stop request names a stop. Each kind's schema
declares these fields (`x-references`; `docs/FORMATS.md`,
"References"), and nothing checks them yet. The record checks blobs only, through the `content-hash`
marks that `store verify` and `store gc` follow. The declaration and
the receive op are where those checks belong. So this unit also checks
each reference by the rules of the kinds at both ends (D-229). A
reference is declared with its kind, and has a stage and a form
(D-232). The receive op replays the writer's own check as of the seq
the writer read. It refuses a write that was wrong in the world the
writer saw, and warns about a write that the world made wrong later
(D-233).

## Scope

1. **The kernel takes its kinds from the list.** Each kind's load
   lives in its owning library. The kernel reads the list, not one
   hand-written function for each kind. Each line of the list names
   the library function that loads its kind, so a reader goes from
   the list to the code in one hop.

   Whether `cap-abi` stops depending on the domain crates, and an
   assembly links only the libraries of the kinds its capabilities
   read, changes how the kernel and the assemblies are built. A
   proposal goes to the user first, with what it costs the arch
   gates and the module sizes. It is one proposal with the fixtures
   crate and the generator of `capability-declared-once.md` (items 3
   and 4; D-235), which take the five optional domain dependencies
   that gen-manifest gives cap-abi. This item follows those items.
2. **The last steps on the one server path.** Every log is on it: a
   `receive` op in the log's capability (`transit_log::receive`
   composes the entry), the log's line `.received(..)`, each kind's own
   route as `.posted(..)`, and the seed from the kind table's `pointers`
   (`seedKept`, the op's `migrate`).
   - A pick's checks: `adoptNetwork` still calls cap-network (the
     network's issues and its canonical bytes) and cap-stops
     (`read_pick`) before the plans log's receive op. Making them one
     op means cap-plans links transit-network's check and
     transit-stops's `read_pick`, which adds to its module's size. A
     proposal goes to the user first. `server/stops.go` (what a pick
     reads of the stops) goes with it.
   - The modules' flags (`-notes-wasm` and the others) and the module
     map in `main.go` name each capability. One setting for every
     capability's module removes them, beside the old flags.

   The routes and their bytes stay the same, and `server/openapi.json`
   compares them. Each error and log line of the route names its kind.

   The receive op takes the seq the write read for each log it depends
   on. It runs the kind's check as of that seq, under the module the
   client names, and refuses a failure. It runs the check again as of
   now, and warns about a failure (D-233). Check what the outbox
   (D-189) carries today.
3. **Generated lists in the docs.** `docs/FORMATS.md`,
   `docs/architecture/server.md`, and `storage.md` list the kinds by
   hand today. They point to the kind table and the registry's docs
   site instead.
4. **The line holds.** A gate in `just test` fails when a kind's name
   appears outside its declaration, its library, its capability, a
   capability or library that reads it, its UI, and their tests. A
   capability reads a kind when its `capability! { documents: [...] }`
   names it (cap-disruptions and transit-disruptions read notes). The
   gate refuses only the plumbing: the kernel, the server, the
   runner, and the engine. The engine's typed op map names each
   capability, and a capability shares its kind's name (`notes`), so
   the map is generated from the manifest's capabilities first.

The order with `capability-declared-once.md` is in that unit ("Order
with kind-registration"). Item 2 comes first. Item 4 is built with
that unit's name gate, and item 3 is one docs sweep with that unit's
item 7.

Every item follows `docs/CONVENTIONS.md`, "Traceable". Where a kind
differs from the others, the difference is a named field in its line,
never a hook.

## The references

These start when every log is on the one path (item 2).

5. **Existence.** cap-log answers each broken loose reference from the
   kind table and the folds: as of a seq, and as of now. `store verify`
   reports them, each with the entry that broke it. The receive op
   answers them as warnings. A revert or a retire answers the
   references that name what it removes, and `store revert` lists the
   references into the commits it drops. A check that cannot judge a
   target in a newer version warns and never refuses. A finding names
   the kind table, the module, and the seq (D-233).
6. **A rule for each reference.** Each owning module states which
   target states each referrer state needs, with a justification for
   each refusal. Start with stop request to stop. A law checks that
   every state of a target kind is covered by each rule that names the
   kind. A rule over a lifecycle that is a Rust enum is a `match` with
   no wildcard arm, so the compiler holds that law. The stop request's
   `status` is a string: find out why before the first rule, then make
   it an enum or give it a coverage test. The rules for notes and
   alerts land with `ui-disruptions.md`. The network references of
   undated records are Blocked: D-234.
7. **The transitions.** An adoption warns about the live references
   it breaks (D-230). Compaction skips a retired record that a live
   reference names (D-231). A promotion pins each loose reference, or
   is refused (rule C). A note's target pins to `(log, seq, entry hash)`.
   The frozen version that the startup migration writes pins its
   source by hash. Check first whether a `derivation` record already
   names it.
8. **Ids across networks.** The stops document refuses an event after
    `retired`. Check whether an imported network can reuse a retired
    stop id, and refuse or flag it if it can.

An attachment that a record names is a gc root. It lands with the
working store (`storage-model.md`, step 2).

## Done when

- The gate runs in `just test`. Every kind passes it, the notes kind
  among them.
- The next new kind (or a test kind added and removed in a Go and Rust
  test) lands touching only the places the gate allows.
- Each log's receive op has a dispatch test (cap-sync, cap-stops,
  cap-plans, cap-notes, and cap-alerts have theirs), and no Go function
  calls more than one op for a write (a pick's checks still do).
- The kind lists in the docs point to the generated sources.
- The laws in each owning crate: the existence check as of a seq, each reference rule and its coverage,
  and the parity law (a write the client passes as of seq s, the server
  passes as of s).
- A Go round trip: a write that fails as of its read seq is refused, a
  write that fails only as of now is accepted with its warning, and
  `store verify` reports the broken reference with its entry.
- The stops page shows the warning on a stop request whose stop was
  retired after the planner read it, in an e2e spec.
- A dogfood session on C-TRAN data: a planner retires a stop that an
  open request names, and the person who wrote the request sees it.
