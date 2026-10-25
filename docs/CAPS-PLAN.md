# Capability bus — the next unit of work

This doc is the plan for the flagged work from the C-TRAN dogfood
session (2026-10-25) and the standing capability-bus queue. It is
written to be picked up cold, in a different session, by a different
agent. Read AGENTS.md first — especially the capability-bus section
and the dogfood-QA expectation. The bus's own state:

- **Done**: cap-abi (ABI macro + manifest contract + compose-time
  validator + `gen-manifest`, which also publishes `registry/`),
  cap-gtfs, cap-router, cap-analysis (the first REAL extraction —
  the monolith's analysis ops are deleted), migrations (watchlist
  v1→v2), agent context (purpose + op docs + worked examples), the
  resolver (web/src/capabilities.svelte.ts), the registry
  (`registry/`, published by `just registry`), and the agent
  runner (`tools/caps-run.mjs`).
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

## 1. Extract the network codec (cap-network) — the bootstrap gap

**What.** `network_import` / `network_to_gtfs` live in
`transit-ops/src/network.rs`. Extract per the standard pattern:
(a) move the codec to a shared library crate (`transit-codec`?),
keeping transit-ops dispatching through it unchanged; (b) wrap it in
`cap-network` — ops `load_feed` (bytes → feed hash), `import`
(feed hash → network document + notes), `export` (network hash →
GTFS), `validate` (network document → issues); (c) delete the
monolith ops, regenerate bindings, watch the artifact shrink again.
The codec fixpoint laws (`crates/transit-ops/tests/codec_prop_test.rs`)
move with the code — they are the extraction's safety net.

**Why.** The planner's bootstrap ("derive a network from the feed
I loaded") is the last planner flow that still requires the
monolith. Until it's a capability, the planner persona cannot be
assembled without the full artifact.

**Dogfood acceptance.** `caps-run` must do the whole planner flow
with zero monolith involvement: `caps run gtfs --load ctran.zip`,
then `caps run network --op '{import, feed_hash}'`, then analysis
on the derived document.

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
load (2.5 MB zip, seconds) — twice in one session.

**What.** `--op` values support references into prior results:
`{"feed_hash": "$load.feed_hash"}` resolves against the session's
load results (and `$1`, `$2` for prior ops). Implement in the
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
