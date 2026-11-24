# stops

`stops` (`src/uis/stops/`) holds every stop's own life, apart from the
service plans, which only read it (D-157). It also holds the work asked
for at the stops (D-159). Each job has a screen the width of the page,
laid out for that job. A bar across the top holds the views (Stops,
Work, Feed review), the date read (today until chosen), a search, New
stop, sign-in, and the status line.

- **Stops**: a filter per status: planned, permitted, built, in
  service, suspended, retired (hidden until shown). Each filter has its
  count for that day (stops `stops_on`'s `counts`). The stops shown
  appear in one of two ways:
  - on a map (coloured by status) beside their list
  - as a table (`StopsTable`: name, id, status, since, until, last
    surveyed, notes, sorted by any column)

  "Nearby stops" asks the device for its location (the host's `locate`).
  It lists the stops nearest the device on the date at the statuses
  shown, each with its distance (stops `stops_near`). The device is
  drawn on the map. The map stays mounted under the other screens, so
  going to a stop and back does not rebuild it.
- **Work** (`WorkTable`): every request across the stops (requests
  `list` over the manifest's `stop_requests_hash`) as a table. The
  columns are request, stop (stops `stops_on` for the requests' stops),
  job, status, requested on, work order, and notes. The table sorts by
  any column. By default it shows the drafted, submitted, and ordered
  requests. Each status is a filter with its count, and each job is a
  filter. A row opens the request's page. The page's back link returns
  to the table.
- **Feed review** (`FeedReview`): what the agency's feed did to the
  stops record (the server records the feed's differences when it
  imports it, D-150). It shows:
  - the feed's first service date
  - the changes that the server recorded (stops `feed_events` for the
    manifest's `feed_hash`)
  - the stops in service that the feed does not list
  - the stops whose difference did not fit their history (stops
    `import_events` run without writing)

  Each stop links to its page, where a change is corrected. The page has
  no GTFS control. A feed that a planner loads into a draft plan is a
  draft, and the page never records it.

A stop (`StopPage`; stops `stop`) opens as a page. The header shows its
name, id and code, and where it stands that day. The header has these
buttons:

- Survey.
- Record change. It offers only what the stop may become that day
  (`stop`'s `next`), and a move and new details. Each has a line that
  says what it does.
- Request work.
- Report issue.

Below the header, its record is shown as headed cards in as many columns
as the screen holds:

- Location: its point on a map of the stops around it, opened at the
  stop.
- Details and amenities.
- Routes: each route and direction once, and whether the stop is a time
  point on it. It also shows the stops either side with the distance
  along the route to each ("~" where the import estimated it). The
  source is directory `stop_detail` over `servedNetwork` on the date. It
  is read only, which is why the page links the network capability too.
- Work: its requests (requests `requests_for`). An open request whose
  stop is retired today says so on its page (stops `request_warnings`,
  on the agency's today, not the date read; D-238).
- Last survey: when, by whom, the answers grouped as the Service Stop
  Worksheet groups them, and its ids in other systems.
- History: every event, newest first, each correctable.

A change (`ChangeFlow`) opens as a card under the header. A change is a
lifecycle event, new details and amenities, or the correction of an
event entered wrong (stops `correct_event`, or the event deleted after a
confirmation). Each change asks only what it needs. The card shows:

- a one-line summary of what will be saved
- the routes that the schedule still runs there, beside a suspension or
  a retirement (the schedule changes only in a service change)
- the request that the change carries out, where the stop has any (its
  event's `request`)
- why the change was refused, beside its Save (`TaskFrame`)

A move, and a new stop, take the map with a panel beside it. This is the
one job that needs a large map. A new stop (`NewStopFlow`) is placed,
then given its id, name, code, and where its life starts: planned, or in
service. A planned one may go straight on to its installation request.
An id that the document holds is refused beside the form.

A survey (`SurveyPage`, D-158) is a page of the worksheet's sections,
each under its heading: the site questions as the worksheet groups them,
then amenities, external IDs, and photos and notes. A section index is
beside it. The index shows each section's answered count and a mark
where it changed. A click goes to the section. A summary column shows
the survey date, what differs from the record, and Save, with why a save
was refused. Each question shows what the record had where the answer
differs. A list answer is a row of choices. A count is typed with − and
+. A section can be put back to the record. The survey starts from what
the stop records. It is saved as one `changed` event that carries every
answer recorded. The server names the surveyor: the person signed in
(D-163). A survey half filled is kept on the device (`localStorage`, per
stop) until it is saved or discarded, so a reload or a dropped
connection loses nothing. The stop says that a draft is kept. On a phone
the survey is the whole screen: the index is a bar of sections, and Save
is held at the bottom.

A request (`RequestPage`, D-159) is a page. The header shows its job,
id, and stop, with its status track: drafted, submitted, work order
open, completed (or cancelled, after a confirmation). Below the header
is the worksheet as headed sections (Location, Service, Utility locate,
Work order, Completion, Notes & photos), each edited in place. Beside
them is the one next step for its status:

- submit
- open the work order, where the deployment connects a work order
  system (the manifest's `services.work_orders`, D-165). The server's
  adapter answers, and the answer lands on the request. With none
  connected, enter the work order by hand (requests
  `record_work_order`, as the adapter's answer would be).
- check its status
- complete it
- when completed, record the change on the stop that carried it out (a
  change naming the request)

A new request is the same page with every section open and the job
chosen first. A field report is a repair, unless chosen otherwise. A new
stop's installation is a new stop. Its id is suggested from the stop and
the date, and is never one that the stop's requests already have. It is
saved as a draft or submitted. Requests `put` places and checks it.
`POST /stop-requests` stores it (409: read again, save again).

Where the deployment keeps attachments (the manifest's
`services.attachments`, D-162), the survey and request pages take photos
and drawings. "Take photo" opens the camera. "Add file" takes a photo or
a PDF (`Attachments`). Each is added at once and named on the record by
its hash with a caption. The history and the request show them small
(`AttachmentThumbs`), read with the signed-in person's token.

A point is clicked on the map or typed as measured in the latitude and
longitude fields. A click is kept to six decimals, about 0.1 m, because
a click knows no more. A typed point is saved as typed. The two edit one
point: a click fills the fields, and a typed pair moves the marker
(`PointFields`). "Use my location" fills them from the device's fix,
shown with its accuracy. The fix is a start and never the measurement.
Whether the point is on the earth is the receive path's check (stops
`add_event`).

Each change is one event. Stops `add_event` places and checks it.
`POST /stops` checks it again and stores it (409: the page reads the
stops again and asks for the change again). With no stops document yet,
the page shows the stops of the network that runs today (stops `seed`),
and the first change saves them. Where the deployment asks for sign-in,
the header shows who is signed in, or a Sign in button (`SignInControl`,
web.md "Signing in"). A change saved before signing in is refused in
words beside its Save, and a survey's draft is kept. The view, the
stops' mode, the date, the open stop, the open request, the task, and
the filters live in the URL hash, so the browser's Back leaves a task.

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
