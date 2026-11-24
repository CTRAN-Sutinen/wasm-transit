# stop-display

`stop-display` (`src/uis/stop-display/`) is the screen at a stop: a
passenger information display over one `grouped_board` answer, in three
full-screen layouts (a transit center, a single route, several routes).
The alerts slot is always on screen. The stop and the layout live in the
URL, so a screen reopens on its stop. It boots from the published
network or a zip, reads the edits and the deployment's live feeds, and
includes the previous date's after-midnight trips. It works offline
([updates-offline.md](../updates-offline.md)) and applies a new build
only while the server answers and the setup is closed. Its inputs and
session are its `ui.json`.

Its failure table.

| Input, and how it fails | The page knows by | Still shows | Refuses | Instead | Catches up | Pin |
|---|---|---|---|---|---|---|
| The server, or the device's network: no answer | a fetch error, or no answer within the read's time limit. The worker answers the manifest and the network from its last copy. | the schedule it holds. It boots offline from the worker, and its board says it is the schedule. Detours it holds, labelled "Detours as of …: no answer since …". With none held: "Detours could not be read". | a new build: it applies one only while the server answers (the manifest's last read answered, not from a working copy) | — | the next poll, each failed read after a random part of its interval. A new network re-pins with no reload. | e2e `deploy-builds` "both pages boot offline…", `static-deploy` "the stop display boots offline…", `ui-stop-display` "a server that never answers the edit document turns the label to stale within the read's time limit" |
| The bucket: no answer (the server answers) | `X-Store-Stale` | detours, with their age | nothing (it writes nothing) | — | the next poll | e2e `ui-stop-display` "a detour served while the server's store is not answering says its age" |
| A live feed: no answer | a fetch error, or a status other than 200 | the schedule | live predictions | — | the next poll | e2e `ui-stop-display` "a live feed that stops answering turns the board to the schedule…" |
| A live feed: frozen | not detected (QAF-129) | old predictions as live | nothing | — | — | none |
| The agency's GTFS URL, in a static deployment that follows it: no answer | a fetch error | the last zip the device kept. With none: "The agency's schedule could not be read. This screen will try again." | a new schedule | — | checks again in an hour | e2e `static-feed-url` "the stop display boots offline on the last zip it had" |
