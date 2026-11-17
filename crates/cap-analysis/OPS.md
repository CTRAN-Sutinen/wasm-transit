# analysis

Planner analysis: what do the numbers of an authored network document say? The ops answer network, route, and stop stats, and per-service-type overview and detail. They also answer the coverage of a weighted layer (people, jobs, or households, as points or polygons), and a proposal's coverage gains and losses. This was the first capability extracted from the monolith. The domain code lives in the shared transit-analysis library. Assemble this capability for planners. Riders never download it. Documents are hash-addressed: load a document once, then query it by hash. No op serializes a document.

## network_stats

Answers the network's numbers on one service date: trips, spans, revenue hours, revenue miles, transfer points, per-route and per-stop activity, and headways.

Every route of the document is a row, sorted by id. A route with no service on the date answers `trips` 0 and null spans. Inactive routes are analysis too, so filter on `trips`.

Each route row carries:

- `revenue_hours` and `revenue_miles`: the per-route cost basis. Every cost model is per hour and per mile. Average operating speed follows from the two.
- its headway: the median gap between departures at its busiest stop. `headway_stop` says which stop. Arrivals never count.

`include_routes`, `include_stops`, and `include_service_types` (default true) shape the response. A false value empties the corresponding list. The day-shape question then answers in under a KiB.

## service_types_overview

Answers every service type's own numbers: its authored service (trips, routes, stops, revenue hours, and revenue miles), on no date.

Service ids key trips. The service a rider gets on a day is a combination of types. `service_groups` answers those combinations.

## service_groups

Answers the service groups: one row per combination of service types that run together on the same dates.

Each row has:

- `id`: the members joined with " + ", for example "1-WKDY + 1-WKDY/30".
- `members`: the types in the combination, sorted. Only types that carry trips are members, because a type with no trips runs no service.
- `dates`: every date the combination holds on.
- `spans`: those dates folded into runs. A new span starts where the next date is more than seven days after the one before. Each span has start, end, and days.
- the members' pooled numbers: trips, routes, stops, revenue hours, and revenue miles.

The pooled numbers are the service on every one of the combination's dates. No single date stands in for a combination.

Types that never share a day are never pooled. For example, an overlay in one period and its successor in the next period meet the base in two rows. A dated exception, such as a holiday that a holiday type runs, is a row of its own.

The rows are sorted with the most days first. This is the axis a planner designs on. Drill-downs key on `members`.

## service_type_detail

Answers one service combination's drill-down, by `members`. The `members` can be in any order, and a `service_groups` row's `members` work.

The answer pools the members' service: routes, stops, and schedules with their trips, plus the totals. It answers `group` (the id) and `members`.

The route rows carry the combination's headways. Service ids key trips, never routes.

The answer is null for members that never run together on a date.

## service_type_compare

Answers what a change does: the proposal (`network_hash`) against its baseline (`base_hash`) under one service combination (`members`). Both documents must be loaded.

The answer has these parts:

- both sides' totals, pooled over the members of the combination: `days`, trips, revenue hours, revenue miles, and routes. The `days` value is the dates the combination holds on that side. It is 0 where the members never run together there.
- only the routes whose numbers changed: trips, span, typical headway, revenue hours, revenue miles, patterns, and stops. Each route has its baseline row and its proposal row. A row is null on the side that does not run the route.
- the count of unchanged routes.

The answer is null when the members never run together in the proposal.

## stop_gaps

Answers one stop's day folded into headway gaps. This is the service-quality answer to "how long until the next bus?".

The answer has:

- the departures.
- the max gap, and the departure it follows.
- the per-hour profile: departures, max gap, and median gap, for the gaps that start in the hour. A gap belongs to the hour it starts in.

Gaps run between consecutive distinct departure times. Two routes that leave together make no gap. Final-stop visits are arrivals, never departures.

A station's waits are its bays' waits. When other stops name the asked stop as their parent, their departures pool with its own. Nothing departs from a GTFS station itself. A bay or a plain stop reads itself alone.

An unknown stop answers zero, null, or empty. It never answers an error.

## hourly_trips

Answers the day's service profile: trips per hour, bucketed by trip start. It shows where service is thick and where it tapers.

The hour is the service-day hour, 0..47. After-midnight starts are legitimate. Each hour row carries the count of every running service type, sorted by type id. Only hours with trips are rows.

This is the planner's texture question. The op answers it from the materialized day, so no one needs a jq fold over the document.

## schedule_texture

Answers one service combination's schedule texture, by `members`. The `members` come from a `service_groups` row: the types that run together on its dates.

The answer has one row per pattern, with the members' pooled starts. Keying by service id would slice a route's day into thin, wrong headways. For example, route 164's weekday trips split across 1-WKDY and its overlays.

