# The stop lifecycle

A stop has a life of its own — planned, permitted, built, in service,
suspended for a while, retired — on a timeline set by permits and
construction, not by service plans (D-157). The stops document holds
it: every stop a list of dated events, on the bus as `transit-stops`
and `cap-stops`, the kernel's `stops` kind, written only through
`POST /stops` (`docs/architecture/crates.md`, `server.md`; D-152, D-153,
D-154). It is managed in an app of its own, the stops page
(`docs/architecture/generated-uis.md`, `stops`). A pick only reads it: it
takes each stop's record when it is published or exported, and the one
thing it says about a stop is whether its service visits it. Nothing
here publishes real time: the document is open, and the agency's own
systems read it to say a stop is suspended in their real-time feeds.
A stop is the rider's stop, kept through a move (D-145); its dates are
service days (D-148). Stations as infrastructure, fare zones, and
layover points stay as they are. Sharing the stops with a GIS (GeoJSON
for a date, JSON-FG over a range) is later work.

## Scope, in order

1. **GTFS in and out, wired.** The library and its ops are built
   (cap-stops `export_versions` and `import_events`; D-150, D-151,
   `docs/architecture/crates.md`). Left: their callers. An export —
   the planner pages' Export and the server's `GET /feed` — runs
   `export_versions` over the pick's period first, so a stop moved or
   renamed inside it is one stops.txt row per version. An import — the
   server's feed at boot, a feed a page loads — runs `import_events`
   on the feed's first service date and writes the answer through the
   receive path; the stops page lists the events it added and the stops
   it did not list, each one fixable (a `moved` turned into a
   correction of the earlier point, stops `correct_event`).

## Deferred

- **The stops page without a server's store, or offline** (the user,
  2026-11-12, deferred): a copy of the stops document on the device that
  every edit lands in first; begun from the deployment's document, the
  network it serves, a GTFS zip loaded in the page, or a stops file
  opened; saved to a file; sent to the deployment when its store
  answers; the page installed to work offline. Open: when the
  deployment's document moved on while edits waited — the edits
  replayed one by one onto it, each checked, the ones that no longer
  fit listed (recommended), or one of the two documents chosen.

## Done when

- An e2e exports a pick over a stop moved inside its period (two rows,
  one code) and imports a feed whose changes the stops page lists and
  fixes.
