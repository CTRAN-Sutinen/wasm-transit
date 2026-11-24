# service-change

`service-change` (`src/uis/service-change/`) serves the same need as
service-design. Its layout follows the stages that a service change took
when it was worked through the ops on C-TRAN (three proposals, assessed,
built, judged and revised). One selection bar sits over six stages. The
bar holds the plan, the plan's day types, the hour window, health, and a
route box. The day types come from analysis `day_types` (a trial,
D-137). Each stage has the page's width.

- **Plan & calendar**: the pick timeline and the picks (PickList,
  D-139).
  - There is no base network. The list shows every pick that the
    deployment publishes and every draft on this device, by pick date.
    The pick that runs today is marked. The engine's `publishedPlanSet`
    reads the published picks. One is fetched as a value when it opens
    (`publishedPickValue`). Plans `in_force` over the published set
    finds the running pick. Before the first pick takes effect, the
    deployment's feed is marked.
  - A published pick and the deployment's feed are read-only. A change
    goes into a fork (author `set_plan`, the fork's calendar its
    period, D-138).
  - A draft is renamed (`update_plan`). It is given its period: it
    picks on a date and, optionally, runs through a date. The fork's
    own edit sets the period (`set_plan`: weekly service carried to the
    end, the calendar cut to the period). The edit is undone like any
    change. A draft is deleted from the device.
  - An imported feed is a draft that picks on its first service date
    (network `import`'s `first_service_date`, D-140). The page opens on
    what runs today.
  - Then the service keys (directory `service_keys`). A key is added
    (`add_service_type`) or removed (`remove_service_type`, refused
    while trips run under the key). Each key's weekdays, date ranges and
    exceptions are edited in place (cap-author's calendar edits). The
    stage also shows what a key carries (a route's trips moved to
    another key, `set_pattern_trips_refs`) and the combinations that it
    forms.
- **Scan**: the frequency grid. The network map (directory
  `route_lines`) is one click away.
- **Stops**: a stop belongs to the network, not to a route, so it has a
  stage of its own.
  - Every stop is on a map over the network's lines. A planner finds a
    stop there or by name (directory `stops`).
  - One stop opens to its card (directory `stop_detail` under the day
    type's members). The card shows its record, and every pattern
    position that visits it with the trips it runs. A station pin that
    no pattern visits names the routes that its bays serve.
  - A visit opens its route on the Route stage in that direction.
  - A stop's own record and life (creating, moving, suspending,
    retiring) are the stops app's (`stops`, below; D-157).
    Consolidating stops on a plan's patterns, and a route's own map,
    are `docs/units/ui-stops.md`.
- **Route**: one route's day in one direction. The route's bar chooses
  the direction (the first by default; a direction that runs nothing
  under the day type offers the others). The stage's tab closes it
  again. The stage has:
  - the string-line or the timetable
  - the changes (the service-design gestures)
  - running time by start
  - gaps by hour
  - the trips by start, with the key and the block that each runs under
    (a block opens on Vehicles & hours)

  A stop that the route serves is read beside it (its connections, near
  misses and waits). Once a plan is compared, the change ledger is
  below it.

  With no route open, New route builds one that the plan does not run.
  This is a task in three steps:
  1. The route (a taken id is refused).
  2. Each way it runs: a name, its stops picked in order, and the time
     end to end. The way back is the way out reversed until changed.
  3. Its trips: for each way and service key, a first and last
     departure and the minutes between.

  The picker offers the plan's stops. It also offers the stops
  document's stops that are in service or on their way by the pick date
  (stops `stops_on`). A stop that the plan does not hold is taken in
  with its record (stops `records`, D-173). The stop's life stays the
  stops app's. The plan only copies its record, as a pick reads it at
  publish. Create is one change: author `add_route`, `upsert_route`,
  `set_direction`, `upsert_stop`, `add_pattern_through` and
  `set_headway`. It is refused whole beside the task. Otherwise the
  route opens.
- **Vehicles & hours**: runcut `fleet_summary` and `suggest_blocks` on a
  date that the day type runs. The stage shows the peak, platform,
  revenue and layover hours, the trips on no block, and the suggested
  blocks adopted as one change. It draws every running block on the day
  (the summary's spans). One block opens to its chain (runcut
  `block_trips`).
- **Proposal**: the plan compared against is chosen here. Nothing is
  compared until the planner chooses (D-141). Then analysis
  `plan_compare` runs across every day type (a day type's card opens it
  on the Scan). The stage also has:
  - readiness: a block that an issue names opens on Vehicles & hours.
    The warnings include the stops that the plan visits that are not in
    service over its period, as service design's do.
  - export: each stop's record on the pick date, as service design's.
  - publish.

A stage's reads re-run when the stage opens. So a route or block chosen
while another stage is open reads in full there. The comparison is the
proposal's. The other stages carry no control for it. Only the Scan (a
changed route's mark, its legend) and the Route (the ledger, the
header's and the timetable's before → after) show it, once one is
chosen. Analysis `service_type_compare` is read on those two stages
only.

The selection lives in the URL hash, which the page follows. Back and
forward move through the stages. The UI has no single-key shortcuts. Its
authoring path (fork, apply, undo, export, publish) is service-design's,
copied. A UI never imports another's files.

Its failure table is service design's. Service change also shows the
published picks. When they cannot be read and the page holds none, it
says so beside the plans.