Each row has:

- the feeding schedule count.
- headway uniformity: the min, median, and max gap between distinct starts.
- clockface alignment: the starts on the quarter-hour grid (start % 900 == 0).

The answer is null for members that never run together.

## route_overlap

Answers the corridor read: every pair of routes that shares at least one stop on the date. The pairs are sorted by (a, b).

Each pair has:

- the shared stops.
- the shared segments. A segment is a pair of consecutive stops that both routes run, in the same order. This is the corridor proper.
- the shared stop where the pair's departures stack highest. `stack_stop` names it, with the combined count. Ties go to the lowest stop id.

For example, the op answers where the Vine Green and Red run together, instead of leaving the planner to infer it.

## timed_transfers

Answers the hub's timed-transfer view: every arrival of one route followed by a departure of another route within the connection window. It shows who can make what at this hub, not just the pooled departures.

The connection window is one of these:

- the caller's `{min_secs, max_secs}`, for a hub with widely separated bays.
- otherwise the transfer-point convention, 60 to 900 s. A rider needs a minute to move. Beyond 15 minutes, a connection is not a timed transfer.

The answer pools over the stop's walking group. Rows name the bays as `arrival_stop` and `departure_stop`. Rows are sorted by arrival, then by routes.

An unknown stop answers empty. It never answers an error.

## hub_connections

Answers the hub as a route-pair matrix: who connects to whom at this hub, and how long the wait runs.

The op folds `timed_transfers` by (`from_route`, `to_route`). It uses the same window and the same walking group. For each pair it answers:

- how many timed connections the pair makes on the date.
- the shortest and the longest wait.
- the median wait, null below four connections.

Pairs are in route order. A pair with no connection is no row.

An unknown stop answers empty. It never answers an error.

## missed_connections

Answers the hub's just-missed connections. The op pools over the stop's walking group, as `timed_transfers` does. A row is an arrival of one route and a departure of another route, where the departure time is in this range:

- It starts `miss_secs` (omitted: 300) before the arrival.
- It ends before `min_secs` (omitted: 60) after the arrival. The arrival itself is not in the range. `min_secs` is the window's minimum.

The rider sees the departure leave. No near miss is a timed transfer.

Each row has:

- `from_route`, `to_route`, `arrival_secs`, `arrival_stop`, `departure_secs`, and `departure_stop`.
- `missed_by_secs`: how much later the departure would have had to leave.
- `next_departure_secs`: that route's first departure the rider can make, or null.

Rows are sorted as `timed_transfers` sorts them. An unknown stop answers empty. It never answers an error.

## longest_waits

Answers where riders wait longest on a date. The op ranks every stop that departs by its longest wait, longest first, then by stop id. It cuts the list to `limit` (omitted: every stop).

The wait depends on `window`:

- Without a window, the wait is the largest gap between distinct departure times. A stop with fewer than two departures is no row.
- With `window` `{start_secs, end_secs}`, the wait is `type_coverage`'s window reading. It runs from the window's start to the first departure in it, between departures, and from the last departure to the window's end. A stop that departs that day but not inside the window waits the whole window.

Each stop reads alone. A station's bays are read one by one.

Each row has `stop_id`, `name`, `departures` (inside the window), `longest_wait_secs`, and `wait_from_secs`.

## day_types

Answers the day types of a period: the service combinations that have a date in [`from`, `through`], arranged by the days of the week their dates fall on. Both bounds are "YYYY-MM-DD", optional, and inclusive.

The op walks the combinations in this order: most in-period dates first, then first date, then id. For each combination:

- If its weekdays fall inside an earlier type's weekdays, it joins the first such type.
- Otherwise it starts a new type as that type's regular service.

A combination with no date in the period is left out.

Each type has `weekdays` (Monday first) and `groups`. Each group has `id`, `members`, `dates` (in the period), `spans`, and `weekdays`.

## plan_compare

Answers the proposal (`network_hash`) against its baseline (`base_hash`) across every day type of the period. The op reads `from` and `through` as `day_types` does.

The answer is the `day_types` of the proposal. Each group is `{group, comparison}`. The `comparison` is `service_type_compare` for the group's members. This gives the whole proposal in one answer.

## network_findings

Answers the worklist under one service combination (`members`, as `route_frequency` takes them): what is weak, as findings that a change can resolve.

There are three kinds of finding:

- holes: route-hours whose longest gap is at least `params.hole_factor` (1.5) times the longer of the two neighbouring hours' longest gaps. The hours come from `route_frequency`'s grid.
- waits: `longest_waits` rows of at least `params.wait_secs` (3600) on the combination's first date. That date is the answer's date.
- misses: `missed_connections` at each hub, folded by route pair. A hub is a station, or a transfer-flagged stop with no parent. A miss counts when its next departure is at least `params.miss_cost_secs` (900) after the arrival, or never. A row has the worst such wait, and how many near misses have no next departure.

