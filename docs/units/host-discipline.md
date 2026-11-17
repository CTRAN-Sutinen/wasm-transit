# The host discipline: node states in the answer graph

The engine (`web/src/engine.ts`) is the one caller of a module's ops.
The answer graph (`answerGraph` in `web/src/uis/kit.ts`) schedules each
host's nodes. A UI's flows are code in its host (D-176). What is left
is that each frame's failure belongs to the answer graph, not to each
host.

## Scope

- **A failure belongs to its frame.** The answer graph keeps each
  node's state: unassembled (its capability did not assemble), starved
  (a document it reads is missing), failed (its op refused, with a
  retry), or answered. The frame renders that state, and every other
  node answers.
- Today each host does this by hand. service-design, service-change,
  and stops keep `failed[frame]` through a local `framed` wrapper.
  ride-now and the stop display keep `off` flags and their own error
  fields (`boardError`, `liveError`). Each node also repeats the same
  steps: check that its capability assembled, guard on missing
  inputs, and catch its op's error.

## Order

Start after `web-conformance.md` lands its rider-UI items. Both edit
the ride-now and stop-display shells, hosts, and components.

## Done when

- Each UI's frames show the four node states from the answer graph,
  with no per-host failure bookkeeping (e2e green).
- Every frame's states are walked on C-TRAN in each UI.
