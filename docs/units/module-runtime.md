# Module runtime — what is left

The mechanisms are built and pinned: every module keeps its
documents in `cap_abi::registry::Registry` and answers the registry
kernel ops (evict, held); ephemeral kinds bound themselves; the
engine's `keep` names a host's keep-set and its recovery re-registers
a dropped document; one bus worker per page runs the modules off the
render thread, with a memory ceiling that replaces an instance
(docs/ARCHITECTURE.md, "The bus worker", and the bus rules). What
remains are three measurements and two gaps.

## Scope

- **The page still parses published documents.** `servedNetwork`
  reads `GET /network` with `resp.json()` (the envelope carries the
  network, its boundary, and the hash), and the engine encodes the
  network back to bytes for each module — on the render thread, and
  copied into the worker per registration. The rider pages need the
  network's name and nothing else of it. The shape: the server serves
  the bare network document (and its boundary) as bytes under the
  ETag it pins, the worker fetches them, and the page holds a handle
  to bytes it never parses. Each poll's live document crosses the
  same way (cap-rt answers it to the page, the engine sends it back
  to the board and the router — QAF-49). Measured on C-TRAN (`just
  perf`, ride-now boot): the page's `Response.json` of `/network`
  is a 110 ms task and the engine's encode back to bytes
  (`Doc.bytes`) another 105 ms — two of boot's three long tasks
  (the third is the map).
- **service-design names no keep-set.** Every `apply` mints a
  network document, and the planner's session keeps every one of
  them in every module (networks are durable). The keep-set is the
  plan set's current networks and the compare baseline; the host
  calls `keep` when the plan set changes.
- **The ceiling's value** (256 MiB per instance, D-70 provisional)
  sits far above one agency's working set. The C-TRAN record
  (`just perf` on `testdata/perf-rt-10m`, 2026-11-04) measures it:
  ~17 MB per module that holds the network, ~1 MB for one that
  holds none, flat across 318 polls on the stop display — no reset
  fired. The ceiling lands at a measured multiple of that once
  QAF-93's reduction sets the working set.
- **The cost gate on the worker path**: the ratchet numbers
  ([runtime-cost-gate.md](runtime-cost-gate.md)) are recorded with
  the modules in the worker.

## Done when

- The rider pages boot with no `resp.json()` of a published
  document (an e2e asserts it), and the live document reaches the
  board and the router without crossing to the page and back.
- service-design's session holds only its plan set's networks (an
  e2e applies edits and reads `window.__bus.held`).
- D-70's ceiling is set from the measured working set.
- The cost gate's numbers are recorded on the worker path.
- QAF-49 is deleted in the change that lands the first item.
