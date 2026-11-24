# A document kind, declared once, and its references checked

A kind is one type of document the agency keeps, such as stops,
alerts, or notes. Each kind has one line in the kind list
(`crates/cap-abi/src/kinds.rs`), and the kind table, the registry's
kernel loads, caps-run's `--load`, and the engine's loads and
`served(kind)` read it. The kernel names no kind: each library loads
its own kind (`transit_version::Load`, D-239), and cap-abi depends on
transit-version alone. The generator names each kind's type for its
schema's example and its load's description. Outside its library, its
capability, the crates that read it, the generator, and its page, the
notes kind is still named in this plumbing:

- in the server: `server/flags.go` and `server/config.go`
  (`-notes-wasm`, an older flag kept beside the `-modules` folder);
- in the engine: `web/src/engine.ts`, as the capability in its typed op
  map.

Every log the server keeps is written through one server path
(`server/logkinds.go`; `docs/architecture/server.md`, "One path for a
log"): its capability's `receive` op, one op for each write. Runs, the
work rules, and the policy in logs are next (`storage-model.md`).

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

3. **Generated lists in the docs.** `docs/FORMATS.md`,
   `docs/architecture/server.md`, and `storage.md` list the kinds by
   hand today. They point to the kind table and the registry's docs
   site instead.
4. **The line holds.** Blocked: D-244 (what the gate matches, and the
   module flags). A gate in `just test` fails when a kind's name
   appears outside its declaration, its library, its capability, a
   capability or library that reads it, its UI, and their tests. A
   capability reads a kind when its `capability! { documents: [...] }`
   names it (cap-disruptions and transit-disruptions read notes). The
   gate refuses only the plumbing: the kernel, the server, the
   runner, and the engine. The server's per-capability module flags
   (`-notes-wasm` and the others in `server/flags.go`) give way to the
   `-modules` folder, and the stop requests' work order path
   (`server/workorders.go`) is read from the kind table, or the gate
   names them. The engine's typed op map names each
   capability, and a capability shares its kind's name (`notes`), so
   the map is generated from the manifest's capabilities first.

Item 4 is built with `capability-declared-once.md`'s name gate (its
item 6), and item 3 is one docs sweep with that unit's item 7.

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
