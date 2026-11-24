# service-design

`service-design` (`src/uis/service-design/`) has six
views over one selection. A rail of views (keys 1–6) and a context
bar (plan, baseline, service group, route, stop, hour window) frame
whichever view is open.

- **Plans**: the pick timeline, the plan set (fork, baseline, feed
  import), and the service groups. The service groups are the
  combinations of service types that run together, each with its dates.
  The day-scoped frames ask on a group's first date. Any of its dates
  answers alike.
- **Network**: the group's numbers against the baseline, trips per
  hour, and the frequency grid. A drag on the trips per hour sets the
  hour window that every view reads. A row opens the route.
- **Coverage**: the parameters and window, the selected group's served
  and frequent share (named by its members, with its dates), and the
  places that its stops leave unserved, listed and mapped.
- **Hubs**: a stop's connection matrix, its waits, and its just-missed
  connections. Also the network's longest waits in the hour window, and
  the shared corridors.
- **Route**: the string-line workbench. It has the directions, the
  string-line or the timetable grid, gaps by hour, running time by
  start, the change ledger with undo and redo, and the route's map. In
  edit mode (E) it also has the gestures, placed above the picture they
  change:
  - span
  - running time
  - one trip moved alone (picked or dragged)
  - stops (one picked by its name on the string-line or its timetable
    column, dropped, or another inserted after it by name)

  Each gesture acts on one service type of the group. The panel picks
  and names that service type. It warns what else runs with it and the
  other groups it runs in (D-8, D-95). The panel is fresh for each
  route and direction. After a change, an undo, or a redo, the status
  line states the plan's issues and warnings as the answer gave them.
  The last export and publish answers are cleared with the version they
  were about. A frequency target is shown as waiting on D-54.
- **Proposal**: the scorecard (with the vehicles that the group's day
  needs; hours against a budget wait on D-108), the redline, readiness,
  the export, and publishing the pick to the deployment (D-74).
  - Readiness shows each issue in words. A broken block's next trip is
    held from its issue (the host's `retime_trip`). The warnings
    include each stop that the plan visits and that the stops document
    has not in service over its period (engine `stopWarnings`, D-157).
  - The export carries the stops' history over the plan's period
    (engine `withStopVersions`).

A frame with no answer says so in a planner's words. It keeps the
decision or finding that it waits on as a tag. The selection lives in
the URL hash. With no stored plan, the UI opens the server's `/network`
as the base plan, which every view calls the Base network. Its host
answers through the answer graph. A view's nodes answer only while the
view is open. Each frame's state is the answer graph's (`sd.frames`),
and the frame renders Unavailable with a retry while the rest answers. The plan's
health adopts cap-author's issues and warnings for a document that the
UI authored. After every change that the host adopts, the host names
its keep-set (the plan set's current networks and the layer). Undo
restores an earlier document version that the host keeps. Nothing is
inverted.

Its failure table. Service change has the same table.

| Input, and how it fails | The page knows by | Still shows | Refuses | Instead | Catches up | Pin |
|---|---|---|---|---|---|---|
| The server, or the device's network: no answer | a fetch error, or no answer within the read's time limit; the worker's last copy of the manifest | the plans kept on the device, and every view and change, which run in the page. It boots offline from the worker, and says "Offline · data as of" the time the server last answered. A page never opened online says that the published network could not be read. | — | a publish waits on the device (the outbox), counted on the page | the outbox sends it when the server answers, against the plan set as it is then | e2e `offline-staff` "service design: a publish made offline…", "service change: it boots offline…" |
| A publish refused when it is sent (a pick now dated in the past, a plan that no longer validates) | `422` | everything | the publish, held with the server's reasons | — | the person sends it again or discards it | e2e `offline-staff` "service design…", "service change…" |
| The bucket: no answer (the server answers) | `503` on a publish | the same | — | the publish waits on the device | the outbox sends it again | Go `TestABucketThatStopsAnsweringServesLabelledReadsAndRefusesWrites` (the server's side); `outbox.test.mjs` "a 503 keeps the write waiting" |
| Sign-in: none, or expired | `401` | everything | — | the publish waits: "Sign in to send" | the outbox sends it once the person signs in | `outbox.test.mjs` "no valid sign-in holds the outbox…" |
