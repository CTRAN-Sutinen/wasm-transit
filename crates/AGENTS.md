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

## Where new things go

The extraction pattern, always in this order:

1. crate boundary first — the domain logic lands in a shared
   library crate (transit-analysis, transit-rt, transit-dispatch)
   with the law tests;
2. capability wrapper second — a thin module over the library;
3. monolith op deletion last — the op dies in transit-ops, and
   the web rewires to the bus.

## Enforcement

The justfile `test` recipe asserts both laws: model-side
capabilities may not depend on transit-core/transit-codec, and
`network_to_gtfs` may not appear outside the codec's homes. If a
gate fails your change, reread this file — do not widen the gate.
