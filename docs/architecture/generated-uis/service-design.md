# service-design

`service-design` (`src/uis/service-design/`) is the planner's
workbench: six views (Plans, Network, Coverage, Hubs, Route, Proposal)
over one selection held in a context bar and the URL hash.

- A service group is a combination of service types that run together,
  shown with its dates. A day-scoped frame asks on its first date, and
  any of its dates answers alike (D-55).
- A timetable gesture acts on one service type of the group. The panel
  names it and warns what else runs with it (D-8, D-95).
- The proposal shows readiness in words, the stops a plan visits that
  are not in service over its period (D-157), the export with the
  stops' history, and publishing the pick (D-74). A frequency target
  waits on D-54; hours against a budget wait on D-108.
- With no stored plan, the server's `/network` is the Base network.
- The host answers through the answer graph, and a view's nodes answer
  only while it is open. A frame with no answer says so in a planner's
  words and offers a retry while the rest answers.
- Undo restores a document version the host keeps. Nothing is inverted.

Its failure table. Service change has the same table.

| Input, and how it fails | The page knows by | Still shows | Refuses | Instead | Catches up | Pin |
|---|---|---|---|---|---|---|
| The server, or the device's network: no answer | a fetch error, or no answer within the read's time limit; the worker's last copy of the manifest | the plans kept on the device, and every view and change, which run in the page. It boots offline from the worker, and says "Offline · data as of" the time the server last answered. A page never opened online says that the published network could not be read. | — | a publish waits on the device (the outbox), counted on the page | the outbox sends it when the server answers, against the plan set as it is then | e2e `offline-staff` "service design: a publish made offline…", "service change: it boots offline…" |
| A publish refused when it is sent (a pick now dated in the past, a plan that no longer validates) | `422` | everything | the publish, held with the server's reasons | — | the person sends it again or discards it | e2e `offline-staff` "service design…", "service change…" |
| The bucket: no answer (the server answers) | `503` on a publish | the same | — | the publish waits on the device | the outbox sends it again | Go `TestABucketThatStopsAnsweringServesLabelledReadsAndRefusesWrites` (the server's side); `outbox.test.mjs` "a 503 keeps the write waiting" |
| Sign-in: none, or expired | `401` | everything | — | the publish waits: "Sign in to send" | the outbox sends it once the person signs in | `outbox.test.mjs` "no valid sign-in holds the outbox…" |
