# UI: stops and alignments in the service change

Stops are shared by the network, not owned by a route. The
service-change UI edits them in a stage of their own, and a route only
chooses which stops it serves and where its line runs. The Stops stage
reads each stop on its card (`docs/architecture/generated-uis.md`,
service-change), and the Route stage can drop a stop from a direction
or insert an existing one by name; nothing creates, moves,
consolidates, or removes a stop, and no gesture redraws a line.

## Scope, in order

A stop's own record and lifecycle — creating it, moving it, its
details, suspending and retiring it — are the stops page's
(the stops page, `docs/architecture/generated-uis.md`; D-157), never this page's:
a plan only says which stops its service visits.

1. **A station's bays on its card.** The card names a bay's station,
   but a station's card does not list its bays: `stop_detail` answers
   them (the host does not filter the stop list for them).
2. **A moved stop's visits.** When a pick reads a stop's new point
   ([stop-lifecycle.md](stop-lifecycle.md) item 1), one library function
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
   acts on the direction or on one of its patterns: Blocked: D-142.
5. **Redrawing a line** on the Route stage's map: waypoints traced
   along the streets (cap-streets `snap` / `route`, the published
   streets registered as ride-now registers them), the streets to
   confirm shown first (D-26), committed with author `draw_alignment`
   (one pattern or every pattern on the line, D-11), and the patterns it
   touched shown. `draw_alignment` re-measures each stop along the new
   line and warns where a stop is now far from it; the stage offers to
   stop serving it or opens it on the Stops stage. With no streets
   document the line is drawn freehand (D-31). What a longer or shorter
   line does to running time: Blocked: D-143. This is item 4 of
   [streets-model.md](streets-model.md); that item closes with this one.

## Done when

- Each new edit and the re-measuring have their laws in
  transit-network (`crates/transit-network/tests/`), a dispatch test in
  cap-author, and a runner gate.
- `web/e2e/ui-service-change.spec.mjs` covers each item above.
- A dogfood session on C-TRAN has walked every control of the Stops
  stage and the Route map (a moved stop re-measured, one consolidated, one
  served, a line redrawn along the county roads).
