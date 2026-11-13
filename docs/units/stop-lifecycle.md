# The stop lifecycle

GTFS speaks only of what is in service now; a stop's past and future
(proposed, opened, closed for a while, moved, changed, removed) has no
place in it. The stops document holds them: every stop a list of dated
events, on the bus as `transit-stops` and `cap-stops`, the kernel's
`stops` kind (`docs/architecture/crates.md`; D-153, D-154). What is left
is joining it to everything else: a pick's stops refreshed from it, the
day serving what is open, GTFS in and out, the server storing it, and
the Stops stage reading it. The network document keeps its shape (D-156):
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

1. **Loading and publishing refresh a pick** (D-156). The refresh is on
   the bus (cap-stops `seed` and `refresh`, transit-network's
   `stop_record`; `docs/architecture/crates.md`). Left: every host
   refreshes a pick before it reads it — the service-change and
   service-design pages when they load or fork a plan, the server when
   it publishes a pick — over the pick's window, from its effective
   date until the next pick's (open-ended when there is none); the
   first stops document is seeded from the network in force. Stop edits
   (add, move, change) are edits of the stops document, never of a
   pick's copy. Publishing a pick publishes the refreshed network; a
   stop event inside a published window publishes it again, and pages
   follow it as they follow a new pick. Rider pages load that one
   document, as now; a page that imports the agency's GTFS URL (D-132)
   builds its network from the feed and sees no stops document; a
   static export carries the network of its day.
2. **Served, not positional.** The day reads a stop's `events`
   (transit-network `stop_record::stop_on`) and does not serve a stop
   that is closed, proposed, retired, or not yet opened on its date; the readers that index a trip's stop times
   by position (the timetable's slots, signs, dispatch's patching and
   geometry, the analysis ops) read the served stops. Every pattern
   keeps its visit and place; the timetable keeps a closed stop's column
   marked closed. A closure is never refused and holds nothing up
   (D-155): a pattern whose first or last stop is closed runs to or from
   its nearest open stop on those dates, and the planner is warned; a
   trip left with under two open stops does not run on those dates, for
   riders and in the export, and the planner is warned with the trips
   and dates. A published plan opens its proposed stops from its first
   date.
3. **GTFS in and out.** transit-codec imports a feed into a network
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
4. **Serving and storing it.** The server keeps the stops document in
   the store, serves it to any reader, and runs cap-stops' `validate`
   on every write; a stop change inside a published window republishes
   the rider's network. A write names the version it read and is
   refused when another writer moved the document first (the store's
   conditional create; the writer re-reads and re-applies).
5. **The Stops stage reads it.** The card and the map show a stop on a
   date the planner chooses — its status, point, details, and amenities
   — and its events; closing and reopening a stop for dates (D-144) are
   its first edits. The stage lists the events the last import added and
   the stops it did not list (item 3), each one fixable — a `moved`
   turned into a correction of the earlier point — and the warnings of
   item 2. The rest of
   [ui-stops.md](ui-stops.md)'s stop edits (create, move, consolidate,
   remove) are edits of this document.

## Done when

- The codec's round-trip law, a closure-splits-trips law, a move-inside-the-window
  law (one row per version, one `stop_code`, the made id read back), and
  the import law (a re-import of this repo's own feed adds no event)
  pass.
- `web/e2e/ui-service-change.spec.mjs` closes a stop for dates and
  sees it closed on the card, the map, and the timetable.
- A dogfood session on C-TRAN walks every control of the Stops stage's
  closure and its event list.
