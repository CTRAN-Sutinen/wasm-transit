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
measurements. (The live document no longer crosses back from the
page; what ride-now still receives is QAF-49.)

## Scope

- **service-design names no keep-set.** Every `apply` mints a
  network document, and the planner's session keeps every one of
  them in every module (networks are durable). The keep-set is the
  plan set's current networks and the compare baseline; the host
  calls `keep` when the plan set changes.
- **The ceiling's value** (256 MiB per instance, D-70 provisional)
  sits far above one agency's working set. An instance is a page's
  assembly: the ceiling is per assembly, and a reset drops every
  capability's state together. The C-TRAN record (`just perf` on
  `testdata/perf-rt-10m`, 2026-11-05) measures the working set:
  16.2 MB for the stop display's assembly and 17.8 MB for
  ride-now's after its session, each holding the network once, flat
  across the display's 118 polls and ride-now's soak — no reset
  fired. The ceiling lands at a measured multiple of that.
- **The cost gate on the worker path**: the ratchet numbers
  ([runtime-cost-gate.md](runtime-cost-gate.md)) are recorded with
  the modules in the worker.

## Done when

- service-design's session holds only its plan set's networks (an
  e2e applies edits and reads `window.__bus.held`).
- D-70's ceiling is set from the measured working set.
- The cost gate's numbers are recorded on the worker path.
