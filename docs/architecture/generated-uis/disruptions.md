# disruptions

`disruptions` (`src/uis/disruptions/`) answers what is disrupted today,
and whether an alert explains it (cap-disruptions, D-213, D-218), for
dispatch, communications, and customer service. Its views are Status
(the day by route), Alerts (the gaps and the agency's alerts), and
Dispatch (late trips, detours, cancels, and today's notes), beside a map
that is always on screen.

- An alert is written beside what riders will see of it (a board
  preview over rt `encode_alerts` and `live`). Started from a gap, it
  carries the gap's effect, riders, and what it explains. A template
  fills its texts (alerts `fill`).
- Every write (an alert, a note) is sent as its intent to its log. The
  server names the writer and the time. A `409` means another person
  changed the same record since the page read it.
- A cancelled trip still shows every stop and its path as scheduled.
- Every frame reads one clock: the last poll, or the write's.

It reads the network, edits, stops, alerts, and notes by the manifest's
hashes (again only when a hash moves) and the live feeds, every 30 s
while visible. Its inputs and session are its `ui.json`.

Its failure table.

| Input, and how it fails | The page knows by | Still shows | Refuses | Instead | Catches up | Pin |
|---|---|---|---|---|---|---|
| The server, or the device's network: no answer | a fetch error, or no answer within the read's time limit; the worker's last copy of the manifest | the documents last read. A reload boots from the worker on them. "Offline · data as of" the time the server last answered, and the detours and cancels labelled with the time the server last confirmed them. A page never opened online says that the network could not be read. | — | a note or an alert saved meanwhile waits on the device (the outbox), counted on the page | the outbox sends each once, in order, when the server answers | e2e `offline-staff` "the disruptions page through an outage…" |
| A write refused when it is sent (another dispatcher changed the same note or alert since the page read it, a change the module refuses) | `409`, `422` | everything | the write, held with the server's reasons | — | the person sends it again (over what the page reads then) or discards it | e2e `offline-staff` "the disruptions page: a note edit refused at send time…" |
| The bucket: no answer (the server answers) | `X-Store-Stale` on the edit document; `503` on a save | detours and cancels, labelled with their age. The stops, alerts, and notes are held by hash, so they are never old: the manifest that pins them carries the age. | — | a save waits on the device | the outbox sends it again | Go `TestABucketThatStopsAnsweringServesLabelledReadsAndRefusesWrites` (the server's side) |
| A live feed: no answer | a fetch error, or a status other than 200 | the schedule's day. The live state shows the feed's error. | the buses on the map, and lateness | — | the next poll | none |
| A live feed: frozen | not detected (QAF-129) | old buses and lateness as live | nothing | — | — | none |
| Sign-in: none, or expired | `401` | everything | — | a save waits: "Saved on this device. Sign in to send it." | the outbox sends it once the person signs in | `outbox.test.mjs` "no valid sign-in holds the outbox…" |
