# AGENTS.md — the crates

Two laws govern everything under this directory. If a design
violates either one, the design is wrong — not the law.

## Law 1: the NETWORK is the core data model

GTFS is a serialization spec for external data sharing. Nothing
new goes "in GTFS," and model-side code does not read
serialization.

- GTFS lives only at the boundary: transit-core (the codec
  substrate), transit-codec (import/export), cap-gtfs and
  cap-network (the boundary capabilities). cap-gtfs's charter is
  CLOSED: parse, export, primitive boundary queries — no more.
- **"Network vs feed" is never a decision.** Model-side
  capabilities compose over the NETWORK document (+ its day
  views) — presenting composition over the feed as an option,
  a trade-off, or an open question is the law broken in prose.
  (cap-router read the feed as a transitional, Stage-B-era
  exception; U5 retired it — the router plans over the network
  document and the effective day.) No new feed-composing code,
  and no plan may reopen the question.
- Capabilities compose over the network document, addressed by
  content hash. The module is the hash authority.
- Operator edits, rt joins, analysis, dispatch views: all work in
  MODEL vocabulary (network, service type, pattern schedule,
  walking group, materialized trip). Route/stop ids cross over;
  GTFS types and row shapes do not.

## Law 2: do not reinvent the monolith inside a module

The monolith is being deleted. A capability that re-erects it
with hash-keyed registries is the same bug with new clothes.
Rejected on sight:

- whole-calendar GTFS expansion where a day would do
  (`network_to_gtfs` inside a capability — it is the codec's
  export path, nothing else);
- derived/effective feed caches keyed by date (the monolith's
  `FeedState`/`with_effective` shape);
- GTFS-shaped registries or GTFS row types in responses
  (`transit_core::Departure`, `BlockTrip`, ...) — responses are
  model-native types.

The day view is `transit_network::materialize(net, date)` — the
day's trips, computed per call. Patch the DAY (operator edits
over materialized stop_times), never the calendar.

## Law 3: one NEED per module — personas assemble at wire time

A capability module is ONE need, atomically ("see the departures
at a stop", "watch one vehicle's connections", "plan a trip").
The persona/bundle comes together at WIRE time — the user's
selection over the manifest — never at build time. Do not design
a module around who will use it, and do not merge ops because
the same users end up wiring them together.

Rejected on sight:

- ops serving different needs in one module because one persona
  downloads them together (the rider board behind an operator
  contract — see docs/PLAN.md, the U2 split);
- DUPLICATING an op across modules (two answers to one need on
  the bus — the monolith rebuilt sideways);
- shared DAY MACHINERY is a LIBRARY's job (transit-dispatch), not
  a reason to merge module surfaces.

Registration ops (`load_network`, `load_edits`, `load_feed`) are
composition fabric, not authoring: the host owns every document,
wholesale-replaced. Authoring ops MINT documents; readers have
none by construction.

## Where new things go

The extraction pattern, always in this order:

1. crate boundary first — the domain logic lands in a shared
   library crate (transit-analysis, transit-rt, transit-dispatch)
   with the law tests;
2. capability wrapper second — a thin module over the library;
3. monolith op deletion last — the op dies in transit-ops, and
   the web rewires to the bus.

## Enforcement

The justfile `test` recipe asserts laws 1 and 2: model-side
capabilities may not depend on transit-core/transit-codec, and
`network_to_gtfs` may not appear outside the codec's homes. Law 3
is a review gate (the need table + U2 in docs/PLAN.md is the checklist —
every new module's ops must each name one need). If a gate fails
your change, reread this file — do not widen the gate.
