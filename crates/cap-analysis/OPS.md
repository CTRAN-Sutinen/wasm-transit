# analysis

Planner analysis over an authored network document: network/route/stop stats, per-service-type overview and detail, coverage of a weighted layer (people, jobs, households — points or polygons) and a proposal's coverage gains and losses. The first capability extracted from the monolith — the domain code lives in the shared transit-analysis library. Assemble for planners; riders never download it. Documents are hash-addressed (load once, query by hash — never per-op document serialization).

## network_stats

The network's numbers on one service date: trips, spans, revenue hours AND revenue miles, transfer points, per-route and per-stop activity, headways. Each route row carries its own revenue_hours and revenue_miles (the per-route cost basis — every cost model is per hour AND per mile; average operating speed follows from the two) and its headway as the MEDIAN gap between departures at its busiest stop (headway_stop says which) — arrivals never count. Every route of the document is a row, sorted by id: a route with no service on the date answers trips 0 and null spans (inactive routes are analysis too — filter on trips). include_routes/include_stops/include_service_types (default true) shape the response: false empties the corresponding lists — the day-shape question answers in under a KiB.

## service_types_overview

Every service type's own numbers: its authored service (trips, routes, stops, revenue hours and miles), on no date. Service ids key trips; the service a rider gets on a day is a COMBINATION of types — service_groups.

## service_groups

The service groups (D-55): one row per COMBINATION of service types that run together on the same dates — `id` (its members joined " + ", e.g. "1-WKDY + 1-WKDY/30"), `members` (sorted; only types that carry trips — a type with none runs no service), `dates` (every date it holds on) and `spans` (those dates folded into runs: a new span where the next date is more than seven days on; each with start, end, days) — and the members' POOLED numbers (trips, routes, stops, revenue hours and miles), which ARE the service on every one of its dates: no date stands in for a combination. Types that never share a day are never pooled (an overlay in one period and its successor in the next meet the base in two rows), and a dated exception (a holiday a holiday type runs) is a row of its own. Most days first. The axis a planner designs; drill-downs key on `members`.

## service_type_detail

One service COMBINATION's drill-down, by `members` (any order — a service_groups row's): its members' service pooled — routes, stops, and schedules with their trips, plus the totals, answered as `group` (the id) and `members`. The route rows carry the combination's headways (service ids key trips, never routes: the 164 lesson). Null for members that never run together on a date.

## service_type_compare

What a change does: the proposal (network_hash) against its baseline (base_hash) under one service COMBINATION (`members`) — both sides' totals pooled over its members (days: the dates it holds on that side, 0 where they never run together there; trips, revenue hours and miles, and routes), then only the routes whose numbers changed (trips, span, typical headway, revenue hours and miles, patterns, stops), each with its baseline and proposal row (null on the side that does not run it), and the count of unchanged routes. Null when the members never run together in the proposal. Both documents must be loaded.

## stop_gaps

One stop's day folded into headway gaps: departures, the max gap and the departure it follows, and the per-hour profile (departures, max and median gap starting in the hour — a gap belongs to the hour it starts in). Gaps run between consecutive DISTINCT departure times — two routes leaving together is no gap at all — and final-stop visits are arrivals, never departures. The service-quality read of \"how long until the next bus?\". A station's waits are its bays': asked for a stop other stops name as parent, their departures pool with its own (nothing departs from a GTFS station itself); a bay or a plain stop reads itself alone. An unknown stop answers zero/null/empty, never an error.

## hourly_trips

The day's service profile: trips per hour bucketed by trip start (the service-day hour, 0..47 — after-midnight starts are legitimate), each hour row carrying every running service type's count, sorted by type id. Only hours with trips are rows — where service is thick, where it tapers. The planner's texture question (QAF-4), answered from the materialized day, not a jq fold over the document.

## schedule_texture

One service COMBINATION's schedule texture, by `members` (a service_groups row's — the types that run together on its dates). One row per PATTERN with the members' POOLED starts — keying by service id would slice a route's day into thin, wrong headways (route 164's weekday trips split across 1-WKDY and its overlays). Each row: the feeding schedule count, headway uniformity (min/median/max gap between distinct starts), and clockface alignment (starts on the quarter-hour grid, start % 900 == 0). Null for members that never run together (QAF-19).

## route_overlap

The corridor read: every pair of routes that shares at least one stop on the date, sorted by (a, b) — shared stops, shared SEGMENTS (consecutive stop pairs both run, in the same order — the corridor proper), and the shared stop where the pair's departures stack highest (stack_stop, with the combined count; ties by lowest stop id). Where the Vine Green and Red run together, answered instead of inferred.

## timed_transfers

The hub's timed-transfer view: every arrival of one route followed by a departure of another within the connection window — the caller's ({min_secs, max_secs}, for a hub with widely separated bays), else the transfer-point convention (60–900 s: a rider needs a minute to move, and beyond 15 minutes it is not a timed transfer) — pooled over the stop's walking group (bays named: arrival_stop, departure_stop), sorted by arrival then routes. Who can make what at this hub, not just pooled departures. An unknown stop answers empty, never an error.

## hub_connections

The hub as a route-pair matrix: timed_transfers (the same window, the same walking group) folded by (from_route, to_route) — how many timed connections the pair makes on the date, the shortest and longest wait, and the median wait (null below four connections), pairs in route order; a pair with no connection is no row. Who connects to whom at this hub, and how long the wait runs. An unknown stop answers empty, never an error.

## missed_connections

The hub's just-missed connections: over the stop's walking group (as timed_transfers pools it), every arrival of one route and departure of another where the departure leaves from miss_secs (omitted: 300) before the arrival up to, not including, the window's min_secs (omitted: 60) after it — the rider sees it go. No near miss is a timed transfer. Each row: from_route, to_route, arrival_secs, arrival_stop, departure_secs, departure_stop, missed_by_secs (how much later it would have had to leave), next_departure_secs (that route's first departure the rider can make, or null). Sorted as timed_transfers sorts. An unknown stop answers empty, never an error.

