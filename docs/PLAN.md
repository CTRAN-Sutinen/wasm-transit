# The plan — decompose the monolith into needs

This is THE plan. Status lives in `docs/CAPS-PLAN.md`; this doc
says where the remaining work goes and in what order. The laws
live in `crates/AGENTS.md`. Nothing here restates status — a
unit's state is read from CAPS-PLAN, never from this file.

## The thesis

**A capability is one NEED, atomically. The persona comes together
at WIRE time — the user's selection over the manifest — never at
build time.** Building caps for users bakes bundles into bytes;
building caps for needs lets every user assemble their own.

**GTFS is a serialization spec for external data sharing. The
NETWORK is the core data model.** "Network vs feed" is never a
decision, an option, or an open question — model-side
capabilities compose over the network document and its day
views. Presenting feed composition as a choice is Law 1 broken in
prose.

## The procedure

Mechanical, and it runs on what exists — no design taste
required, only honesty about the need each op serves:

1. **Inventory** every remaining op the monolith answers.
2. **Name the need** — one line, no personas. Two ops serving one
   need belong together; two needs sharing one op is a design
   error; two modules answering one need is the monolith rebuilt
   sideways.
3. **Name the home** — an existing cap, a new cap (one per need),
   the host's view code, or a permanent seam resident.
4. **Extract mechanically** — the pattern from `crates/AGENTS.md`:
   crate boundary first, capability wrapper second, monolith op
   deletion third. A unit is DONE when the monolith ops are
   deleted and `just check` pins it.

## The table — every remaining monolith op, its need, its home

| monolith op(s) | the need | home | unit |
|---|---|---|---|
| stops, routes, route_stop_sequence, summary | pick a stop / route; know what is in the feed | cap-gtfs (exists: `stops`, `stops_by_ids`; add `routes`, `route_stop_sequence`; `load_feed` already returns the summary) | U3 |
| streets_load, street_snap, street_route | author a detour: snap to the street graph, route around | cap-streets — NEW; transit-streets is already the pure library with its own seam registry | U1 |
| coverage_load | measure population coverage | cap-analysis (`load_population_layer` already registers layers by hash) | U4 |
| plan_trip, plan_trip_compare | plan a trip, with the diversion delta | cap-router (exists; its feed-document input is a Stage-B-era shape — U5 moves it onto the NETWORK + the effective day, like every model-side capability) | U5 |
| stops_geojson, stop_point, trip_path_geojson, route_path_geojson, trip_segment_geojson | draw the map | the HOST's view code over model documents — patterns carry paths, stops carry coords, the edit document carries detour polylines, the trip map carries click-throughs. The map is a VIEW, not a cap. | U6 |
| set_diversions | plumbing: apply the edit document to the remaining feed-side reads | none — it dies when U5 + U6 are done (the last consumers) | — |
| sync_edits | sync the edit document (LWW merge, the sync protocol) | transit-sync is the library; the open question is placement only: cap-sync vs permanent seam resident | U7 |
| plans_resolve, plans_validate, plans_periods | know which service plan governs a date | same standing as transit-sync's merge — the U7 placement carries it | U7 |
| load_feed, drop_feed | the artifact's own registry lifecycle | none — the plumbing dies with the last feed_id op (U3 + U6) | — |

## The units

- **U1 cap-streets.** Wrapper over transit-streets; the detour
  editor's snap/route reads the bus; delete the three monolith
  ops. Nothing open.
- **U2 the day/dispatch split.** cap-day: load_network,
  load_edits, departures, station_departures — and the WATCHLIST
  moves here from cap-gtfs (a personalized multi-stop day board
  is the same need parameterized by a personal document).
  cap-dispatch: load_network, load_edits, block_trips with the
  ripple, the edit validation reports, the day's blocks LIST
  (moved from cap-gtfs, model-native) — active_services decides
  with it. Delete cap-gtfs's duplicate feed-side `block_trips`
  (two answers to one need). Both modules over the SAME
  transit-dispatch library.
- **U3 pickers + feed line (Stage C only — no new module).** Add
  the missing boundary queries to cap-gtfs, rewire the UI pickers
  and the feed line, delete the monolith ops.
- **U4 coverage (Stage C only).** The UI registers population
  layers through cap-analysis directly; delete `coverage_load`.
- **U5 planning (mechanical — there is no decision).** Planning
  composes over the NETWORK and the effective day, like every
  model-side capability — the router's feed-document input is a
  Stage-B-era shape, not an option to weigh. RAPTOR moves onto
  materialized model trips over the network (+ the day's edits —
  the same transit-dispatch composition), the wrapper answers by
  network_hash, the rider planner rewires, plan_trip /
  plan_trip_compare die. The diversion delta (compare) falls out
  of the same composition: the baseline is the unpatched day,
  already one materialization apart.
- **U6 the map view (mechanical).** The map renders from model
  documents — the network (patterns, stops), the effective day
  (cap-day/cap-dispatch), the edit document's detour polylines,
  the trip map for click-throughs. Delete the five geojson ops.
  `set_diversions` dies here, with U5.
- **U7 sync + plans (rides sync persistence).** Placement only:
  cap-sync vs permanent seam residents for `sync_edits` and the
  plans chain — both are cross-document logic over transit-sync,
  the same standing as its merge. The sync-persistence design
  (blobs + pointers, migrate-at-receive, pointer CAS) is the
  standing queue's own unit.

**Order:** U1, U2, U3, U4 are independent — land in that order
(U1 and U2 first: they fix the boundary debt before more surface
lands on the modules being split). U5 before U6 (the map's
effective times come from the day; planning's move settles the
document shape). U7 independent.

## The finish line

After U7 no domain op remains in the monolith. transit-ops
reduces to the seam's permanent residents (or nothing),
transit-wasm retires, and the Go server hosts the capability
modules directly. The monolith goes away ENTIRELY — that is the
point of the whole plan.
