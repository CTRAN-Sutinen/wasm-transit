# ride-now

`ride-now` (`src/uis/ride-now/`) is the rider's app: a full-bleed map of
every stop and live bus, with a panel (a card on a wide screen, a sheet
on a phone) that moves through Home (search, places near, saved and
recent stops), one stop's live board, one bus's trip, the routes,
Getting there, and the alerts in force.

- A trip's end is a stop, an address, the rider's location, or a spot
  on the map. A point end plans as a router place over cap-streets
  `access`, and a dropped pin is named by the geocoder's nearest address
  (D-37). A trip asked for "now" is asked again on each poll.
- It boots from the published network or a hand-loaded zip, then loads
  the published streets and walks in the background. Until they land,
  or where none is published, walks are straight lines. A hand-loaded
  zip walks no streets.
- Addresses come from the geocoder the manifest lists. The host sends
  only the typed text and a near point, shows the answer as given, and
  keeps no query ([web.md](../web.md), "External services").
- Boards include the previous date's after-midnight trips. It installs
  as an app and offers a new build ([updates-offline.md](../updates-offline.md)).

It reads the edits and the deployment's live feeds (or the rider's own)
every 30 s. Its inputs and session are its `ui.json`.

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
