# stops

`stops` (`src/uis/stops/`) holds every stop's own life, apart from the
service plans, which only read it (D-157), and the work asked for at the
stops (D-159). Its views are Stops (a map or table by status on the
date read, and the stops nearest the device), Work (every request as a
table), and Feed review (what the agency's feed did to the stops record,
D-150). A stop, a survey, and a request each open as a page.

- A stop changes only by an event. A change offers only what the stop
  may become that day, and a wrong event is corrected, not added to.
  Beside a suspension or a retirement the page names the routes the
  schedule still runs there, because the schedule changes only in a
  service change.
- The page has no GTFS control. A feed loaded into a draft plan is a
  draft, and this page never records it.
- A survey (D-158) is the Service Stop Worksheet. It starts from what
  the stop records and saves as one `changed` event. The server names
  the surveyor (D-163). A half-filled survey is kept on the device
  until saved or discarded.
- A request (D-159) moves through drafted, submitted, work order open,
  and completed, with one next step for its status. Where the
  deployment connects a work order system, the server's adapter opens
  and reads the work order; otherwise it is entered by hand (D-165).
  An open request on a stop retired today says so (D-238).
- Photos and drawings are named by hash where the deployment keeps
  attachments (D-162), and read with the signed-in person's token.
- A clicked point is kept to six decimals; a typed point is saved as
  typed; the device's fix is a start, never the measurement.
- Every write is checked by the page's op and again by the server
  (409: read again, ask again). With no stops document yet, the page
  shows the stops of the network that runs today, and the first change
  saves them. A change saved before sign-in is refused in words.

The view, the date, the open stop or request, the task, and the filters
live in the URL hash, so Back leaves a task.

Its failure table.

| Input, and how it fails | The page knows by | Still shows | Refuses | Instead | Catches up | Pin |
|---|---|---|---|---|---|---|
| The server, or the device's network: no answer | a fetch error, or no answer within the read's time limit; the worker's last copy of the manifest | the stops and requests last read. A reload boots from the worker on them, with "Offline · data as of" the time the server last answered. A survey half filled stays on the device. A stops document that cannot be read and is not held says "Stops could not be read", never that the deployment publishes none. | — | a change, a request, a work order request, or a photo saved meanwhile waits on the device (the outbox), in order, a photo before the survey that names it | the outbox sends each when the server answers; the server runs each change again over the stops it holds | e2e `offline-staff` "the stops page: a queued change runs over a moved head…", `ui-stops` "a stops document whose read fails says so…" |
| A change refused when it is sent (another writer changed the same stop since the page read it, a change the module refuses) | `409`, `422` | everything | the change, held with the server's reasons | — | the person sends it again (over what the page reads then) or discards it | e2e `offline-staff` "the stops page…" |
| The bucket: no answer (the server answers) | `503` on a save | the same | — | the save waits on the device | the outbox sends it again | Go `TestABucketThatStopsAnsweringServesLabelledReadsAndRefusesWrites` (the server's side) |
| The network: unreadable | the read fails | the stop's record | the Routes card says the routes cannot be read, never that no route stops there | — | the next read | e2e `ui-stops` "a stop whose routes cannot be read says so…" |
| Sign-in: none, or expired | `401` | everything | — | a save waits: "Saved on this device. Sign in to send it." | the outbox sends it once the person signs in | e2e `ui-stops` "a write needs sign-in…" |
| The work order system: none connected | `501` | the request | opening a work order: "No work order system is connected. Enter the work order by hand." | the person enters the work order by hand, on the page | — | none |
| The attachments store: no answer | a fetch error | the record | — | a photo or a drawing waits on the device, named by its hash, shown from the device's copy | the outbox sends it before the survey that names it | none |
