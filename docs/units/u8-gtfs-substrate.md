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
today (29 of 32 file laws; calendar, calendar_dates, and
frequencies pass), so `just test` is red until this unit lands;
the failing set is the to-do list. The document fixpoint
(`codec_prop_test.rs`) stays — it catches export-side loss.

Landed: the whole-spec `Feed` (step 1's reader and writer), and
the trip model — trips are stored records with the agency's ids
and their own times, frequency trips are trips, blocks name trips,
and `transit_codec::import` / `export` run on the `Feed` (laws:
`trip_law_test.rs`, `trip_import_law_test.rs`, the codec
fixpoint). C-TRAN imports every trip with no notes and no issues,
and its export re-imports to the same document.

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
     continuous stopping, and whether its distance was published
     (a feed without `shape_dist_traveled` must not get one
     invented on export); per trip: headsign, accessibility, bikes,
     cars, public trip number;
   - transfers: minimum times, route- and trip-level, in-seat;
   - demand-responsive service: location groups, zones
     (locations.geojson), booking rules, pickup windows;
   - fares v1 and v2: attributes and rules, products, rider
     categories, media, areas, networks, timeframes, leg / join /
     transfer rules;
   - translations; publisher and attribution metadata.

   **The boundary keeps only serialization** (`transit_codec::
   Boundary`): today the feed's stop_sequence numbers and a
   frequency trip's stop_times base; shape ids join it when
   alignments stop being keyed by pattern. On edit, ids follow
   their entity, sequence numbers are rewritten when a pattern
   changes, and new entities get new ids.
3. **Import and export through it** until every law in
   `spec_round_trip_test.rs` passes. The test's `round_trip` helper
   is the one seam that changes with the new API.
4. **Re-point the dependents** — cap-gtfs, transit-rt (its joins
   move onto the network: an rt trip id IS a model trip id),
   transit-router,
   transit-analysis, cap-abi, and every cap crate that names
   transit-core (`grep transit-core crates/*/Cargo.toml`). The dead
   feed-side paths die rather than move: `apply_diversions`,
   `report_all`, and the feed-side day answers (`next_departures`,
   pooling, `blocks` / `blocks_on`, `block_trips`,
   `active_services`) have no bus caller.
5. **Delete transit-core.** The Law 1 arch gate in the justfile
   names the codec alone.

Follow-ups the trip model left, inside this unit:

- **A trip instance is addressed (pattern trips, start)** on the
  wire — the live document (`LiveTrip`, `LateEntry`,
  `LiveVehicle`), the boards' rows, `trip_stops`, `trip_path`,
  `LiveDay::visit`. Two trips may now share a start, so the address
  becomes (trip, start). Then the trip map (`network.trip_map`,
  `rt.load_trip_map`) is the identity and goes, and a frequency
  trip's rt instance resolves by the update's start_time (the map
  answers its first instance today).
- The trip laws encode answers the intent did not state, entered
  as D-1 – D-5 in `docs/DECISIONS.md` (provisional; nothing here
  waits on them except the geometry split below).
- QAF-64 (a retimed trip reads as removed plus added) is answerable
  now: the timetable comparison can match by trip id.

The geometry split (alignments as their own document, for
registration cost — `runtime-cost-gate.md`) must keep shapes
exact: C-TRAN's shapes carry 14–15-digit coordinates, so nothing
rounds at import. Blocked: D-4 (whether a shape is part of a
pattern's identity decides what the alignments are keyed by).

## Done when

- Every law in `spec_round_trip_test.rs` passes and `just check` is
  green.
- `crates/transit-core` is gone; the root AGENTS.md tree,
  `docs/ARCHITECTURE.md`'s layering, and `crates/AGENTS.md` (Law 1's
  boundary list) no longer name it.
- A C-TRAN `caps-run` import → export reproduces the source feed
  under the law's comparison — all 15 files, including the six fare
  files and the shapes' `shape_dist_traveled`.
- No op addresses a trip instance by (pattern trips, start), and
  the trip map is gone.
