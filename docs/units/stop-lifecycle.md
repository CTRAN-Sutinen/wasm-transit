# The stop lifecycle

GTFS speaks only of what is in service now; a stop's past and future
(proposed, opened, closed for a while, moved, changed, removed) has no
place in it. The stops document holds them: every stop a list of dated
events, on the bus as `transit-stops` and `cap-stops`, the kernel's
`stops` kind (`docs/architecture/crates.md`; D-153, D-154). A pick's stops
are a copy refreshed from it — by the server when it publishes and on
every stops write, by the planner pages when they open a draft — and a
dated day serves only open stops (`docs/architecture/crates.md`,
`server.md`, `web.md`). What is left is the planner's timetable on a
date, GTFS in and out, and the Stops stage reading and writing it. The
network document keeps its shape (D-156):
nothing that reads a network today changes. A stop is the rider's
stop, kept through a move (D-145); the document sits beside the plans,
and a fork never copies it (D-147); its dates are service days (D-148).
The split is the authoring side's: riders get one published network, as
now, and never the stops' past or a draft's proposed stops (D-147).
Stops only: stations as infrastructure, fare zones, and layover points
stay as they are. Sharing the stops with a GIS (GeoJSON for a date,
JSON-FG over a range, edits coming back) is later work, not in this
unit.

## Scope, in order

1. **The timetable on a date.** A dated day serves only open stops and
   the planner is warned of short trips (transit-network `materialize`,
   `served_warnings`; D-155; `docs/architecture/crates.md`). Left: the
   planner's timetable is date-free (one route, one direction, one
   service combination), so it does not show a closure; read for a
   date, it keeps a closed stop's column and marks it closed. And a
   published plan opens its proposed stops from its first date (an
   `opened` event its publish writes to the stops document).
2. **GTFS in and out.** transit-codec imports a feed into a network
   and a stops document, and exports a network with its stops over the
   feed's window: where a closure starts or ends inside the window, the
   affected trips split by calendar. The lossless round-trip law holds
   for a stops document with no dates. Where the stops document exists,
   an import compares the feed with the stops on its first service date
   and adds each difference as an event dated that day (`opened`,
   `moved`, `changed`); a stop the feed does not list keeps its events,
   later events stand, and riders get the feed at once (D-150). It
   answers the planner the events it added and the stops it did not
   list. A stop that moves or is renamed inside the export's window
   exports one stops.txt row per version — the version in force on the
   window's last date under the stop's id, each other under an id made
   from the stop's id and its start date, all sharing the `stop_code` —
   with the trips split by calendar, and an import reads a made id back
   as a version of its stop (D-151).
3. **The Stops stage reads it.** The card and the map show a stop on a
   date the planner chooses — its status, point, details, and amenities
   — and its events; closing and reopening a stop for dates (D-144) are
   its first edits. The stage lists the events the last import added and
   the stops it did not list (item 2), each one fixable — a `moved`
   turned into a correction of the earlier point — and the warnings of
   item 1. The rest of
   [ui-stops.md](ui-stops.md)'s stop edits (create, move, consolidate,
   remove) are edits of this document, never of a pick's copy; the
   stage's first write makes the stops document with cap-stops `seed`
   when the manifest pins none.

## Done when

- The codec's round-trip law, a closure-splits-trips law, a move-inside-the-window
  law (one row per version, one `stop_code`, the made id read back), and
  the import law (a re-import of this repo's own feed adds no event)
  pass.
- `web/e2e/ui-service-change.spec.mjs` closes a stop for dates and
  sees it closed on the card, the map, and the timetable.
- A dogfood session on C-TRAN walks every control of the Stops stage's
  closure and its event list.
