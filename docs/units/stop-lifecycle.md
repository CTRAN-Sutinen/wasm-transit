# The stop lifecycle

A stop has a life of its own — planned, permitted, built, in service,
suspended for a while, retired — on a timeline set by permits and
construction, not by service plans (D-157). The stops document holds
it: every stop a list of dated events, on the bus as `transit-stops`
and `cap-stops`, the kernel's `stops` kind, written only through
`POST /stops` (`docs/architecture/crates.md`, `server.md`; D-152, D-153,
D-154). It is managed in an app of its own. A pick only reads it: it
takes each stop's record when it is published or exported, and the one
thing it says about a stop is whether its service visits it. Nothing
here publishes real time: the document is open, and the agency's own
systems read it to say a stop is suspended in their real-time feeds.
A stop is the rider's stop, kept through a move (D-145); its dates are
service days (D-148). Stations as infrastructure, fare zones, and
layover points stay as they are. Sharing the stops with a GIS (GeoJSON
for a date, JSON-FG over a range) is later work.

## Scope, in order

1. **The stops app.** A generated UI of its own (its own product,
   needing the server): every stop on a map and in a list, read on a
   date the user chooses — its status, point, details, and amenities —
   and its history. Its edits are events, each one `add_event` written
   through `POST /stops`: a new stop planned at a point, then permitted,
   built, in service; suspended for dates or with no end, back in
   service, retired; moved; its details and amenities changed. The
   first write makes the document with `seed`. Creating, moving, and
   retiring a stop are this app's; which stops a plan visits stays the
   planner pages' ([ui-stops.md](ui-stops.md)).
2. **What a pick reads.** Publishing and exporting a pick take each
   stop's record (point, name, code, details) from the stops document
   as it stands on the pick's first date; no events are copied, and a
   stops write republishes nothing. The planner is warned, never
   refused, when a pick visits a stop not in service on its dates:
   always for a stop planned, permitted, built, or retired; for a
   suspended one only when the suspension covers the pick's whole
   period or has no end.
3. **GTFS in and out.** An import compares the feed with the stops on
   its first service date and adds each difference as an event dated
   that day (`in_service` for a stop the document lacks, `moved`,
   `changed`); a stop the feed does not list keeps its events, later
   events stand, and riders get the feed at once (D-150). It answers the
   events it added and the stops it did not list, which the stops app
   lists, each one fixable — a `moved` turned into a correction of the
   earlier point. A stop that moves or is renamed inside the export's
   window exports one stops.txt row per version sharing its
   `stop_code`, and an import reads a made id back as a version of its
   stop (D-151).

## Done when

- A pick's record and warning
  laws (transit-network), and the codec's round-trip, move-inside-the-window,
  and import laws (a re-import of this repo's own feed adds no event)
  pass; nothing in a pick changes when only a suspension is written.
- An e2e walks the stops app: a stop planned, put in service, suspended
  for dates and back, moved, and retired, each on the map, its card, and
  its history; and a pick published over a stop retired before its
  first date carries the warning.
- A dogfood session on C-TRAN walks every control of the stops app.
