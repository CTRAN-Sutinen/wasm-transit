# Integrity: the references between records

A record names other records: a note names an alert, an alert names a
detour, a stop request names a stop. Nothing checks these names today.
The record checks blobs only, through the `content-hash` marks that
`store verify` and `store gc` follow. This unit checks each reference
by the rules of the kinds at both ends (D-229). A reference is declared
in its kind's schema, and has a stage and a form (D-232). The receive
path replays the writer's own check as of the seq the writer read. It
refuses a write that was wrong in the world the writer saw. It warns
about a write that the world made wrong later (D-233).

## Scope, in order

1. **The declarations.** Each reference field states its target in its
   schema (`x-references`), external targets too. gen-manifest
   collects them into the kind table (`references`). A law in each
   owning crate lists its reference fields and checks that each is
   declared. When `kind-registration.md` lands, the kind's one
   declaration names them.
2. **Existence.** cap-log answers each broken loose reference from the
   kind table and the folds: as of a seq, and as of now. `store verify`
   reports them, each with the entry that broke it. The receive path
   answers them as warnings. A revert or a retire answers the
   references that name what it removes.
3. **The read context.** A write carries the seq it read for each log
   it depends on. Check what the outbox (D-189) carries today. The
   server refuses what fails as of the read seq, under the client's
   module, and warns about what fails only as of now.
4. **A rule for each reference.** Each owning module states which
   target states each referrer state needs, with a justification for
   each refusal. Start with stop request to stop. A law checks that
   every state of a target kind is covered by each rule that names the
   kind. The network references of undated records are Blocked: D-234.
5. **The transitions.** An adoption warns about the live references
   it breaks (D-230). Compaction skips a retired record that a live
   reference names (D-231). A promotion pins each loose reference, or
   is refused (rule C). A note's target pins to `(log, seq, entry hash)`.
6. **Attachments.** An attachment that a record names is a gc root.
   This lands with the working store (`storage-model.md`, step 2).
7. **Ids across networks.** The stops document refuses an event after
   `retired`. Check whether an imported network can reuse a retired
   stop id, and refuse or flag it if it can.

## Done when

- The laws in each owning crate: the declarations, the existence check
  as of a seq, each reference rule and its coverage, and the parity law
  (a write the client passes as of seq s, the server passes as of s).
- A dispatch test for each new op, and the caps-smoke and runner gates.
- A Go round trip: a write that fails as of its read seq is refused, a
  write that fails only as of now is accepted with its warning, and
  `store verify` reports the broken reference with its entry.
- The stops page shows the warning on a stop request whose stop was
  retired after the planner read it, in an e2e spec.
- A dogfood session on C-TRAN data: a planner retires a stop that an
  open request names, and the person who wrote the request sees it.
