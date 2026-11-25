# service-change

`service-change` (`src/uis/service-change/`) serves the same need as
service-design, laid out as the stages a service change took when it
was worked through on C-TRAN: Plan & calendar, Scan, Stops, Route,
Vehicles & hours, and Proposal, under one selection bar. Its authoring
path is service-design's, copied; a UI never imports another's files.

- There is no base network. The picks the deployment publishes and the
  drafts on this device are one list by pick date. A published
  pick and the deployment's feed are read-only; a change goes into a
  fork whose calendar is its period. An imported feed picks on
  its first service date. Day types are a trial (D-137).
- A stop belongs to the network, not a route, so it has its own stage.
  A stop's life is the stops app's. A new route may take in a
  stop the plan does not hold by copying its record, and is
  created in one change, refused whole.
- Nothing is compared until the planner chooses a plan to compare
  against. Only the Scan and the Route show the comparison.
- A stage's reads run again when it opens.

Its failure table is service design's. Service change also shows the
published picks. When they cannot be read and the page holds none, it
says so beside the plans.
