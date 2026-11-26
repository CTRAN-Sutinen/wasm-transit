# author

Authoring: what does this network become after these edits? A change is a list of typed edits. Examples: retime a trip, add a stop, extend a calendar, or fork the plan into the next pick. `apply` runs the list atomically on a registered network document. It answers the next document under its content hash, with its validation issues and warnings. The next document is registered, so the next change can chain on it. The last document is never touched. The host replaces its own copy as a whole. A removal is refused while anything still references the entity. The edit vocabulary lives in transit-network. Use this capability to change a network: a planner's UI, a script, or an agent.

## apply

Apply `edits` in order, as one atomic change, to the registered network document. One edit that cannot apply rejects the whole change. The error names the position of that edit. An edit cannot apply when it names an unknown id, when its index is out of range, when a reference is missing, or when it removes something that is still referenced. For example, a stop that a pattern visits, or pattern trips that a block carries. No edit fails silently.

The answer has these parts:

- `network_hash`: the hash of the next document. The document is already registered.
- `network`: the next document.
- `issues`: problems that gate export. A block's wait is `layover_infeasible {gap_secs, required_secs, required_by}` when it is shorter than the recovery required there. A layover's own authored length is slack. It is never the requirement. The required recovery comes from the first source that applies:
  - With `scheduling_constraints_hash`, the scheduling constraints' most specific recovery rule. `required_by` is `policy`, and the rule wins.
  - Otherwise, for a pair that has an authored layover, the smallest authored layover at that terminal. `required_by` is `terminal`.
- `warnings`: problems that never gate. They are unassigned trips, and a pick with no service in its period.
- `created`: `{kind, id}` for each entity the change made.
- `touched`: `{kind, id}` for each entity the change altered beyond that. For example, the patterns that `draw_alignment` redrew. Nothing moves silently.

Each edit is `name {arguments}`. An argument that ends in `?` is optional. The edits come in groups.

Plan and name:

