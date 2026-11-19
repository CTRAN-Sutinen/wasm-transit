# disruptions

What is disrupted on a day, and does an alert explain it (library: transit-disruptions)? A disruption is a change to the day's service from one of three documents:

- the edit document: a submitted or accepted diversion or trip cancel in force on the day. A draft is not yet anyone's news.
- the live document: a trip instance that the live feed cancels.
- the stops document: a stop suspended on the day.

Each disruption is listed as its document gives it (`from`: `edits`, `feed`, or `stops`). A trip that both the edit document and the live feed cancel is listed twice, once from each. No rule picks one.

Each disruption carries:

- `source`: what it is, as an alert's `explains` names it (an `edit`, a `stop`, or a `trip` instance).
- `effect`: a diversion is a detour. A trip cancel and a suspended stop are no service.
- the names a rider uses: the route, a cancelled trip's start, first stop, and headsign, and the stops it takes away, each with its name.
- `next_start_secs`: for a cancelled instance, the start of the next trip of the route from the same first stop, in the same direction, that runs on the day and is not cancelled in either document.
- its first and last dates.
- `explained_by`: the alerts that name it and are not withdrawn or ended at `now`. A draft explains, because someone is writing it.
- `latest_note`: the newest note about it.

Every document is named by hash. Each one absent is an empty one. With no live document, there are no feed cancels and no late trips. `now` is the host's clock in Unix seconds, and `date` is the service date, "YYYY-MM-DD".

## today

The day by route. Each route with a disruption or a late trip is listed once, by the route's sort order, then its name (numbers by value), then its id. Each route carries its disruptions, how many of its trips the live document classes late or very late, and the worst of their delays. A cancelled trip is not late. A suspended stop is listed on every route that serves it. `unaffected_routes` counts the network's routes that are not listed.

`route` answers one route. `stop` answers the disruptions that touch one stop: its closure, a detour that skips it, or a cancelled trip that serves it. With `stop`, a route is listed for its disruptions there only, and no route is listed for lateness alone.

## gaps

What no alert explains on the day, with what an alert about each starts from: its source, effect, route or trip and start, stops, and dates. The order is the order of `today`'s disruptions before they are grouped: the edit document's, in its order; then the feed's cancels, by start; then the suspended stops, by id.
