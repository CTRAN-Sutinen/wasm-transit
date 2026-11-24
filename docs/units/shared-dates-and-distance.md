# Shared dates and distance

Goal: each library parses and writes the service date ("YYYY-MM-DD")
through one helper, with one rule for what it accepts, and
transit-network measures distance through transit-geo (D-250), as the
other libraries do.

Blocked: D-251 (where the date helper lives, and whether
transit-network may use it and transit-geo).

Scope:

1. The service-date helper: the laws first (a valid date reads back
   as written; what the helper refuses, such as spaces, a short month,
   or an impossible day), then the helper, its parse, its format, and
   its serde helper.
2. The ten `parse_date` copies, the inline parses and formats, and the
   serde helpers in transit-network and transit-sync move to it. GTFS's
   "YYYYMMDD" in transit-codec and transit-rt stays where it is.
3. transit-network's `haversine_m` (`listing.rs`) moves to
   `transit_geo::distance_m`.

Done when: `just test` passes with no `parse_date` or `haversine_m`
left outside the shared helpers, the date laws are in the helper's
tests, and `docs/architecture/crates/` names the helper.
