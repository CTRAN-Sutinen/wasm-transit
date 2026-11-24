# ride-now

`ride-now` (`src/uis/ride-now/`) is a full-bleed map with a panel over
it. The panel is a card at the left on a wide screen, and a sheet over
the bottom of the map on a phone. The live pill and the alerts button
are in the corner. The map draws every stop and every live bus. A bus is
colored by its status, points its bearing, and is labeled with its
route. A tapped bus, stop, or spot opens it.

The panel follows the rider through these views:

- **Home**: the type-ahead, the places near the rider or a tapped spot,
  saved stops, and recent stops.
  - The type-ahead finds stops. It also finds addresses and places when
    the server runs a geocoder. A picked address is a spot, "Near 1400
    NE 136th Ave".
  - "Get directions" and "Routes" start from here.
  - Each place near the rider or a tapped spot shows its next
    departures. When the server publishes the streets, the nearest
    places are ordered by the walk along the streets.
- **One stop**: its live board. A picked departure draws its route,
  marks its bus, and offers to track it.
- **One bus**: its trip stop by stop. The stop it heads to is marked.
  The map follows it.
- **The routes and one route**: the route's directions' variants as
  line diagrams. Its buses stay bright on the map, and the rest dim.
- **Getting there**: the ends, the time, and the itinerary.
  - Each end is a type-ahead, or the rider's location, or chosen on the
    map. "Choose on map" is per end: the next tap sets it. The tap
    sets a stop. With the streets published, it sets any spot. A
    dropped pin is named by the geocoder's nearest address (D-37).
  - An address typed in an end's field is a point end the same way.
  - "Directions here" on a tapped spot does the same.
  - A point end plans as a router place over cap-streets `access`. Its
    walks are drawn along the streets.
  - The time is leave now, leave at, arrive by, or the last bus.
  - The itinerary is a timeline, drawn on the map.
- **The alerts in force**.

The UI boots from the server's published network (`servedNetwork`,
bytes it never parses; or a zip that the rider loads). Then it loads the
server's `/streets` and `/walks` in the background (`servedStreets`,
`servedWalks`). Until they land, or where none is published, walks are
straight lines and trips run stop to stop. A hand-loaded zip walks no
streets, because the walk table walks the published network. The UI
reads the edit document from `GET /edits`. It reads the live feeds that
the deployment names in `public/live-feeds.json` (the rider may set
their own; kit's `fetchFeed`). It polls both and the clock every 30 s.
A trip asked for "now" is re-asked on each poll. Its boards include the
previous date's after-midnight trips (`previous`). It follows the
deployment and installs as an app ([updates-offline.md](updates-offline.md)).
A new build is offered as "a new version is ready".

Addresses come from the geocoder that the manifest lists (read once on
boot; [web.md](web.md), "External services"). The host sends the typed
text and the near point or the rider's. It shows Pelias's answer as
given. A point that it answers keeps the name it was shown with. Nothing
keeps the query. Its questions and session are its `ui.json`.

Its failure table.

| Input, and how it fails | The page knows by | Still shows | Refuses | Instead | Catches up | Pin |
|---|---|---|---|---|---|---|
| The server, or the device's network: no answer | a fetch error, or no answer within the read's time limit. The worker answers the manifest and the network from its last copy. | the schedule it holds. It boots offline from the worker, and its boards say they are the schedule. Detours it holds, labelled "Detours as of …: no answer since …". With none held: "Detours could not be read". | address search | — | the next poll reads the manifest and the edit document, each failed read after a random part of its interval. A new network re-pins with no reload. | e2e `deploy-builds` "both pages boot offline…", `static-deploy` "ride now boots offline…" |
| The bucket: no answer (the server answers) | `X-Store-Stale` | detours, labelled "Detours as of …" | nothing (it writes nothing) | — | the next poll | e2e `ui-ride-now` "says how old detours are…" |
| A live feed: no answer | a fetch error, or a status other than 200 | the schedule, and the feed's error in the live feeds panel | live predictions and buses | — | the next poll | none |
| A live feed: frozen | not detected (QAF-129) | old buses and predictions as live | nothing | — | — | none |
| The geocoder: no answer | a fetch error | stops by name, the device's location, and a tapped point | addresses | — | the next search | none |
| The agency's GTFS URL, in a static deployment that follows it: no answer | a fetch error | the last zip the device kept. With none: "The agency's schedule could not be read. This page will try again." | a new schedule | — | checks again in an hour | none |
| The map's tiles: no answer | not detected | the map with no base layer | nothing | — | the next tile request | none |
