# cap-runcut

The runcut capability answers one question: what does this vehicle do
today? Its ops:

- `blocks`: the day's blocks. This is the index facet: the vehicles out,
  with the count of trips each runs that day. A block whose every trip
  cannot be represented or is skipped is not offered. The hole is shown
  as a hole.
- `fleet_summary`: the day's fleet. It gives the peak of concurrent
  vehicles and when it happens, the platform, revenue, and layover
  hours, and an outline of each running block (span, trips, and the
  longest internal gap). One call replaces a `block_trips` call for each
  block.
- `block_trips`: one block's chain. This is the detail facet: the
  authored runcut with the layover ripple. The ripple is the time the
  in-force edits add. With a live document, the ripple is a trip's
  predicted delay at its last stop, and that wins (D-18).
  `layover_remaining_secs` goes negative when a connection is eaten.
  `layover_health` classifies the layover as overlap, tight, or ok,
  under the caller's `tight_layover_secs`. Each row names its trip by
  id. A frequency trip appears once for each instance.
- `trip_stops`: one trip instance's effective stops on the date. This
  is the trip detail facet.
- `trip_path`: one trip instance's line, which the map's click-through
  draws. It is the authored alignment. An in-force diversion's traced
  detour is spliced in between the stops the diversion keeps.

Blocking is where cutting the work begins (D-46). So the same question
also answers under the work rules (the kernel's `work_rules` kind):

- `recovery`: what each wait requires. It gives the required time and
  the slack, for each wait and each pattern.
- `suggest_blocks`: which vehicles the trips that no block carries need.
  It gives the suggested blocks, each wait's slack, the adopted peak
  against the authored one, and the edits that author `apply` adopts.

cap-runcut reads the same documents and the same held day as cap-board.
The domain code is in the shared transit-dispatch and transit-runs
libraries. The arch gate forbids a GTFS layer here. The UI's blocks
panel and trip lines answer through this module.
