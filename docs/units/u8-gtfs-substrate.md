# U8 — the GTFS boundary moves into the codec, losslessly

transit-core is the GTFS serialization spec wearing core's name,
and it reads only a subset of the spec. The network is the core
model; GTFS exists to translate it, so the spec's one home is
transit-codec — and that home reads and writes ALL of it:
GTFS → network → GTFS loses nothing in the GTFS Schedule spec,
ids included.

The law is `crates/transit-codec/tests/spec_round_trip_test.rs`:
one proptest per spec file, against an oracle that reads both zips
as raw tables (never through the parser under test). It is red
today (32 of 32 file laws), so `just test` is red until this unit
lands; the failing set is the to-do list. The document fixpoint
(`codec_prop_test.rs`) stays — it catches export-side loss.

## Scope, in order

1. **A full-spec feed in the codec.** Every spec file and column
   parsed (typed where the codec reasons about it, kept verbatim
   where it does not), and a deterministic writer. transit-core's
   parse, calendar, and write fold in here; nothing is ported that
   the codec does not call.
2. **The network models everything the spec describes**, in model
   vocabulary — GTFS is one encoding of it, so edits carry it
   naturally (a pattern stop's pickup rule moves with the stop).
   Folded in, splitting deferred: everything lands in the network
   document first; where to split it (fares, flex, station
   internals as their own documents) is decided afterwards, against
   measured registration cost (`runtime-cost-gate.md`).
   - operators (every agency field; which routes each runs);
   - stops: code, description, URL, spoken name, fare zone,
     accessibility, platform, level, timezone, access, and the kind
     of place (stop, station, entrance, node, boarding area);
     levels and pathways;
   - routes: short and long names, description, URL, text color,
     sort order, continuous stopping, operator;
   - per pattern stop: headsign, pickup / drop-off rules,
     continuous stopping; per trip: accessibility, bikes, cars,
     public trip number; two vehicles running one trip;
   - frequency-based service (`Departures::HeadwayBands`, with
     exact_times);
   - transfers: minimum times, route- and trip-level, in-seat;
   - demand-responsive service: location groups, zones
     (locations.geojson), booking rules, pickup windows;
   - fares v1 and v2: attributes and rules, products, rider
     categories, media, areas, networks, timeframes, leg / join /
     transfer rules;
   - translations; publisher and attribution metadata;
   - the calendar exported as authored (calendar.txt from the rule,
     calendar_dates for its exceptions — not exploded per date).

   **The boundary keeps only serialization**: the id join (agency
   trip, service, and shape ids — the trip-id map extended),
   original stop_sequence numbers, and the feed's approximate times
   at non-timepoint stops. On edit, ids follow their entity,
   sequence numbers are rewritten when a pattern changes, and new
   entities get new ids.
3. **Import and export through it** until every law in
   `spec_round_trip_test.rs` passes. The test's `round_trip` helper
   is the one seam that changes with the new API.
4. **Re-point the dependents** — cap-gtfs, cap-network, transit-rt
   (its joins move onto the network + the id join), transit-router,
   transit-analysis, cap-abi, and every cap crate that names
   transit-core (`grep transit-core crates/*/Cargo.toml`). The dead
   feed-side paths die rather than move: `apply_diversions`,
   `report_all`, and the feed-side day answers (`next_departures`,
   pooling, `blocks` / `blocks_on`, `block_trips`,
   `active_services`) have no bus caller.
5. **Delete transit-core.** The Law 1 arch gate in the justfile
   names the codec alone.

The geometry split (alignments as their own document, for
registration cost — `runtime-cost-gate.md`) must keep shapes
exact: C-TRAN's shapes carry 14–15-digit coordinates, so nothing
rounds at import.

## Done when

- Every law in `spec_round_trip_test.rs` passes and `just check` is
  green.
- `crates/transit-core` is gone; the root AGENTS.md tree,
  `docs/ARCHITECTURE.md`'s layering, and `crates/AGENTS.md` (Law 1's
  boundary list) no longer name it.
- A C-TRAN `caps-run` import → export reproduces the source feed
  under the law's comparison — all 15 files, including the six fare
  files and the shapes' `shape_dist_traveled`.
