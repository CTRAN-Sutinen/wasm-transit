# The host discipline: the journal and replay

The engine (`web/src/engine.ts`) is the one caller of a module's ops.
It holds documents as `Doc`s, caches answers under the journal's key,
and runs the modules on the page's one bus worker. The answer graph
(`answerGraph` in `web/src/uis/kit.ts`) schedules each host's nodes.
A UI's flows are code in its host, and its capabilities link at build
time (D-176). What is left is the journal written down, replay, and two
small additions to the answer graph.

## Scope

- **The journal.** The engine writes a log for each session. For each
  op it records the input document hashes, the hash of the assembly,
  the capability that answers, and the output hashes. It records
  hashes only. It marks ephemeral inputs (rt snapshots), so replay
  says which entries it cannot repeat. The journal format is the
  `caps-run` chained-session format, and the journal uses the runner's
  session store (`--session`; `docs/architecture/registry.md`). Do not
  add a second store.
- **Replay.** `caps-run` replays a journal on single-capability builds
  (the same module bytes and input hashes), and compares the output
  hashes. The assembly-independence law makes this sound. To replace a
  module, pin the candidate again and replay against it.
- **A failure belongs to its frame.** The answer graph keeps each
  node's state: unassembled (its capability did not assemble), starved
  (a document it reads is missing), failed (its op refused, with a
  retry), or answered. The frame renders that state, and every other
  node answers. Today each host does this by hand (`off` flags,
  `sd.failed` in service-design, a `try` in each node).
- **A gate on `ui.json`'s ops.** Every op that a host asks is in its
  UI's `ui.json` `ops` list. A missing op today turns a frame off at
  runtime without an error.

## Done when

- A C-TRAN session journal replays through `caps-run` to the same
  output hashes.
- Each UI's frames show the four node states from the answer graph,
  with no per-host failure bookkeeping (e2e green).
- The `ui.json` ops gate runs in `just test`, with a fixture that
  breaks it.