- `set_plan {plan: {id, name, effective_from}, through?}`: fork the network into the next pick, or stamp a fresh import into the chain. The fork's calendar becomes its period.
  - Without `through`, the period is open at the end.
  - With `through` (the period's last day), the calendar is carried forward first. Each weekly service type that still runs in the calendar's final week, and whose last range ends before `through`, now ends on `through`.
  - A type still runs in the final week when its last range ends within six days of the latest date that any type names.
  - A service type with a closed period stays closed.
  - Dated-only service types gain nothing. Next year's holidays are the planner's to add.
  - The calendar is read only up to `through`.
  - Then every range is cut to `effective_from` .. `through`. A range or exception outside the period is removed.
  - Every service type stays, even one with no date left, because trips still name it.
  - A `through` before `effective_from` is an error.
- `update_plan {name?, effective_from?, id?}`: change the plan field by field. `effective_from` is "YYYY-MM-DD", or "" to clear the date. `id` only stamps an unversioned plan (an empty id). A pick that has an id keeps it.
- `set_name {name}`: set the document's display name.

Whole entities:

- `upsert_stop`, `upsert_route`, `upsert_pattern`, `upsert_service_type`, `upsert_pattern_trips`, `upsert_block`: put the whole entity under its id. An entity with that id is replaced. To change one, read it from the document, change it, and send it back.
- `remove_stop`, `remove_route`, `remove_pattern`, `remove_service_type`, `remove_pattern_trips`, `remove_block`: remove the entity. A removal is refused while something references the entity.

Trips:

- `retime_trip {trip, to_secs}`: move a one-start trip to start at `to_secs`. The trip keeps its id, its running times, and its blocks. Blocks name the trip by id.
- `add_trip {pattern_trips, start_secs, id?}`: add a one-start trip. The pattern's timing profile that covers the start times it. Without `id`, the id is `{pattern_trips}@{start}`. The id is reported in `created`.
- `remove_trip {trip}`: remove the trip everywhere. Its pattern trips go when they are empty. Its block closes up, and goes when it is empty.

Add one thing. Each of these is one thing a planner does, and each carries its own coherence rule. Without an `id`, the edit mints the first free id of its kind (`s1`, `r1`, `pat1`, `cal1`, `svc1`, or `b1`). The id is reported in `created`.

- `add_stop {id?, lat?, lon?}`: add a stop named "Stop N". It is placed at `lat` and `lon`, else at the centre of the network's stops.
- `add_route {id?}`: add a bus route named "Route N".
- `add_pattern {id?, route?}`: add a pattern on `route`, else on the first route. It is a skeleton to author from: the first two stops, both timepoints, 500 m apart, with one all-day timing profile at 0 and 10 minutes.
- `add_pattern_through {route, direction?, stops, run_secs, name?, id?}`: lay a new pattern through `stops` in order. It can be a new route's pattern, or another variant of a route.
  - Each visit sits at the great-circle length of the legs before it. The distance is an estimate and is never published.
  - The first and last visits are timepoints.
  - One all-day timing profile runs `run_secs` from end to end.
  - `direction` is one of the route's directions.
  - The name is `name`, else "<route> → <last stop>".
  - The line is drawn stop to stop.
  - It is an error when there are fewer than two stops, a stop is unknown, a stop appears twice in a row, the route or direction is unknown, or the run time is zero.
  - The new pattern has no trips. Give it trips with `set_headway` and its `pattern`.
- `add_service_type {id?, from}`: add a weekday service type that runs for a year from `from`.
- `add_pattern_trips {id?, pattern?, service_type?}`: add pattern trips for `pattern` and `service_type` (the first of each when absent). It holds one trip at 08:00, timed by the pattern's profile.
- `add_block {id?}`: add an empty block.

Change fields:

- `update_stop {id, name?, lat?, lon?, transfer?}`, `update_route`, `update_service_type {id, name}`, `set_pattern_trips_refs {id, pattern?, service_type?}`: change the entity field by field. References are checked.
- `update_pattern`: change a pattern's fields. `direction` names one of its route's directions, and "" leaves the pattern in none. A move to another `route` leaves the pattern in no direction, unless the same edit places it with `direction`. Direction ids are per route.
- `set_direction {route, id, name}`: add a direction to a route, or rename one.
- `remove_direction {route, id}`: remove a direction. It is refused while a pattern runs in it.

Lines and stops of a pattern:

- `draw_alignment {pattern, points: [{lat, lon}, …], redraw: one|all, id?}`: redraw a pattern's line. It needs two points or more, each on the globe. `redraw` names what moves.
  - `one`: the pattern gets a new alignment of its own. Its id is `id`, else `al{n}`. The id is reported in `created`. The other patterns on the old line keep it.
  - `all`: the alignment that the pattern runs is redrawn, so every pattern on it moves. A pattern with no alignment gets a new one, as with `one`.
  - The redrawn patterns are reported in `touched`.
  - Their stop distances keep their meters, but stop claiming to be published.
- `insert_pattern_stop {pattern, stop, index?}`: add a visit to `stop` before visit `index`. The default is the last stop. The new visit is halfway along that leg. It is not a timepoint, and it is unscheduled on every trip. The endpoints stay the endpoints.
- `remove_pattern_stop {pattern, index}`, `toggle_timepoint {pattern, index}`: change a middle visit only. Every timing profile and every trip's times stay aligned.
  - A new timepoint takes its neighbours' midpoint in each profile. Each trip is scheduled there where it ran.
  - Removing a timepoint's visit removes its entry from every profile.
- `set_pattern_stop {pattern, index, stop?, distance_m?}`: change one visit.
- `add_timing_profile {pattern}`: add a profile after the last one. It is 6 hours long and copies the times of the last one.
- `remove_timing_profile {pattern, index}`: remove a profile. The last profile stays.
- `set_timing_profile {pattern, index, start_secs?, end_secs?}`: change a profile's window.
- `set_timepoint_time {pattern, profile, timepoint, secs}`: set one timepoint's time in one profile.

Service days:

- `toggle_service_day {id, day}`: flip one weekday (`mon` to `sun`) of a service type.
- `add_service_range {id, start?, end?}`: add a date range ("YYYY-MM-DD"). A date left out continues the last range: `start` where the last range ends, and `end` at `start`. A key with no range takes its first range only with a `start`. A range never ends before it starts.
- `set_service_range {id, index, start?, end?}`, `remove_service_range {id, index}`: change or remove one range.
- `set_service_exception {id, date, state}`: add or change the exception for `date`. There is one for each date, kept in date order.
- `remove_service_exception {id, date}`: remove it.

Frequency bands of a trip:

- `set_band {trip, index, start_secs?, end_secs?, headway_secs?}`: change one band.
- `add_band {trip}`: add a band after the last one. It is 3 hours long, with a 15-minute headway.
- `remove_band {trip, index}`: remove a band. The last band stays.

Blocks:

- `add_block_trip {block, trip: {trip}}`: add a trip to a block. The block stays in order of first start. Layovers past the last trip are dropped. A layover stays only while its two trips stay adjacent.
- `remove_block_trip {block, index}`: remove a trip from a block. A layover stays only while its two trips stay adjacent.
- `set_layover {block, after_trip, secs | null}`: set the layover after trip `after_trip`, or clear it with `null`. A layover exists only between two trips of the block.

Timetable gestures. These address a route and a direction, and a service type for most of them. They are the directory's `timetable`. Each applies across every pattern of the direction. `direction` null, or omitted, means the route's patterns in no direction. Only one-start trips move. Pattern trips that a gesture empties are removed. New pattern trips are named `{service_type}:{pattern}` and reported in `created`.

- `set_headway {route, direction, service_type, from_secs, to_secs, headway_secs, pattern?}`: set the frequency for a period. There is a one-start trip every `headway_secs`, from `from_secs` (the first departure) until before `to_secs`.
  - One-start trips already in the window move to the nearest slots, in order. They keep their ids, times, and blocks.
  - Open slots become new trips with new ids. They run on `pattern`, else on the pattern of the trip before. The pattern's profile times them. They are unassigned.
  - Surplus trips go.
  - Frequency trips are left alone.
- `set_span {route, direction, service_type, first_secs?, last_secs?, headway_secs?}`: set the first and last trip. Trips before `first_secs` or after `last_secs` go. A bound past the first or last trip is reached at `headway_secs`, else at the edge's median gap. The new trips run on the edge trip's pattern.
- `adjust_running_time {route, direction, service_type, from_secs, to_secs, delta_secs}`: the direction's one-start trips under that service type, departing from `from_secs` until before `to_secs`, run `delta_secs` longer. A negative value makes them shorter. The change is in proportion along each trip. Each trip keeps its id and its start. Other service types, frequency trips, and the profiles are not changed.
- `insert_direction_stop {route, direction, stop, after}`: visit `stop` right after the stop `after`, on every pattern of the direction that visits `after` before its end.
- `remove_direction_stop {route, direction, stop}`: stop visiting `stop`, on every pattern of the direction. It is refused when the stop is an endpoint.

## stamp_pick

What is an imported feed as a pick? `stamp_pick` stamps the registered network as a pick: one `set_plan` edit under `id` and `name`, effective from the feed's first service date (the earliest date any service type with trips runs).

A document that is a pick already (it carries a plan id) is not stamped. Neither is a feed that runs no service, because it has no date. It comes back as it was, and `stamped` is false.

The answer is the next document, registered under its own hash, with `issues` and `warnings` as `apply` answers them. The host gives the id and the name. It applies no rule of its own.
