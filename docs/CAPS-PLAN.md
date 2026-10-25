# Capability bus — the next unit of work

This doc is the plan for the flagged work from the C-TRAN dogfood
session (2026-10-25) and the standing capability-bus queue. It is
written to be picked up cold, in a different session, by a different
agent. Read AGENTS.md first — especially the capability-bus section
and the dogfood-QA expectation. The bus's own state:

- **Done**: cap-abi (ABI macro + manifest contract + compose-time
  validator + `gen-manifest`, which also publishes `registry/`),
  cap-gtfs, cap-router, cap-analysis (the first REAL extraction —
  the monolith's analysis ops are deleted), cap-network (the second —
  the codec: import/validate/export, the monolith's network ops are
  deleted), migrations (watchlist v1→v2), agent context (purpose +
  op docs + worked examples — the network examples are computed from
  the REAL fixture at generation time), the resolver
  (web/src/capabilities.svelte.ts), the registry (`registry/`,
  published by `just registry`), and the agent runner
  (`tools/caps-run.mjs`).
- **The pattern for every extraction** (AGENTS.md): crate boundary
  first, capability wrapper second, monolith op deletion last.
- **The QA feed**: `testdata/ctran-gtfs.zip` — the real C-TRAN
  GTFS. NOT COMMITTED (licensed agency data; fetch from
  c-tran.com). The dogfood session that used it found four real
  defects that every fixture-scale gate missed.

## The working state this unit must reach: ALL work through the caps

QA is done when a planner's whole session runs on capabilities +
`caps-run` ALONE — **no one-off scripts**. The founding dogfood
session violated this three times, and each violation was a
missing piece of this plan:

- the bootstrap scratch script → item 1 (cap-network; the flow
  becomes `caps run gtfs` + `caps run network`);
- hash-copying between sessions → item 3 (result references);
- a Node summarizer to read a 199 KiB answer → item 4 (query
  shaping — right-size at the op).

**The rule for whoever runs these sessions: a one-off script is
never tooling — it is a FINDING.** If a question needs bespoke
glue to ask, that is a missing op (right-shape the response) or a
missing runner feature (route it into this doc). The mirror
principle extends to the web UI in the same way: a question easy
via the CLI but absent from the UI is a UI gap (item 5); a
question hard via the CLI is an op gap. Both signals land here.

## 1. Extract the network codec (cap-network) — DONE (2026-10-25)

Landed per the standard pattern, one change: `transit-codec` (the
moved codec + its fixpoint laws, `codec_prop_test` and
`import_prop_test`), `cap-network` (ops `load_feed`, `import`,
`load_document`, `validate`, `export`), the monolith ops DELETED
(`network_import` / `network_load` / `network_validate` gone from
Request/Response, wasm.ts, the corpus, and the ABI smoke — which now
pins that the retired ops error through the standard outcome), and
transit-core gained the WRITE half of the codec (`src/write.rs`:
deterministic zip bytes, so the export path produces a real GTFS
artifact whose content hash is its identity). The web planner's
bootstrap, validation, and materialize cross the bus
(capabilities.svelte.ts `networkCap`; derive uses the cached feed
bytes, materialize exports → zip → the same LoadFeed a parsed zip
uses) and degrade loudly: `validation off` is a chip state, not a
claim of validity.

