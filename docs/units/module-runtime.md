# Module runtime — what is left

The mechanisms are built and pinned: every module keeps its
documents in `cap_abi::registry::Registry` and answers the registry
kernel ops (evict, held); ephemeral kinds bound themselves; the
engine's `keep` names a host's keep-set and its recovery re-registers
a dropped document; one bus worker per page runs the modules off the
render thread, with a memory ceiling that replaces an instance
(docs/ARCHITECTURE.md, "The bus worker", and the bus rules); the
rider pages register the published network from gzipped bytes they
never parse (`servedNetwork`). What remains is one gap and two
measurements. (The live document's round trip through the page,
QAF-49, is [linked-assembly.md](linked-assembly.md)'s.)

## Scope

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
  QAF-93's reduction sets the working set. Under
  [linked-assembly.md](linked-assembly.md) an instance is a page's
  assembly: the ceiling is per assembly, and a reset drops every
  capability's state together.
- **The cost gate on the worker path**: the ratchet numbers
  ([runtime-cost-gate.md](runtime-cost-gate.md)) are recorded with
  the modules in the worker.

## Done when

- service-design's session holds only its plan set's networks (an
  e2e applies edits and reads `window.__bus.held`).
- D-70's ceiling is set from the measured working set.
- The cost gate's numbers are recorded on the worker path.
