# disruptions

`disruptions` (`src/uis/disruptions/`) answers what is disrupted today,
and whether an alert explains it (cap-disruptions, D-213, D-218). A bar
across the top holds the date and clock, the views (Status, Alerts,
Dispatch),
whether the times are live, and sign-in. Below it, a panel sits beside
a large map that is always on screen.

- **Status**: the search, then each affected route as a card. A search
  pick narrows the day to one route or one stop (a station with its
  bays). A route's badge picks the route.
- **Alerts**: the disruptions no alert explains, each starting an
  alert, and the agency's alerts in force, upcoming, drafts, and ended.
- **Dispatch**: the trips late now, the detours, the cancelled trips
  (from the edit document and the live feed, each as given), and
  today's notes, newest first.
- **A disruption opened**: what it is and where it came from, the next
  trip, whether an alert explains it, the stops it takes away, and its
  notes, oldest first. A cancelled trip shows its every stop and draws
  its path as scheduled: the edit document's cancel takes the trip out
  of the patched day. A note is added or edited there (notes `write`,
  sent as its intent to `POST /logs/notes`; the server names the writer
  and the time, and the note keeps the id it was made with).

An alert is written on half the page, beside what riders see of it
(AlertReach). The day's map is set aside while it is open. "Starts
now" tells riders from the moment the alert is saved: the start is
stamped then. Started from a gap, it carries the gap's effect, the riders it
informs, and what it explains, and it is told from now with no end. A
template fills its cause, effect, severity, and texts (alerts `fill`
over the alert's routes, stops, and dates). Alerts `put` checks it as
it is edited. Save draft and Publish put it and send the put as its
intent to `POST /logs/alerts`. A `409` means another person changed the
same alert since the page read it; a change to another alert does not
refuse it. End alert ends it at the host's clock (alerts `end`, the time
in its arguments). Preview shows one stop's board
with the alert on it (alerts `put`, rt `encode_alerts`, rt `live`,
board `stop_board`), and downloads the feed's `.pb` file. The list is then read once the
end's second has passed, because a period holds through its end. Every
node reads one clock: the page's last poll, or the write's.

The page reads the deployment's network, the edit document (`GET
/edits`), the stops, the service alerts, and the notes (by the
manifest's hashes, each read again only when its hash moves), and the
deployment's live feeds. It polls them and the clock every 30 s while
it is visible. The view, the pick, and the open disruption live in the
URL hash. Its questions and session are its `ui.json`.

Its failure table.

| Input, and how it fails | The page knows by | Still shows | Refuses | Instead | Catches up | Pin |
|---|---|---|---|---|---|---|
| The server, or the device's network: no answer | a fetch error, or no answer within the read's time limit; the worker's last copy of the manifest | the documents last read. A reload boots from the worker on them. "Offline · data as of" the time the server last answered, and the detours and cancels labelled with the time the server last confirmed them. A page never opened online says that the network could not be read. | — | a note or an alert saved meanwhile waits on the device (the outbox), counted on the page | the outbox sends each once, in order, when the server answers | e2e `offline-staff` "the disruptions page through an outage…" |
| A write refused when it is sent (another dispatcher changed the same note or alert since the page read it, a change the module refuses) | `409`, `422` | everything | the write, held with the server's reasons | — | the person sends it again (over what the page reads then) or discards it | e2e `offline-staff` "the disruptions page: a note edit refused at send time…" |
| The bucket: no answer (the server answers) | `X-Store-Stale` on the edit document; `503` on a save | detours and cancels, labelled with their age. The stops, alerts, and notes are held by hash, so they are never old: the manifest that pins them carries the age. | — | a save waits on the device | the outbox sends it again | Go `TestABucketThatStopsAnsweringServesLabelledReadsAndRefusesWrites` (the server's side) |
| A live feed: no answer | a fetch error, or a status other than 200 | the schedule's day. The live state shows the feed's error. | the buses on the map, and lateness | — | the next poll | none |
| A live feed: frozen | not detected (QAF-129) | old buses and lateness as live | nothing | — | — | none |
| Sign-in: none, or expired | `401` | everything | — | a save waits: "Saved on this device. Sign in to send it." | the outbox sends it once the person signs in | `outbox.test.mjs` "no valid sign-in holds the outbox…" |
