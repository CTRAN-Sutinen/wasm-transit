# UI: stops and alignments in the service change

Stops are shared by the network, not owned by a route. The
service-change UI edits them in a stage of their own, and a route only
chooses which stops it serves and where its line runs. The Stops stage
reads each stop on its card (`docs/architecture/generated-uis.md`,
service-change), and the Route stage can drop a stop from a direction
or insert an existing one by name; nothing creates, moves,
consolidates, or removes a stop, and no gesture redraws a line.

## Scope, in order

Items 2 and 3 edit the stops document and wait on
[stop-lifecycle.md](stop-lifecycle.md): its `add_stop`, `update_stop`,
and removals become dated edits of that document, not of the network.

1. **A station's bays on its card.** The card names a bay's station,
   but a station's card does not list its bays: `stop_detail` answers
   them (the host does not filter the stop list for them).
2. **Create, move, and edit a stop** in the Stops stage: a new stop
   from a point on the map (author `add_stop`), a pin dragged (author
   `update_stop`, which moves it for every pattern that visits it; the
   card names them before the change), and the record's fields
   (`update_stop` grows code, access, fare zone, and station). One
   library function measures where a stop falls along a pattern's line;
   a move re-measures every visit and warns where the line is now far
   from the stop. Closing and reopening a stop (D-144) is built in
   [stop-lifecycle.md](stop-lifecycle.md) item 3.
3. **Consolidate and remove.** One stop replaced by another on every
   pattern that visits it, then removed (a new gesture,
   `replace_stop {from, to}`); a stop withdrawn from every pattern and
   removed in one change, refused where it ends a pattern
   (`remove_stop` keeps refusing a stop still visited).
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
  stage and the Route map (a stop moved, one consolidated, one
  served, a line redrawn along the county roads).
