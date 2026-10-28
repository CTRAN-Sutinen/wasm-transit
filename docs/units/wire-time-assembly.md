# Wire-time assembly

The thesis depends on it: the user's selection over the manifest
assembles their capabilities at WIRE time, so capability bytes
differ per user while the documents and contracts stay identical.
Today every page loads its modules on demand from the whole
manifest; there is no user selection, and generated UI layouts
are hand-placed Svelte.

Assemblies are per NEED and reassemble per request. The persona
tabs (rider / dispatcher / planner) in the original app are a
proof-of-concept cut, neither exhaustive nor the right cuts — no
layout, manifest, or registry entry is ever keyed to a persona.

## Scope

- **The user's selection** — declared, persisted (IndexedDB),
  defaulting to the whole manifest, with its store surface in the
  DataSources modal.
- **Probe-based sections** — three states, not one "degraded":
  unassembled, starved (assembled, missing a document), ready.
- **The UI manifest** — host-side, op → render | flow map
  (the flow maps of [host-discipline.md](host-discipline.md));
  referential integrity asserted, coverage as a diagnostic. It is a
  document in the store. It may choose among existing things,
  never describe new behavior.
- **Layouts as documents** — a generated UI's placement (today its
  `Shell.svelte`) becomes a layout document over the thin
  component catalog.
- **The coupled components** — MapView and the entity forms are
  coupled to the original app's `app` state. Generated UIs replace
  them with thin components (FocusMap, ScheduleChange) rather than
  rebuilding them in place.
- **The agent side** — harnesses (assembly / projection / flow /
  budget) are ephemeral, discovered then crystallized, never
  pre-built.

The anchor chain: strategy (the user's selection) → scope (the
manifest) → structure (the document DAG) → skeleton (the UI
manifest) → surface (renderers). Coherence: a human and an agent
ask the same question and get the same bytes, through the same ops
on the same hash-addressed documents through the same manifest.

## Done when

- A user can assemble a subset of the manifest in the browser, and
  a page renders its sections in the three probe states (e2e).
- At least one generated UI renders from a layout document
  instead of a hand-placed shell (e2e + the GENERATED UIs gate).
- Measured, before more modules are split on the thesis: the cost
  of the selection (per-module registration of each document
  version — see [runtime-cost-gate.md](runtime-cost-gate.md))
  against what a smaller assembly saves.
