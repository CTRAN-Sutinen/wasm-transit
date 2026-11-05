# Module runtime — what is left

The mechanisms are built and pinned: every module keeps its
documents in `cap_abi::registry::Registry` and answers the registry
kernel ops (evict, held); ephemeral kinds bound themselves; the
engine's `keep` names a host's keep-set and its recovery re-registers
a dropped document; one bus worker per page runs the modules off the
render thread, with a memory ceiling that replaces an instance
(docs/ARCHITECTURE.md, "The bus worker", and the bus rules); the
rider pages register the published network from gzipped bytes they
never parse (`servedNetwork`). What remains are three measurements
and two gaps.

## Scope

- **The live document crosses the page.** Each poll, cap-rt answers
  the live document (388 KB on C-TRAN) to the page; the engine
  caches the answer and encodes it back to bytes for the board and
  the router (QAF-49), and the page never reads it. The shape: a
  document a module produced moves to the next module inside the
  worker, and the page holds a handle to it.
- **service-design names no keep-set.** Every `apply` mints a
  network document, and the planner's session keeps every one of
  them in every module (networks are durable). The keep-set is the
  plan set's current networks and the compare baseline; the host
  calls `keep` when the plan set changes.
- **The ceiling's value** (256 MiB per instance, D-70 provisional)
  sits far above one agency's working set. The C-TRAN record
  (`just perf` on `testdata/perf-rt-10m`, 2026-11-04) measures it:
  ~9–12 MB per module that holds the network (the network
  registered from its gzip stream, `just perf` 2026-11-04), ~1 MB
  for one that holds none, flat across 118 polls on the stop
  display — no reset fired. The ceiling lands at a measured multiple of that once
  QAF-93's reduction sets the working set.
- **The cost gate on the worker path**: the ratchet numbers
  ([runtime-cost-gate.md](runtime-cost-gate.md)) are recorded with
  the modules in the worker.

## Done when

- The live document reaches the board and the router without
  crossing to the page and back.
- service-design's session holds only its plan set's networks (an
  e2e applies edits and reads `window.__bus.held`).
- D-70's ceiling is set from the measured working set.
- The cost gate's numbers are recorded on the worker path.
- QAF-49 is deleted in the change that lands the first item.
