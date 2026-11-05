# The host discipline — the composition layer

Documents-only composition moved every sequenced decision into the
host. The host pays that debt as a designed layer, or it rots into
glue copied per component. The engine (`web/src/engine.ts`) is that
layer's execution half: the one caller of a module's ops, with
documents as `Doc` handles registered and identity-checked per
module, answers cached under the journal's key, and the modules on
the page's one bus worker behind it — gated in `just test` (the
one-op-caller web gates and the DETERMINISM gate;
`docs/ARCHITECTURE.md`, web section). What is left is the
describing half (flow maps), the journal written down, and replay.

## Scope

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
- **The Merkle journal.** Every session is a log written at the
  engine: per op, the input document hashes, the module
  bytes' hashes, the output hashes — hashes only. The module is a
  page's assembly, and the entry names the assembly and the
  capability answering (the engine's answer cache already keys on
  the assembly's hash and a request that names its capability); the assembly-independence law is what lets the runner
  replay a page's journal on single-capability builds. Ephemeral inputs
  (rt snapshots) are marked so replay degrades honestly. One
  journal, three uses: the trace, the answer-cache key (the engine
  already keys its cache this way), the migration witness.
- **Replay is the proof procedure.** The runner replays journals
  natively (same module bytes, same input hashes, compare output
  hashes). Module replacement becomes re-pin + replay against the
  candidate.

## Order

The engine landed first, keyed by the journal's entry, and the
worker behind it. Next the journal and replay in the runner (`caps-run`'s chained-session
format becomes the journal format, closing
[caps-run-sessions.md](caps-run-sessions.md)'s session store); flow
maps as data last, with
[wire-time-assembly.md](wire-time-assembly.md)'s UI manifest.

Flow maps replace the TRANSIENT answer graph in `web/src/uis/kit.ts`
(`answerGraph`) — they do not grow out of it — and the change that
lands them deletes it. A node is a flow-map entry's draft (inputs,
frame, visibility, op calls). The generated-UI units (`ui-*.md`)
each carry an answer-graph table (`docs/ARCHITECTURE.md`,
"Generated UIs", step 5), and flow maps are designed from those
tables. What a table cannot express (a timer-driven live input, for
one) is written here as an engine requirement. The store's log and
pointer CAS live in the Go server (`server/store/`,
`docs/FORMATS.md`) — the per-device journal reuses the same envelope
+ hash-chained log shape, so a device journal replays against the
server store; a second persistence design is rejected on sight.

Engine requirements the tables cannot express:

- **A polled input** ([ui-ride-now.md](ui-ride-now.md)): a timer
  re-reads external documents (the live feeds, the edit document)
  and the clock, and only the inputs whose content hash changed
  re-answer their nodes. Each poll mints an ephemeral document
  (the live snapshot) that the journal marks as such.
- **A failure belongs to its frame** ([ui-service-design.md](ui-service-design.md)):
  a node whose capability did not assemble, or whose op refused,
  fails its frame alone — the frame renders why, with a retry, and
  every other node answers. The transient runner reports errors to
  one handler, so the service-design host wraps each node and keeps
  the failures by frame (`sd.failed`); the engine carries a node's
  failure as that node's answer.

## Done when

- A C-TRAN session journal replays through `caps-run` to the same
  output hashes.
- A device journal, in the store's envelope + log shape, replays
  against a server store (a Go round trip over its log).
- The UIs request flows through the engine (e2e green), and
  `answerGraph` is gone from `web/src/uis/kit.ts`.
