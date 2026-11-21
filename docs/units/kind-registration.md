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
- in the server: `server/flags.go` (`-notes-wasm`, an older flag kept
  beside the `-modules` folder);
- in the engine: `web/src/engine.ts`, as the capability in its typed op
  map.

Every log the server keeps is written through one server path
(`server/logkinds.go`; `docs/architecture/server.md`, "One path for a
log"): its capability's `receive` op, one op for each write.
`cap-abi` depends on 18 domain crates (five of them optional, for
gen-manifest), so the kernel is a hub that every new kind edits. Runs, the work
rules, and the policy in logs are next (`storage-model.md`), so this
cost limits each of them.

The goal: a new kind touches its declaration, its library, its
capability, and its UI. Nothing else names it.

A record also names other records: a note names an alert, an alert
names a detour, a stop request names a stop. Each kind's schema
declares these fields (`x-references`; `docs/FORMATS.md`,
"References"). The existence check reads them: `store verify`, each
write, and `store revert` name the references whose target record is
gone. What is left is the rules of the kinds at both ends (D-229). A
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

   Each library implements a loading trait in transit-version for its
   own kind, a capability names the kind's type, and the kernel keeps
   one registry for each type (D-239). cap-abi then depends on no
   domain library, and an arch gate holds it. This item follows the
   fixtures crate and the generator of `capability-declared-once.md`
   (items 3 and 4; D-235), which take the five optional domain
   dependencies that gen-manifest gives cap-abi.
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
with kind-registration"). Item 4 is built with
that unit's name gate, and item 3 is one docs sweep with that unit's
item 7.

Every item follows `docs/CONVENTIONS.md`, "Traceable". Where a kind
differs from the others, the difference is a named field in its line,
never a hook.

## The references

6. **A rule for each reference.** Each owning module states which
   target states each referrer state needs, with a justification for
   each refusal, and a law covers every state. The stop request's rule
   for its stop is built (transit-stops `request_rule`, D-238). Its
   pattern holds for the next: a `match` with no wildcard over the
   target's states, a coverage law over a string lifecycle, the date in
   `write.today`, and the reason in `rule`. The rules for notes and
   alerts land with `ui-disruptions.md`.

   A rule is judged as the existence check is (D-233): as of the seqs
   the writer read, a failure refused, and as of now, a failure warned.
   A rule that a page runs too is replayed under the module the client
   names (the build's `produced_by`), from the builds the server served
   within a window the operator sets; a module the server never served
   is refused. No page runs a rule yet.
7. **The transitions.** An adoption warns about the live references
   it breaks (D-230). A promotion pins each loose reference, or
   is refused (rule C). A note's target pins to `(log, seq, entry hash)`.

An attachment that a record names is a gc root. It lands with the
working store (`storage-model.md`, step 2).

## Done when

- The gate runs in `just test`. Every kind passes it, the notes kind
  among them.
- The next new kind (or a test kind added and removed in a Go and Rust
  test) lands touching only the places the gate allows.
- Each log's receive op has a dispatch test (cap-sync, cap-stops,
  cap-plans, cap-notes, and cap-alerts have theirs), and no Go function
  calls more than one op for a write.
- The kind lists in the docs point to the generated sources.
- The laws in each owning crate: the existence check as of a seq, each reference rule and its coverage,
  and the parity law (a write the client passes as of seq s, the server
  passes as of s).
- A Go round trip: a write that fails as of its read seq is refused, a
  write that fails only as of now is accepted with its warning, and
  `store verify` reports the broken reference with its entry.
- A dogfood session on C-TRAN data: a planner retires a stop that an
  open request names, and the person who wrote the request sees it.
