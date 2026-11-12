# The stop lifecycle

GTFS speaks only of what is in service now; a stop's past and future
(proposed, opened, closed for a while, moved, changed, removed) has no
place in it. Stops leave the network and become one document of their
own, on a timeline every plan refers to by stop id: plain JSON, one
record per stop, each a list of dated events (D-153). A stop is the
rider's stop, kept through a move (D-145); the document sits beside the
plans, and a fork never copies it (D-147); its dates are service days
(D-148). The split is the authoring side's: riders get one published
network, as now — the pick with the stops in force over its window,
built when it is published — and never the stops' past or a draft's
proposed stops (D-147). Stops only: stations as infrastructure, fare
zones, and layover points stay as they are. Sharing the stops with a
GIS (GeoJSON for a date, JSON-FG over a range, edits coming back) is
later work, not in this unit.

## The document

```json
{
  "kind": "stops",
  "schema_version": 1,
  "stops": {
    "6124": [
      { "on": "2004-09-01", "event": "opened", "at": [-122.6512, 45.6934],
        "name": "NE 99th St & Hwy 99", "code": "6124", "wheelchair": true },
      { "on": "2025-05-12", "event": "changed",
        "amenities": { "shelter": true, "bench": true, "bin": true } },
      { "on": "2027-03-03", "event": "closed", "until": "2027-07-01" },
      { "on": "2027-09-01", "event": "moved", "at": [-122.6509, 45.6936] }
    ]
  }
}
```

- **Events, oldest first.** Each has a type, the service date it takes
  effect (`on`, in the agency's zone), and the details it sets:
  `opened` (the point and every detail; an import's stop, whose opening
  no feed records, has no `on`), `closed` (not served; with `until` it
  reopens on that date, without one it waits for `reopened`), `moved`
  (a new point; with `until`, temporary), `changed` (only the details
  that change), `proposed` (a draft plan's stop, naming the plan), and
  `retired` (the end; the tombstone).
- **Nothing means "today".** Every detail — the point, name, code,
  station, access, fare zone, amenities — changes only by an event; a
  stop on a date is its events applied in order up to that date, so no
  field goes stale. Every reader asks the library, never the raw list:
  the stop on a date, and whether a trip instance serves it. A finer
  grain than a day (D-148) changes those functions, not their callers.
- **A correction edits an event; a change adds one.** A wrong opening
  point is fixed in the `opened` event, and the store keeps the earlier
  version; a stop that really moves gets a `moved` event.
- **Amenities** (D-154) use OpenStreetMap's bus-stop tag names where
  one exists — a core list of shelter, bench, garbage can (`bin`),
  lighting (`lit`), printed schedule, real-time sign, bike rack,
  boarding pad, and tactile paving, the names checked against
  OpenStreetMap's tags in item 1. Each is `true` (there), `false`
  (checked, not there), or absent (not recorded). The agency's own
  names are kept and not checked. Condition and repair stay out.
- Who changed what, and when, is the store's history of the document
  (every write a new version, the pointer log its order), not a field
  in it.

## Scope, in order

1. **The library and the capability.** `transit-stops`: the document,
   the stop on a date, the served question, and the canonical bytes
   (stops by id, each stop's events in date order, a tie as written).
   Validation answers refusals apart from issues: an unknown event type,
   events out of date order, a stop with no `opened` or `proposed`
   first, an event after `retired`, a `reopened` with nothing closed, a
   `proposed` naming no plan, or an `until` not after its `on` is
   refused. `cap-stops` wraps it with a `validate` op the server's
   receive runs (`docs/architecture/agency-data.md`); the kernel gains
   its registry (`load_stops`, `stops_hash`).
2. **The network refers to stops by id.** `Network.stops` leaves the
   network document (a new version; cap-migrate splits an older
   network into a network and a stops document, each stop one `opened`
   event with no date). An authoring capability reaches a stop's record through the
   kernel's composed entry for a pick and the stops, so an op's
   arguments keep one network handle. Publishing a pick builds the
   riders' network (D-147): its stops as they are over the window — from
   the pick's effective date until the next pick's, open-ended when
   there is none — each carrying the events that fall inside it, so a
   rider's day skips a closed stop on its dates; a reader that knows no
   events still reads it. Earlier events and unpublished proposals stay
   out. A stop event inside a published window rebuilds it, and pages
   follow it as they follow a new pick. Rider pages load that one
   document, as now; a page that imports the agency's GTFS URL (D-132)
   builds its network from the feed and sees no stops document; a static
   export carries the network of its day.
3. **Served, not positional.** The day does not serve a stop that is
   closed, proposed, retired, or not yet opened on its date; the readers that index a trip's stop times
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
4. **GTFS in and out.** transit-codec imports a feed into a network
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
5. **Serving and storing it.** The server keeps the stops document in
   the store, serves it to any reader, and runs cap-stops' `validate`
   on every write; a stop change inside a published window republishes
   the rider's network. A write names the version it read and is
   refused when another writer moved the document first (the store's
   conditional create; the writer re-reads and re-applies).
6. **The Stops stage reads it.** The card and the map show a stop on a
   date the planner chooses — its status, point, details, and amenities
   — and its events; closing and reopening a stop for dates (D-144) are
   its first edits. The stage lists the events the last import added and
   the stops it did not list (item 4), each one fixable — a `moved`
   turned into a correction of the earlier point — and the warnings of
   item 3. The rest of
   [ui-stops.md](ui-stops.md)'s stop edits (create, move, consolidate,
   remove) are edits of this document.

## Done when

- `crates/transit-stops/tests/` holds the laws: the stop on a date and
  the served question against an oracle that applies the events by
  hand, a temporary closure or move ending on its `until`, each refusal,
  canonical bytes stable under reordered stops and keys, the agency's own amenity
  names and unknown fields kept through a read and a write, and the
  migration's frozen fixture.
- cap-stops has a dispatch test and a runner gate; the codec's
  round-trip law, a closure-splits-trips law, a move-inside-the-window
  law (one row per version, one `stop_code`, the made id read back), and
  the import law (a re-import of this repo's own feed adds no event)
  pass.
- `web/e2e/ui-service-change.spec.mjs` closes a stop for dates and
  sees it closed on the card, the map, and the timetable.
- A dogfood session on C-TRAN walks every control of the Stops stage's
  closure and its event list.
