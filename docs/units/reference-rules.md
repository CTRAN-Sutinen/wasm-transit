# The rules between records

A record names other records: a note names an alert, an alert names a
detour, a stop request names a stop. Each kind's schema declares these
fields (`x-references`; `docs/FORMATS.md`, "References"). The existence
check reads them: `store verify`, each write, and `store revert` name
the references whose target record is gone. A reference is declared
with its kind, and has a stage and a form (D-232). The receive op
replays the writer's own check as of the seq the writer read. It
refuses a write that was wrong in the world the writer saw, and warns
about a write that the world made wrong later (D-233).

What is left is the rules of the kinds at both ends (D-229), and the
transitions.

## Scope

1. **A rule for each reference.** Each owning module states which
   target states each referrer state needs, with a justification for
   each refusal, and a law covers every state. The stop request's rule
   for its stop is built (transit-stops `request_rule`, D-238). Its
   pattern holds for the next: a `match` with no wildcard over the
   target's states, a coverage law over a string lifecycle, the date
   in `write.today`, and the reason in `rule`. The rules for notes and
   alerts land with `ui-disruptions.md`.

   A rule is judged as the existence check is (D-233): as of the seqs
   the writer read, a failure is refused, and as of now, a failure is
   warned. A rule that a page runs too is replayed under the module the
   client names (the build's `produced_by`), from the builds the server
   served within a window the operator sets. A module the server never
   served is refused. No page runs a rule yet.
2. **The transitions.** An adoption warns about the live references it
   breaks (D-230). A promotion pins each loose reference, or is refused
   (rule C). A note's target pins to `(log, seq, entry hash)`.

An attachment that a record names is a gc root. It lands with the
working store (`storage-model.md`, step 2).

## Done when

- The laws in each owning crate: the existence check as of a seq, each
  reference rule and its coverage, and the parity law (a write the
  client passes as of seq s, the server passes as of s).
- A Go round trip: a write that fails as of its read seq is refused, a
  write that fails only as of now is accepted with its warning, and
  `store verify` reports the broken reference with its entry.