Dogfood acceptance met on the real C-TRAN feed, zero monolith:
`caps-run network --load ctran.zip` → import (2.5 MB document,
notes reported) → analysis on the derived document (1,773 trips /
29 routes / 886 stops / 646.5 revenue hours — the founding
session's numbers, reproduced through capabilities alone) → export
(1.1 MB spec-shaped zip, identity asserted when cap-gtfs loads it).
The published artifact is a first-class feed document.

Two findings from that session, both routing into item 3: chaining
import → analysis → export required hand-copying `network_hash`
between `caps-run` invocations and building a request file with the
2.5 MB document embedded — result references
(`$import.network_hash`, `$load.feed_hash`) remove both, and now
that a session's whole flow is capability-side the need is daily,
not occasional.

## 2. Parent stations answer zero departures (cap-gtfs)

**Finding.** `caps run gtfs --op '{departures, stop_id: "106"}'`
(Fisher's Landing Transit Center) returns EMPTY. The bays (9000+)
carry the trips. The model HAS the walking-group concept (transfer
points compose over parents/siblings), but the departures op is
per-stop-id.

**What.** Pool the walking group in cap-gtfs departures (the
parent, siblings sharing a parent, and children — the same group
definition transfer points use), tagged per row with the concrete
stop so callers can see which bay. Decide deliberately: change
`departures` semantics, or add `station_departures` — the planner
UI has the same gap either way.

**Tests.** A fixture with a parent + two bays (extend
`web/e2e/make_fixtures.py`), the property-law style oracle for the
pooling, a dispatch test, and the C-TRAN repro: stop 106 must
return the union of its bays.

## 3. Runner ergonomics: result references (caps-run)

**Finding.** Chained ops need a hash only the PREVIOUS op's output
contains. Every placeholder-hash mistake costs a full document
load (2.5 MB zip, seconds) — twice in one founding session, and
twice again in the cap-network dogfood (import → analysis → export
chains by `network_hash`; see item 1's findings).

**What.** `--op` values support references into prior results:
`{"feed_hash": "$load.feed_hash"}` resolves against the session's
load results (and `$1`, `$2` for prior ops — `$import.network_hash`
is the network flow's everyday shape). Implement in the
worker before invoking. Tests: extend the `just test` runner line
to a chained two-op session that uses `$load.feed_hash`.

## 4. Query shaping on analysis ops (cap-analysis)

**Finding.** `network_stats` on C-TRAN returns 199 KiB — every
route and every stop inline — for a question whose answer is five
numbers. The UI wants the full payload; an agent wants the summary.

**What.** Add optional include flags (`include_routes`,
`include_stops`, defaulting true so the UI is unchanged) or a
`level` enum (`summary | routes | stops | full`). Generate the TS
bindings; document in op_docs; extend the worked examples (the
registry currently has `example: null` for every analysis op —
fill those in while here; the gtfs/router examples proved their
worth in this session).

**Dogfood acceptance.** `caps run analysis --op '{network_stats,
...}'` answers the day-shape question in under a KiB.

## 5. The day-shape question deserves a screen (web)

**Finding (product-level).** The dogfood session answered "what
does a Wednesday look like?" in one op: 1,773 trips, 29 routes,
886 stops, 646.5 revenue hours, span 4:17–25:10. The web planner
has NO screen that answers this — the `network_stats` op was dead
UI surface for its entire life (never called from web/src), while
the CLI found it in one command. The interface now affords
questions the UI doesn't ask.

**What.** A planner "day summary" surface (network overview
header or its own screen): date picker → trips, routes, stops,
revenue hours, span, transfer points. It renders `network_stats`
through cap-analysis like the rest of the planner's numbers — the
degraded state is already handled by the bus.

## Standing queue (after this unit)

- **Registry search** as a derived artifact: embed purpose +
  op_docs + worked examples; ranking carries evidence (laws,
  provenance) so "known good" is part of relevance.
- **Fuel metering** in the runner (wasmtime) once it may point at
  untrusted registry entries; the wall-clock worker guard is the
  stopgap.
- **caps tools → MCP facade** (generated; optional interop).
- **Sync persistence** on blobs + pointers (see the persistence
  discussions: migrate-at-receive, pointer CAS, one writer per
  prefix).
- **cap-authz** as a capability (policy document, server-side
  assembly, fail-closed laws; the validator's scope check).

## Conventions for whoever picks this up

- `just check` is the gate; `just registry` re-publishes; `just
  caps` rebuilds modules + the browser manifest.
- New ops: dispatch test + TS bindings regenerate + registry
  op_docs + smoke assertion (undocumented ops fail the build).
- Real-data QA is mandatory for anything touching feeds or
  payload size: run the C-TRAN session before declaring done.
  Fixture-scale gates passed a 64 KiB output-truncation bug; the
  real feed caught it in one command.
