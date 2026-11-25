# UI: stops and alignments in the service change

Stops are shared by the network, not owned by a route. The
service-change UI edits them in a stage of their own, and a route only
chooses which stops it serves and where its line runs. The Stops stage
reads each stop on its card (`docs/architecture/generated-uis/service-change.md`), and the Route stage can drop a stop from a direction
or insert an existing one by name; nothing creates, moves,
consolidates, or removes a stop, and no gesture redraws a line.

## Scope, in order

A stop's own record and lifecycle — creating it, moving it, its
details, suspending and retiring it — are the stops page's
(the stops page, `docs/architecture/generated-uis/stops.md`), never this page's:
a plan only says which stops its service visits.

1. **A station's bays on its card.** The card names a bay's station,
   but a station's card does not list its bays: `stop_detail` answers
   them (the host does not filter the stop list for them).
2. **A moved stop's visits.** When a pick reads a stop's new point
   (publishing and exporting read each stop's record), one library function
   measures where the stop falls along each pattern's line that visits
   it, re-measures those visits, and warns where the line is now far
   from the stop.
3. **Consolidate and stop serving.** One stop replaced by another on
   every pattern that visits it (a new gesture, `replace_stop {from,
   to}`); a stop withdrawn from every pattern in one change, refused
   where it ends a pattern. Retiring the stop itself is the stops
   app's.
4. **The Route stage's map.** The open direction's line and stops, the
   stops near it faded: serve a nearby stop (a new gesture placing it by
   where it falls along the line — `insert_direction_stop` needs an
   `after` and guesses the distance), stop serving one
   (`remove_direction_stop`), and open a stop on the Stops stage. The
   Route stage never creates, moves, or renames a stop. Whether serving
   acts on the direction by default and on one pattern on request. The
   stage shows each pattern when a direction has more than one (D-142).
5. **Redrawing a line** on the Route stage's map: waypoints traced
   along the streets (cap-streets `snap` / `route`, the published
   streets registered as ride-now registers them), the streets to
   confirm shown first, committed with author `draw_alignment`
   (one pattern or every pattern on the line), and the patterns it
   touched shown. `draw_alignment` re-measures each stop along the new
   line and warns where a stop is now far from it; the stage offers to
   stop serving it or opens it on the Stops stage. With no streets
   document, or a street the data lacks, the line is drawn freehand. A
   streets edit document beside the plan is not built. What a longer or shorter
   line does to running time: nothing. The stage shows the change in
   length, and the planner uses `adjust_running_time` (D-143). The trace, the
   streets to confirm, and the commit are on the bus (cap-streets
   `route`'s `to_confirm`, `crates/transit-streets/tests/bus_profile_law_test.rs`;
   author `draw_alignment`). What is left is the gesture.

## Done when

- Each new edit and the re-measuring have their laws in
  transit-network (`crates/transit-network/tests/`), a dispatch test in
  cap-author, and a runner gate.
- `web/e2e/ui-service-change.spec.mjs` covers each item above.
- A dogfood session on C-TRAN has walked every control of the Stops
  stage and the Route map (a moved stop re-measured, one consolidated, one
  served, a line redrawn along the county roads).
