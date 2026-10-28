# The host discipline — the composition layer

Documents-only composition moved every sequenced decision into the
host. The host pays that debt as a designed layer, or it rots into
glue copied per component. Today the resolver
(`web/src/capabilities.svelte.ts`) loads modules and each host
layer (`web/src/actions.ts`, `planner.ts`, `capdemo.svelte.ts`,
`src/uis/<id>/host.svelte.ts`) calls ops directly.

## Scope

- **The choke point.** Exactly one piece of host code calls a
  module op — the ENGINE, the resolver grown an execution half.
  Host layers request flows by name; they never see the ABI, the
  op surface, or an unverified hash.
- **The engine interprets; flow maps describe.** A flow map —
  which documents, which ops, which order, which renderer, which
  DECLARED degrade — is data in the UI manifest
  ([wire-time-assembly.md](wire-time-assembly.md)). The engine
  carries zero domain knowledge: if it names a departure or a
  trip, domain logic leaked. Flow maps validate at load (every op
  exists in the manifest, every edge matches the generated
  contracts, every op node has a degrade path), so a partial
  selection that would blank a screen fails at assembly, not at
  runtime. Every flow request carries a generation; superseded
  generations never land.
- **Verified documents by construction.** Query ops take a branded
  `VerifiedDoc` minted by the engine's load-and-assert step, so
  `assertSameDocument` holds by construction, not discipline.
- **The Merkle journal.** Every session is a log written at the
  choke point: per op, the input document hashes, the module
  bytes' hashes, the output hashes — hashes only. Ephemeral inputs
  (rt snapshots) are marked so replay degrades honestly. One
  journal, three uses: the trace, the answer-cache key, the
  migration witness.
- **Replay is the proof procedure.** The runner replays journals
  natively (same module bytes, same input hashes, compare output
  hashes). Module replacement becomes re-pin + replay against the
  candidate.
- **Registry eviction.** Each module keeps every registered
  document for the session; a long-running host needs eviction
  (QAF-49: the live document is 388 KB per snapshot on C-TRAN,
  re-minted every poll).
- **Two gates.** No op call outside the engine; and determinism in
  the modules — no clocks, randomness, or environment inside a
  cap crate (hosts pass time in; `now_ms` is the only route).

## Order

The journal lands first, in the runner: `caps-run`'s
chained-session format BECOMES the journal format. The engine and
flow maps crystallize from the recurring host glue. The store's
reflog and pointer CAS land with
[sync-persistence.md](sync-persistence.md) — a second persistence
design is rejected on sight.

## Done when

- The two gates are in `just test`, each with an empty allowlist.
- A C-TRAN session journal replays through `caps-run` to the same
  output hashes.
- The UIs request flows through the engine (e2e green), and QAF-49
  is closed by eviction.