## longest_waits

Where riders wait longest on a date: every stop that departs, ranked by its longest wait (longest first, then stop id), cut to limit (omitted: every stop). Without a window the wait is the largest gap between distinct departure times (a stop with fewer than two is no row); with window {start_secs, end_secs} it is type_coverage's window reading — from the window's start to the first departure in it, between departures, and from the last to its end — and a stop that departs that day but not inside the window waits the whole window. Each stop reads alone (a station's bays one by one). Rows: stop_id, name, departures (inside the window), longest_wait_secs, wait_from_secs.

## day_types

The day types of a period: the service combinations with a date in [from, through] ("YYYY-MM-DD", each optional, inclusive), arranged by the days of the week their dates fall on (D-137). Walking the combinations with the most in-period dates first (then first date, then id), one whose weekdays fall inside an earlier type's joins the first such type; any other starts a new type as its regular service. Each type: weekdays (Monday first) and groups — id, members, dates (in the period), spans, weekdays. A combination with no date in the period is left out.

## plan_compare

The proposal (network_hash) against its baseline (base_hash) across every day type of the period (from, through as day_types takes them): day_types of the proposal, each group {group, comparison} carrying service_type_compare for its members — the whole proposal in one answer.

## network_findings

The worklist under one service combination (members, as route_frequency takes them), D-136: holes — route-hours whose longest gap is at least params.hole_factor (1.5) times the longer of both neighbouring hours' (route_frequency's grid); waits — longest_waits rows of at least params.wait_secs (3600) on the combination's first date (the answer's date); misses — missed_connections at each hub (a station, or a transfer-flagged stop with no parent) folded by route pair: the near misses whose next departure is at least params.miss_cost_secs (900) after the arrival or never, the worst such wait, and how many have none. params.window keeps hours, waits and arrivals inside it; each kind sorted (holes and waits longest first, misses most first, then key) and cut to params.limit. Each finding carries a key (the same finding in another version); each row is {now, base}: with base_hash, base is the baseline's finding under the same key (null when the change made it), and resolved lists the baseline's findings with no key now. Null when the members never run together.

## route_frequency

One service COMBINATION's frequency grid, by `members`: every route that departs under it (the members service_type_detail pools, route id order), read at its headway stop (the most departures, lowest id on a tie — service_type_detail's headway_stop) hour by hour: every departure in the hour, and the shortest, longest and median gap (null below four gaps) between distinct departure times starting in the hour — and the route's numbers under the group (stats: service_type_detail's row for it: trips, span, typical headway, revenue hours and miles). The gaps are the trips' own — a target adjusted through the day for traffic, layover and reliefs reads as the range it runs, never rounded to a grid. Null for members that never run together.

## type_coverage

Coverage of a hash-addressed WEIGHTED LAYER by one service COMBINATION — `members`, any order (a service_groups row's): the service types that run together, pooled over their authored service with no date standing in (on every date the combination holds exactly its members run, so this is what each of those dates covers). Answers the combination's `group` (its members joined " + "), its `members`, the `spans` of dates it holds on, and the layer weight it serves (within radius_m of a stop it visits) and serves frequently, as weights, shares and stop counts; null when the members never run together. `field` names the weight — any numeric property: people, jobs, zero-car households (omitted: the layer's one numeric property; a layer with several must name one) — and `label` what it means (omitted: the field); the answer's `layer` echoes the reading with the layer's resolution: point_spacing_m (a point layer's average area per point, as a side — a tract centroid that far from the next swings with the radius) or sample_spacing_m (a polygon's weight spreads over its area, sampled on a lattice that fine; a polygon is served by the share of its area inside the radius). Frequent counts DEPARTURES only (a terminal nobody boards at never is): the whole-day average headway over departures, unless `window` ({start_secs, end_secs}) is given — then a stop is frequent when the worst wait anywhere in the window (edges included, every route pooled) fits frequent_secs: the planner's "every 15 minutes, 07:00–19:00". A midday or peak improvement is visible only through a window.

## unserved_points

The unserved WHERE of coverage under one service COMBINATION (`members`, any order, as type_coverage): every layer feature with weight beyond radius_m of every stop the combination serves (the same served set type_coverage measures against) — a point whole, a polygon by its unserved part (its unserved share and weight, the centre of that part, and its rings to draw) — each with the nearest served stop and its distance: which part, and where, not just the share. Answers `unserved`: the combination's `group`, `members` and `points`, sorted by unserved weight descending so the biggest unserved places read first; null when the members never run together. `field` and `label` read the layer as type_coverage does. Served + unserved conserves the layer total.

## coverage_compare

A proposal (network_hash) against its base (base_hash) on a weighted layer, each side the SERVICE TYPES the host names — base_members of the base, members of the proposal, each pooled over its authored service (a combination of types that run together): each side's served and frequent weight, shares and stops, and the CHURN a net change hides — the weight the proposal newly serves (served_gained) and no longer serves (served_lost), and the same for frequent service; proposal = base + gained − lost. The op never matches one plan's types to the other's: which combination answers to which when a fork renames or splits a type is not decided (QAF-20), so the host names both sides. `field`, `label`, radius_m, frequent_secs, min_departures and `window` read as type_coverage's. Both networks and the layer must be loaded; one layer per call.