`params.window` keeps hours, waits, and arrivals inside it. Each kind is sorted and cut to `params.limit`. Holes and waits sort longest first. Misses sort most first. Ties sort by key.

Each finding carries a `key`. The same finding in another version has the same key. Each row is `{now, base}`:

- With `base_hash`, `base` is the baseline's finding under the same key. It is null when the change made the finding.
- `resolved` lists the baseline's findings that have no key now.

The answer is null when the members never run together.

## route_frequency

Answers one service combination's frequency grid, by `members`. It covers every route that departs under the combination. These are the routes the members' `service_type_detail` pools, in route id order.

The op reads each route at its headway stop hour by hour. The headway stop has the most departures, and the lowest id on a tie. It is `service_type_detail`'s `headway_stop`. For each hour the row has:

- every departure in the hour.
- the shortest, longest, and median gap between distinct departure times that start in the hour. The median is null below four gaps.

Each route also carries its numbers under the group as `stats`. This is `service_type_detail`'s row for the route: trips, span, typical headway, revenue hours, and revenue miles.

The gaps are the trips' own. A target that is adjusted through the day for traffic, layover, and reliefs reads as the range it runs. The op never rounds it to a grid.

The answer is null for members that never run together.

## type_coverage

Answers the coverage of a hash-addressed weighted layer by one service combination. The combination is `members`, in any order. A `service_groups` row's `members` work. The members are the service types that run together. The op pools them over their authored service, with no date standing in. On every date the combination holds, exactly its members run. So this is what each of those dates covers.

The answer has:

- `group`: the members joined with " + ".
- `members`.
- `spans`: the dates the combination holds on.
- the layer weight the combination serves (within `radius_m` of a stop it visits), and the weight it serves frequently. These are given as weights, shares, and stop counts.
- `layer`: an echo of the reading, with the layer's resolution (see below).

The answer is null when the members never run together.

The layer arguments are:

- `field`: names the weight. It can be any numeric property, such as people, jobs, or zero-car households. Omitted, it is the layer's one numeric property. A layer with several numeric properties must name one.
- `label`: what the weight means. Omitted, it is the `field`.

The layer's resolution in the answer is one of these:

- `point_spacing_m`: for a point layer. It is the layer's average area per point, as a side. A tract centroid that far from the next swings with the radius.
- `sample_spacing_m`: for a polygon layer. A polygon's weight spreads over its area. The op samples the area on a lattice that fine. A polygon is served by the share of its area inside the radius.

Frequent service counts departures only. A terminal where nobody boards is never frequent. The frequent test depends on `window`:

- Without `window`, a stop is frequent by the whole-day average headway over departures.
- With `window` `{start_secs, end_secs}`, a stop is frequent when the worst wait anywhere in the window fits `frequent_secs`. The window's edges are included, and every route is pooled. This is the planner's "every 15 minutes, 07:00 to 19:00".

A midday or peak improvement is visible only through a window.

## unserved_points

Answers the unserved where of coverage under one service combination (`members`, in any order, as in `type_coverage`). It shows which part is unserved, and where, not just the share.

The op lists every layer feature with weight beyond `radius_m` of every stop the combination serves. This is the same served set that `type_coverage` measures against. A point is listed whole. A polygon is listed by its unserved part: its unserved share and weight, the center of that part, and its rings to draw. Each feature also carries the nearest served stop and its distance.

The op answers `unserved`: the combination's `group`, `members`, and `points`. The `points` are sorted by unserved weight, descending, so the biggest unserved places read first.

`field` and `label` read the layer as `type_coverage` does. Served plus unserved equals the layer total.

The answer is null when the members never run together.

## coverage_compare

Answers a proposal (`network_hash`) against its base (`base_hash`) on a weighted layer. Both networks and the layer must be loaded. One layer is allowed for each call.

The host names the service types of each side: `base_members` for the base, and `members` for the proposal. Each side is pooled over its authored service, as a combination of types that run together.

The answer has, for each side, the served weight and the frequent weight, as weights, shares, and stops. It also has the churn that a net change hides:

- `served_gained`: the weight the proposal newly serves.
- `served_lost`: the weight the proposal no longer serves.
- the same two values for frequent service.

The proposal's weight is the base's weight plus gained minus lost.

The op never matches one plan's types to the other's. When a fork renames or splits a type, which combination answers to which is not decided. So the host names both sides.

`field`, `label`, `radius_m`, `frequent_secs`, `min_departures`, and `window` read as `type_coverage`'s do.
