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
  to the board and the router — QAF-49).
- **service-design names no keep-set.** Every `apply` mints a
  network document, and the planner's session keeps every one of
  them in every module (networks are durable). The keep-set is the
  plan set's current networks and the compare baseline; the host
  calls `keep` when the plan set changes.
- **A C-TRAN dogfood record**: a dispatcher-length session (hours
  of polls) on the real feed stays bounded — the rt module's
  snapshots, the board's and the router's live documents and
  graphs, and each module's linear memory (the record, not the gate
  — the feed is not committed).
- **The ceiling's value** (256 MiB per instance, D-70 provisional)
  sits far above one agency's working set. The C-TRAN record measures the
  working set per module; the ceiling lands at a measured multiple
  of it, inside D-68's budgets.
- **The cost gate on the worker path**: the ratchet numbers
  ([runtime-cost-gate.md](runtime-cost-gate.md)) are recorded with
  the modules in the worker.

## Done when

- The rider pages boot with no `resp.json()` of a published
  document (an e2e asserts it), and the live document reaches the
  board and the router without crossing to the page and back.
- service-design's session holds only its plan set's networks (an
  e2e applies edits and reads `window.__bus.held`).
- The C-TRAN record is in this unit's deletion change, with the
  ceiling it sets.
- The cost gate's numbers are recorded on the worker path.
- QAF-49 is deleted in the change that lands the first item.
