# directory

The directory: what exists in the network. It answers these questions:

- The stops, with their walking-group structure. A host can ask for them by name, or for the places nearest a point.
- The routes, with their short and long names.
- One route's patterns, with their ordered stops (list to detail).

The pickers and the names join read here. These are questions about the model. They are never reads of a feed.

A route runs in directions. Each direction's stop sequences are its patterns (bay and short-turn variants). Each pattern has its own ordered stops. A diversion editor anchors against them.

The planner's lists also ask here:

- Rows of any kind, narrowed by a filter.
- A route's composition: each pattern trips' trip count, and its patterns by direction.
- The routes that serve a stop.
- Where an entity is on the map.
- The timetable: one route, one direction, and one service combination, across the direction's patterns.

## about

What the network is: its name and the plan it is (`{id, name, effective_from}`), as the document carries them. A host shows this as the heading over a network that it registered from bytes it never parsed.

## stops

The stop list, name-sorted (the pickers' shape). Every stop has its coords, its parent, and its transfer flag. The parent is the flag that the station board keys off.

`query` narrows the list the way a picker's typing does. Every whitespace-separated term must be answered, and the match is case-insensitive. A term is answered in one of these ways:

- It is inside the name or the stop id. For example, "fisher landing" finds Fisher's Landing and its bays.
- It is an abbreviation on the sign. This is a name word of three letters or more that begins the term. It can also be a word that is a subsequence of the term and shares its first and last letter. For example, "vancouver mall" finds Van Mall, and "main street" finds Main St.
- It is within a typo of a name word or its start. A typo is one edit from four letters, and two edits from eight letters. An edit is a substituted, added, dropped, or swapped letter. A term under four letters never matches by typo.

Exact matches list first, then abbreviations, then typos. Each tier is sorted by name, then id.

`limit` truncates the list after the sort. Omit it for the whole list. A directory that truncates without saying so would be wrong.

## stops_near

The stop list asked from a point (`{lat, lon}`): the places nearest it. The order is by great-circle distance in meters (haversine), nearest first, with ties by stop id.

Each row has the stop's id, name, coords, parent, and transfer flag (the fields of the `stops` row). It also has `distance_m`.

The point's walks are optional. They are `[{stop_id, walk_secs}]`. The streets capability's access answers them. Pass them as they are. With the walks, the order changes:

- The places that a walk reaches come first. They are ordered by walk, then by distance. Each has `walk_secs`: the shortest walk to any of its members.
- The other places follow by distance, with `walk_secs` null.

A place is what a rider walks to. A station's bays answer once, as the station (the walking group that the station board departs across). The distance is that of its nearest member (the pin or any bay). A stop whose station the network does not hold is its own place.

`limit` caps the rows (default 10). `radius_m` (meters) drops every place that is farther than it. Omit `radius_m` for the nearest `limit` places, however far they are.

These are request errors: a `lat` outside -90..90, a `lon` outside -180..180, or a negative radius.

## stops_by_ids

Stops by id, in the request's order. This is the names join that hosts otherwise do by hand when a view (like the one in rt) carries ids that a human reads as names. Unknown ids are omitted.

## routes

The route list, name-sorted. Every route has these fields:

- `name`: the short public name, else the long one.
- `long_name`: for example "Lincoln". It is null for a route without one.
- `mode`: the model's closed set, not GTFS's numeric codes.
- The route's authored color, when there is one.

## route_stop_sequence

One route's patterns with their ordered stops (`route_detail` groups them by direction). An unknown route answers empty. It is never an error.

## focus

Where one entity is on the map. The request is `{kind: stop | route | pattern | pattern_trips | service_type | block, id}`. The answer has the entity's lines and the stops it serves. A line is each pattern's authored alignment, else its stops joined.

The entity decides what the answer covers:

- A stop is its point.
- Pattern trips are their pattern.
- A route, a service type, or a block is every pattern that it runs.

The answer is null for an unknown id.

## route_lines

The network on a map: every route that draws a line, in id order. Each row has these fields:

- `route_id` and `name`.
- `color`: the agency's color, or null.
- `lines`: the distinct lines of the route's patterns. Each is its authored alignment, else its stops joined. This is what `focus` frames for the route.

`members` (service types, optional) keeps only the patterns that run trips under one of them. A route with none is no row.

## service_keys

The service keys, in id order. Each has these fields:

- `id` and `name`.
- `rule`: its calendar as authored, with days `mon` to `sun`, ranges `[{start, end}]`, and exceptions `[{date, state}]`. The `toggle_service_day` edit, the service-range edits, and the service-exception edits change it.
- `trips`: the materialized count. A frequency trip counts its instances.
- `routes`: `[{route_id, name, pattern_trips, trips}]`. `pattern_trips` holds the ids under the key on that route. The `set_pattern_trips_refs` edit moves them to another key. A route that the key runs nothing on is no row.

## entities

Every entity of one kind, in id order. The request names the kind: `{kind: stop | route | pattern | pattern_trips | service_type | block}`.

An optional `query` narrows the list the way a filter box's typing does. Every whitespace-separated term must be in the id, the name, or a route's long name. The match is case-insensitive. For example, "lin" finds 002 Lincoln.

Each row has these fields:

- `id` and `name`.
- `context`: for a pattern, its route. For pattern trips, the pattern and the service type (`pattern · service type`).
- `transfer`: for stops.
- `trips`: for pattern trips, the materialized trip count. For a block, its chained trips.
- `long_name`: for routes, the long name beside the short one. It is null without one.

## route_detail

One route's composition. The answer has these parts:

- Its patterns.
- The stops that they visit, in first-visit order.
- Its pattern trips grouped by service type. Each has the trips that it runs (the same expansion that the materialized day runs).
- Its patterns grouped by direction: the route's named directions in id order, then any patterns in none.

Each direction also lists its variants, most trips first. A variant is a way a rider rides the direction. Patterns that visit the same places in the same order are one variant. A station's bays count as one place, so patterns that differ only by the bay they use at a transit center are one variant.

Each variant has these fields:

- A name: "first place → last place". It adds "via" a place that only this variant visits. It adds ", skips" a place that it does not visit, when another variant shares its ends.
- Its pattern ids.
- Its places: the station or stop, and the stops used there.
- Its trips.

The answer is null for an unknown route.

## stop_detail

One stop's composition. The answer is null for an unknown stop. It has these parts:

- The routes whose patterns visit the stop (id, name, and `long_name` rows, in id order). A stop with service of its own answers its own routes. A bay never answers its siblings' routes. A stop that no pattern visits (a station pin) answers its walking group's routes.
- `stop`: the stop's own record (name, point, parent, kind, transfer, info).
- `visits`: every pattern position that visits this stop. It never lists its group's positions. The order is by route, then pattern id, then position.

Each visit has these fields:

- Its route and label.
- Its direction and its name.
- Its pattern and name.
- Its index.
- Whether it ends the pattern.
- Whether it is a timepoint.
- `previous` and `next`: the stop before it and the stop after it on the pattern. There is none at the pattern's first and last stop. Each has its stop, name, and the meters along the pattern between them. Each also has `published`: whether the agency published both distances, or the import estimated one.
- `trips`: what the pattern runs under `members` (the service types asked, every type when omitted). It lists each trip's instances.

## timetable

The planner's timetable: one route, one `direction`, and one service combination, named by its `members`. An omitted or null `direction` means the patterns in no direction. The `members` are the types that run together on the same dates (the rows of the analysis `service_groups` answer).

The answer is headed by `route_name` and `route_long_name` (null without one). It is also headed by `group` (the members joined with " + ") and `members`. It lists every trip that the members run on the direction's patterns. Service ids key trips, never routes. The rows are the materialized day's trips.

There is one row for each trip, ordered by start. A row has these fields:

- Its identity: `trip` and `start_secs`.
- Its pattern trips, and the service key that they run under (`service_type`).
- Its pattern and block.
- `run_secs`: its running time end to end. This is the last stop's arrival less its start, and each trip has its own.
- Its departure at each column. It is null where the trip's pattern does not stop.

The columns are every stop that those patterns visit, in route order. A column is flagged when it is a timepoint. The legend lists the patterns, most trips first. It marks the headway-band ones.

`compare_hash` names a loaded baseline. With it, a trip's id is its identity:

- A one-start trip that both run at different starts carries `retimed_from_secs`, its baseline start. The trip is kept. It is never shown as removed and added.
- A trip that the baseline does not run is marked `added`.
- `removed` lists the baseline's trips that this one no longer runs.
- A frequency trip's instances share its id. So each is matched by (`trip`, `start_secs`).
- The baseline side runs what the members run there.

The answer is null for an unknown route or direction. It is also null for members that never run together.
